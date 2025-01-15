use std::io::{self, Read, Seek};

use byteorder::{LittleEndian, ReadBytesExt};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReadEventError {
    #[error("read error")]
    Read(#[from] io::Error),
    #[error("size field too small")]
    SizeTooSmall,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum PossiblyUnknownEvent {
    Event(Event),
    UnknownEvent {
        #[allow(unused)]
        kind: u32,
    },
}

#[derive(Debug)]
pub struct CalibrationData {
    pub src_epoch: u64,
    pub ref_epoch: u64,
    pub mul: u64,
    pub shift: u32,
}

#[inline]
fn mul_div_po2_u64(value: u64, numer: u64, denom: u32) -> u64 {
    // Modified muldiv routine where the denominator has to be a power of two. `denom` is expected
    // to be the number of bits to shift, not the actual decimal value.
    let mut v = u128::from(value);
    v *= u128::from(numer);
    v >>= denom;
    v as u64
}

impl CalibrationData {
    pub fn scale_src_to_ref(&self, src_raw: u64) -> u64 {
        let delta = src_raw.saturating_sub(self.src_epoch);
        let scaled = mul_div_po2_u64(delta, self.mul, self.shift);
        scaled + self.ref_epoch
    }

    pub fn scale_src_duration_to_ref(&self, delta: u64) -> u64 {
        mul_div_po2_u64(delta, self.mul, self.shift)
    }
}

pub fn read_event<R: Read + Seek>(
    r: &mut R,
) -> Result<Option<PossiblyUnknownEvent>, ReadEventError> {
    let size = match r.read_u32::<LittleEndian>() {
        Ok(size) => size,
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };
    let mut poll_size = 4 + 4;
    if size < poll_size {
        return Err(ReadEventError::SizeTooSmall);
    }
    let kind = r.read_u32::<LittleEndian>()?;

    let res = match kind {
        0 => {
            poll_size = 4 + 4 + 8 + 8 + 8 + 4;
            if size < poll_size {
                return Err(ReadEventError::SizeTooSmall);
            }
            let start = r.read_u64::<LittleEndian>()?;
            let end = r.read_u64::<LittleEndian>()?;
            let clock_end = r.read_u64::<LittleEndian>()?;
            let tid = r.read_u32::<LittleEndian>()?;

            PossiblyUnknownEvent::Event(Event::Poll {
                start,
                end,
                clock_end,
                tid,
            })
        }
        1 => {
            poll_size = 4 + 4 + 8 + 8 + 8 + 4;
            if size < poll_size {
                return Err(ReadEventError::SizeTooSmall);
            }
            let src_epoch = r.read_u64::<LittleEndian>()?;
            let ref_epoch = r.read_u64::<LittleEndian>()?;
            let mul = r.read_u64::<LittleEndian>()?;
            let shift = r.read_u32::<LittleEndian>()?;

            PossiblyUnknownEvent::Event(Event::CalibrateTscToMonotonic {
                data: CalibrationData {
                    src_epoch,
                    ref_epoch,
                    mul,
                    shift,
                },
            })
        }
        _ => PossiblyUnknownEvent::UnknownEvent { kind },
    };

    r.seek_relative((size - poll_size).into())?;
    return Ok(Some(res));
}

#[test]
fn test_read_event() -> Result<(), ReadEventError> {
    let mut buf = io::Cursor::new(vec![
        // unknown event of type 0x12345678
        16, 0, 0, 0, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0, 0, 0, 0, 0, // poll event
        36, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0,
        0, 0, 4, 0, 0, 0, // poll event with extra data
        40, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0,
        0, 0, 4, 0, 0, 0, 1, 2, 3, 4, // calibration event
        36, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0,
        0, 0, 4, 0, 0, 0, // calibration event with extra data
        40, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0,
        0, 0, 4, 0, 0, 0, 1, 2, 3, 4, // another unknown event of type 0x12345679
        16, 0, 0, 0, 0x79, 0x56, 0x34, 0x12, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    match read_event(&mut buf)? {
        Some(PossiblyUnknownEvent::UnknownEvent { kind: 0x12345678 }) => {}
        e => panic!("bad event {:?}", e),
    };
    match read_event(&mut buf)? {
        Some(PossiblyUnknownEvent::Event(Event::Poll {
            start: 1,
            end: 2,
            clock_end: 3,
            tid: 4,
        })) => {}
        e => panic!("bad event {:?}", e),
    };
    match read_event(&mut buf)? {
        Some(PossiblyUnknownEvent::Event(Event::Poll {
            start: 1,
            end: 2,
            clock_end: 3,
            tid: 4,
        })) => {}
        e => panic!("bad event {:?}", e),
    };
    match read_event(&mut buf)? {
        Some(PossiblyUnknownEvent::Event(Event::CalibrateTscToMonotonic {
            data:
                CalibrationData {
                    src_epoch: 1,
                    ref_epoch: 2,
                    mul: 3,
                    shift: 4,
                },
        })) => {}
        e => panic!("bad event {:?}", e),
    };
    match read_event(&mut buf)? {
        Some(PossiblyUnknownEvent::Event(Event::CalibrateTscToMonotonic {
            data:
                CalibrationData {
                    src_epoch: 1,
                    ref_epoch: 2,
                    mul: 3,
                    shift: 4,
                },
        })) => {}
        e => panic!("bad event {:?}", e),
    };
    match read_event(&mut buf)? {
        Some(PossiblyUnknownEvent::UnknownEvent { kind: 0x12345679 }) => {}
        e => panic!("bad event {:?}", e),
    };
    match read_event(&mut buf)? {
        None => {}
        e => panic!("bad event {:?}", e),
    };
    Ok(())
}
