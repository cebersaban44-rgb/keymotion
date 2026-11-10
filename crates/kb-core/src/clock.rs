//! The host's local time in the form the keyboard clock takes.

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use kb_protocol::sonix::ClockTime;

/// Current local time (the keyboard shows local time, like the official
/// driver sends it).
pub fn now() -> ClockTime {
    from_datetime(&Local::now())
}

/// Convert any time zone's date and time; the keyboard has no zone.
pub fn from_datetime<Tz: TimeZone>(t: &DateTime<Tz>) -> ClockTime {
    ClockTime {
        year: u16::try_from(t.year()).unwrap_or(2000),
        month: t.month() as u8,
        day: t.day() as u8,
        hour: t.hour() as u8,
        minute: t.minute() as u8,
        second: t.second().min(59) as u8,
        weekday: t.weekday().num_days_from_sunday() as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, NaiveDate};

    #[test]
    fn converts_wall_clock_fields() {
        let tz = FixedOffset::east_opt(3 * 3600).unwrap();
        let t = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap().and_hms_opt(15, 7, 32).unwrap();
        let t = tz.from_local_datetime(&t).unwrap();
        assert_eq!(
            from_datetime(&t),
            ClockTime { year: 2026, month: 10, day: 29, hour: 15, minute: 7, second: 32, weekday: 4 }
        );
    }

    #[test]
    fn sunday_is_zero() {
        let t = chrono::Utc.with_ymd_and_hms(2026, 11, 15, 8, 0, 0).unwrap();
        assert_eq!(from_datetime(&t).weekday, 0);
    }
}
