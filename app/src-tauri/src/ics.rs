//! Reading published calendars (iCalendar, `.ics`). Google ("secret
//! address in iCal format"), Outlook ("publish calendar") and Apple iCloud
//! ("public calendar") all hand out a calendar as one such feed.
//!
//! * The text is parsed by `icalendar`'s low-level parser; this module
//!   reads the few properties a card needs.
//! * Repeats are opened up by [`crate::recur`] (RRULE, EXDATE, RDATE). A moved
//!   or changed instance (RECURRENCE-ID) replaces the one it overrides;
//!   cancelled events and instances are left out.
//! * Times: UTC (`…Z`), a zone by IANA name (Google, Apple), by Windows
//!   name (Outlook, "Turkey Standard Time"), by a vendor path
//!   ("/mozilla.org/…/Europe/Istanbul"), or floating (the PC's zone). A
//!   zone none of these know falls back to its VTIMEZONE's standard
//!   offset, or the PC's zone.

use crate::recur::Rule;
use icalendar::parser::{read_calendar, unfold, Component, Property};
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{AmbiguousOffset, TimeZone};
use jiff::{SignedDuration, Timestamp, ToSpan, Zoned};
use std::collections::{HashMap, HashSet};

/// One event (or one instance of a repeating event) in the asked span.
#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub uid: String,
    pub title: String,
    pub location: String,
    pub start_ms: f64,
    pub end_ms: f64,
    /// A whole day ("Proje teslimi"), not a time.
    pub all_day: bool,
}

/// Most repeats a single event may open up into within the span.
const MAX_REPEATS: usize = 1_000;

/// The address gave a web page (a sign-in page, an event's or the
/// calendar's page) instead of a calendar.
pub const WEB_PAGE: &str = "web page";
/// The address gave something else that is not a calendar.
pub const NOT_A_CALENDAR: &str = "not a calendar";

/// The events and time zones of a feed, each read on its own: one line
/// the parser does not take spoils only its own event, not the calendar
/// (and not the events after it).
fn blocks(unfolded: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<(String, Vec<&str>)> = None;
    for line in unfolded.lines() {
        let l = line.trim_end_matches('\r');
        let upper = l.trim().to_ascii_uppercase();
        if let Some((name, lines)) = &mut open {
            lines.push(l);
            if upper == format!("END:{name}") {
                let mut block = lines.join("\r\n");
                block.push_str("\r\n");
                out.push(block);
                open = None;
            }
        } else if let Some(name) = upper.strip_prefix("BEGIN:") {
            if name == "VEVENT" || name == "VTIMEZONE" {
                open = Some((name.to_string(), vec![l]));
            }
        }
    }
    out
}

/// Events of `text` that overlap `from_ms..to_ms`, sorted by start.
/// `local`: the zone of floating times and whole days (the PC's zone).
/// Errors: [`WEB_PAGE`], [`NOT_A_CALENDAR`].
pub fn occurrences(text: &str, from_ms: f64, to_ms: f64, local: &TimeZone) -> Result<Vec<Occurrence>, String> {
    let unfolded = unfold(text);
    let head = unfolded.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with('<') {
        return Err(WEB_PAGE.into());
    }
    if !head.get(..15).is_some_and(|h| h.eq_ignore_ascii_case("BEGIN:VCALENDAR")) {
        return Err(NOT_A_CALENDAR.into());
    }
    let blocks = blocks(&unfolded);
    let mut skipped = 0;
    let components: Vec<Component> = blocks
        .iter()
        .filter_map(|b| match read_calendar(b) {
            Ok(cal) => Some(cal.components),
            Err(_) => {
                skipped += 1;
                None
            }
        })
        .flatten()
        .collect();
    if skipped > 0 {
        tracing::debug!(skipped, "calendar entries not understood; left out");
    }
    let zones = Zones::new(&components, local);
    let events: Vec<&Component> =
        components.iter().filter(|c| c.name.as_str().eq_ignore_ascii_case("VEVENT")).collect();

    // Instances moved or changed by an override, by (UID, original start).
    let mut overridden: HashSet<(String, i64)> = HashSet::new();
    for e in &events {
        if let (Some(uid), Some(rid)) = (text_of(e, "UID"), prop(e, "RECURRENCE-ID")) {
            if let Some(w) = zones.parse(rid) {
                overridden.insert((uid, w.at.timestamp().as_millisecond()));
            }
        }
    }

    let mut out = Vec::new();
    for e in events {
        if text_of(e, "STATUS").is_some_and(|s| s.eq_ignore_ascii_case("CANCELLED")) {
            continue;
        }
        let Some(start) = prop(e, "DTSTART").and_then(|p| zones.parse(p)) else { continue };
        let all_day = start.all_day;
        let length = event_length(e, &zones, &start);
        let uid = text_of(e, "UID").unwrap_or_default();
        let title = text_of(e, "SUMMARY").map(|s| unescape(&s)).unwrap_or_default();
        let location = text_of(e, "LOCATION").map(|s| unescape(&s)).unwrap_or_default();
        let length_ms = length.as_millis() as f64;
        let make = |s: Timestamp| Occurrence {
            uid: uid.clone(),
            title: title.clone(),
            location: location.clone(),
            start_ms: s.as_millisecond() as f64,
            end_ms: s.as_millisecond() as f64 + length_ms,
            all_day,
        };
        let overlaps = |o: &Occurrence| o.end_ms > from_ms && o.start_ms < to_ms;

        let is_override = prop(e, "RECURRENCE-ID").is_some();
        let starts = match (prop(e, "RRULE"), is_override) {
            (Some(rule), false) => repeats(e, &zones, rule.val.as_str(), &start, length, from_ms, to_ms)
                .unwrap_or_else(|err| {
                    tracing::debug!(%uid, "repeat rule not understood ({err}); first instance only");
                    vec![start.at.timestamp()]
                }),
            _ => vec![start.at.timestamp()],
        };
        for s in starts {
            if !is_override && overridden.contains(&(uid.clone(), s.as_millisecond())) {
                continue;
            }
            let o = make(s);
            if overlaps(&o) {
                out.push(o);
            }
        }
    }
    out.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms).then_with(|| a.title.cmp(&b.title)));
    out.dedup();
    Ok(out)
}

/// Starts of a repeating event within the span (an instance still running
/// at `from_ms` counts).
fn repeats(
    e: &Component,
    zones: &Zones,
    rule: &str,
    start: &When,
    length: SignedDuration,
    from_ms: f64,
    to_ms: f64,
) -> Result<Vec<Timestamp>, String> {
    let series = rule.parse::<Rule>()?.validate(&start.at, start.floating)?;
    let times = |name: &'static str| -> Vec<Timestamp> {
        props(e, name).flat_map(|p| zones.parse_list(p)).map(|w| w.at.timestamp()).collect()
    };
    let (excluded, extra) = (times("EXDATE"), times("RDATE"));
    let at = |ms: f64| Timestamp::from_millisecond(ms as i64).ok();
    let (Some(after), Some(before)) = (at(from_ms - length.as_millis() as f64 - 1.0), at(to_ms)) else {
        return Err("span out of range".into());
    };
    Ok(series.between(&extra, &excluded, after, before, MAX_REPEATS))
}

/// DTEND, or DURATION, or the defaults (a whole day; no length).
fn event_length(e: &Component, zones: &Zones, start: &When) -> SignedDuration {
    if let Some(end) = prop(e, "DTEND").and_then(|p| zones.parse(p)) {
        if end.at.timestamp() > start.at.timestamp() {
            return start.at.timestamp().duration_until(end.at.timestamp());
        }
    }
    if let Some(d) = prop(e, "DURATION").and_then(|p| parse_duration(p.val.as_str())) {
        return d;
    }
    if start.all_day {
        SignedDuration::from_hours(24)
    } else {
        SignedDuration::ZERO
    }
}

/// `P1D`, `PT30M`, `PT1H30M`, `P1W` (the sign and seconds too).
fn parse_duration(v: &str) -> Option<SignedDuration> {
    let (neg, v) = match v.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, v.strip_prefix('+').unwrap_or(v)),
    };
    let v = v.strip_prefix('P')?;
    let mut total = SignedDuration::ZERO;
    let mut num = String::new();
    let mut time = false;
    for ch in v.chars() {
        match ch {
            'T' => time = true,
            '0'..='9' => num.push(ch),
            unit => {
                let n: i64 = num.parse().ok()?;
                num.clear();
                let seconds = match (unit, time) {
                    ('W', false) => 7 * 86_400,
                    ('D', false) => 86_400,
                    ('H', true) => 3_600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
                total = total.checked_add(SignedDuration::from_secs(n.checked_mul(seconds)?))?;
            }
        }
    }
    Some(if neg { -total } else { total })
}

/// Undo iCalendar text escapes; line breaks become spaces (one-line card).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn prop<'a, 'b>(c: &'a Component<'b>, name: &str) -> Option<&'a Property<'b>> {
    c.properties.iter().find(|p| p.name.as_str().eq_ignore_ascii_case(name))
}

fn props<'a, 'b>(c: &'a Component<'b>, name: &'a str) -> impl Iterator<Item = &'a Property<'b>> + 'a {
    c.properties.iter().filter(move |p| p.name.as_str().eq_ignore_ascii_case(name))
}

fn text_of(c: &Component, name: &str) -> Option<String> {
    prop(c, name).map(|p| p.val.as_str().trim().to_string()).filter(|s| !s.is_empty())
}

fn param<'a>(p: &'a Property, key: &str) -> Option<&'a str> {
    p.params
        .iter()
        .find(|q| q.key.as_str().eq_ignore_ascii_case(key))
        .and_then(|q| q.val.as_ref())
        .map(|v| v.as_str().trim_matches('"'))
}

/// A date-time value of the feed.
#[derive(Debug, Clone)]
struct When {
    at: Zoned,
    /// A whole day, not a time.
    all_day: bool,
    /// In the PC's zone: a floating time, a whole day or a zone nobody
    /// knows. A repeat of such a start may end at a wall time there.
    floating: bool,
}

/// The first moment a wall clock in `tz` shows `t`; none when a clock
/// change skips it.
fn earliest(tz: &TimeZone, t: DateTime) -> Option<Zoned> {
    let at = tz.to_ambiguous_zoned(t);
    match at.offset() {
        AmbiguousOffset::Gap { .. } => None,
        _ => at.earlier().ok(),
    }
}

/// `YYYYMMDD`.
fn parse_date(v: &str) -> Option<Date> {
    let b = v.as_bytes();
    if b.len() != 8 || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| v[r].parse::<i16>().ok();
    Date::new(n(0..4)?, n(4..6)? as i8, n(6..8)? as i8).ok()
}

/// `YYYYMMDDTHHMMSS`.
fn parse_date_time(v: &str) -> Option<DateTime> {
    let (date, time) = v.split_at_checked(8)?;
    let time = time.strip_prefix('T')?;
    let b = time.as_bytes();
    if b.len() != 6 || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n = |r: std::ops::Range<usize>| time[r].parse::<i8>().ok();
    Some(parse_date(date)?.to_datetime(Time::new(n(0..2)?, n(2..4)?, n(4..6)?, 0).ok()?))
}

/// Time zones of one calendar.
struct Zones {
    local: TimeZone,
    /// Zones only the calendar's own VTIMEZONE explains: its standard
    /// offset, as a fixed zone.
    fallback: HashMap<String, TimeZone>,
}

impl Zones {
    fn new(components: &[Component], local: &TimeZone) -> Self {
        let mut fallback = HashMap::new();
        for c in components.iter().filter(|c| c.name.as_str().eq_ignore_ascii_case("VTIMEZONE")) {
            let Some(id) = text_of(c, "TZID") else { continue };
            let standard = c.components.iter().find(|s| s.name.as_str().eq_ignore_ascii_case("STANDARD"));
            if let Some(tz) = standard.and_then(|s| text_of(s, "TZOFFSETTO")).and_then(|o| fixed_zone(&o)) {
                fallback.insert(id, tz);
            }
        }
        Self { local: local.clone(), fallback }
    }

    /// The zone of a TZID; true when it is the PC's zone.
    fn zone(&self, tzid: Option<&str>) -> (TimeZone, bool) {
        let Some(id) = tzid.map(str::trim).filter(|s| !s.is_empty()) else { return (self.local.clone(), true) };
        match named_zone(id).or_else(|| self.fallback.get(id).cloned()) {
            Some(tz) => (tz, false),
            None => (self.local.clone(), true),
        }
    }

    /// A date-time value with its parameters.
    fn parse(&self, p: &Property) -> Option<When> {
        self.parse_value(p.val.as_str(), param(p, "TZID"), param(p, "VALUE"))
    }

    /// Comma-separated values (EXDATE, RDATE).
    fn parse_list(&self, p: &Property) -> Vec<When> {
        p.val
            .as_str()
            .split(',')
            .filter_map(|v| self.parse_value(v.trim(), param(p, "TZID"), param(p, "VALUE")))
            .collect()
    }

    fn parse_value(&self, v: &str, tzid: Option<&str>, value: Option<&str>) -> Option<When> {
        let is_date = value.is_some_and(|t| t.eq_ignore_ascii_case("DATE")) || (v.len() == 8 && !v.contains('T'));
        if is_date {
            let at = earliest(&self.local, parse_date(v)?.to_datetime(Time::midnight()))?;
            return Some(When { at, all_day: true, floating: true });
        }
        if let Some(utc) = v.strip_suffix('Z').or_else(|| v.strip_suffix('z')) {
            let at = parse_date_time(utc)?.to_zoned(TimeZone::UTC).ok()?;
            return Some(When { at, all_day: false, floating: false });
        }
        let n = parse_date_time(v)?;
        let (tz, floating) = self.zone(tzid);
        // A time skipped by a clock change (spring forward) moves on by
        // the gap; one that happens twice takes the first.
        let at = earliest(&tz, n).or_else(|| earliest(&tz, n.checked_add(1.hour()).ok()?))?;
        Some(When { at, all_day: false, floating })
    }
}

/// `+0300` → a fixed zone (`Etc/GMT-3`; whole hours only).
fn fixed_zone(offset: &str) -> Option<TimeZone> {
    let sign = match offset.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits = &offset[1..];
    let hours: i32 = digits.get(0..2)?.parse().ok()?;
    let minutes: i32 = digits.get(2..4).unwrap_or("00").parse().ok()?;
    if minutes != 0 {
        return None;
    }
    let total = sign * hours;
    let name = if total == 0 { "Etc/UTC".to_string() } else { format!("Etc/GMT{:+}", -total) };
    TimeZone::get(&name).ok()
}

/// A zone by IANA name, Windows name or a vendor path ending in an IANA name.
fn named_zone(id: &str) -> Option<TimeZone> {
    if let Ok(tz) = TimeZone::get(id) {
        return Some(tz);
    }
    if let Some((_, iana)) = WINDOWS_ZONES.iter().find(|(w, _)| w.eq_ignore_ascii_case(id)) {
        return TimeZone::get(iana).ok();
    }
    // "/mozilla.org/20050126_1/Europe/Istanbul", "/softwarestudio.org/…".
    let parts: Vec<&str> = id.split('/').filter(|s| !s.is_empty()).collect();
    (1..=3.min(parts.len())).find_map(|n| TimeZone::get(&parts[parts.len() - n..].join("/")).ok())
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

    fn ist() -> TimeZone {
        TimeZone::get("Europe/Istanbul").unwrap()
    }

    /// Milliseconds of a wall time in Istanbul (UTC+3).
    fn at(y: i16, mo: i8, d: i8, h: i8, mi: i8) -> f64 {
        jiff::civil::date(y, mo, d).at(h, mi, 0, 0).to_zoned(ist()).unwrap().timestamp().as_millisecond() as f64
    }

    fn day(y: i16, mo: i8, d: i8) -> (f64, f64) {
        (at(y, mo, d, 0, 0), at(y, mo, d, 23, 59))
    }

    /// Like Google's feed: UTC times, a weekly repeat with a zone, an
    /// excluded date, a moved instance, a cancelled event.
    const GOOGLE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Google Inc//Google Calendar 70.9054//EN\r\nX-WR-TIMEZONE:Europe/Istanbul\r\n\
BEGIN:VEVENT\r\nDTSTART;TZID=Europe/Istanbul:20261102T100000\r\nDTEND;TZID=Europe/Istanbul:20261102T101500\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE\r\nEXDATE;TZID=Europe/Istanbul:20261111T100000\r\nUID:standup@google.com\r\nSUMMARY:Günlük toplantı\r\nLOCATION:Meet\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART;TZID=Europe/Istanbul:20261109T113000\r\nDTEND;TZID=Europe/Istanbul:20261109T114500\r\nRECURRENCE-ID;TZID=Europe/Istanbul:20261109T100000\r\nUID:standup@google.com\r\nSUMMARY:Günlük toplantı (kaydı)\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART:20261111T110000Z\r\nDTEND:20261111T120000Z\r\nUID:design@google.com\r\nSUMMARY:Tasarım incelemesi\r\nLOCATION:Toplantı odası 2\\, 3. kat\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nDTSTART:20261111T130000Z\r\nDTEND:20261111T133000Z\r\nUID:gone@google.com\r\nSTATUS:CANCELLED\r\nSUMMARY:İptal edildi\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    /// Like Outlook's published feed: Windows zone names, a folded line.
    const OUTLOOK: &str = "BEGIN:VCALENDAR\r\nMETHOD:PUBLISH\r\nPRODID:Microsoft Exchange Server 2010\r\nVERSION:2.0\r\n\
BEGIN:VTIMEZONE\r\nTZID:Turkey Standard Time\r\nBEGIN:STANDARD\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0300\r\nTZOFFSETTO:+0300\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VTIMEZONE\r\nTZID:Custom Zone\r\nBEGIN:STANDARD\r\nDTSTART:16010101T000000\r\nTZOFFSETFROM:+0200\r\nTZOFFSETTO:+0200\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n\
BEGIN:VEVENT\r\nUID:040000008200E00074C5B7101A82E008\r\nSUMMARY;LANGUAGE=tr-TR:Birebir görüşme ve çeyrek planlaması üzerine uzun bir\r\n  başlık\r\nDTSTART;TZID=Turkey Standard Time:20261111T163000\r\nDTEND;TZID=Turkey Standard Time:20261111T170000\r\nLOCATION;LANGUAGE=tr-TR:Ofis\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:custom-1\r\nSUMMARY:Özel bölge\r\nDTSTART;TZID=Custom Zone:20261111T090000\r\nDURATION:PT45M\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    /// Like Apple's public calendar: a whole day, a daily repeat with a
    /// count, a vendor zone path.
    const APPLE: &str = "BEGIN:VCALENDAR\nVERSION:2.0\nPRODID:-//Apple Inc.//macOS 15.0//EN\n\
BEGIN:VEVENT\nUID:A1\nDTSTART;VALUE=DATE:20261111\nDTEND;VALUE=DATE:20261112\nSUMMARY:Proje teslimi\nEND:VEVENT\n\
BEGIN:VEVENT\nUID:A2\nDTSTART;TZID=/mozilla.org/20050126_1/Europe/Istanbul:20261110T083000\nDURATION:PT15M\nRRULE:FREQ=DAILY;COUNT=3\nSUMMARY:Spor\nEND:VEVENT\n\
END:VCALENDAR\n";

    #[test]
    fn google_repeats_exclusions_moves_and_cancellations() {
        let (from, to) = (at(2026, 11, 9, 0, 0), at(2026, 11, 12, 0, 0));
        let o = occurrences(GOOGLE, from, to, &ist()).unwrap();
        let got: Vec<(f64, &str)> = o.iter().map(|o| (o.start_ms, o.title.as_str())).collect();
        assert_eq!(
            got,
            vec![
                // Monday's 10:00 moved to 11:30.
                (at(2026, 11, 9, 11, 30), "Günlük toplantı (kaydı)"),
                // Wednesday 11th 10:00 excluded; the UTC event is 14:00 here.
                (at(2026, 11, 11, 14, 0), "Tasarım incelemesi"),
            ]
        );
        assert_eq!(o[1].location, "Toplantı odası 2, 3. kat");
        assert_eq!(o[1].end_ms - o[1].start_ms, 3_600_000.0);
        // The week after, both days come back.
        let o = occurrences(GOOGLE, at(2026, 11, 16, 0, 0), at(2026, 11, 19, 0, 0), &ist()).unwrap();
        let starts: Vec<f64> = o.iter().map(|o| o.start_ms).collect();
        assert_eq!(starts, vec![at(2026, 11, 16, 10, 0), at(2026, 11, 18, 10, 0)]);
    }

    #[test]
    fn outlook_windows_zones_folded_lines_and_vtimezone_fallback() {
        let (from, to) = day(2026, 11, 11);
        let o = occurrences(OUTLOOK, from, to, &ist()).unwrap();
        assert_eq!(o.len(), 2);
        // "Custom Zone" is +02:00 by its VTIMEZONE: 09:00 there is 10:00 here.
        assert_eq!((o[0].title.as_str(), o[0].start_ms), ("Özel bölge", at(2026, 11, 11, 10, 0)));
        assert_eq!(o[0].end_ms - o[0].start_ms, 45.0 * 60_000.0);
        assert_eq!(o[1].title, "Birebir görüşme ve çeyrek planlaması üzerine uzun bir başlık");
        assert_eq!(o[1].start_ms, at(2026, 11, 11, 16, 30));
    }

    #[test]
    fn apple_whole_days_counts_and_vendor_zone_paths() {
        let (from, to) = (at(2026, 11, 10, 0, 0), at(2026, 11, 14, 0, 0));
        let o = occurrences(APPLE, from, to, &ist()).unwrap();
        let sport: Vec<f64> = o.iter().filter(|o| o.title == "Spor").map(|o| o.start_ms).collect();
        assert_eq!(sport, vec![at(2026, 11, 10, 8, 30), at(2026, 11, 11, 8, 30), at(2026, 11, 12, 8, 30)]);
        let whole = o.iter().find(|o| o.all_day).unwrap();
        assert_eq!((whole.title.as_str(), whole.start_ms), ("Proje teslimi", at(2026, 11, 11, 0, 0)));
        assert_eq!(whole.end_ms, at(2026, 11, 12, 0, 0));
    }

    #[test]
    fn an_event_running_at_the_start_of_the_span_counts() {
        let o = occurrences(GOOGLE, at(2026, 11, 11, 14, 30), at(2026, 11, 11, 18, 0), &ist()).unwrap();
        assert_eq!(o.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), vec!["Tasarım incelemesi"]);
    }

    #[test]
    fn web_pages_and_odd_entries() {
        // A sign-in page or an event's page, not a calendar.
        assert_eq!(occurrences("<!DOCTYPE html><html>…</html>", 0.0, 1.0, &ist()), Err(WEB_PAGE.into()));
        assert_eq!(occurrences("\u{feff}  <html>", 0.0, 1.0, &ist()), Err(WEB_PAGE.into()));
        assert_eq!(occurrences("Not found", 0.0, 1.0, &ist()), Err(NOT_A_CALENDAR.into()));
        // One entry the parser does not take is left out; the ones after it stay.
        let feed = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
BEGIN:VEVENT\r\nUID:bad\r\nTHIS LINE HAS NO COLON\r\nDTSTART:20261111T070000Z\r\nSUMMARY:Bozuk\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:good\r\nDTSTART:20261111T090000Z\r\nDTEND:20261111T093000Z\r\nSUMMARY:Sağlam\r\n\
ATTENDEE;CUTYPE=INDIVIDUAL;ROLE=REQ-PARTICIPANT;PARTSTAT=ACCEPTED;CN=\"Ayşe Yılmaz\";X-NUM-GUESTS=0:mailto:a@b.c\r\n\
DESCRIPTION:Bu etkinlik bir e-postadan otomatik olarak oluşturuldu.\\n<a href=\"https://x\">x</a>\r\n\
BEGIN:VALARM\r\nACTION:DISPLAY\r\nTRIGGER:-P0DT0H30M0S\r\nEND:VALARM\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let (from, to) = day(2026, 11, 11);
        let o = occurrences(feed, from, to, &ist()).unwrap();
        let good: Vec<&Occurrence> = o.iter().filter(|o| o.title == "Sağlam").collect();
        assert_eq!(good.len(), 1);
        assert_eq!(good[0].start_ms, at(2026, 11, 11, 12, 0));
        let broken = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nUID:x\nDTSTART:nonsense\nSUMMARY:x\nEND:VEVENT\nEND:VCALENDAR\n";
        assert_eq!(occurrences(broken, 0.0, f64::MAX / 4.0, &ist()).unwrap(), vec![]);
    }

    #[test]
    fn durations_zones_and_escapes() {
        assert_eq!(parse_duration("PT1H30M"), Some(SignedDuration::from_mins(90)));
        assert_eq!(parse_duration("P1W"), Some(SignedDuration::from_hours(7 * 24)));
        assert_eq!(parse_duration("-PT5M"), Some(SignedDuration::from_mins(-5)));
        assert_eq!(parse_duration("1H"), None);
        assert_eq!(
            named_zone("Turkey Standard Time").and_then(|z| z.iana_name().map(str::to_string)),
            Some("Europe/Istanbul".into())
        );
        assert_eq!(
            named_zone("/softwarestudio.org/Olson_20011030_5/America/New_York")
                .and_then(|z| z.iana_name().map(str::to_string)),
            Some("America/New_York".into())
        );
        assert!(named_zone("Nowhere Time").is_none());
        assert_eq!(fixed_zone("+0300").and_then(|z| z.iana_name().map(str::to_string)), Some("Etc/GMT-3".into()));
        assert_eq!(unescape("A\\, B\\; C\\nD\\\\"), "A, B; C D\\");
    }

    /// Repeats across zones and clock changes, with ends, skipped and
    /// extra dates.
    const REPEATS: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n\
BEGIN:VEVENT\r\nUID:r1\r\nDTSTART;VALUE=DATE:20261102\r\nDTEND;VALUE=DATE:20261103\r\nRRULE:FREQ=DAILY;UNTIL=20261106\r\nEXDATE;VALUE=DATE:20261104\r\nSUMMARY:Nöbet\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:r2\r\nDTSTART;TZID=W. Europe Standard Time:20261019T100000\r\nDTEND;TZID=W. Europe Standard Time:20261019T103000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO;UNTIL=20261102T090000Z\r\nSUMMARY:Haftalık\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:r3\r\nDTSTART;TZID=Europe/Istanbul:20261102T090000\r\nRRULE:FREQ=DAILY;UNTIL=20261105T090000\r\nSUMMARY:Saatsiz son\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:r4\r\nDTSTART:20261102T070000Z\r\nDURATION:PT1H\r\nRRULE:FREQ=WEEKLY;COUNT=2\r\nRDATE:20261104T120000Z\r\nEXDATE:20261109T070000Z\r\nSUMMARY:Ek gün\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";

    #[test]
    fn repeats_with_ends_skipped_and_extra_dates() {
        let o = occurrences(REPEATS, at(2026, 10, 1, 0, 0), at(2026, 12, 1, 0, 0), &ist()).unwrap();
        let of = |uid: &str| -> Vec<f64> { o.iter().filter(|o| o.uid == uid).map(|o| o.start_ms).collect() };
        // Whole days end on a date (the PC's zone) and skip the 4th.
        let days = [2, 3, 5, 6].map(|d| at(2026, 11, d, 0, 0));
        assert_eq!(of("r1"), days);
        assert!(o.iter().filter(|o| o.uid == "r1").all(|o| o.all_day && o.end_ms - o.start_ms == 86_400_000.0));
        // 10:00 in Berlin is 11:00 here in summer time, 12:00 after 25
        // October; the UTC end takes in the last one.
        assert_eq!(of("r2"), [at(2026, 10, 19, 11, 0), at(2026, 10, 26, 12, 0), at(2026, 11, 2, 12, 0)]);
        // A zoned start with a wall-time end breaks the rule: the first only.
        assert_eq!(of("r3"), [at(2026, 11, 2, 9, 0)]);
        // An extra date joins, an excluded one goes.
        assert_eq!(of("r4"), [at(2026, 11, 2, 10, 0), at(2026, 11, 4, 15, 0)]);
    }

    #[test]
    fn floating_times_follow_the_pc_zone_through_clock_changes() {
        // New York springs forward on 8 March 2026: a floating 02:30 that
        // day moves on to 03:30; a daily 02:30 keeps its wall time.
        let ny = TimeZone::get("America/New_York").unwrap();
        let feed = "BEGIN:VCALENDAR\r\n\
BEGIN:VEVENT\r\nUID:f1\r\nDTSTART:20260306T023000\r\nRRULE:FREQ=DAILY;COUNT=4\r\nSUMMARY:x\r\nEND:VEVENT\r\n\
BEGIN:VEVENT\r\nUID:f2\r\nDTSTART:20260308T023000\r\nSUMMARY:y\r\nEND:VEVENT\r\n\
END:VCALENDAR\r\n";
        let wall = |ms: f64| {
            let t = Timestamp::from_millisecond(ms as i64).unwrap().to_zoned(ny.clone());
            t.strftime("%m-%d %H:%M").to_string()
        };
        let o = occurrences(feed, 0.0, 4e12, &ny).unwrap();
        let got: Vec<(String, &str)> = o.iter().map(|o| (wall(o.start_ms), o.uid.as_str())).collect();
        let want = [
            ("03-06 02:30", "f1"),
            ("03-07 02:30", "f1"),
            ("03-08 03:30", "f1"),
            ("03-08 03:30", "f2"),
            ("03-09 02:30", "f1"),
        ];
        assert_eq!(got, want.map(|(w, u)| (w.to_string(), u)));
    }
}
