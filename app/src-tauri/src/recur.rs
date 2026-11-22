//! Opening up repeat rules (RRULE, RFC 5545 §3.3.10) on `jiff`.
//!
//! Written to replace the `rrule` crate, which is built on `chrono`
//! (unmaintained since RUSTSEC-2026-0543), and to keep its results. Both
//! follow python-dateutil:
//! * The rule is read strictly: an unknown or repeated part, or a value out
//!   of range, makes the whole rule unreadable (the calendar then shows the
//!   first instance only).
//! * The first start counts only when the rule matches it, and COUNT counts
//!   from it, excluded dates included. INTERVAL=0 or COUNT=0 give nothing.
//! * UNTIL is in UTC (`…Z`). Only a start in the PC's zone (a floating time
//!   or a whole day) may also end at a wall time or a date there.
//! * A wall time a clock change skips or repeats is that day's midnight
//!   plus the time of day: on a spring-forward day 02:30 becomes 03:30, on
//!   a fall-back day the first 01:30 counts.
//! * A rule that finds nothing for 100 000 periods in a row stops.

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, Zoned};
use std::collections::{HashSet, VecDeque};
use std::str::FromStr;

/// Periods (or excluded dates) in a row without a result before a rule
/// gives up.
const MAX_EMPTY: u32 = 100_000;
/// Years a rule may reach.
const YEARS: std::ops::RangeInclusive<i32> = -10_000..=10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Freq {
    Yearly,
    Monthly,
    Weekly,
    Daily,
    Hourly,
    Minutely,
    Secondly,
}

/// UNTIL as written: in UTC, or a wall time (a date is its midnight).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Until {
    Utc(DateTime),
    Wall(DateTime),
}

/// One BYDAY entry: every such weekday (`n == 0`), or the n-th one (from
/// the end when negative) of the month or the year. Monday is 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ByDay {
    n: i16,
    day: u8,
}

/// A repeat rule as read, before it meets its start.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    freq: Freq,
    interval: u16,
    count: Option<u32>,
    until: Option<Until>,
    /// Monday is 0.
    week_start: u8,
    by_set_pos: Vec<i32>,
    by_month: Vec<u8>,
    /// Positive month days; the negative ones move to `by_n_month_day`.
    by_month_day: Vec<i8>,
    by_n_month_day: Vec<i8>,
    by_year_day: Vec<i16>,
    by_week_no: Vec<i8>,
    by_day: Vec<ByDay>,
    by_hour: Vec<u8>,
    by_minute: Vec<u8>,
    by_second: Vec<u8>,
}

fn weekday(s: &str) -> Result<u8, String> {
    Ok(match s.to_uppercase().as_str() {
        "MO" => 0,
        "TU" => 1,
        "WE" => 2,
        "TH" => 3,
        "FR" => 4,
        "SA" => 5,
        "SU" => 6,
        _ => return Err(format!("bad weekday {s:?}")),
    })
}

/// `2MO`, `-1FR`, `SU` (a number that does not read means every one).
fn by_day(s: &str) -> Result<ByDay, String> {
    let split = s.len().checked_sub(2).filter(|&i| s.is_char_boundary(i)).ok_or_else(|| format!("bad day {s:?}"))?;
    let day = weekday(&s[split..])?;
    Ok(ByDay { n: s[..split].parse().unwrap_or(0), day })
}

/// Comma-separated numbers, each `ok`; sorted, without repeats. Nothing
/// at all is an empty list.
fn numbers<T: FromStr + Ord + Copy>(s: &str, ok: impl Fn(T) -> bool, part: &str) -> Result<Vec<T>, String> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for v in s.split(',') {
        match v.parse::<T>() {
            Ok(n) if ok(n) => out.push(n),
            _ => return Err(format!("bad {part} {v:?}")),
        }
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// `20261130`, `20261130T090000` or `20261130T090000Z`.
fn until(s: &str) -> Result<Until, String> {
    let bad = || format!("bad UNTIL {s:?}");
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| -> Result<i16, String> {
        let part = b.get(r).filter(|p| p.iter().all(u8::is_ascii_digit)).ok_or_else(bad)?;
        std::str::from_utf8(part).ok().and_then(|p| p.parse().ok()).ok_or_else(bad)
    };
    let (y, mo, d) = (digits(0..4)?, digits(4..6)?, digits(6..8)?);
    let date = Date::new(y, mo as i8, d as i8).map_err(|_| bad())?;
    match b.len() {
        8 => Ok(Until::Wall(date.to_datetime(Time::midnight()))),
        15 | 16 if b[8] == b'T' && (b.len() == 15 || b[15] == b'Z') => {
            let (h, mi, sec) = (digits(9..11)?, digits(11..13)?, digits(13..15)?);
            let time = Time::new(h as i8, mi as i8, sec as i8, 0).map_err(|_| bad())?;
            let dt = date.to_datetime(time);
            Ok(if b.len() == 16 { Until::Utc(dt) } else { Until::Wall(dt) })
        }
        _ => Err(bad()),
    }
}

impl FromStr for Rule {
    type Err = String;

    /// The value of an RRULE line: `FREQ=WEEKLY;BYDAY=MO,WE;UNTIL=…`.
    fn from_str(s: &str) -> Result<Self, String> {
        let mut rule = Rule {
            freq: Freq::Yearly,
            interval: 1,
            count: None,
            until: None,
            week_start: 0,
            by_set_pos: Vec::new(),
            by_month: Vec::new(),
            by_month_day: Vec::new(),
            by_n_month_day: Vec::new(),
            by_year_day: Vec::new(),
            by_week_no: Vec::new(),
            by_day: Vec::new(),
            by_hour: Vec::new(),
            by_minute: Vec::new(),
            by_second: Vec::new(),
        };
        let mut seen = HashSet::new();
        let mut freq = None;
        for part in s.split(';').filter(|p| !p.is_empty()) {
            let (key, v) = part.split_once('=').ok_or_else(|| format!("bad part {part:?}"))?;
            let key = match key.to_uppercase().as_str() {
                "BYWEEKDAY" => "BYDAY".to_string(),
                k => k.to_string(),
            };
            if !seen.insert(key.clone()) {
                return Err(format!("{key} given twice"));
            }
            match key.as_str() {
                "FREQ" => {
                    freq = Some(match v.to_uppercase().as_str() {
                        "YEARLY" => Freq::Yearly,
                        "MONTHLY" => Freq::Monthly,
                        "WEEKLY" => Freq::Weekly,
                        "DAILY" => Freq::Daily,
                        "HOURLY" => Freq::Hourly,
                        "MINUTELY" => Freq::Minutely,
                        "SECONDLY" => Freq::Secondly,
                        _ => return Err(format!("bad FREQ {v:?}")),
                    })
                }
                "INTERVAL" => rule.interval = v.parse().map_err(|_| format!("bad INTERVAL {v:?}"))?,
                "COUNT" => rule.count = Some(v.parse().map_err(|_| format!("bad COUNT {v:?}"))?),
                "UNTIL" => rule.until = Some(until(v)?),
                "WKST" => rule.week_start = weekday(v)?,
                "BYSETPOS" => rule.by_set_pos = numbers(v, |_: i32| true, "BYSETPOS")?,
                "BYMONTH" => rule.by_month = numbers(v, |m: u8| (1..=12).contains(&m), "BYMONTH")?,
                "BYMONTHDAY" => rule.by_month_day = numbers(v, |d: i8| (-31..=31).contains(&d), "BYMONTHDAY")?,
                "BYYEARDAY" => rule.by_year_day = numbers(v, |d: i16| (-366..=366).contains(&d), "BYYEARDAY")?,
                "BYWEEKNO" => rule.by_week_no = numbers(v, |w: i8| (-53..=53).contains(&w), "BYWEEKNO")?,
                "BYDAY" => rule.by_day = v.split(',').map(by_day).collect::<Result<_, _>>()?,
                "BYHOUR" => rule.by_hour = numbers(v, |h: u8| h < 24, "BYHOUR")?,
                "BYMINUTE" => rule.by_minute = numbers(v, |m: u8| m < 60, "BYMINUTE")?,
                "BYSECOND" => rule.by_second = numbers(v, |s: u8| s < 60, "BYSECOND")?,
                _ => return Err(format!("unknown part {key}")),
            }
        }
        rule.freq = freq.ok_or("no FREQ")?;
        Ok(rule)
    }
}

/// The bounds of the n-th weekday of a span, by frequency.
fn nth_range(freq: Freq) -> std::ops::RangeInclusive<i16> {
    match freq {
        Freq::Yearly | Freq::Daily => -52..=53,
        Freq::Monthly => -4..=5,
        Freq::Weekly => -7..=8,
        Freq::Hourly => -3..=4,
        Freq::Minutely | Freq::Secondly => -8..=9,
    }
}

impl Rule {
    /// Meets the rule with its first start: fills in what the rule leaves
    /// to the start and checks the parts against each other. `floating`:
    /// the start is a floating time or a whole day (the PC's zone), so
    /// UNTIL may be a wall time there.
    pub fn validate(mut self, start: &Zoned, floating: bool) -> Result<Series, String> {
        let wall = start.datetime();
        let (pos, neg): (Vec<i8>, Vec<i8>) = self.by_month_day.iter().filter(|&&d| d != 0).partition(|&&d| d > 0);
        self.by_month_day = pos;
        self.by_n_month_day.extend(neg);
        if self.by_week_no.is_empty()
            && self.by_year_day.is_empty()
            && self.by_month_day.is_empty()
            && self.by_n_month_day.is_empty()
            && self.by_day.is_empty()
        {
            match self.freq {
                Freq::Yearly => {
                    if self.by_month.is_empty() {
                        self.by_month = vec![wall.month() as u8];
                    }
                    self.by_month_day = vec![wall.day()];
                }
                Freq::Monthly => self.by_month_day = vec![wall.day()],
                Freq::Weekly => self.by_day = vec![ByDay { n: 0, day: wall.weekday().to_monday_zero_offset() as u8 }],
                _ => {}
            }
        }
        if self.by_hour.is_empty() && self.freq < Freq::Hourly {
            self.by_hour = vec![wall.hour() as u8];
        }
        if self.by_minute.is_empty() && self.freq < Freq::Minutely {
            self.by_minute = vec![wall.minute() as u8];
        }
        if self.by_second.is_empty() && self.freq < Freq::Secondly {
            self.by_second = vec![wall.second() as u8];
        }
        self.by_month_day.sort_unstable();
        self.by_month_day.dedup();
        self.by_n_month_day.sort_unstable();
        self.by_n_month_day.dedup();
        self.by_day.sort_unstable();
        self.by_day.dedup();

        let until = match self.until {
            None => None,
            Some(Until::Utc(dt)) => Some(dt.to_zoned(TimeZone::UTC).map_err(|e| e.to_string())?.timestamp()),
            Some(Until::Wall(_)) if !floating => return Err("UNTIL must be in UTC".into()),
            Some(Until::Wall(dt)) => Some(
                start.time_zone().to_ambiguous_timestamp(dt).unambiguous().map_err(|_| "UNTIL not a single time")?,
            ),
        };
        if until.is_some_and(|u| u < start.timestamp()) {
            return Err("UNTIL before the start".into());
        }
        let set_pos_range = match self.freq {
            Freq::Yearly | Freq::Daily => -366..=366,
            Freq::Monthly => -31..=31,
            Freq::Weekly => -53..=53,
            Freq::Hourly => -24..=24,
            Freq::Minutely | Freq::Secondly => -60..=60,
        };
        if self.by_set_pos.iter().any(|p| *p == 0 || !set_pos_range.contains(p)) {
            return Err("BYSETPOS out of range".into());
        }
        // As `rrule` did, these two look at the positive month days only.
        if !self.by_set_pos.is_empty()
            && self.by_hour.is_empty()
            && self.by_minute.is_empty()
            && self.by_second.is_empty()
            && self.by_month_day.is_empty()
            && self.by_month.is_empty()
            && self.by_year_day.is_empty()
            && self.by_week_no.is_empty()
            && self.by_day.is_empty()
        {
            return Err("BYSETPOS alone".into());
        }
        if !self.by_month_day.is_empty() && self.freq == Freq::Weekly {
            return Err("BYMONTHDAY with WEEKLY".into());
        }
        if self.by_year_day.contains(&0) || self.by_week_no.contains(&0) {
            return Err("day or week 0".into());
        }
        if !self.by_year_day.is_empty() && matches!(self.freq, Freq::Monthly | Freq::Weekly | Freq::Daily) {
            return Err("BYYEARDAY with this FREQ".into());
        }
        if !self.by_week_no.is_empty() && self.freq != Freq::Yearly {
            return Err("BYWEEKNO without YEARLY".into());
        }
        let nth = nth_range(self.freq);
        if self.by_day.iter().any(|d| d.n != 0 && !nth.contains(&d.n)) {
            return Err("BYDAY number out of range".into());
        }
        Ok(Series { rule: self, start: start.clone(), until })
    }
}

/// A rule that met its start: the instances it opens up into.
#[derive(Debug, Clone)]
pub struct Series {
    rule: Rule,
    start: Zoned,
    until: Option<Timestamp>,
}

impl Series {
    /// The rule's own starts, in order.
    fn starts(&self) -> Starts<'_> {
        Starts::new(self)
    }

    /// The instances (the rule's and `extra` ones) in `after..=before`, at
    /// most `limit`, in order; none at an `excluded` time (to the second).
    /// An extra time the rule also gives comes twice.
    pub fn between(
        &self,
        extra: &[Timestamp],
        excluded: &[Timestamp],
        after: Timestamp,
        before: Timestamp,
        limit: usize,
    ) -> Vec<Timestamp> {
        let excluded: HashSet<i64> = excluded.iter().map(|t| t.as_second()).collect();
        let mut extra = extra.to_vec();
        extra.sort();
        let mut extra = Kept::new(extra.into_iter(), &excluded);
        let mut own = Kept::new(self.starts(), &excluded);
        let mut out = Vec::new();
        while out.len() < limit {
            let next = match (own.peek(), extra.peek()) {
                (None, None) => break,
                (Some(a), Some(b)) if a < b => own.take(),
                (Some(_), None) => own.take(),
                _ => extra.take(),
            };
            let Some(t) = next else { break };
            if after <= t && t <= before {
                out.push(t);
            }
            if t > before {
                break;
            }
        }
        out
    }
}

/// The times of `inner` not excluded. Gives up after too many excluded
/// ones in a row.
struct Kept<'a, I: Iterator<Item = Timestamp>> {
    inner: I,
    excluded: &'a HashSet<i64>,
    next: Option<Timestamp>,
    stopped: bool,
}

impl<'a, I: Iterator<Item = Timestamp>> Kept<'a, I> {
    fn new(inner: I, excluded: &'a HashSet<i64>) -> Self {
        Self { inner, excluded, next: None, stopped: false }
    }

    fn peek(&mut self) -> Option<Timestamp> {
        if self.next.is_none() && !self.stopped {
            let mut skipped = 0;
            for t in self.inner.by_ref() {
                if !self.excluded.contains(&t.as_second()) {
                    self.next = Some(t);
                    break;
                }
                skipped += 1;
                if skipped >= MAX_EMPTY {
                    break;
                }
            }
            self.stopped = self.next.is_none();
        }
        self.next
    }

    fn take(&mut self) -> Option<Timestamp> {
        self.peek();
        self.next.take()
    }
}

fn is_leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn year_len(y: i32) -> i32 {
    if is_leap(y) {
        366
    } else {
        365
    }
}

/// Days from 1970-01-01 to `y-m-d` (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The weekday of 1 January (Monday is 0).
fn new_year_weekday(y: i32) -> i32 {
    (days_from_civil(i64::from(y), 1, 1) + 3).rem_euclid(7) as i32
}

const MONTH_DAYS: [i32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// One year, its days counted from 0 (1 January); the week after it
/// (the next January's first days) counts on.
struct Year {
    year: i32,
    len: i32,
    next_len: i32,
    first_weekday: i32,
    /// The day each month starts on; `[12]` is the year's length.
    month_start: [i32; 13],
    /// Days in the asked week numbers (`len + 7` long; only with BYWEEKNO).
    week_no: Vec<bool>,
}

impl Year {
    fn new(year: i32, rule: &Rule) -> Self {
        let len = year_len(year);
        let mut month_start = [0; 13];
        for m in 0..12 {
            let days = if m == 1 && len == 366 { 29 } else { MONTH_DAYS[m] };
            month_start[m + 1] = month_start[m] + days;
        }
        let mut y = Year {
            year,
            len,
            next_len: year_len(year + 1),
            first_weekday: new_year_weekday(year),
            month_start,
            week_no: Vec::new(),
        };
        if !rule.by_week_no.is_empty() {
            y.week_no = y.week_numbers(rule);
        }
        y
    }

    fn weekday(&self, i: i32) -> u8 {
        (self.first_weekday + i).rem_euclid(7) as u8
    }

    fn month(&self, i: i32) -> u8 {
        if i >= self.len {
            return 1;
        }
        (1..=12u8).find(|&m| i < self.month_start[usize::from(m)]).unwrap_or(12)
    }

    fn month_day(&self, i: i32) -> i8 {
        if i >= self.len {
            return (i - self.len + 1) as i8;
        }
        (i - self.month_start[usize::from(self.month(i)) - 1] + 1) as i8
    }

    /// The day counted from the month's end (the last is -1).
    fn neg_month_day(&self, i: i32) -> i8 {
        if i >= self.len {
            return (i - self.len - 31) as i8;
        }
        (i - self.month_start[usize::from(self.month(i))]) as i8
    }

    fn date(&self, i: i32) -> Option<Date> {
        let (year, month, day) = if i >= self.len {
            (self.year + 1, 1, i - self.len + 1)
        } else {
            (self.year, self.month(i), i32::from(self.month_day(i)))
        };
        Date::new(i16::try_from(year).ok()?, month as i8, day as i8).ok()
    }

    /// RFC 5545 week numbers: week 1 is the first with at least four days
    /// of the year, weeks start on WKST.
    fn week_numbers(&self, rule: &Rule) -> Vec<bool> {
        let wkst = i32::from(rule.week_start);
        let mut mask = vec![false; (self.len + 7) as usize];
        let mark = |from: i32, mask: &mut Vec<bool>| {
            for j in from..from + 7 {
                if let Some(m) = usize::try_from(j).ok().and_then(|j| mask.get_mut(j)) {
                    *m = true;
                }
                if i32::from(self.weekday(j + 1)) == wkst {
                    break;
                }
            }
        };
        let first_week_start = (7 - self.first_weekday + wkst).rem_euclid(7);
        let (no1, week_year_len) = if first_week_start >= 4 {
            (0, self.len + (self.first_weekday - wkst).rem_euclid(7))
        } else {
            (first_week_start, self.len - first_week_start)
        };
        let weeks = week_year_len / 7 + week_year_len.rem_euclid(7) / 4;
        let start_of = |n: i32| {
            let mut i = no1 + (n - 1) * 7;
            if no1 != first_week_start {
                i -= 7 - first_week_start;
            }
            i
        };
        for &n in &rule.by_week_no {
            let mut n = i32::from(n);
            if n < 0 {
                n += weeks + 1;
            }
            if !(1..=weeks).contains(&n) {
                continue;
            }
            mark(if n > 1 { start_of(n) } else { no1 }, &mut mask);
        }
        if rule.by_week_no.contains(&1) {
            // The year's last days may be week 1 of the next.
            let i = start_of(weeks + 1);
            if i < self.len {
                mark(i, &mut mask);
            }
        }
        if no1 > 0 {
            // The year's first days may be the previous year's last week.
            let last_weeks = if rule.by_week_no.contains(&-1) {
                -1
            } else {
                let prev_weekday = new_year_weekday(self.year - 1);
                let prev_no1 = (7 - prev_weekday + wkst).rem_euclid(7);
                let prev_len = if prev_no1 >= 4 {
                    year_len(self.year - 1) + (prev_weekday - wkst).rem_euclid(7)
                } else {
                    self.len - no1
                };
                52 + prev_len.rem_euclid(7) / 4
            };
            if rule.by_week_no.iter().any(|&w| i32::from(w) == last_weeks) {
                for m in mask.iter_mut().take(no1 as usize) {
                    *m = true;
                }
            }
        }
        mask
    }
}

/// Where a rule's walk through time is: a wall date and time that may
/// briefly hold a day past the month's end.
#[derive(Debug, Clone, Copy)]
struct Counter {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

impl Counter {
    fn days_in_month(&self) -> u32 {
        let m = self.month.clamp(1, 12) as usize;
        if m == 2 && is_leap(self.year) {
            29
        } else {
            MONTH_DAYS[m - 1] as u32
        }
    }

    fn add_years(&mut self, n: u16) -> Result<(), ()> {
        self.year += i32::from(n);
        if !YEARS.contains(&self.year) {
            return Err(());
        }
        self.fix_day()
    }

    fn add_months(&mut self, n: u32) -> Result<(), ()> {
        self.month += n;
        if self.month > u32::from(u16::MAX) {
            return Err(());
        }
        if self.month > 12 {
            let mut years = self.month / 12;
            self.month %= 12;
            if self.month == 0 {
                self.month = 12;
                years -= 1;
            }
            self.year += i32::try_from(years).map_err(|_| ())?;
            if !YEARS.contains(&self.year) {
                return Err(());
            }
        }
        Ok(())
    }

    fn add_days(&mut self, n: u32) -> Result<(), ()> {
        self.day += n;
        self.fix_day()
    }

    /// A day past the month's end moves on into the next months.
    fn fix_day(&mut self) -> Result<(), ()> {
        let mut days = self.days_in_month();
        while self.day > days {
            self.day -= days;
            self.add_months(1)?;
            days = self.days_in_month();
        }
        Ok(())
    }

    /// On to the start of the week `interval` weeks on.
    fn add_weeks(&mut self, interval: u16, week_start: u8) -> Result<(), ()> {
        let weekday = (days_from_civil(i64::from(self.year), i64::from(self.month), i64::from(self.day)) + 3)
            .rem_euclid(7) as u32;
        let week_start = u32::from(week_start);
        let since_start = if week_start > weekday { weekday + 7 - week_start } else { weekday - week_start };
        self.add_days(u32::from(interval) * 7 - since_start)
    }

    /// `next_day`: the period gave no day, so the rest of it is skipped.
    fn add_hours(&mut self, interval: u32, by_hour: &[u8], next_day: bool) -> Result<(), ()> {
        if next_day {
            self.hour += (23 - self.hour.min(23)) / interval * interval;
        }
        let mut seen = HashSet::new();
        loop {
            self.hour = self.hour.checked_add(interval).ok_or(())?;
            let h = (self.hour % 24) as u8;
            if by_hour.is_empty() || by_hour.contains(&h) {
                break;
            }
            if !seen.insert(h) {
                return Err(());
            }
        }
        let days = self.hour / 24;
        self.hour %= 24;
        if days > 0 {
            self.add_days(days)?;
        }
        Ok(())
    }

    fn add_minutes(&mut self, interval: u32, by_hour: &[u8], by_minute: &[u8], next_day: bool) -> Result<(), ()> {
        if next_day {
            let done = self.hour * 60 + self.minute;
            self.minute += (1439u32.saturating_sub(done)) / interval * interval;
        }
        let mut seen = HashSet::new();
        loop {
            self.minute = self.minute.checked_add(interval).ok_or(())?;
            let hours = self.minute / 60;
            if hours > 0 {
                self.minute %= 60;
                self.add_hours(hours, by_hour, next_day)?;
            }
            let (h, m) = ((self.hour % 24) as u8, (self.minute % 60) as u8);
            if (by_hour.is_empty() || by_hour.contains(&h)) && (by_minute.is_empty() || by_minute.contains(&m)) {
                break;
            }
            if !seen.insert((h, m)) {
                return Err(());
            }
        }
        Ok(())
    }

    fn add_seconds(
        &mut self,
        interval: u32,
        by_hour: &[u8],
        by_minute: &[u8],
        by_second: &[u8],
        next_day: bool,
    ) -> Result<(), ()> {
        if next_day {
            let done = self.hour * 3600 + self.minute * 60 + self.second;
            self.second += (86_399u32.saturating_sub(done)) / interval * interval;
        }
        let mut seen = HashSet::new();
        loop {
            self.second = self.second.checked_add(interval).ok_or(())?;
            let minutes = self.second / 60;
            if minutes > 0 {
                self.second %= 60;
                self.add_minutes(minutes, by_hour, by_minute, next_day)?;
            }
            let (h, m, s) = ((self.hour % 24) as u8, (self.minute % 60) as u8, (self.second % 60) as u8);
            if (by_hour.is_empty() || by_hour.contains(&h))
                && (by_minute.is_empty() || by_minute.contains(&m))
                && (by_second.is_empty() || by_second.contains(&s))
            {
                break;
            }
            if !seen.insert((h, m, s)) {
                return Err(());
            }
        }
        Ok(())
    }

    fn step(&mut self, rule: &Rule, next_day: bool) -> Result<(), ()> {
        let i = rule.interval;
        match rule.freq {
            Freq::Yearly => self.add_years(i),
            Freq::Monthly => self.add_months(u32::from(i)),
            Freq::Weekly => self.add_weeks(i, rule.week_start),
            Freq::Daily => self.add_days(u32::from(i)),
            Freq::Hourly => self.add_hours(u32::from(i), &rule.by_hour, next_day),
            Freq::Minutely => self.add_minutes(u32::from(i), &rule.by_hour, &rule.by_minute, next_day),
            Freq::Secondly => self.add_seconds(u32::from(i), &rule.by_hour, &rule.by_minute, &rule.by_second, next_day),
        }
    }
}

/// The rule's starts, period by period (a year, a month, a week, a day, an
/// hour, … as FREQ says).
struct Starts<'a> {
    rule: &'a Rule,
    tz: TimeZone,
    start: Timestamp,
    until: Option<Timestamp>,
    at: Counter,
    year: Year,
    /// BYDAY weekdays without a number.
    every: Vec<u8>,
    /// The days of the asked n-th weekdays, for (year, month).
    nth: Option<(i32, u32, Vec<bool>)>,
    times: Vec<Time>,
    ready: VecDeque<Timestamp>,
    left: Option<u32>,
    done: bool,
}

impl<'a> Starts<'a> {
    fn new(s: &'a Series) -> Self {
        let rule = &s.rule;
        let wall = s.start.datetime();
        let at = Counter {
            year: i32::from(wall.year()),
            month: wall.month() as u32,
            day: wall.day() as u32,
            hour: wall.hour() as u32,
            minute: wall.minute() as u32,
            second: wall.second() as u32,
        };
        let mut me = Starts {
            rule,
            tz: s.start.time_zone().clone(),
            start: s.start.timestamp(),
            until: s.until,
            at,
            year: Year::new(at.year, rule),
            every: rule.by_day.iter().filter(|d| d.n == 0).map(|d| d.day).collect(),
            nth: None,
            times: Vec::new(),
            ready: VecDeque::new(),
            left: rule.count,
            done: false,
        };
        me.rebuild();
        me.times = me.first_times();
        me
    }

    fn has_nth(&self) -> bool {
        self.rule.by_day.iter().any(|d| d.n != 0)
    }

    /// Year and month facts for where the walk is now.
    fn rebuild(&mut self) {
        if self.year.year != self.at.year {
            self.year = Year::new(self.at.year, self.rule);
        }
        if self.has_nth() && !matches!(&self.nth, Some((y, m, _)) if *y == self.at.year && *m == self.at.month) {
            self.nth = Some((self.at.year, self.at.month, self.nth_days()));
        }
    }

    /// The days of the asked n-th weekdays: in the year or each asked
    /// month (YEARLY), in this month (MONTHLY). Nothing for other FREQs.
    fn nth_days(&self) -> Vec<bool> {
        let y = &self.year;
        let span = |m: u32| (y.month_start[m as usize - 1], y.month_start[m as usize] - 1);
        let spans: Vec<(i32, i32)> = match self.rule.freq {
            Freq::Yearly if self.rule.by_month.is_empty() => vec![(0, y.len - 1)],
            Freq::Yearly => self.rule.by_month.iter().map(|&m| span(u32::from(m))).collect(),
            Freq::Monthly => vec![span(self.at.month)],
            _ => return Vec::new(),
        };
        let mut mask = vec![false; y.len as usize];
        for (first, last) in spans {
            for d in self.rule.by_day.iter().filter(|d| d.n != 0) {
                let (n, day) = (i32::from(d.n), i32::from(d.day));
                let i = if n < 0 {
                    let i = last + (n + 1) * 7;
                    if i < 0 {
                        continue;
                    }
                    i - (i32::from(y.weekday(i)) - day).rem_euclid(7)
                } else {
                    let i = first + (n - 1) * 7;
                    i + (7 - i32::from(y.weekday(i)) + day).rem_euclid(7)
                };
                if first <= i && i <= last {
                    mask[i as usize] = true;
                }
            }
        }
        mask
    }

    /// The times of day of the first period.
    fn first_times(&self) -> Vec<Time> {
        let r = self.rule;
        let (h, m, s) = (self.at.hour as u8, self.at.minute as u8, self.at.second as u8);
        if r.freq >= Freq::Hourly {
            let off = |by: &[u8], v: u8| !by.is_empty() && !by.contains(&v);
            if off(&r.by_hour, h)
                || (r.freq >= Freq::Minutely && off(&r.by_minute, m))
                || (r.freq >= Freq::Secondly && off(&r.by_second, s))
            {
                return Vec::new();
            }
            return self.times_at(h, m, s);
        }
        let mut out = Vec::new();
        for &h in &r.by_hour {
            for &m in &r.by_minute {
                for &s in &r.by_second {
                    out.extend(Time::new(h as i8, m as i8, s as i8, 0).ok());
                }
            }
        }
        out
    }

    /// The times of day of an hour, minute or second (HOURLY and finer).
    fn times_at(&self, h: u8, m: u8, s: u8) -> Vec<Time> {
        let r = self.rule;
        let time = |m: u8, s: u8| Time::new(h as i8, m as i8, s as i8, 0).ok();
        match r.freq {
            Freq::Hourly => {
                r.by_minute.iter().flat_map(|&m| r.by_second.iter().filter_map(move |&s| time(m, s))).collect()
            }
            Freq::Minutely => r.by_second.iter().filter_map(|&s| time(m, s)).collect(),
            _ => time(m, s).into_iter().collect(),
        }
    }

    fn excluded(&self, i: i32) -> bool {
        let (r, y) = (self.rule, &self.year);
        let nth = self.nth.as_ref().map(|(_, _, m)| m.as_slice()).unwrap_or(&[]);
        (!r.by_month.is_empty() && !r.by_month.contains(&y.month(i)))
            || (!r.by_week_no.is_empty() && !y.week_no.get(i as usize).copied().unwrap_or(false))
            || (!self.every.is_empty() && !self.every.contains(&y.weekday(i)))
            || (!nth.is_empty() && !nth.get(i as usize).copied().unwrap_or(false))
            || ((!r.by_month_day.is_empty() || !r.by_n_month_day.is_empty())
                && !r.by_month_day.contains(&y.month_day(i))
                && !r.by_n_month_day.contains(&y.neg_month_day(i)))
            || (!r.by_year_day.is_empty() && {
                let day = |v: i32| i16::try_from(v).is_ok_and(|v| r.by_year_day.contains(&v));
                if i < y.len {
                    !day(i + 1) && !day(i - y.len)
                } else {
                    !day(i + 1 - y.len) && !day(i - y.next_len - y.len)
                }
            })
    }

    /// Where the walk is, as a day of its year.
    fn day_of_year(&self) -> i32 {
        self.year.month_start[self.at.month as usize - 1] + self.at.day as i32 - 1
    }

    /// The candidate days of the period, by day of the year.
    fn days(&self) -> Vec<i32> {
        let y = &self.year;
        let mut days: Vec<i32> = match self.rule.freq {
            Freq::Yearly => (0..y.len).collect(),
            Freq::Monthly => {
                let m = self.at.month as usize;
                (y.month_start[m - 1]..y.month_start[m]).collect()
            }
            Freq::Weekly => {
                let mut out = Vec::new();
                let mut i = self.day_of_year();
                for _ in 0..7 {
                    if i >= y.len + 7 {
                        break;
                    }
                    out.push(i);
                    i += 1;
                    if y.weekday(i) == self.rule.week_start {
                        break;
                    }
                }
                out
            }
            _ => vec![self.day_of_year()],
        };
        days.retain(|&i| !self.excluded(i));
        days
    }

    /// A wall time on day `i`; one a clock change skips or repeats is the
    /// day's midnight plus the time of day.
    fn wall(&self, i: i32, time: Time) -> Option<Timestamp> {
        let date = self.year.date(i)?;
        if let Ok(t) = self.tz.to_ambiguous_timestamp(date.to_datetime(time)).unambiguous() {
            return Some(t);
        }
        let midnight = self.tz.to_ambiguous_timestamp(date.to_datetime(Time::midnight())).unambiguous().ok()?;
        let since = SignedDuration::from_secs(
            i64::from(time.hour()) * 3600 + i64::from(time.minute()) * 60 + i64::from(time.second()),
        );
        midnight.checked_add(since).ok()
    }

    /// BYSETPOS: the chosen ones of the period's times, in order.
    fn chosen(&self, days: &[i32]) -> Vec<Timestamp> {
        let mut out: Vec<Timestamp> = Vec::new();
        let per_day = self.times.len() as i32;
        if per_day == 0 {
            return out;
        }
        for &pos in &self.rule.by_set_pos {
            let p = if pos > 0 { pos - 1 } else { pos };
            let (day, time) = (p.div_euclid(per_day), p.rem_euclid(per_day));
            let day = if day < 0 { days.len() as i32 + day } else { day };
            let Some(&d) = usize::try_from(day).ok().and_then(|d| days.get(d)) else { continue };
            if let Some(t) = self.wall(d, self.times[time as usize]) {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        out.sort();
        out
    }

    /// Keeps `t` if the rule reached it; true when the rule is over.
    fn keep(&mut self, t: Timestamp) -> bool {
        if self.until.is_some_and(|u| t > u) {
            return true;
        }
        if t >= self.start {
            self.ready.push_back(t);
            if let Some(left) = &mut self.left {
                *left -= 1;
                if *left == 0 {
                    return true;
                }
            }
        }
        false
    }

    /// Walks on until a period gives starts; true when the rule is over.
    fn walk(&mut self) -> bool {
        if self.left == Some(0) || self.rule.interval == 0 {
            return true;
        }
        let mut empty = 0;
        while self.ready.is_empty() {
            empty += 1;
            if empty >= MAX_EMPTY {
                return true;
            }
            let days = self.days();
            if self.rule.by_set_pos.is_empty() {
                for &d in &days {
                    for ti in 0..self.times.len() {
                        if let Some(t) = self.wall(d, self.times[ti]) {
                            if self.keep(t) {
                                return true;
                            }
                        }
                    }
                }
            } else {
                for t in self.chosen(&days) {
                    if self.keep(t) {
                        return true;
                    }
                }
            }
            if self.at.step(self.rule, days.is_empty()).is_err() {
                return true;
            }
            if self.rule.freq >= Freq::Hourly {
                self.times = self.times_at(self.at.hour as u8, self.at.minute as u8, self.at.second as u8);
            }
            self.rebuild();
        }
        false
    }
}

impl Iterator for Starts<'_> {
    type Item = Timestamp;

    fn next(&mut self) -> Option<Timestamp> {
        if let Some(t) = self.ready.pop_front() {
            return Some(t);
        }
        if self.done {
            return None;
        }
        self.done = self.walk() || self.ready.is_empty();
        self.ready.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn start(zone: &str, y: i16, mo: i8, d: i8, h: i8, mi: i8) -> Zoned {
        date(y, mo, d).at(h, mi, 0, 0).to_zoned(TimeZone::get(zone).unwrap()).unwrap()
    }

    fn series(rule: &str, start: &Zoned) -> Series {
        rule.parse::<Rule>().unwrap().validate(start, false).unwrap()
    }

    fn wall(t: Timestamp, zone: &TimeZone) -> String {
        t.to_zoned(zone.clone()).strftime("%Y-%m-%d %H:%M").to_string()
    }

    /// The first `n` starts, as wall times in the start's zone.
    fn walls(rule: &str, start: &Zoned, n: usize) -> Vec<String> {
        series(rule, start).starts().take(n).map(|t| wall(t, start.time_zone())).collect()
    }

    fn utc(s: &[Timestamp]) -> Vec<String> {
        s.iter().map(|t| t.to_string()).collect()
    }

    /// 09:00 on these days of `y`.
    fn nine(y: i16, days: &[(i8, i8)]) -> Vec<String> {
        days.iter().map(|(m, d)| format!("{y}-{m:02}-{d:02} 09:00")).collect()
    }

    const NY: &str = "America/New_York";
    const IST: &str = "Europe/Istanbul";

    #[test]
    fn rfc_5545_examples() {
        let s = start(NY, 1997, 9, 2, 9, 0);
        assert_eq!(walls("FREQ=DAILY;COUNT=10", &s, 99).len(), 10);
        assert_eq!(walls("FREQ=DAILY;INTERVAL=2", &s, 3), nine(1997, &[(9, 2), (9, 4), (9, 6)]));
        assert_eq!(
            walls("FREQ=WEEKLY;UNTIL=19971007T000000Z;WKST=SU;BYDAY=TU,TH", &s, 99),
            nine(1997, &[(9, 2), (9, 4), (9, 9), (9, 11), (9, 16), (9, 18), (9, 23), (9, 25), (9, 30), (10, 2)])
        );
        let fri13 = walls("FREQ=MONTHLY;BYDAY=FR;BYMONTHDAY=13", &s, 5);
        assert_eq!(
            fri13,
            ["1998-02-13 09:00", "1998-03-13 09:00", "1998-11-13 09:00", "1999-08-13 09:00", "2000-10-13 09:00"]
        );
        let twenty_min = walls("FREQ=DAILY;BYHOUR=9,10,11,12,13,14,15,16;BYMINUTE=0,20,40", &s, 4);
        assert_eq!(twenty_min, ["1997-09-02 09:00", "1997-09-02 09:20", "1997-09-02 09:40", "1997-09-02 10:00"]);

        let s = start(NY, 1997, 9, 5, 9, 0);
        let first_friday = walls("FREQ=MONTHLY;COUNT=10;BYDAY=1FR", &s, 99);
        assert_eq!(first_friday[..4], nine(1997, &[(9, 5), (10, 3), (11, 7), (12, 5)]));
        assert_eq!(first_friday[4..], nine(1998, &[(1, 2), (2, 6), (3, 6), (4, 3), (5, 1), (6, 5)]));

        let s = start(NY, 1997, 9, 22, 9, 0);
        let second_last_monday = walls("FREQ=MONTHLY;COUNT=6;BYDAY=-2MO", &s, 99);
        assert_eq!(second_last_monday[..4], nine(1997, &[(9, 22), (10, 20), (11, 17), (12, 22)]));
        assert_eq!(second_last_monday[4..], nine(1998, &[(1, 19), (2, 16)]));

        let s = start(NY, 1997, 9, 29, 9, 0);
        let last_work_day = walls("FREQ=MONTHLY;BYDAY=MO,TU,WE,TH,FR;BYSETPOS=-1", &s, 5);
        assert_eq!(last_work_day[..4], nine(1997, &[(9, 30), (10, 31), (11, 28), (12, 31)]));
        assert_eq!(last_work_day[4], "1998-01-30 09:00");

        let s = start(NY, 1997, 9, 4, 9, 0);
        let third = walls("FREQ=MONTHLY;COUNT=3;BYDAY=TU,WE,TH;BYSETPOS=3", &s, 9);
        assert_eq!(third, nine(1997, &[(9, 4), (10, 7), (11, 6)]));

        let s = start(NY, 1996, 11, 5, 9, 0);
        let election = walls("FREQ=YEARLY;INTERVAL=4;BYMONTH=11;BYDAY=TU;BYMONTHDAY=2,3,4,5,6,7,8", &s, 3);
        assert_eq!(election, ["1996-11-05 09:00", "2000-11-07 09:00", "2004-11-02 09:00"]);

        let s = start(NY, 1997, 5, 12, 9, 0);
        let week_20 = walls("FREQ=YEARLY;BYWEEKNO=20;BYDAY=MO", &s, 3);
        assert_eq!(week_20, ["1997-05-12 09:00", "1998-05-11 09:00", "1999-05-17 09:00"]);

        let s = start(NY, 1997, 1, 1, 9, 0);
        let year_days = walls("FREQ=YEARLY;INTERVAL=3;COUNT=10;BYYEARDAY=1,100,200", &s, 99);
        assert_eq!(year_days[..3], nine(1997, &[(1, 1), (4, 10), (7, 19)]));
        assert_eq!(year_days[3..6], nine(2000, &[(1, 1), (4, 9), (7, 18)]));
        assert_eq!(year_days[9], "2006-01-01 09:00");

        let s = start(NY, 1997, 8, 5, 9, 0);
        let mo = walls("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=MO", &s, 9);
        assert_eq!(mo, nine(1997, &[(8, 5), (8, 10), (8, 19), (8, 24)]));
        let su = walls("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=TU,SU;WKST=SU", &s, 9);
        assert_eq!(su, nine(1997, &[(8, 5), (8, 17), (8, 19), (8, 31)]));
    }

    #[test]
    fn month_ends_leap_days_and_a_start_the_rule_does_not_match() {
        let s = start(IST, 2026, 1, 31, 10, 0);
        // Months without a 31st are skipped; -1 is always the last day.
        assert_eq!(walls("FREQ=MONTHLY;COUNT=3", &s, 9), ["2026-01-31 10:00", "2026-03-31 10:00", "2026-05-31 10:00"]);
        let last = walls("FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=3", &s, 9);
        assert_eq!(last, ["2026-01-31 10:00", "2026-02-28 10:00", "2026-03-31 10:00"]);
        let s = start(IST, 2024, 2, 29, 10, 0);
        assert_eq!(walls("FREQ=YEARLY;COUNT=2", &s, 9), ["2024-02-29 10:00", "2028-02-29 10:00"]);
        // A Tuesday start of a Monday-and-Wednesday rule is not an instance.
        let s = start(IST, 2026, 11, 3, 10, 0);
        let got = walls("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=3", &s, 9);
        assert_eq!(got, ["2026-11-04 10:00", "2026-11-09 10:00", "2026-11-11 10:00"]);
    }

    #[test]
    fn clock_changes_keep_the_wall_time() {
        // New York springs forward on 8 March 2026: 02:30 does not exist
        // that day and becomes 03:30.
        let s = start(NY, 2026, 3, 6, 2, 30);
        let got = walls("FREQ=DAILY;COUNT=4", &s, 9);
        assert_eq!(got, ["2026-03-06 02:30", "2026-03-07 02:30", "2026-03-08 03:30", "2026-03-09 02:30"]);
        // It falls back on 1 November: 01:30 happens twice, the first counts.
        let s = start(NY, 2026, 10, 31, 1, 30);
        let t: Vec<Timestamp> = series("FREQ=DAILY;COUNT=3", &s).starts().collect();
        assert_eq!(utc(&t), ["2026-10-31T05:30:00Z", "2026-11-01T05:30:00Z", "2026-11-02T06:30:00Z"]);
        // A weekly 10:00 in Berlin stays 10:00 across the change (25 October).
        let s = start("Europe/Berlin", 2026, 10, 19, 10, 0);
        let t: Vec<Timestamp> = series("FREQ=WEEKLY;COUNT=3", &s).starts().collect();
        assert_eq!(utc(&t), ["2026-10-19T08:00:00Z", "2026-10-26T09:00:00Z", "2026-11-02T09:00:00Z"]);
        // A UTC start repeats in UTC.
        let s = start("UTC", 2026, 10, 19, 8, 0);
        let t: Vec<Timestamp> = series("FREQ=WEEKLY;COUNT=2", &s).starts().collect();
        assert_eq!(utc(&t), ["2026-10-19T08:00:00Z", "2026-10-26T08:00:00Z"]);
    }

    #[test]
    fn until_and_count() {
        let s = start(IST, 2026, 11, 2, 10, 0);
        // UNTIL is inclusive.
        assert_eq!(walls("FREQ=DAILY;UNTIL=20261104T070000Z", &s, 9).len(), 3);
        assert_eq!(walls("FREQ=DAILY;UNTIL=20261104T065959Z", &s, 9).len(), 2);
        // A zoned start must end in UTC; a wall time or a date is refused.
        let rule = |r: &str| r.parse::<Rule>().unwrap();
        assert!(rule("FREQ=DAILY;UNTIL=20261104T100000").validate(&s, false).is_err());
        assert!(rule("FREQ=DAILY;UNTIL=20261104").validate(&s, false).is_err());
        // A whole day (floating) may end on a date.
        let day = start(IST, 2026, 11, 2, 0, 0);
        let all_day = rule("FREQ=DAILY;UNTIL=20261104").validate(&day, true).unwrap();
        let got: Vec<String> = all_day.starts().map(|t| wall(t, day.time_zone())).collect();
        assert_eq!(got, ["2026-11-02 00:00", "2026-11-03 00:00", "2026-11-04 00:00"]);
        // An end before the start is refused.
        assert!(rule("FREQ=DAILY;UNTIL=20261101T000000Z").validate(&s, false).is_err());
        // COUNT=0 and INTERVAL=0 give nothing.
        assert!(walls("FREQ=DAILY;COUNT=0", &s, 9).is_empty());
        assert!(walls("FREQ=DAILY;INTERVAL=0", &s, 9).is_empty());
    }

    #[test]
    fn excluded_and_extra_dates_and_the_span() {
        let s = start(IST, 2026, 11, 2, 10, 0);
        let at = |d: i8, h: i8| start(IST, 2026, 11, d, h, 0).timestamp();
        let (from, to) = (at(1, 0), at(30, 0));
        // COUNT counts an excluded instance too.
        let three = series("FREQ=DAILY;COUNT=3", &s);
        assert_eq!(three.between(&[], &[at(3, 10)], from, to, 99), [at(2, 10), at(4, 10)]);
        // Extra dates join in order; one the rule also gives comes twice;
        // an excluded one is left out.
        let got = three.between(&[at(10, 15), at(2, 10), at(20, 9)], &[at(20, 9)], from, to, 99);
        assert_eq!(got, [at(2, 10), at(2, 10), at(3, 10), at(4, 10), at(10, 15)]);
        // The span is inclusive at both ends; the limit counts kept ones.
        let daily = series("FREQ=DAILY", &s);
        assert_eq!(daily.between(&[], &[], at(5, 10), at(7, 10), 99), [at(5, 10), at(6, 10), at(7, 10)]);
        assert_eq!(daily.between(&[], &[], at(5, 10), to, 2), [at(5, 10), at(6, 10)]);
    }

    #[test]
    fn rules_are_read_strictly() {
        let s = start(IST, 2026, 11, 2, 10, 0);
        let check = |r: &str| r.parse::<Rule>().and_then(|r| r.validate(&s, false)).map(|_| ());
        assert!(check("freq=daily;count=2").is_ok());
        assert!(check("FREQ=WEEKLY;BYWEEKDAY=MO").is_ok());
        for bad in [
            "COUNT=2",
            "FREQ=DAILY;X-NAME=1",
            "FREQ=DAILY;FREQ=WEEKLY",
            "FREQ=WEEKLY;BYDAY=MO;BYWEEKDAY=TU",
            "FREQ=FORTNIGHTLY",
            "FREQ=DAILY;COUNT=-1",
            "FREQ=DAILY;BYHOUR=24",
            "FREQ=MONTHLY;BYDAY=6MO",
            "FREQ=WEEKLY;BYMONTHDAY=1",
            "FREQ=MONTHLY;BYWEEKNO=1",
            "FREQ=DAILY;BYYEARDAY=1",
            "FREQ=SECONDLY;BYSETPOS=1",
            "FREQ=DAILY;BYHOUR=1;BYSETPOS=0",
            "FREQ=DAILY;UNTIL=2026-11-30",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }
}
