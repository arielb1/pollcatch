use std::{ffi::OsString, io::BufReader};

use clap::{Parser, Subcommand};
use jfrs::reader::{
    event::Accessor,
    value_descriptor::{Primitive, ValueDescriptor},
    Chunk, JfrReader,
};
use pr_parser::PossiblyUnknownEvent;
use std::io::{Read, Seek};
use std::time::Duration;

mod pr_parser;

#[derive(Debug, Parser)]
#[command(name = "pollcatch-decoder")]
#[command(about = "Find slow polls from a JFR")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Print long polls from a JFR file
    Longpolls {
        /// JFR file to read from
        jfr_file: OsString,
        /// PR file to read performance data from
        #[arg(long)]
        pr_file: Option<OsString>,
        /// Duration to mark from
        #[clap(value_parser = humantime::parse_duration)]
        min_length: Duration,
        #[arg(long, default_value = "5")]
        stack_depth: usize,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PollEventKey {
    tid: u32,
    clock_start: u64,
    duration: u64,
}

#[derive(PartialEq, Eq, Copy, Clone)]
enum ClockSource {
    Tsc,
    Monotonic,
}


fn make_pr_map<R: Read + Seek>(pr_reader: &mut R, clock_source: ClockSource) -> anyhow::Result<Vec<PollEventKey>> {
    let mut pr_map = Vec::new();
    let mut calibration = None;
    while let Some(record) = pr_parser::read_event(pr_reader)? {
        match record {
            PossiblyUnknownEvent::UnknownEvent { .. } => continue,
            PossiblyUnknownEvent::Event(pr_parser::Event::CalibrateTscToMonotonic { data }) => {
                calibration = Some(data);
            }
            PossiblyUnknownEvent::Event(pr_parser::Event::Poll {
                start,
                end,
                clock_end,
                tid,
            }) => {
                let (clock_start, duration) = match clock_source {
                    ClockSource::Tsc => {
                        (start, end.saturating_sub(start))
                    }
                    ClockSource::Monotonic => {
                        let Some(calibration) = &calibration else {
                            tracing::warn!("got poll event but no calibration");
                            continue;
                        };
                        let poll_duration = end.saturating_sub(start);
                        let duration = calibration.scale_src_duration_to_ref(poll_duration);
                        let clock_start = clock_end.saturating_sub(duration);
                        (clock_start, duration)
                    }
                };
                pr_map.push(PollEventKey {
                    tid,
                    clock_start,
                    duration,
                });
            }
        }
    }
    pr_map.sort();
    Ok(pr_map)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt::init();
    match cli.command {
        Commands::Longpolls {
            jfr_file,
            pr_file,
            min_length,
            stack_depth,
        } => {
            let (tsc_pr_map, monotonic_pr_map) = if let Some(pr_file) = pr_file {
                let mut pr_reader = BufReader::new(std::fs::File::open(pr_file.clone())?);
                let tsc_pr_map = make_pr_map(&mut pr_reader, ClockSource::Tsc)?;
                let mut pr_reader = BufReader::new(std::fs::File::open(pr_file)?);
                let monotonic_pr_map = make_pr_map(&mut pr_reader, ClockSource::Monotonic)?;
                (tsc_pr_map, monotonic_pr_map)
            } else {
                (Vec::new(), Vec::new())
            };
            let mut reader = BufReader::new(std::fs::File::open(jfr_file)?);
            print_samples(jfr_samples(&mut reader, min_length, &tsc_pr_map, &monotonic_pr_map)?, stack_depth);
            Ok(())
        }
    }
}

fn symbol_to_string(s: Accessor<'_>) -> Option<&str> {
    if let Some(sym) = s.get_field("string") {
        if let Ok(val) = sym.value.try_into() {
            return Some(val);
        }
    }

    None
}

fn print_samples(samples: Vec<Sample>, stack_depth: usize) {
    for sample in samples {
        if sample.frames.iter().any(|f| {
            f.name.as_ref().is_some_and(|n| {
                n.contains(
                    "<tokio::runtime::scheduler::multi_thread::worker::Context>::park_timeout",
                )
            })
        }) {
            // skip samples that are of sleeps
            continue;
        }
        println!(
            "[{:.6}] thread {} - poll of {}us",
            sample.start_time.as_secs_f64(),
            sample.thread_id,
            sample.delta_t.as_micros()
        );
        for (i, frame) in sample.frames.iter().enumerate() {
            if i == stack_depth {
                println!(
                    " - {:3} more frame(s) (pass --stack-depth={} to show)",
                    sample.frames.len() - stack_depth,
                    sample.frames.len()
                );
                break;
            }
            println!(
                " - {:3}: {}.{}",
                i + 1,
                frame.class_name.as_deref().unwrap_or("<unknown>"),
                frame.name.as_deref().unwrap_or("<unknown>")
            );
        }
        println!();
    }
}

struct Sample {
    delta_t: Duration,
    start_time: Duration,
    thread_id: i64,
    frames: Vec<StackFrame>,
}

struct StackFrame {
    class_name: Option<String>,
    name: Option<String>,
}

fn resolve_stack_trace(trace: Accessor<'_>) -> Vec<StackFrame> {
    let mut res = vec![];
    if let Some(frames) = trace.get_field("frames") {
        if let Some(frames) = frames.as_iter() {
            for frame in frames {
                let mut class_name_s = None;
                let mut name_s = None;
                if let Some(method) = frame.get_field("method") {
                    if let Some(class) = method.get_field("type") {
                        if let Some(class_name) = class.get_field("name") {
                            class_name_s = symbol_to_string(class_name).map(|x| x.to_owned());
                        }
                    }
                    if let Some(name) = method.get_field("name") {
                        name_s = symbol_to_string(name).map(|x| x.to_owned());
                    }
                }
                res.push(StackFrame {
                    class_name: class_name_s,
                    name: name_s,
                });
            }
        }
    }
    res
}

fn find_delta_t_from_clock(pr_map: &Vec<PollEventKey>, tid: i64, clock_start: i64) -> Option<u64> {
    if let (Ok(tid), Ok(clock_start)) = (tid.try_into(), clock_start.try_into()) {
        let partition_point = pr_map
            .partition_point(|x| x.tid < tid || (tid == x.tid && x.clock_start <= clock_start));
        if let Some(index) = partition_point.checked_sub(1) {
            let bound = pr_map[index];
            let inside = tid == bound.tid
                && bound.clock_start < clock_start
                && clock_start - bound.clock_start < bound.duration;
            if inside {
                return Some(clock_start - bound.clock_start);
            }
        }
        None
    } else {
        None
    }
}

fn process_sample(
    chunk: &Chunk,
    pr_map: &Vec<PollEventKey>,
    sampled_thread: Option<&ValueDescriptor>,
    stacktrace: Option<&ValueDescriptor>,
    appword: Option<i64>,
    start_time_ticks: i64,
    os_thread_index: usize,
    long_poll_duration: u128,
) -> Option<Sample> {
    let mut delta_t = 0;
    let mut thread_id = !0;
    if let Some(ValueDescriptor::Object(st)) = sampled_thread {
        if let Some(&ValueDescriptor::Primitive(Primitive::Long(tid))) =
            st.fields.get(os_thread_index)
        {
            thread_id = tid as i64;
        }
    }
    if let Some(appword) = appword {
        delta_t = appword as u64;
    }
    if delta_t == 0 {
        if let Some(delta_t_) = find_delta_t_from_clock(pr_map, thread_id, start_time_ticks) {
            delta_t = delta_t_;
        }
    }

    let delta_t_micros = (delta_t as u128) * 1000000 / (chunk.header.ticks_per_second as u128);
    if delta_t_micros < long_poll_duration {
        return None;
    }
    stacktrace.map(|trace| Sample {
        thread_id,
        start_time: Duration::from_nanos(
            ((start_time_ticks as u128) * 1_000_000_000 / (chunk.header.ticks_per_second as u128))
                as u64,
        ),
        delta_t: Duration::from_micros(delta_t_micros as u64),
        frames: resolve_stack_trace(Accessor::new(chunk, trace)),
    })
}

fn jfr_samples<T>(
    reader: &mut T,
    long_poll_duration: Duration,
    tsc_pr_map: &Vec<PollEventKey>,
    monotonic_pr_map: &Vec<PollEventKey>,
) -> anyhow::Result<Vec<Sample>>
where
    T: Read + Seek,
{
    let mut jfr_reader = JfrReader::new(reader);
    let long_poll_duration = long_poll_duration.as_micros();

    let mut samples = vec![];
    for chunk in jfr_reader.chunks() {
        let (mut c_rdr, c) = chunk?;
        let mut wall_clock_sample = None;
        let mut execution_sample = None;
        let mut wcs_start_time_index = !0;
        let mut exs_start_time_index = !0;
        let mut appword_index = !0;
        let mut wcs_stacktrace_index = !0;
        let mut exs_stacktrace_index = !0;
        let mut wcs_sampled_thread_index = !0;
        let mut exs_sampled_thread_index = !0;
        let mut active_setting_name_index = !0;
        let mut active_setting_value_index = !0;
        let mut os_thread_index = !0;
        let mut active_setting = None;
        for ty in c.metadata.type_pool.get_types() {
            if ty.name() == "profiler.WallClockSample" {
                wall_clock_sample = Some(ty.class_id);
                for (i, field) in ty.fields.iter().enumerate() {
                    match field.name() {
                        "startTime" => wcs_start_time_index = i,
                        "appword" => appword_index = i,
                        "stackTrace" => wcs_stacktrace_index = i,
                        "sampledThread" => wcs_sampled_thread_index = i,
                        _ => {}
                    }
                }
            }
            if ty.name() == "jdk.ExecutionSample" {
                execution_sample = Some(ty.class_id);
                for (i, field) in ty.fields.iter().enumerate() {
                    match field.name() {
                        "startTime" => exs_start_time_index = i,
                        "stackTrace" => exs_stacktrace_index = i,
                        "sampledThread" => exs_sampled_thread_index = i,
                        _ => {}
                    }
                }
            }
            if ty.name() == "java.lang.Thread" {
                for (i, field) in ty.fields.iter().enumerate() {
                    match field.name() {
                        "osThreadId" => os_thread_index = i,
                        _ => {}
                    }
                }
            }
            if ty.name() == "jdk.ActiveSetting" {
                active_setting = Some(ty.class_id);
                for (i, field) in ty.fields.iter().enumerate() {
                    match field.name() {
                        "name" => active_setting_name_index = i,
                        "value" => active_setting_value_index = i,
                        _ => {}
                    }
                }
            }
        }
        let mut pr_map = monotonic_pr_map;
        for event in c_rdr.events(&c) {
            let event = event?;
            if Some(event.class.class_id) == active_setting {
                if let ValueDescriptor::Object(o) = event.value().value {
                    let name = o
                        .fields
                        .get(active_setting_name_index)
                        .and_then(|st| Accessor::new(&c, st).resolve())
                        .map(|a| a.value);
                    let value = o
                        .fields
                        .get(active_setting_value_index)
                        .and_then(|st| Accessor::new(&c, st).resolve())
                        .map(|a| a.value);
                    match (name, value) {
                        (Some(ValueDescriptor::Primitive(Primitive::String(name))),
                        Some(ValueDescriptor::Primitive(Primitive::String(value)))) if name == "clock" => {
                            if value == "tsc" {
                                pr_map = tsc_pr_map;
                            } else {
                                pr_map = monotonic_pr_map;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if Some(event.class.class_id) == wall_clock_sample {
                if let ValueDescriptor::Object(o) = event.value().value {
                    let start_time_ticks =
                        if let Some(&ValueDescriptor::Primitive(Primitive::Long(start_time))) =
                            o.fields.get(wcs_start_time_index)
                        {
                            start_time
                        } else {
                            0
                        };
                    let sampled_thread = o
                        .fields
                        .get(wcs_sampled_thread_index)
                        .and_then(|st| Accessor::new(&c, st).resolve())
                        .map(|a| a.value);
                    let appword = if appword_index == !0 {
                        None
                    } else {
                        match o.fields.get(appword_index) {
                            Some(&ValueDescriptor::Primitive(Primitive::Long(appword))) => {
                                Some(appword)
                            }
                            _ => None,
                        }
                    };
                    let stacktrace = o.fields.get(wcs_stacktrace_index);
                    if let Some(sample) = process_sample(
                        &c,
                        pr_map,
                        sampled_thread,
                        stacktrace,
                        appword,
                        start_time_ticks,
                        os_thread_index,
                        long_poll_duration,
                    ) {
                        samples.push(sample);
                    }
                }
            }
            if Some(event.class.class_id) == execution_sample {
                if let ValueDescriptor::Object(o) = event.value().value {
                    let start_time_ticks =
                        if let Some(&ValueDescriptor::Primitive(Primitive::Long(start_time))) =
                            o.fields.get(exs_start_time_index)
                        {
                            start_time
                        } else {
                            0
                        };
                    let sampled_thread = o
                        .fields
                        .get(exs_sampled_thread_index)
                        .and_then(|st| Accessor::new(&c, st).resolve())
                        .map(|a| a.value);
                    let stacktrace = o.fields.get(exs_stacktrace_index);
                    if let Some(sample) = process_sample(
                        &c,
                        pr_map,
                        sampled_thread,
                        stacktrace,
                        None, /* appword */
                        start_time_ticks,
                        os_thread_index,
                        long_poll_duration,
                    ) {
                        samples.push(sample);
                    }
                }
            }
        }
    }
    Ok(samples)
}
