//! The PC's clock and time zone as Windows has them: the date and time the
//! taskbar clock shows, and the time zone setting (Settings › Time &
//! language). Asked afresh on every call, so a change of zone shows at once.

/// A wall-clock date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallClock {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Sunday is 0.
    pub weekday: u8,
}

/// The time zone setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneSetting {
    /// Windows' name for the zone ("Turkey Standard Time").
    pub key: String,
    /// "Adjust for daylight saving time automatically" is off.
    pub dst_off: bool,
    /// The offset from UTC Windows applies now, in minutes east.
    pub offset_now_min: i32,
    /// The zone's standard (winter) offset, in minutes east.
    pub standard_min: i32,
}

/// The taskbar clock's date and time. `None` off Windows.
pub fn local_clock() -> Option<WallClock> {
    sys::local_clock()
}

/// The time zone setting. `None` off Windows or when Windows cannot say.
pub fn zone_setting() -> Option<ZoneSetting> {
    sys::zone_setting()
}

#[cfg(windows)]
mod sys {
    use super::{WallClock, ZoneSetting};
    use windows::Win32::System::SystemInformation::GetLocalTime;
    use windows::Win32::System::Time::{
        GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION, TIME_ZONE_ID_INVALID,
    };

    /// `GetDynamicTimeZoneInformation`: summer time is in effect.
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    /// … standard time is.
    const TIME_ZONE_ID_STANDARD: u32 = 1;

    pub fn local_clock() -> Option<WallClock> {
        // SAFETY: takes no arguments and returns the time by value.
        let t = unsafe { GetLocalTime() };
        Some(WallClock {
            year: t.wYear,
            month: t.wMonth as u8,
            day: t.wDay as u8,
            hour: t.wHour as u8,
            minute: t.wMinute as u8,
            second: t.wSecond as u8,
            weekday: t.wDayOfWeek as u8,
        })
    }

    pub fn zone_setting() -> Option<ZoneSetting> {
        let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
        // SAFETY: `info` is a valid, writable structure of the right type.
        let rc = unsafe { GetDynamicTimeZoneInformation(&mut info) };
        if rc == TIME_ZONE_ID_INVALID {
            return None;
        }
        let end = info.TimeZoneKeyName.iter().position(|&c| c == 0).unwrap_or(info.TimeZoneKeyName.len());
        let key = String::from_utf16(&info.TimeZoneKeyName[..end]).ok()?;
        let dst_off = info.DynamicDaylightTimeDisabled;
        // Bias is minutes west of UTC; the standard or summer bias adds on.
        let extra = match rc {
            _ if dst_off => info.StandardBias,
            TIME_ZONE_ID_DAYLIGHT => info.DaylightBias,
            TIME_ZONE_ID_STANDARD => info.StandardBias,
            _ => 0,
        };
        Some(ZoneSetting {
            key,
            dst_off,
            offset_now_min: -(info.Bias + extra),
            standard_min: -(info.Bias + info.StandardBias),
        })
    }
}

#[cfg(not(windows))]
mod sys {
    use super::{WallClock, ZoneSetting};

    pub fn local_clock() -> Option<WallClock> {
        None
    }

    pub fn zone_setting() -> Option<ZoneSetting> {
        None
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_gives_a_clock_and_a_zone() {
        let c = local_clock().unwrap();
        assert!(c.year >= 2024 && (1..=12).contains(&c.month) && c.weekday < 7);
        let z = zone_setting().unwrap();
        assert!(!z.key.is_empty());
        assert!((-14 * 60..=14 * 60).contains(&z.offset_now_min));
    }
}
