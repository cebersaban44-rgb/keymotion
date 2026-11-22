//! The host's local time in the form the keyboard clock takes.

use jiff::civil::DateTime;
use jiff::Zoned;
use kb_protocol::sonix::ClockTime;

/// Current local time (the keyboard shows local time, like the official
/// driver sends it).
pub fn now() -> ClockTime {
    from_datetime(Zoned::now().datetime())
}

/// Convert a wall-clock date and time; the keyboard has no zone.
pub fn from_datetime(t: DateTime) -> ClockTime {
    ClockTime {
        year: u16::try_from(t.year()).unwrap_or(2000),
        month: t.month() as u8,
        day: t.day() as u8,
        hour: t.hour() as u8,
        minute: t.minute() as u8,
        second: t.second().min(59) as u8,
        weekday: t.weekday().to_sunday_zero_offset() as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jiff::tz::{offset, TimeZone};

    #[test]
    fn converts_wall_clock_fields() {
        // 15:07:32 at UTC+3: the wall clock, whatever the zone.
        let t = date(2026, 10, 29).at(15, 7, 32, 0).to_zoned(TimeZone::fixed(offset(3))).unwrap();
        assert_eq!(
            from_datetime(t.datetime()),
            ClockTime { year: 2026, month: 10, day: 29, hour: 15, minute: 7, second: 32, weekday: 4 }
        );
    }

    #[test]
    fn sunday_is_zero() {
        let t = date(2026, 11, 15).at(8, 0, 0, 0).to_zoned(TimeZone::UTC).unwrap();
        assert_eq!(from_datetime(t.datetime()).weekday, 0);
    }
}
