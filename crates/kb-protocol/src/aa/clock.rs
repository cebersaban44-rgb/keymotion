//! The clock (§4.10): `34`, written with LEN 24 like AJAZZ's WebHID driver
//! does on every connection: `5A 01 5A YY MM DD hh mm ss DOW`, zero-padded.
//! Sets the clock; whether the screen switches to its clock page depends on
//! the firmware (on 769, reportedly not).

use super::CodecError;
use crate::sonix::ClockTime;

/// Bytes sent (LEN).
pub const LEN: usize = 24;
/// The fixed first bytes.
pub const MARKER: [u8; 3] = [0x5A, 0x01, 0x5A];

/// The clock block.
pub fn encode(t: &ClockTime) -> Result<[u8; LEN], CodecError> {
    let check = |ok: bool, what| if ok { Ok(()) } else { Err(CodecError::Invalid(what)) };
    check((2000..=2099).contains(&t.year), "year")?;
    check((1..=12).contains(&t.month), "month")?;
    check((1..=31).contains(&t.day), "day")?;
    check(t.hour < 24, "hour")?;
    check(t.minute < 60, "minute")?;
    check(t.second <= 60, "second")?;
    check(t.weekday < 7, "weekday")?;
    let mut d = [0u8; LEN];
    d[..3].copy_from_slice(&MARKER);
    d[3..10].copy_from_slice(&[(t.year % 100) as u8, t.month, t.day, t.hour, t.minute, t.second.min(59), t.weekday]);
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aa::frame::{chunks, request};

    #[test]
    fn matches_the_ak820_capture() {
        // S11 (AJAZZ's WebHID driver on an AK820 Pro):
        // `AA 34 18 00 00 00 01 00 5A 01 5A 1A 0A 13 0B 18 22 01`,
        // 2026-10-19 11:24:34, a Monday.
        let t = ClockTime { year: 2026, month: 10, day: 19, hour: 11, minute: 24, second: 34, weekday: 1 };
        let d = encode(&t).unwrap();
        let r = request(crate::aa::cmd::CLOCK, &chunks(LEN, 0, true)[0], 0, &d);
        assert_eq!(
            &r[..18],
            &[
                0xAA, 0x34, 0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x5A, 0x01, 0x5A, 0x1A, 0x0A, 0x13, 0x0B, 0x18, 0x22,
                0x01
            ]
        );
        assert!(r[18..].iter().all(|&b| b == 0));
    }

    #[test]
    fn impossible_values_are_refused() {
        let ok = ClockTime { year: 2026, month: 11, day: 23, hour: 23, minute: 59, second: 59, weekday: 0 };
        assert!(encode(&ok).is_ok());
        for bad in [
            ClockTime { year: 1999, ..ok },
            ClockTime { year: 2100, ..ok },
            ClockTime { month: 0, ..ok },
            ClockTime { day: 32, ..ok },
            ClockTime { hour: 24, ..ok },
            ClockTime { minute: 60, ..ok },
            ClockTime { weekday: 7, ..ok },
        ] {
            assert!(encode(&bad).is_err(), "{bad:?}");
        }
        assert_eq!(encode(&ClockTime { second: 60, ..ok }).unwrap()[8], 59, "a leap second shows as 59");
    }
}
