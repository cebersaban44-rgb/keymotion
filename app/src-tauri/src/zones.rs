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

/// Windows zone names (Outlook) → IANA: CLDR's
/// `common/supplemental/windowsZones.xml`, every `territory="001"` entry
/// in its order (typeVersion 2021a; the same table jiff 0.2.41 carries).
/// The IANA names are CLDR's, old ones included (`Asia/Calcutta`); the
/// bundled time zone data knows them all (tested).
const WINDOWS_ZONES: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("UTC-11", "Etc/GMT+11"),
    ("Aleutian Standard Time", "America/Adak"),
    ("Hawaiian Standard Time", "Pacific/Honolulu"),
    ("Marquesas Standard Time", "Pacific/Marquesas"),
    ("Alaskan Standard Time", "America/Anchorage"),
    ("UTC-09", "Etc/GMT+9"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("UTC-08", "Etc/GMT+8"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("US Mountain Standard Time", "America/Phoenix"),
    ("Mountain Standard Time (Mexico)", "America/Mazatlan"),
    ("Mountain Standard Time", "America/Denver"),
    ("British Columbia Standard Time", "America/Vancouver"),
    ("Yukon Standard Time", "America/Whitehorse"),
    ("Alberta Standard Time", "America/Edmonton"),
    ("Central America Standard Time", "America/Guatemala"),
    ("Central Standard Time", "America/Chicago"),
    ("Easter Island Standard Time", "Pacific/Easter"),
    ("Central Standard Time (Mexico)", "America/Mexico_City"),
    ("Canada Central Standard Time", "America/Regina"),
    ("Manitoba Standard Time", "America/Winnipeg"),
    ("SA Pacific Standard Time", "America/Bogota"),
    ("Eastern Standard Time (Mexico)", "America/Cancun"),
    ("Eastern Standard Time", "America/New_York"),
    ("Haiti Standard Time", "America/Port-au-Prince"),
    ("Cuba Standard Time", "America/Havana"),
    ("US Eastern Standard Time", "America/Indianapolis"),
    ("Turks And Caicos Standard Time", "America/Grand_Turk"),
    ("Paraguay Standard Time", "America/Asuncion"),
    ("Atlantic Standard Time", "America/Halifax"),
    ("Venezuela Standard Time", "America/Caracas"),
    ("Central Brazilian Standard Time", "America/Cuiaba"),
    ("SA Western Standard Time", "America/La_Paz"),
    ("Pacific SA Standard Time", "America/Santiago"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("Tocantins Standard Time", "America/Araguaina"),
    ("E. South America Standard Time", "America/Sao_Paulo"),
    ("SA Eastern Standard Time", "America/Cayenne"),
    ("Argentina Standard Time", "America/Buenos_Aires"),
    ("Greenland Standard Time", "America/Godthab"),
    ("Montevideo Standard Time", "America/Montevideo"),
    ("Magallanes Standard Time", "America/Punta_Arenas"),
    ("Saint Pierre Standard Time", "America/Miquelon"),
    ("Bahia Standard Time", "America/Bahia"),
    ("UTC-02", "Etc/GMT+2"),
    ("Azores Standard Time", "Atlantic/Azores"),
    ("Cape Verde Standard Time", "Atlantic/Cape_Verde"),
    ("UTC", "Etc/UTC"),
    ("GMT Standard Time", "Europe/London"),
    ("Greenwich Standard Time", "Atlantic/Reykjavik"),
    ("Sao Tome Standard Time", "Africa/Sao_Tome"),
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
    ("West Bank Standard Time", "Asia/Hebron"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("FLE Standard Time", "Europe/Kiev"),
    ("Israel Standard Time", "Asia/Jerusalem"),
    ("South Sudan Standard Time", "Africa/Juba"),
    ("Kaliningrad Standard Time", "Europe/Kaliningrad"),
    ("Sudan Standard Time", "Africa/Khartoum"),
    ("Libya Standard Time", "Africa/Tripoli"),
    ("Namibia Standard Time", "Africa/Windhoek"),
    ("Arabic Standard Time", "Asia/Baghdad"),
    ("Turkey Standard Time", "Europe/Istanbul"),
    ("Arab Standard Time", "Asia/Riyadh"),
    ("Belarus Standard Time", "Europe/Minsk"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("E. Africa Standard Time", "Africa/Nairobi"),
    ("Iran Standard Time", "Asia/Tehran"),
    ("Arabian Standard Time", "Asia/Dubai"),
    ("Astrakhan Standard Time", "Europe/Astrakhan"),
    ("Azerbaijan Standard Time", "Asia/Baku"),
    ("Russia Time Zone 3", "Europe/Samara"),
    ("Mauritius Standard Time", "Indian/Mauritius"),
    ("Saratov Standard Time", "Europe/Saratov"),
    ("Georgian Standard Time", "Asia/Tbilisi"),
    ("Volgograd Standard Time", "Europe/Volgograd"),
    ("Caucasus Standard Time", "Asia/Yerevan"),
    ("Afghanistan Standard Time", "Asia/Kabul"),
    ("West Asia Standard Time", "Asia/Tashkent"),
    ("Ekaterinburg Standard Time", "Asia/Yekaterinburg"),
    ("Pakistan Standard Time", "Asia/Karachi"),
    ("Qyzylorda Standard Time", "Asia/Qyzylorda"),
    ("India Standard Time", "Asia/Calcutta"),
    ("Sri Lanka Standard Time", "Asia/Colombo"),
    ("Nepal Standard Time", "Asia/Katmandu"),
    ("Central Asia Standard Time", "Asia/Bishkek"),
    ("Bangladesh Standard Time", "Asia/Dhaka"),
    ("Omsk Standard Time", "Asia/Omsk"),
    ("Myanmar Standard Time", "Asia/Rangoon"),
    ("SE Asia Standard Time", "Asia/Bangkok"),
    ("Altai Standard Time", "Asia/Barnaul"),
    ("W. Mongolia Standard Time", "Asia/Hovd"),
    ("North Asia Standard Time", "Asia/Krasnoyarsk"),
    ("N. Central Asia Standard Time", "Asia/Novosibirsk"),
    ("Tomsk Standard Time", "Asia/Tomsk"),
    ("China Standard Time", "Asia/Shanghai"),
    ("North Asia East Standard Time", "Asia/Irkutsk"),
    ("Singapore Standard Time", "Asia/Singapore"),
    ("W. Australia Standard Time", "Australia/Perth"),
    ("Taipei Standard Time", "Asia/Taipei"),
    ("Ulaanbaatar Standard Time", "Asia/Ulaanbaatar"),
    ("Aus Central W. Standard Time", "Australia/Eucla"),
    ("Transbaikal Standard Time", "Asia/Chita"),
    ("Tokyo Standard Time", "Asia/Tokyo"),
    ("North Korea Standard Time", "Asia/Pyongyang"),
    ("Korea Standard Time", "Asia/Seoul"),
    ("Yakutsk Standard Time", "Asia/Yakutsk"),
    ("Cen. Australia Standard Time", "Australia/Adelaide"),
    ("AUS Central Standard Time", "Australia/Darwin"),
    ("E. Australia Standard Time", "Australia/Brisbane"),
    ("AUS Eastern Standard Time", "Australia/Sydney"),
    ("West Pacific Standard Time", "Pacific/Port_Moresby"),
    ("Tasmania Standard Time", "Australia/Hobart"),
    ("Vladivostok Standard Time", "Asia/Vladivostok"),
    ("Lord Howe Standard Time", "Australia/Lord_Howe"),
    ("Bougainville Standard Time", "Pacific/Bougainville"),
    ("Russia Time Zone 10", "Asia/Srednekolymsk"),
    ("Magadan Standard Time", "Asia/Magadan"),
    ("Norfolk Standard Time", "Pacific/Norfolk"),
    ("Sakhalin Standard Time", "Asia/Sakhalin"),
    ("Central Pacific Standard Time", "Pacific/Guadalcanal"),
    ("Russia Time Zone 11", "Asia/Kamchatka"),
    ("New Zealand Standard Time", "Pacific/Auckland"),
    ("UTC+12", "Etc/GMT-12"),
    ("Fiji Standard Time", "Pacific/Fiji"),
    ("Chatham Islands Standard Time", "Pacific/Chatham"),
    ("UTC+13", "Etc/GMT-13"),
    ("Tonga Standard Time", "Pacific/Tongatapu"),
    ("Samoa Standard Time", "Pacific/Apia"),
    ("Line Islands Standard Time", "Pacific/Kiritimati"),
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

    #[test]
    fn every_windows_name_opens_a_zone_with_windows_offsets() {
        for (w, iana) in WINDOWS_ZONES {
            assert!(windows_zone(w).is_some(), "{w} → {iana}");
        }
        // Zones whose old IANA match moved: La Paz and Mazatlán stay at
        // -07:00 (Chihuahua went to -06:00 in 2022), Bishkek at +06:00
        // (Almaty went to +05:00 in 2024). An Outlook meeting at 10:00 there
        // is at 17:00 and 04:00 UTC.
        let january = jiff::civil::date(2027, 1, 15).at(12, 0, 0, 0).to_zoned(TimeZone::UTC).unwrap().timestamp();
        let hours = |w: &str| windows_zone(w).unwrap().to_offset(january).seconds() / 3600;
        assert_eq!(hours("Mountain Standard Time (Mexico)"), -7);
        assert_eq!(hours("Central Asia Standard Time"), 6);
        // Zones with summer time that used to fall back to a fixed offset.
        let july = jiff::civil::date(2026, 7, 1).at(12, 0, 0, 0).to_zoned(TimeZone::UTC).unwrap().timestamp();
        let summer = |w: &str| windows_zone(w).unwrap().to_offset(july).seconds() / 60;
        assert_eq!(summer("US Eastern Standard Time"), -4 * 60);
        assert_eq!(summer("Cuba Standard Time"), -4 * 60);
        assert_eq!(summer("Aleutian Standard Time"), -9 * 60);
    }
}
