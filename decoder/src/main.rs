use std::{ffi::OsString, io::BufReader};

use clap::{Parser, Subcommand};
use jfrs::reader::{
    event::Accessor,
    value_descriptor::{Primitive, ValueDescriptor},
    JfrReader,
};
use std::io::{Read, Seek};
use std::time::Duration;

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
        /// Duration to mark from
        #[clap(value_parser = humantime::parse_duration)]
        min_length: Duration,
        #[arg(long, default_value = "5")]
        stack_depth: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Longpolls {
            jfr_file,
            min_length,
            stack_depth,
        } => {
            let mut reader = BufReader::new(std::fs::File::open(jfr_file)?);
            print_samples(jfr_samples(&mut reader, min_length)?, stack_depth);
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
            "[{:.6}] poll of {}us",
            sample.start_time.as_secs_f64(),
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

fn jfr_samples<T>(reader: &mut T, long_poll_duration: Duration) -> anyhow::Result<Vec<Sample>>
where
    T: Read + Seek,
{
    let mut jfr_reader = JfrReader::new(reader);
    let long_poll_duration = long_poll_duration.as_micros();

    let mut samples = vec![];
    for chunk in jfr_reader.chunks() {
        let (mut c_rdr, c) = chunk?;
        let mut wall_clock_sample = None;
        let mut start_time_index = !0;
        let mut appword_index = !0;
        let mut stacktrace_index = !0;
        for ty in c.metadata.type_pool.get_types() {
            if ty.name() == "profiler.WallClockSample" {
                wall_clock_sample = Some(ty.class_id);
                for (i, field) in ty.fields.iter().enumerate() {
                    match field.name() {
                        "startTime" => start_time_index = i,
                        "appword" => appword_index = i,
                        "stackTrace" => stacktrace_index = i,
                        _ => {}
                    }
                }
            }
        }
        for event in c_rdr.events(&c) {
            let event = event?;
            if Some(event.class.class_id) == wall_clock_sample {
                if let ValueDescriptor::Object(o) = event.value().value {
                    let start_time =
                        if let Some(&ValueDescriptor::Primitive(Primitive::Long(start_time))) =
                            o.fields.get(start_time_index)
                        {
                            Duration::from_nanos(
                                ((start_time as u128) * 1_000_000_000
                                    / (c.header.ticks_per_second as u128))
                                    as u64,
                            )
                        } else {
                            Duration::ZERO
                        };

                    if let Some(&ValueDescriptor::Primitive(Primitive::Long(appword))) =
                        o.fields.get(appword_index)
                    {
                        let delta_t = appword as u32;
                        let delta_t_micros =
                            (delta_t as u128) * 1000000 / (c.header.ticks_per_second as u128);
                        if delta_t_micros < long_poll_duration {
                            continue;
                        }
                        if let Some(trace) = o.fields.get(stacktrace_index) {
                            samples.push(Sample {
                                start_time,
                                delta_t: Duration::from_micros(delta_t_micros as u64),
                                frames: resolve_stack_trace(Accessor::new(&c, trace)),
                            })
                        }
                    }
                }
            }
        }
    }
    Ok(samples)
}
