//! Time zones: Windows' names for them, and the PC's own zone.
//!
//! jiff's `TimeZone::system()` keeps the zone it found for five minutes,
//! honours a `TZ` variable, keeps summer time even when Windows is told not
//! to, and turns into UTC for a Windows zone it cannot name. The calendar,
//! reminders and the menu next to the clock follow Windows instead, asked
//! afresh each time ([`pc_zone`]).

use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};
use kb_winsys::time::ZoneSetting;
use std::sync::atomic::{AtomicBool, Ordering};

/// The zone Windows is set to:
/// * "Adjust for daylight saving time automatically" off: the standard
///   offset, all year;
/// * otherwise its IANA zone, by Windows' name, or as jiff reads it, when
///   that agrees with the offset Windows applies now;
/// * otherwise the offset Windows applies now, all year (noted once in the
///   log).
pub(crate) fn pc_zone() -> TimeZone {
    match kb_winsys::time::zone_setting() {
        Some(w) => choose(&w, TimeZone::try_system().ok(), Timestamp::now()),
        None => TimeZone::try_system().unwrap_or_else(|e| {
            warn_once(&format!("Windows gave no time zone and jiff found none ({e}); using UTC"));
            TimeZone::UTC
        }),
    }
}

/// Now, in the PC's zone.
pub(crate) fn now() -> Zoned {
    Timestamp::now().to_zoned(pc_zone())
}

fn fixed(minutes: i32) -> TimeZone {
    Offset::from_seconds(minutes * 60).map_or(TimeZone::UTC, TimeZone::fixed)
}

fn warn_once(message: &str) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!("{message}");
    }
}

fn choose(w: &ZoneSetting, system: Option<TimeZone>, now: Timestamp) -> TimeZone {
    if w.dst_off {
        return fixed(w.standard_min);
    }
    let agrees = |tz: &TimeZone| tz.to_offset(now).seconds() == w.offset_now_min * 60;
    if let Some(tz) = windows_zone(&w.key).filter(agrees).or_else(|| system.filter(agrees)) {
        return tz;
    }
    warn_once(&format!("time zone {:?} not known here; using its present offset all year", w.key));
    fixed(w.offset_now_min)
}

/// A zone by its Windows name ("Turkey Standard Time"), any case.
pub(crate) fn windows_zone(name: &str) -> Option<TimeZone> {
    let (_, iana) = WINDOWS_ZONES.iter().find(|(w, _)| w.eq_ignore_ascii_case(name))?;
    TimeZone::get(iana).ok()
}

/// Windows zone names (Outlook) → IANA (CLDR windowsZones, main territory).
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("UTC-11", "Etc/GMT+11"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("UTC-09", "Etc/GMT+9"),
    ("UTC-08", "Etc/GMT+8"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Mountain Standard Time (Mexico)", "America/Chihuahua"),
    ("Mountain Standard Time", "America/Denver"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Central Standard Time", "America/Chicago"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Eastern Standard Time", "America/New_York"),
    ("Venezuela Standard Time", "America/Caracas"),
    ("Paraguay Standard Time", "America/Asuncion"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("Argentina Standard Time", "America/Buenos_Aires"),
    ("SA Eastern Standard Time", "America/Cayenne"),
    ("Greenland Standard Time", "America/Godthab"),
    ("Montevideo Standard Time", "America/Montevideo"),
    ("Bahia Standard Time", "America/Bahia"),
    ("UTC-02", "Etc/GMT+2"),
    ("Mid-Atlantic Standard Time", "Etc/GMT+2"),
    ("Azores Standard Time", "Atlantic/Azores"),
    ("Cape Verde Standard Time", "Atlantic/Cape_Verde"),
    ("UTC", "Etc/UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("W. Europe Standard Time", "Europe/Berlin"),
    ("Central Europe Standard Time", "Europe/Budapest"),
    ("Romance Standard Time", "Europe/Paris"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("W. Central Africa Standard Time", "Africa/Lagos"),
    ("Jordan Standard Time", "Asia/Amman"),
    ("GTB Standard Time", "Europe/Bucharest"),
    ("Middle East Standard Time", "Asia/Beirut"),
    ("Egypt Standard Time", "Africa/Cairo"),
    ("E. Europe Standard Time", "Europe/Chisinau"),
    ("Syria Standard Time", "Asia/Damascus"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("Kaliningrad Standard Time", "Europe/Kaliningrad"),
    ("Sudan Standard Time", "Africa/Khartoum"),
    ("Libya Standard Time", "Africa/Tripoli"),
    ("Namibia Standard Time", "Africa/Windhoek"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Arabic Standard Time", "Asia/Baghdad"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Belarus Standard Time", "Europe/Minsk"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Volgograd Standard Time", "Europe/Volgograd"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Astrakhan Standard Time", "Europe/Astrakhan"),
    ("Azerbaijan Standard Time", "Asia/Baku"),
    ("Russia Time Zone 3", "Europe/Samara"),
    ("Mauritius Standard Time", "Indian/Mauritius"),
    ("Saratov Standard Time", "Europe/Saratov"),
    ("Georgian Standard Time", "Asia/Tbilisi"),
    ("Caucasus Standard Time", "Asia/Yerevan"),
    ("Afghanistan Standard Time", "Asia/Kabul"),
    ("West Asia Standard Time", "Asia/Tashkent"),
    ("Ekaterinburg Standard Time", "Asia/Yekaterinburg"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("India Standard Time", "Asia/Kolkata"),
    ("Sri Lanka Standard Time", "Asia/Colombo"),
    ("Nepal Standard Time", "Asia/Kathmandu"),
    ("Central Asia Standard Time", "Asia/Almaty"),
    ("Bangladesh Standard Time", "Asia/Dhaka"),
    ("Omsk Standard Time", "Asia/Omsk"),
    ("Myanmar Standard Time", "Asia/Yangon"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("N. Central Asia Standard Time", "Asia/Novosibirsk"),
    ("North Asia Standard Time", "Asia/Krasnoyarsk"),
    ("China Standard Time", "Asia/Shanghai"),
    ("North Asia East Standard Time", "Asia/Irkutsk"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("Ulaanbaatar Standard Time", "Asia/Ulaanbaatar"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Yakutsk Standard Time", "Asia/Yakutsk"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("AUS Central Standard Time", "Australia/Darwin"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("Tasmania Standard Time", "Australia/Hobart"),
    ("Vladivostok Standard Time", "Asia/Vladivostok"),
    ("Magadan Standard Time", "Asia/Magadan"),
    ("Central Pacific Standard Time", "Pacific/Guadalcanal"),
    ("UTC+12", "Etc/GMT-12"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("Fiji Standard Time", "Pacific/Fiji"),
    ("UTC+13", "Etc/GMT-13"),
    ("Tonga Standard Time", "Pacific/Tongatapu"),
    ("Samoa Standard Time", "Pacific/Apia"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(key: &str, dst_off: bool, offset_now_min: i32, standard_min: i32) -> ZoneSetting {
        ZoneSetting { key: key.into(), dst_off, offset_now_min, standard_min }
    }

    #[test]
    fn the_pc_zone_follows_windows() {
        // 1 July 2026: summer time in Warsaw (+02:00), standard +01:00.
        let july = jiff::civil::date(2026, 7, 1).at(12, 0, 0, 0).to_zoned(TimeZone::UTC).unwrap().timestamp();
        let name = |tz: TimeZone| tz.iana_name().map(str::to_string);
        let offset = |tz: TimeZone| tz.to_offset(july).seconds() / 60;
        let warsaw = setting("Central European Standard Time", false, 120, 60);
        assert_eq!(name(choose(&warsaw, None, july)).as_deref(), Some("Europe/Warsaw"));
        // Summer time switched off in Windows: the standard offset all year.
        let off = setting("Central European Standard Time", true, 60, 60);
        assert_eq!(offset(choose(&off, None, july)), 60);
        // A zone this table lacks: jiff's reading when it agrees with Windows.
        let odd = setting("Some New Standard Time", false, 120, 60);
        let berlin = TimeZone::get("Europe/Berlin").unwrap();
        assert_eq!(name(choose(&odd, Some(berlin), july)).as_deref(), Some("Europe/Berlin"));
        // … and Windows' offset when nothing agrees (a TZ variable, stale data).
        let tokyo = TimeZone::get("Asia/Tokyo").unwrap();
        assert_eq!(offset(choose(&odd, Some(tokyo.clone()), july)), 120);
        assert_eq!(offset(choose(&warsaw, Some(tokyo), july)), 120);
        // A Windows name in any case.
        assert_eq!(name(windows_zone("turkey standard time").unwrap()).as_deref(), Some("Europe/Istanbul"));
    }
}
