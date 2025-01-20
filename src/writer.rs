use byteorder::{LittleEndian, WriteBytesExt};
use std::{
    io::{BufWriter, Write},
    sync::mpsc::{RecvError, RecvTimeoutError},
    time::{Duration, Instant},
};

pub enum Event {
    Poll {
        start: u64,
        end: u64,
        clock_end: u64,
        tid: u32,
    },
    /// monotonic time = (tsc-time - src-epoch) * mul >> shift + ref-epoch
    CalibrateTscToMonotonic { data: CalibrationData },
}

pub struct CalibrationData {
    pub src_epoch: u64,
    pub ref_epoch: u64,
    pub mul: u64,
    pub shift: u32,
}

fn write_event(w: &mut impl Write, e: Event) -> std::io::Result<()> {
    match e {
        Event::Poll {
            start,
            end,
            clock_end,
            tid,
        } => {
            w.write_u32::<LittleEndian>(4 + 4 + 8 + 8 + 8 + 4)?; // size
            w.write_u32::<LittleEndian>(0)?; // 0 for poll
            w.write_u64::<LittleEndian>(start)?;
            w.write_u64::<LittleEndian>(end)?;
            w.write_u64::<LittleEndian>(clock_end)?;
            w.write_u32::<LittleEndian>(tid)?;
            Ok(())
        }
        Event::CalibrateTscToMonotonic {
            data:
                CalibrationData {
                    src_epoch,
                    ref_epoch,
                    mul,
                    shift,
                },
        } => {
            w.write_u32::<LittleEndian>(4 + 4 + 8 + 8 + 8 + 4)?; // size
            w.write_u32::<LittleEndian>(1)?; // 1 for calibrate
            w.write_u64::<LittleEndian>(src_epoch)?;
            w.write_u64::<LittleEndian>(ref_epoch)?;
            w.write_u64::<LittleEndian>(mul)?;
            w.write_u32::<LittleEndian>(shift)?;
            Ok(())
        }
    }
}

pub fn writer_fn(
    rx: std::sync::mpsc::Receiver<Event>,
    f: Box<dyn Write + Send>,
) -> std::io::Result<()> {
    let mut w = BufWriter::new(f);
    loop {
        match rx.recv() {
            Ok(e) => write_event(&mut w, e)?,
            Err(RecvError) => return Ok(()),
        }
        let flush_start = Instant::now();
        loop {
            match rx.recv_timeout(Duration::from_secs(1).saturating_sub(flush_start.elapsed())) {
                Ok(e) => write_event(&mut w, e)?,
                Err(e) => {
                    w.flush()?;
                    match e {
                        RecvTimeoutError::Disconnected => return Ok(()),
                        RecvTimeoutError::Timeout => break,
                    }
                }
            }
        }
    }
}

pub(crate) fn start_writer(f: Box<dyn Write + Send>) -> std::sync::mpsc::Sender<Event> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(|| {
        if let Err(e) = writer_fn(rx, f) {
            tracing::error!(message="performance writer error", error=?e);
        }
    });
    tx
}
