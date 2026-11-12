//! Takvim (phase C, third source): a card on the screen before a meeting,
//! and a key that blinks in the minute before it starts.
//!
//! * Calendars are published feeds (`ics.rs`): Google, Outlook and Apple
//!   iCloud all give one an address. The address is a secret: it is never
//!   logged and never goes to the UI (only its host does). In
//!   `calendar.json` it is sealed for the Windows user (DPAPI, `dpapi:` +
//!   base64); a plain address from an older version is sealed on start.
//!   An address that cannot be opened here (another user or computer) stays
//!   as it is and the calendar reports `address unavailable`. Systems
//!   without DPAPI keep the plain address.
//! * A reader thread reads each calendar every 15 minutes (or when asked)
//!   with the Windows HTTP client, and opens up two days of events.
//! * The ticker decides what the screen shows (owner's choices,
//!   2026-11-11): a meeting's card comes `lead_min` before it and goes
//!   5 minutes after it starts (or at the start, or at the end). Whole-day
//!   events get no card. The calendar holds the screen while its card is
//!   due, over the focus timer and "Çalan şarkı"; when it lets go, the
//!   timer's card comes back, or the song's, or after a moment the user's
//!   own picture. The card shows absolute times, so it never goes stale
//!   and one upload is enough.
//! * The reminder is a light layer (`LightLayer::Reminders`, RAM only): a
//!   key blinks amber in the last minute before a meeting, over everything
//!   but the Lighting page preview. It needs no live mode.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{
    CalendarCard, CalendarEvent, CalendarProvider, CalendarSettings, CalendarSourceInfo, CalendarState, CalendarUntil,
    Connection,
};
use crate::ics::{self, Occurrence};
use crate::live::{now_ms, LiveHandle};
use crate::settings;
use kb_protocol::sonix::{level_factor, Rgb};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Live mode source id of the calendar cards.
pub const SOURCE: &str = "calendar";
pub const EVENT_CALENDAR: &str = "calendar";
pub const EVENT_CALENDAR_CARD: &str = "calendar-card";
const FILE: &str = "calendar.json";
/// Version 1 throughout: a sealed address is marked by its prefix, so an
/// older version reading the file keeps it (it only cannot read the feed).
const VERSION: u32 = 1;
/// A sealed address on disk: this prefix, then base64 of the DPAPI blob.
const SEAL_PREFIX: &str = "dpapi:";
/// Binds a sealed address to this purpose.
const SEAL_ENTROPY: &[u8] = b"kb calendar address";
/// A calendar's error when its address cannot be opened on this computer.
pub const ADDRESS_UNAVAILABLE: &str = "address unavailable";

const MINUTE_MS: f64 = 60_000.0;
const HOUR_MS: f64 = 60.0 * MINUTE_MS;
/// Each calendar is read this often.
const READ_EVERY_MS: f64 = 15.0 * MINUTE_MS;
/// After a failed read, try again sooner.
const RETRY_MS: f64 = 2.0 * MINUTE_MS;
/// Events are opened up from a day back to two days ahead.
const SPAN_BACK_MS: f64 = 24.0 * HOUR_MS;
const SPAN_AHEAD_MS: f64 = 48.0 * HOUR_MS;
/// Two cards closer than this follow each other without the own picture
/// in between (it would cost two uploads for a moment).
const BRIDGE_MS: f64 = 3.0 * MINUTE_MS;
/// `CalendarUntil::Shortly`: the card stays this long after the start.
const SHORTLY_MS: f64 = 5.0 * MINUTE_MS;
/// After the card's time, the own picture comes back after this long
/// (unless the timer's or the song's card comes).
const BACK_AFTER: Duration = Duration::from_secs(5);
/// The reminder key blinks in the last minute before a meeting.
const REMIND_MS: f64 = MINUTE_MS;
const BLINK_MS: f64 = 500.0;
const REMIND_COLOR: Rgb = [255, 159, 26];
/// "Dene": the reminder blinks this long.
const TRY_MS: f64 = 4_000.0;
/// "Klavyede 1 dk göster": the tried card stays this long, then the own
/// picture comes back (owner's request, 2026-11-11).
const TRY_CARD_BACK: Duration = Duration::from_secs(60);
const TICK: Duration = Duration::from_secs(1);
const TICK_BLINK: Duration = Duration::from_millis(100);
const MAX_SOURCES: usize = 8;
const MAX_URL: usize = 2_048;
const MAX_NAME: usize = 40;
/// Larger feeds are refused (years of history in one file).
const MAX_FEED_BYTES: usize = 20 * 1024 * 1024;
/// Colours of new calendars, in turn.
const COLORS: [[u8; 3]; 6] =
    [[66, 133, 244], [52, 168, 83], [234, 67, 53], [251, 188, 4], [142, 36, 170], [0, 172, 193]];

// ----------------------------------------------------------------- stored

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Stored {
    settings: CalendarSettings,
    sources: Vec<StoredSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSource {
    id: String,
    name: String,
    /// The secret address (`https://…`).
    url: String,
    color: [u8; 3],
    enabled: bool,
}

/// A calendar in a backup file (`backup.rs`): settings always, the
/// calendars only when the user chose to include their addresses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarBackup {
    pub settings: CalendarSettings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<CalendarSourceBackup>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSourceBackup {
    pub name: String,
    /// The secret address, in the open: the file is the user's to guard.
    pub url: String,
    pub color: [u8; 3],
    pub enabled: bool,
}

// ----------------------------------------------------------------- sealing

fn is_sealed(url: &str) -> bool {
    url.starts_with(SEAL_PREFIX)
}

/// The address as it goes to disk: sealed where the system can, else as
/// it is. An address that could not be opened stays sealed as it was.
fn seal(url: &str) -> String {
    use base64::Engine;
    if is_sealed(url) {
        return url.to_string();
    }
    match kb_winsys::secret::protect(url.as_bytes(), SEAL_ENTROPY) {
        Ok(blob) => format!("{SEAL_PREFIX}{}", base64::engine::general_purpose::STANDARD.encode(blob)),
        Err(kb_winsys::secret::SecretError::Unsupported) => url.to_string(),
        Err(e) => {
            tracing::warn!("calendar address kept unsealed: {e}");
            url.to_string()
        }
    }
}

/// The address from disk, and whether it was stored in the open. One that
/// cannot be opened here comes back sealed (see [`is_sealed`]).
fn unseal(stored: &str) -> (String, bool) {
    use base64::Engine;
    let Some(text) = stored.strip_prefix(SEAL_PREFIX) else { return (stored.to_string(), true) };
    let opened = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| e.to_string())
        .and_then(|blob| kb_winsys::secret::unprotect(&blob, SEAL_ENTROPY).map_err(|e| e.to_string()))
        .and_then(|plain| String::from_utf8(plain).map_err(|e| e.to_string()));
    match opened {
        Ok(url) => (url, false),
        Err(e) => {
            tracing::warn!("a calendar address cannot be opened on this computer: {e}");
            (stored.to_string(), false)
        }
    }
}

/// The calendars as they go to disk (addresses sealed).
fn for_disk(stored: &Stored) -> Stored {
    let mut out = stored.clone();
    for s in &mut out.sources {
        s.url = seal(&s.url);
    }
    out
}

/// The calendars from disk with their addresses opened, and whether any
/// address was stored in the open (to be sealed now).
fn from_disk(mut stored: Stored) -> (Stored, bool) {
    let mut plain = false;
    for s in &mut stored.sources {
        let (url, was_plain) = unseal(&s.url);
        plain |= was_plain;
        s.url = url;
    }
    (stored, plain)
}

/// `webcal://` → `https://`; only secure addresses (a plain `http://`
/// address would send the secret in the open).
fn normalize_url(input: &str) -> Result<String, &'static str> {
    let s = input.trim();
    if s.is_empty() || s.len() > MAX_URL || s.chars().any(char::is_whitespace) {
        return Err("bad address");
    }
    let lower = s.to_ascii_lowercase();
    let rest = if let Some(r) = lower.strip_prefix("webcals://").or_else(|| lower.strip_prefix("webcal://")) {
        &s[s.len() - r.len()..]
    } else if lower.starts_with("https://") {
        &s[8..]
    } else {
        return Err("not https");
    };
    if host_of_rest(rest).is_empty() {
        return Err("no host");
    }
    Ok(format!("https://{rest}"))
}

fn host_of_rest(rest: &str) -> &str {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..end];
    // user:pass@host
    host.rsplit('@').next().unwrap_or(host)
}

fn host_of(url: &str) -> String {
    host_of_rest(url.split_once("://").map_or(url, |(_, r)| r)).to_ascii_lowercase()
}

fn provider_of(host: &str) -> CalendarProvider {
    if host.ends_with("google.com") {
        CalendarProvider::Google
    } else if host.contains("outlook.") || host.ends_with("office365.com") || host.ends_with("live.com") {
        CalendarProvider::Outlook
    } else if host.ends_with("icloud.com") {
        CalendarProvider::Apple
    } else {
        CalendarProvider::Other
    }
}

fn clean_name(name: &str, fallback: &str) -> String {
    let n: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_NAME).collect();
    if n.is_empty() {
        fallback.chars().take(MAX_NAME).collect()
    } else {
        n
    }
}

// ---------------------------------------------------------------- windows

/// Events of the calendars that are on, merged and sorted.
fn merged(sources: &[StoredSource], reads: &HashMap<String, Read>) -> Vec<CalendarEvent> {
    let mut out: Vec<CalendarEvent> = sources
        .iter()
        .filter(|s| s.enabled)
        .flat_map(|s| {
            reads.get(&s.id).into_iter().flat_map(|r| r.events.iter()).map(|o| CalendarEvent {
                title: o.title.clone(),
                location: o.location.clone(),
                start_ms: o.start_ms,
                end_ms: o.end_ms,
                all_day: o.all_day,
                color: s.color,
                source: s.id.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms).then_with(|| a.title.cmp(&b.title)));
    out
}

/// When each meeting's card is due: `(from, to, index)`. Whole-day events
/// get none. Cards closer than `BRIDGE_MS` follow each other directly.
fn card_windows(events: &[CalendarEvent], s: &CalendarSettings) -> Vec<(f64, f64, usize)> {
    let lead = f64::from(s.lead_min) * MINUTE_MS;
    let mut out: Vec<(f64, f64, usize)> = Vec::new();
    for (i, e) in events.iter().enumerate().filter(|(_, e)| !e.all_day) {
        let to = match s.until {
            CalendarUntil::Start => e.start_ms,
            CalendarUntil::Shortly => e.start_ms + SHORTLY_MS,
            CalendarUntil::End => e.end_ms.max(e.start_ms + SHORTLY_MS),
        };
        let mut from = e.start_ms - lead;
        if let Some(prev_to) = out.iter().map(|w| w.1).reduce(f64::max) {
            if from > prev_to && from - prev_to < BRIDGE_MS {
                from = prev_to;
            }
        }
        out.push((from, to, i));
    }
    out
}

/// The meeting whose card is due at `now`: of those, the one that starts
/// last (the coming one over the one that already started).
fn showing(events: &[CalendarEvent], s: &CalendarSettings, now: f64) -> Option<usize> {
    card_windows(events, s)
        .into_iter()
        .filter(|&(from, to, _)| from <= now && now < to)
        .max_by(|a, b| events[a.2].start_ms.total_cmp(&events[b.2].start_ms))
        .map(|w| w.2)
}

/// When the next card would come (after `now`).
fn next_card_ms(events: &[CalendarEvent], s: &CalendarSettings, now: f64) -> Option<f64> {
    card_windows(events, s).into_iter().map(|w| w.0).filter(|&from| from > now).reduce(f64::min)
}

/// The meeting starting within the next minute (the reminder blinks).
fn reminder_due(events: &[CalendarEvent], now: f64) -> Option<&CalendarEvent> {
    events.iter().find(|e| !e.all_day && e.start_ms - REMIND_MS <= now && now < e.start_ms)
}

/// The card of `events[i]`, with the meeting after it (within 12 hours).
fn card_of(events: &[CalendarEvent], i: usize) -> CalendarCard {
    let e = &events[i];
    let next = events
        .iter()
        .find(|n| !n.all_day && n.start_ms > e.start_ms && n.start_ms < e.start_ms + 12.0 * HOUR_MS)
        .cloned();
    CalendarCard { event: e.clone(), next }
}

fn key_of(e: &CalendarEvent) -> String {
    format!("{}@{}", e.source, e.start_ms)
}

// ----------------------------------------------------------------- worker

#[derive(Debug, Default)]
struct Read {
    last_ok_ms: Option<f64>,
    error: Option<String>,
    /// When to read again (0: now).
    next_ms: f64,
    /// The last feed that read well, opened up again on each read.
    text: Option<String>,
    events: Vec<Occurrence>,
}

struct Inner {
    stored: Stored,
    reads: HashMap<String, Read>,
    reading: bool,
    /// The meeting whose card was asked for.
    shown_key: Option<String>,
    card: Option<CalendarCard>,
    startup_checked: bool,
    reminder_sent: Option<Vec<(u8, Rgb)>>,
    try_until: f64,
    model: Option<(String, Arc<HashMap<String, u8>>)>,
    last_state: Option<CalendarState>,
}

enum Msg {
    Wake,
}

/// Handle to the calendar workers (managed Tauri state).
pub struct CalendarHandle {
    tick_tx: Sender<Msg>,
    read_tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    live: Arc<LiveHandle>,
    config_dir: Option<PathBuf>,
}

impl CalendarHandle {
    pub fn spawn(
        app: AppHandle,
        live: Arc<LiveHandle>,
        device: Arc<DeviceHandle>,
        config_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let (mut stored, plain) = from_disk(
            config_dir.as_deref().and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).unwrap_or_default(),
        );
        stored.settings = stored.settings.sanitized();
        if !stored.sources.is_empty() {
            tracing::info!(calendars = stored.sources.len(), "calendars");
        }
        // Addresses saved in the open by an older version: seal them now.
        if let (true, true, Some(dir)) = (plain, kb_winsys::secret::AVAILABLE, config_dir.as_deref()) {
            match settings::save(dir, FILE, VERSION, for_disk(&stored)) {
                Ok(()) => tracing::info!("calendar addresses sealed"),
                Err(e) => tracing::warn!("calendar addresses not sealed: {e}"),
            }
        }
        let inner = Arc::new(Mutex::new(Inner {
            stored,
            reads: HashMap::new(),
            reading: false,
            shown_key: None,
            card: None,
            startup_checked: false,
            reminder_sent: None,
            try_until: 0.0,
            model: None,
            last_state: None,
        }));
        let (tick_tx, tick_rx) = mpsc::channel();
        let (read_tx, read_rx) = mpsc::channel();
        let handle = Arc::new(Self { tick_tx, read_tx, inner, app, live, config_dir });

        let ticker = handle.clone();
        std::thread::Builder::new()
            .name("calendar".into())
            .spawn(move || loop {
                let wait = ticker.tick(&device);
                match tick_rx.recv_timeout(wait) {
                    Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            })
            .expect("spawn calendar thread");
        let reader = handle.clone();
        std::thread::Builder::new()
            .name("calendar-read".into())
            .spawn(move || loop {
                let wait = reader.read_due();
                match read_rx.recv_timeout(wait) {
                    Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            })
            .expect("spawn calendar reader thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tick_tx.send(Msg::Wake);
    }

    fn wake_reader(&self) {
        let _ = self.read_tx.send(Msg::Wake);
    }

    pub fn state(&self) -> Result<CalendarState, String> {
        let inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
        Ok(Self::state_of(&inner, now_ms()))
    }

    fn state_of(inner: &Inner, now: f64) -> CalendarState {
        let s = &inner.stored.settings;
        let events = merged(&inner.stored.sources, &inner.reads);
        let want = s.screen;
        CalendarState {
            settings: s.clone(),
            sources: inner
                .stored
                .sources
                .iter()
                .map(|src| {
                    let read = inner.reads.get(&src.id);
                    let host = if is_sealed(&src.url) { String::new() } else { host_of(&src.url) };
                    CalendarSourceInfo {
                        id: src.id.clone(),
                        name: src.name.clone(),
                        provider: provider_of(&host),
                        host,
                        color: src.color,
                        enabled: src.enabled,
                        last_ok_ms: read.and_then(|r| r.last_ok_ms),
                        error: read.and_then(|r| r.error.clone()),
                        events: read.map_or(0, |r| r.events.iter().filter(|e| e.end_ms > now).count() as u32),
                    }
                })
                .collect(),
            upcoming: events
                .iter()
                .filter(|e| e.end_ms > now && e.start_ms < now + 24.0 * HOUR_MS)
                .take(12)
                .cloned()
                .collect(),
            showing: if want { showing(&events, s, now).map(|i| events[i].clone()) } else { None },
            next_card_ms: if want { next_card_ms(&events, s, now) } else { None },
            reading: inner.reading,
        }
    }

    fn emit(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            let now = now_ms();
            let state = Self::state_of(&inner, now);
            inner.last_state = Some(state.clone());
            let _ = self.app.emit(EVENT_CALENDAR, state);
        }
    }

    fn save(&self, stored: &Stored) -> Result<(), String> {
        match &self.config_dir {
            Some(dir) => settings::save(dir, FILE, VERSION, for_disk(stored)).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    /// The calendars for a backup file; addresses only when asked (they
    /// leave the sealed store in the open). Unopenable addresses are left out.
    pub fn export(&self, with_addresses: bool) -> Result<CalendarBackup, String> {
        let inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
        let sources = with_addresses.then(|| {
            inner
                .stored
                .sources
                .iter()
                .filter(|s| !is_sealed(&s.url))
                .map(|s| CalendarSourceBackup {
                    name: s.name.clone(),
                    url: s.url.clone(),
                    color: s.color,
                    enabled: s.enabled,
                })
                .collect()
        });
        Ok(CalendarBackup { settings: inner.stored.settings.clone(), sources })
    }

    /// Settings from a backup, and its calendars (when it has their
    /// addresses) in place of these. Returns how many calendars were left
    /// out (bad address, duplicate, over the limit). New calendars are read
    /// at once.
    pub fn import(&self, backup: CalendarBackup) -> Result<u32, String> {
        let mut skipped = 0;
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
            inner.stored.settings = backup.settings.sanitized();
            if let Some(list) = backup.sources {
                let base = now_ms() as u64;
                let mut sources: Vec<StoredSource> = Vec::new();
                for (i, s) in list.into_iter().enumerate() {
                    let Ok(url) = normalize_url(&s.url) else {
                        skipped += 1;
                        continue;
                    };
                    if sources.len() >= MAX_SOURCES || sources.iter().any(|x| x.url == url) {
                        skipped += 1;
                        continue;
                    }
                    let host = host_of(&url);
                    sources.push(StoredSource {
                        id: format!("c{base:x}{i}"),
                        name: clean_name(&s.name, &host),
                        url,
                        color: s.color,
                        enabled: s.enabled,
                    });
                }
                tracing::info!(calendars = sources.len(), skipped, "calendars restored from a backup");
                inner.reads.clear();
                inner.stored.sources = sources;
            }
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake_reader();
        self.wake();
        self.emit();
        Ok(skipped)
    }

    pub fn configure(&self, settings: CalendarSettings) -> Result<CalendarState, String> {
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
            inner.stored.settings = settings.sanitized();
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake();
        self.emit();
        self.state()
    }

    /// Add a calendar by its published address; it is read at once.
    pub fn add(&self, url: &str, name: &str) -> Result<CalendarState, String> {
        let url = normalize_url(url).map_err(String::from)?;
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
            if inner.stored.sources.len() >= MAX_SOURCES {
                return Err("too many calendars".into());
            }
            if inner.stored.sources.iter().any(|s| s.url == url) {
                return Err("already added".into());
            }
            let n = inner.stored.sources.len();
            let id = format!("c{:x}{n}", now_ms() as u64);
            let host = host_of(&url);
            inner.stored.sources.push(StoredSource {
                id: id.clone(),
                name: clean_name(name, &host),
                url,
                color: COLORS[n % COLORS.len()],
                enabled: true,
            });
            inner.reads.insert(id.clone(), Read::default());
            tracing::info!(%id, %host, "calendar added");
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake_reader();
        self.emit();
        self.state()
    }

    pub fn update(
        &self,
        id: &str,
        name: Option<String>,
        color: Option<[u8; 3]>,
        enabled: Option<bool>,
    ) -> Result<CalendarState, String> {
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
            let src = inner.stored.sources.iter_mut().find(|s| s.id == id).ok_or("no such calendar")?;
            if let Some(n) = name {
                let fallback = src.name.clone();
                src.name = clean_name(&n, &fallback);
            }
            if let Some(c) = color {
                src.color = c;
            }
            if let Some(e) = enabled {
                src.enabled = e;
            }
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake();
        self.emit();
        self.state()
    }

    pub fn remove(&self, id: &str) -> Result<CalendarState, String> {
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "calendar unavailable")?;
            inner.stored.sources.retain(|s| s.id != id);
            inner.reads.remove(id);
            inner.stored.clone()
        };
        tracing::info!(%id, "calendar removed");
        self.save(&stored)?;
        self.wake();
        self.emit();
        self.state()
    }

    /// Read every calendar now.
    pub fn refresh(&self) -> Result<CalendarState, String> {
        if let Ok(mut inner) = self.inner.lock() {
            for r in inner.reads.values_mut() {
                r.next_ms = 0.0;
            }
        }
        self.wake_reader();
        self.state()
    }

    /// The UI handed in a card to try on the keyboard: it goes now (counted
    /// like a manual upload, never waiting for the budget) and the own
    /// picture comes back after a minute.
    pub fn try_card(&self) {
        tracing::info!("calendar card tried on the keyboard");
        let _ = self.live.send_now();
        self.live.return_after(SOURCE, TRY_CARD_BACK);
    }

    /// Blink the reminder key for a few seconds.
    pub fn try_reminder(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.try_until = now_ms() + TRY_MS;
        }
        self.wake();
    }

    // ---------------------------------------------------------- reading

    /// Read the calendars that are due; returns how long to sleep.
    fn read_due(&self) -> Duration {
        let now = now_ms();
        let due = {
            let Ok(mut inner) = self.inner.lock() else { return Duration::from_secs(60) };
            let ids: Vec<String> = inner.stored.sources.iter().map(|s| s.id.clone()).collect();
            inner.reads.retain(|id, _| ids.contains(id));
            for id in &ids {
                inner.reads.entry(id.clone()).or_default();
            }
            let due = inner
                .stored
                .sources
                .iter()
                .find(|s| s.enabled && inner.reads.get(&s.id).is_none_or(|r| r.next_ms <= now))
                .map(|s| (s.id.clone(), s.url.clone()));
            // An address that cannot be opened here is not sent anywhere.
            if let Some((id, _)) = due.as_ref().filter(|(_, url)| is_sealed(url)) {
                if let Some(read) = inner.reads.get_mut(id) {
                    read.error = Some(ADDRESS_UNAVAILABLE.into());
                    read.next_ms = now + READ_EVERY_MS;
                }
                drop(inner);
                self.emit();
                return Duration::ZERO;
            }
            if due.is_some() {
                inner.reading = true;
            }
            due
        };
        let Some((id, url)) = due else {
            let next = self
                .inner
                .lock()
                .ok()
                .and_then(|inner| {
                    inner
                        .stored
                        .sources
                        .iter()
                        .filter(|s| s.enabled)
                        .filter_map(|s| inner.reads.get(&s.id).map(|r| r.next_ms))
                        .reduce(f64::min)
                })
                .unwrap_or(now + READ_EVERY_MS);
            return Duration::from_millis((next - now).clamp(1_000.0, READ_EVERY_MS) as u64);
        };
        self.emit();

        let fetched = net::fetch(&url);
        let now = now_ms();
        if let Ok(mut inner) = self.inner.lock() {
            inner.reading = false;
            if let Some(read) = inner.reads.get_mut(&id) {
                match fetched {
                    Ok(text) => {
                        read.text = Some(text);
                        read.last_ok_ms = Some(now);
                        read.error = None;
                        read.next_ms = now + READ_EVERY_MS;
                    }
                    Err(e) => {
                        tracing::info!(%id, "calendar not read: {e}");
                        read.error = Some(e);
                        read.next_ms = now + RETRY_MS;
                    }
                }
                // Open the feed up again even when this read failed: the
                // span moves on with time.
                if let Some(text) = &read.text {
                    match ics::occurrences(text, now - SPAN_BACK_MS, now + SPAN_AHEAD_MS, rrule::Tz::LOCAL) {
                        Ok(events) => {
                            tracing::debug!(%id, events = events.len(), "calendar read");
                            read.events = events;
                        }
                        Err(e) => {
                            tracing::info!(%id, "calendar not understood: {e}");
                            read.events.clear();
                            read.error = Some(if e == ics::WEB_PAGE { e } else { ics::NOT_A_CALENDAR.into() });
                        }
                    }
                }
            }
        }
        self.wake();
        self.emit();
        Duration::ZERO
    }

    // ----------------------------------------------------------- ticking

    fn tick(&self, device: &DeviceHandle) -> Duration {
        let now = now_ms();
        let live_on = self.live.enabled();
        let Ok(mut guard) = self.inner.lock() else { return TICK };
        let inner = &mut *guard;
        let s = inner.stored.settings.clone();
        let events = merged(&inner.stored.sources, &inner.reads);

        // Screen.
        let want = s.screen && live_on;
        let show = if want { showing(&events, &s, now) } else { None };
        let key = show.map(|i| key_of(&events[i]));
        if !std::mem::replace(&mut inner.startup_checked, true)
            && key.is_none()
            && live_on
            && self.live.has_card_from(SOURCE)
        {
            // A meeting's card left from before the app started.
            tracing::info!("a calendar card from before the start is on the screen");
            self.live.return_after(SOURCE, BACK_AFTER);
        }
        if key != inner.shown_key {
            match show {
                Some(i) => {
                    let card = card_of(&events, i);
                    tracing::info!(
                        starts_in_min = ((card.event.start_ms - now) / MINUTE_MS).round(),
                        "calendar card wanted"
                    );
                    self.live.set_hold(SOURCE, true);
                    self.live.cancel_return(SOURCE);
                    let _ = self.app.emit(EVENT_CALENDAR_CARD, card.clone());
                    inner.card = Some(card);
                }
                None => {
                    inner.card = None;
                    self.live.set_hold(SOURCE, false);
                    self.live.drop_pending(SOURCE);
                    // The timer's card (it holds the screen) or the song's
                    // comes back by itself; otherwise the own picture.
                    let song = self.live.track_playing() && self.live.now_playing_wanted();
                    if live_on && !self.live.held() && !song {
                        self.live.return_after(SOURCE, BACK_AFTER);
                    }
                }
            }
            inner.shown_key = key;
        } else if let Some(i) = show {
            // The same meeting, changed in the calendar (renamed, moved room).
            let card = card_of(&events, i);
            if inner.card.as_ref() != Some(&card) {
                let _ = self.app.emit(EVENT_CALENDAR_CARD, card.clone());
                inner.card = Some(card);
            }
        }

        // Reminder light.
        let keys = self.usb_keys(inner, device);
        let led = keys.as_ref().and_then(|k| k.get(&s.reminder_key).copied());
        let blink_from = if now < inner.try_until {
            Some(inner.try_until - TRY_MS)
        } else if s.reminder {
            reminder_due(&events, now).map(|e| e.start_ms - REMIND_MS)
        } else {
            None
        };
        let frame = match (blink_from, led) {
            (Some(from), Some(led)) => {
                let on = ((now - from) / BLINK_MS).floor() as i64 % 2 == 0;
                Some(vec![(led, if on { REMIND_COLOR } else { [0, 0, 0] })])
            }
            _ => None,
        };
        if frame != inner.reminder_sent {
            if frame.is_some() != inner.reminder_sent.is_some() {
                tracing::debug!(on = frame.is_some(), "calendar reminder light");
            }
            let _ = device.send(Request::SetLayer(
                LightLayer::Reminders,
                frame.clone().map(|colors| LiveFrame { colors, level: level_factor(5) }),
            ));
            inner.reminder_sent = frame;
        }
        let blinking = inner.reminder_sent.is_some();

        // The page (times are counted down there; the state changes only
        // at the meetings' edges).
        let state = Self::state_of(inner, now);
        if inner.last_state.as_ref() != Some(&state) {
            inner.last_state = Some(state.clone());
            let _ = self.app.emit(EVENT_CALENDAR, state);
        }

        if blinking || now < inner.try_until {
            TICK_BLINK
        } else {
            TICK
        }
    }

    /// Key ids of the keyboard on the USB cable, cached per model.
    fn usb_keys(&self, inner: &mut Inner, device: &DeviceHandle) -> Option<Arc<HashMap<String, u8>>> {
        let model_id = device.snapshot.lock().ok().and_then(|s| {
            s.keyboards
                .iter()
                .find(|k| k.connection == Connection::Usb && k.has_command_channel)
                .map(|k| k.model_id.clone())
        })?;
        if inner.model.as_ref().is_none_or(|(id, _)| *id != model_id) {
            let model = kb_devices::catalogue().ok()?.iter().find(|m| m.id == model_id)?;
            let keys = model
                .geometry
                .as_ref()
                .map(|g| g.keys.iter().map(|k| (k.id.clone(), k.led)).collect())
                .unwrap_or_default();
            inner.model = Some((model_id, Arc::new(keys)));
        }
        inner.model.as_ref().map(|(_, k)| k.clone())
    }
}

/// The HTTP status inside a Windows HTTP error (`HTTP_E_STATUS_*`,
/// `0x8019xxxx`): Windows words it in the system language and says little
/// ("… 404 ()"), so the UI words it itself.
fn http_status(hresult: u32) -> Option<u16> {
    (hresult & 0xFFFF_0000 == 0x8019_0000).then_some((hresult & 0xFFFF) as u16).filter(|s| (100..600).contains(s))
}

/// GET a text with Windows' own HTTP client (also used to read GitHub's
/// release list for test versions, updates.rs). Blocking.
pub(crate) fn http_get(url: &str) -> Result<String, String> {
    net::fetch(url)
}

#[cfg(windows)]
mod net {
    use super::MAX_FEED_BYTES;
    use windows::core::HSTRING;
    use windows::Foundation::Uri;
    use windows::Web::Http::Filters::{HttpBaseProtocolFilter, HttpCacheReadBehavior};
    use windows::Web::Http::HttpClient;

    /// GET the feed with Windows' own HTTP client (proxy and certificates
    /// as the system has them; nothing cached).
    pub fn fetch(url: &str) -> Result<String, String> {
        let short = |e: windows::core::Error| {
            if let Some(status) = super::http_status(e.code().0 as u32) {
                return format!("http {status}");
            }
            let m = e.message();
            let m = m.trim();
            let m: String = if m.is_empty() { format!("0x{:08X}", e.code().0) } else { m.chars().take(160).collect() };
            m
        };
        let filter = HttpBaseProtocolFilter::new().map_err(short)?;
        filter.SetAllowUI(false).map_err(short)?;
        filter.CacheControl().and_then(|c| c.SetReadBehavior(HttpCacheReadBehavior::NoCache)).map_err(short)?;
        let client = HttpClient::Create(&filter).map_err(short)?;
        let _ = client
            .DefaultRequestHeaders()
            .and_then(|h| h.UserAgent())
            .and_then(|ua| ua.TryParseAdd(&HSTRING::from("KlavyeStudio/0.1")));
        let uri = Uri::CreateUri(&HSTRING::from(url)).map_err(short)?;
        let text = client.GetStringAsync(&uri).map_err(short)?.join().map_err(short)?;
        if text.len() > MAX_FEED_BYTES / 2 {
            return Err("calendar too large".into());
        }
        Ok(text.to_string_lossy())
    }
}

#[cfg(not(windows))]
mod net {
    pub fn fetch(_url: &str) -> Result<String, String> {
        Err("no HTTP client on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: f64 = MINUTE_MS;

    fn ev(title: &str, start_min: f64, len_min: f64) -> CalendarEvent {
        CalendarEvent {
            title: title.into(),
            location: String::new(),
            start_ms: start_min * M,
            end_ms: (start_min + len_min) * M,
            all_day: false,
            color: [0, 0, 255],
            source: "c1".into(),
        }
    }

    fn s(lead: u16, until: CalendarUntil) -> CalendarSettings {
        CalendarSettings { lead_min: lead, until, ..CalendarSettings::default() }
    }

    #[test]
    fn a_card_comes_before_the_meeting_and_goes_after_the_chosen_time() {
        // A meeting at 600 (10:00), 15 minutes long.
        let events = vec![ev("Günlük toplantı", 600.0, 15.0)];
        let shortly = s(10, CalendarUntil::Shortly);
        assert_eq!(showing(&events, &shortly, 589.0 * M), None);
        assert_eq!(showing(&events, &shortly, 590.0 * M), Some(0));
        assert_eq!(showing(&events, &shortly, 604.0 * M), Some(0));
        assert_eq!(showing(&events, &shortly, 605.0 * M), None);
        assert_eq!(showing(&events, &s(10, CalendarUntil::Start), 600.0 * M), None);
        assert_eq!(showing(&events, &s(10, CalendarUntil::End), 614.0 * M), Some(0));
        assert_eq!(next_card_ms(&events, &shortly, 500.0 * M), Some(590.0 * M));
        assert_eq!(next_card_ms(&events, &shortly, 595.0 * M), None);
    }

    #[test]
    fn close_meetings_follow_each_other_and_the_coming_one_wins() {
        // 14:00–15:00 and 15:00–15:30, cards until the end.
        let events = vec![ev("Tasarım", 840.0, 60.0), ev("Birebir", 900.0, 30.0)];
        let end = s(10, CalendarUntil::End);
        // From 14:50 both are due: the coming one shows.
        assert_eq!(showing(&events, &end, 889.0 * M), Some(0));
        assert_eq!(showing(&events, &end, 890.0 * M), Some(1));
        // 16:30 after a card ending at 16:25 (gap 2 min < 3): bridged.
        let events = vec![ev("A", 600.0, 0.0), ev("B", 617.0, 30.0)];
        let w = card_windows(&events, &s(10, CalendarUntil::Shortly));
        assert_eq!((w[1].0, w[1].1), (605.0 * M, 622.0 * M));
        assert_eq!(showing(&events, &s(10, CalendarUntil::Shortly), 606.0 * M), Some(1));
    }

    #[test]
    fn whole_days_get_no_card_and_the_reminder_is_the_last_minute() {
        let mut day = ev("Proje teslimi", 0.0, 1440.0);
        day.all_day = true;
        let events = vec![day, ev("Standup", 600.0, 15.0)];
        let w = card_windows(&events, &s(10, CalendarUntil::Shortly));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].2, 1);
        assert!(reminder_due(&events, 598.9 * M).is_none());
        assert_eq!(reminder_due(&events, 599.0 * M).map(|e| e.title.as_str()), Some("Standup"));
        assert!(reminder_due(&events, 600.0 * M).is_none());
    }

    #[test]
    fn the_card_names_the_next_meeting_the_same_half_day() {
        let events = vec![ev("A", 600.0, 15.0), ev("B", 840.0, 60.0), ev("C", 840.0 + 13.0 * 60.0, 30.0)];
        assert_eq!(card_of(&events, 0).next.map(|n| n.title), Some("B".into()));
        assert_eq!(card_of(&events, 1).next, None);
    }

    #[test]
    fn addresses_are_checked_and_never_plain_http() {
        assert_eq!(
            normalize_url(" webcal://p52-caldav.icloud.com/published/2/abc "),
            Ok("https://p52-caldav.icloud.com/published/2/abc".into())
        );
        assert_eq!(
            normalize_url("https://calendar.google.com/calendar/ical/x%40gmail.com/private-1/basic.ics"),
            Ok("https://calendar.google.com/calendar/ical/x%40gmail.com/private-1/basic.ics".into())
        );
        assert_eq!(normalize_url("http://example.com/cal.ics"), Err("not https"));
        assert_eq!(normalize_url("https:///nohost"), Err("no host"));
        assert_eq!(normalize_url("https://a b"), Err("bad address"));
        assert_eq!(host_of("https://outlook.office365.com/owa/calendar/x/reachcalendar.ics"), "outlook.office365.com");
        assert_eq!(provider_of("calendar.google.com"), CalendarProvider::Google);
        assert_eq!(provider_of("outlook.live.com"), CalendarProvider::Outlook);
        assert_eq!(provider_of("p52-caldav.icloud.com"), CalendarProvider::Apple);
        assert_eq!(provider_of("cal.example.org"), CalendarProvider::Other);
        assert_eq!(clean_name("  İş   takvimi ", "x"), "İş takvimi");
        assert_eq!(clean_name("", "calendar.google.com"), "calendar.google.com");
    }

    #[test]
    fn http_errors_give_their_status() {
        assert_eq!(http_status(0x8019_0194), Some(404));
        assert_eq!(http_status(0x8019_0191), Some(401));
        assert_eq!(http_status(0x8007_0005), None);
        assert_eq!(http_status(0x8019_0000), None);
    }

    fn src(id: &str, url: &str) -> StoredSource {
        StoredSource { id: id.into(), name: "İş".into(), url: url.into(), color: [1, 2, 3], enabled: true }
    }

    #[test]
    fn addresses_are_sealed_on_disk_and_open_again() {
        let url = "https://calendar.google.com/calendar/ical/x%40gmail.com/private-abc/basic.ics";
        let stored = Stored { settings: CalendarSettings::default(), sources: vec![src("c1", url)] };
        let disk = for_disk(&stored);
        let json = serde_json::to_string(&disk).unwrap();
        if kb_winsys::secret::AVAILABLE {
            assert!(disk.sources[0].url.starts_with(SEAL_PREFIX), "{json}");
            assert!(!json.contains("private-abc") && !json.contains("google"), "{json}");
        } else {
            assert_eq!(disk.sources[0].url, url, "no DPAPI: kept as it is");
        }
        let (back, plain) = from_disk(disk.clone());
        assert_eq!(back.sources[0].url, url);
        assert_eq!(plain, !kb_winsys::secret::AVAILABLE);
        assert_eq!(for_disk(&back).sources[0].url.starts_with(SEAL_PREFIX), kb_winsys::secret::AVAILABLE);

        // An older version's file: plain, sealed on the next save.
        let (old, plain) = from_disk(stored.clone());
        assert!(plain);
        assert_eq!(old.sources[0].url, url);
    }

    #[test]
    fn an_address_that_cannot_be_opened_is_kept_and_never_read() {
        let damaged = format!("{SEAL_PREFIX}AAAA");
        let (back, plain) =
            from_disk(Stored { settings: CalendarSettings::default(), sources: vec![src("c1", &damaged)] });
        assert!(!plain);
        assert_eq!(back.sources[0].url, damaged, "kept for the computer that can open it");
        assert_eq!(for_disk(&back).sources[0].url, damaged, "not sealed twice");
        assert!(is_sealed(&back.sources[0].url));
    }

    #[test]
    fn backups_carry_addresses_only_when_asked() {
        let b = CalendarBackup { settings: CalendarSettings::default(), sources: None };
        let json = serde_json::to_string(&b).unwrap();
        assert!(!json.contains("sources"), "{json}");
        let back: CalendarBackup = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn stored_calendars_tolerate_missing_fields() {
        let s: Stored = serde_json::from_str(
            r#"{"sources":[{"id":"c1","name":"İş","url":"https://x/y.ics","color":[1,2,3],"enabled":true}]}"#,
        )
        .unwrap();
        assert_eq!(s.sources.len(), 1);
        assert_eq!(s.settings, CalendarSettings::default());
        assert_eq!(
            CalendarSettings { lead_min: 500, reminder_key: "../".into(), ..CalendarSettings::default() }
                .sanitized()
                .lead_min,
            60
        );
    }
}
