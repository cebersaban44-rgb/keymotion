//! The host's local time in the form the keyboard clock takes.

use jiff::civil::DateTime;
use kb_protocol::sonix::ClockTime;
use kb_winsys::time::WallClock;

/// Current local time, exactly as the taskbar clock shows it (the keyboard
/// shows local time, like the official driver sends it). Windows is asked
/// every time, so a change of time zone or of the summer-time setting is
/// followed at once.
pub fn now() -> ClockTime {
    match kb_winsys::time::local_clock() {
        Some(w) => from_wall_clock(w),
        None => from_datetime(jiff::ZonedDateTime::now().datetime()),
    }
}

/// Convert Windows' wall clock.
pub fn from_wall_clock(w: WallClock) -> ClockTime {
    ClockTime {
        year: w.year,
        month: w.month,
        day: w.day,
        hour: w.hour,
        minute: w.minute,
        second: w.second.min(59),
        weekday: w.weekday,
    }
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
        let t = date(2026, 10, 29).at(15, 7, 32, 0).to_zoned_datetime(TimeZone::fixed(offset(3))).unwrap();
        let want = ClockTime { year: 2026, month: 10, day: 29, hour: 15, minute: 7, second: 32, weekday: 4 };
        assert_eq!(from_datetime(t.datetime()), want);
        let w = WallClock { year: 2026, month: 10, day: 29, hour: 15, minute: 7, second: 32, weekday: 4 };
        assert_eq!(from_wall_clock(w), want);
    }

    #[test]
    fn sunday_is_zero() {
        let t = date(2026, 11, 15).at(8, 0, 0, 0).to_zoned_datetime(TimeZone::UTC).unwrap();
        assert_eq!(from_datetime(t.datetime()).weekday, 0);
    }

    #[test]
    fn now_is_a_real_date() {
        let t = now();
        assert!(t.year >= 2024 && (1..=12).contains(&t.month) && (1..=31).contains(&t.day) && t.weekday < 7);
    }
}
