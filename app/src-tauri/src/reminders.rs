//! Hatırlatıcı (phase C, fourth source): reminders that go off once, every
//! day at a time, or every few minutes in working hours.
//!
//! Owner's choices (2026-11-11, after the preview in the chat):
//! * The light always shows: a wave of the reminder's colour sweeps over the
//!   keyboard twice (`LightLayer::Nudges`, RAM only, no live mode needed).
//! * A card (the time large, the title, when it comes again) goes on the
//!   screen only when the reminder asks for one, the live mode is on, no
//!   meeting's card holds the screen and the budget has an upload now. A
//!   late reminder means nothing, so its card never waits: the light alone
//!   goes off. The card stays `stay_min` (2), then what was under it comes
//!   back. Order: calendar → reminder → focus timer → song → own picture.
//! * Repeating reminders wait while nobody is at the computer (no input for
//!   5 minutes, which a locked screen also gives) and count again from the
//!   return. Timed ones go off anyway.
//! * The card shows absolute times only, so it never goes stale.
//! * A Windows notification comes too when the user turns it on, and always
//!   while the keyboard is not on the USB cable (the keys cannot light up).
//!
//! Titles are the user's own words: they are not logged.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{
    CardSkip, CardSkipReason, Connection, Reminder, ReminderCard, ReminderInfo, ReminderSettings, ReminderState,
    ReminderWhen,
};
use crate::live::{now_ms, LiveHandle};
use crate::notify;
use crate::pomodoro::{play_sound, KeyMap};
use crate::settings;
use chrono::{Datelike, Days, Local, NaiveDate, TimeZone};
use kb_protocol::sonix::{level_factor, Rgb};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Why a reminder's card cannot go on the screen now, `None` if it can.
/// Away from the USB cable the upload would fail and be dropped when the
/// card's time ends, so it is not tried (K5).
fn card_skip_reason(
    cards_on: bool,
    live_on: bool,
    on_cable: bool,
    meeting: bool,
    budget: bool,
) -> Option<CardSkipReason> {
    if !cards_on {
        Some(CardSkipReason::CardsOff)
    } else if !live_on {
        Some(CardSkipReason::LiveOff)
    } else if !on_cable {
        Some(CardSkipReason::NoCable)
    } else if meeting {
        Some(CardSkipReason::Meeting)
    } else if !budget {
        Some(CardSkipReason::Budget)
    } else {
        None
    }
}

/// Live mode source id of the reminder cards.
pub const SOURCE: &str = "reminder";
pub const EVENT_REMINDERS: &str = "reminders";
pub const EVENT_REMINDER_CARD: &str = "reminder-card";
const FILE: &str = "reminders.json";
const VERSION: u32 = 1;

const MINUTE_MS: f64 = 60_000.0;
const DAY_MIN: u16 = 24 * 60;
/// A reminder missed while the app was closed or the PC slept still goes
/// off this late; later, it is skipped.
const GRACE_MS: f64 = 10.0 * MINUTE_MS;
/// No input this long: nobody is at the computer.
const AWAY_MS: u64 = 5 * 60_000;
/// The wave: one pass from left to right, twice.
const PASS_MS: f64 = 1_500.0;
const PASSES: f64 = 2.0;
/// Width of the wave's band, as a share of the keyboard's width.
const WAVE_WIDTH: f32 = 0.10;
/// After the card, the own picture comes back this soon (if nothing else does).
const BACK_AFTER: Duration = Duration::from_secs(5);
/// "Klavyede 1 dk göster": the tried card stays this long.
const TRY_CARD_BACK: Duration = Duration::from_secs(60);
const TICK: Duration = Duration::from_secs(1);
const TICK_WAVE: Duration = Duration::from_millis(33);
const MAX_REMINDERS: usize = 20;
const MAX_TITLE: usize = 40;
const MAX_NOTE: usize = 60;

// ----------------------------------------------------------------- stored

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Stored {
    settings: ReminderSettings,
    reminders: Vec<Reminder>,
    /// When each reminder last went off (or was saved): a timed one goes
    /// off once per time, also across a restart.
    fired: HashMap<String, f64>,
}

fn kind_of(when: &ReminderWhen) -> &'static str {
    match when {
        ReminderWhen::Once { .. } => "once",
        ReminderWhen::Daily { .. } => "daily",
        ReminderWhen::Every { .. } => "every",
    }
}

/// The user's reminder, checked: a title, a sane time. Errors are words the
/// UI looks up.
fn clean(mut r: Reminder, now: f64) -> Result<Reminder, String> {
    r.title = r.title.trim().chars().take(MAX_TITLE).collect();
    if r.title.is_empty() {
        return Err("title empty".into());
    }
    r.note = r.note.trim().chars().take(MAX_NOTE).collect();
    r.when = match r.when {
        ReminderWhen::Once { at } => {
            if !at.is_finite() || at <= 0.0 {
                return Err("bad time".into());
            }
            if r.enabled && at <= now {
                return Err("time passed".into());
            }
            ReminderWhen::Once { at }
        }
        ReminderWhen::Daily { minute, days } => {
            let days = days & 0x7F;
            if minute >= DAY_MIN || days == 0 {
                return Err("bad time".into());
            }
            ReminderWhen::Daily { minute, days }
        }
        ReminderWhen::Every { every, from, to, days } => {
            let days = days & 0x7F;
            if !(5..=240).contains(&every) || from >= to || to > DAY_MIN || days == 0 {
                return Err("bad time".into());
            }
            ReminderWhen::Every { every, from, to, days }
        }
    };
    Ok(r)
}

// --------------------------------------------------------------- schedule

fn date_of<Tz: TimeZone>(tz: &Tz, ms: f64) -> Option<NaiveDate> {
    tz.timestamp_millis_opt(ms as i64).single().map(|d| d.date_naive())
}

/// `minute` (after midnight; 1440 is the next midnight) of `day`, local.
fn at_minute<Tz: TimeZone>(tz: &Tz, day: NaiveDate, minute: u16) -> Option<f64> {
    let m = u32::from(minute.min(DAY_MIN));
    let t = if m == u32::from(DAY_MIN) {
        day.succ_opt()?.and_hms_opt(0, 0, 0)?
    } else {
        day.and_hms_opt(m / 60, m % 60, 0)?
    };
    tz.from_local_datetime(&t).earliest().map(|d| d.timestamp_millis() as f64)
}

fn day_on(days: u8, day: NaiveDate) -> bool {
    days & (1 << day.weekday().num_days_from_monday()) != 0
}

/// A timed reminder's latest time at or before `now` (within a week).
fn last_due<Tz: TimeZone>(when: &ReminderWhen, tz: &Tz, now: f64) -> Option<f64> {
    match *when {
        ReminderWhen::Once { at } => (at <= now).then_some(at),
        ReminderWhen::Daily { minute, days } => {
            let today = date_of(tz, now)?;
            (0..8)
                .filter_map(|i| today.checked_sub_days(Days::new(i)))
                .filter(|d| day_on(days, *d))
                .filter_map(|d| at_minute(tz, d, minute))
                .find(|&t| t <= now)
        }
        ReminderWhen::Every { .. } => None,
    }
}

/// A timed reminder's first time after `after` (within a week).
fn next_timed<Tz: TimeZone>(when: &ReminderWhen, tz: &Tz, after: f64) -> Option<f64> {
    match *when {
        ReminderWhen::Once { at } => (at > after).then_some(at),
        ReminderWhen::Daily { minute, days } => {
            let first = date_of(tz, after)?;
            (0..8)
                .filter_map(|i| first.checked_add_days(Days::new(i)))
                .filter(|d| day_on(days, *d))
                .filter_map(|d| at_minute(tz, d, minute))
                .find(|&t| t > after)
        }
        ReminderWhen::Every { .. } => None,
    }
}

/// A repeating reminder's next time: `every` after the anchor (the last
/// time it went off, or the user's return), inside a chosen day's window,
/// whose start also counts as an anchor. `None` for timed reminders.
fn next_every<Tz: TimeZone>(when: &ReminderWhen, tz: &Tz, anchor: f64) -> Option<f64> {
    let ReminderWhen::Every { every, from, to, days } = *when else { return None };
    let step = f64::from(every.max(1)) * MINUTE_MS;
    let mut day = date_of(tz, anchor)?;
    let mut a = anchor;
    for _ in 0..9 {
        if day_on(days, day) {
            let start = at_minute(tz, day, from)?;
            let end = at_minute(tz, day, to)?;
            let t = a.max(start) + step;
            if t <= end {
                return Some(t);
            }
        }
        day = day.succ_opt()?;
        a = f64::NEG_INFINITY;
    }
    None
}

/// What a reminder does at `now`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Check {
    Quiet,
    /// It goes off for this time.
    Fire(f64),
    /// Its time went by too long ago (app closed, PC asleep): skip it.
    Missed(f64),
}

/// `fired`: when it last went off or was saved; `anchor`: a repeating one's
/// anchor.
fn check<Tz: TimeZone>(when: &ReminderWhen, tz: &Tz, now: f64, fired: f64, anchor: f64) -> Check {
    let due = match when {
        ReminderWhen::Every { .. } => next_every(when, tz, anchor).filter(|&t| t <= now),
        _ => last_due(when, tz, now).filter(|&t| t > fired),
    };
    match due {
        Some(t) if now - t <= GRACE_MS => Check::Fire(t),
        Some(t) => Check::Missed(t),
        None => Check::Quiet,
    }
}

/// When it goes off after `t` (the card's "Sonraki").
fn next_after<Tz: TimeZone>(when: &ReminderWhen, tz: &Tz, t: f64) -> Option<f64> {
    match when {
        ReminderWhen::Every { .. } => next_every(when, tz, t),
        _ => next_timed(when, tz, t),
    }
}

// ------------------------------------------------------------------ light

fn scale(c: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    c.map(|v| (f32::from(v) * k).round() as u8)
}

/// The wave `since` ms after it started: a band of `color` sweeping from
/// left to right, twice. Only the keys it lights, so the layers under it
/// show around the band. `None` once it is over.
fn wave_frame(map: &KeyMap, color: Rgb, since: f64) -> Option<Vec<(u8, Rgb)>> {
    if !(0.0..PASS_MS * PASSES).contains(&since) {
        return None;
    }
    let pos = ((since % PASS_MS) / PASS_MS) as f32 * 1.4 - 0.2;
    Some(
        map.keys
            .iter()
            .filter_map(|&(led, x, _)| {
                let a = (-((x - pos) / WAVE_WIDTH).powi(2)).exp();
                (a > 0.04).then(|| (led, scale(color, a)))
            })
            .collect(),
    )
}

// ----------------------------------------------------------------- worker

struct Inner {
    stored: Stored,
    /// Repeating reminders: the anchor their next time counts from.
    anchors: HashMap<String, f64>,
    /// The wave: its colour and when it started.
    wave: Option<(Rgb, f64)>,
    wave_sent: Option<Vec<(u8, Rgb)>>,
    /// The reminder whose card holds the screen, and until when.
    showing: Option<(String, f64)>,
    /// The last card that could not go on the screen, and why.
    card_skip: Option<CardSkip>,
    startup_checked: bool,
    away: bool,
    model: Option<(String, KeyMap)>,
    last_state: Option<ReminderState>,
}

enum Msg {
    Wake,
}

/// Handle to the reminder worker (managed Tauri state).
pub struct RemindersHandle {
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    live: Arc<LiveHandle>,
    config_dir: Option<PathBuf>,
    /// The app id Windows notifications are shown under.
    app_id: String,
}

impl RemindersHandle {
    pub fn spawn(
        app: AppHandle,
        live: Arc<LiveHandle>,
        device: Arc<DeviceHandle>,
        config_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let mut stored =
            config_dir.as_deref().and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).unwrap_or_default();
        stored.settings = stored.settings.sanitized();
        if !stored.reminders.is_empty() {
            tracing::info!(reminders = stored.reminders.len(), "reminders");
        }
        let now = now_ms();
        let anchors = stored.reminders.iter().map(|r| (r.id.clone(), now)).collect();
        let inner = Arc::new(Mutex::new(Inner {
            stored,
            anchors,
            wave: None,
            wave_sent: None,
            showing: None,
            card_skip: None,
            startup_checked: false,
            away: false,
            model: None,
            last_state: None,
        }));
        let (tx, rx) = mpsc::channel();
        let app_id = notify::app_id(&app.config().identifier);
        let handle = Arc::new(Self { tx, inner, app, live, config_dir, app_id });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("reminders".into())
            .spawn(move || loop {
                let wait = worker.tick(&device);
                match rx.recv_timeout(wait) {
                    Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            })
            .expect("spawn reminders thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    pub fn state(&self) -> Result<ReminderState, String> {
        let inner = self.inner.lock().map_err(|_| "reminders unavailable")?;
        Ok(Self::state_of(&inner, &Local))
    }

    fn state_of<Tz: TimeZone>(inner: &Inner, tz: &Tz) -> ReminderState {
        let s = &inner.stored.settings;
        let now = now_ms();
        ReminderState {
            settings: s.clone(),
            reminders: inner
                .stored
                .reminders
                .iter()
                .map(|r| {
                    let repeating = matches!(r.when, ReminderWhen::Every { .. });
                    let next_ms = if !s.enabled || !r.enabled || (repeating && inner.away && s.away_wait) {
                        None
                    } else if repeating {
                        next_every(&r.when, tz, inner.anchors.get(&r.id).copied().unwrap_or(now))
                    } else {
                        next_timed(&r.when, tz, now)
                    };
                    ReminderInfo { reminder: r.clone(), next_ms }
                })
                .collect(),
            showing: inner.showing.as_ref().map(|(id, _)| id.clone()),
            showing_until_ms: inner.showing.as_ref().map(|(_, until)| *until),
            away: inner.away,
            card_skip: inner.card_skip.clone(),
        }
    }

    fn emit(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            let state = Self::state_of(&inner, &Local);
            inner.last_state = Some(state.clone());
            let _ = self.app.emit(EVENT_REMINDERS, state);
        }
    }

    fn save(&self, stored: &Stored) -> Result<(), String> {
        match &self.config_dir {
            Some(dir) => settings::save(dir, FILE, VERSION, stored).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    pub fn configure(&self, settings: ReminderSettings) -> Result<ReminderState, String> {
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "reminders unavailable")?;
            inner.stored.settings = settings.sanitized();
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake();
        self.emit();
        self.state()
    }

    /// Add a reminder (empty id) or change one. Its times count from now:
    /// a time that went by today does not go off.
    pub fn save_reminder(&self, reminder: Reminder) -> Result<ReminderState, String> {
        let now = now_ms();
        let mut r = clean(reminder, now)?;
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "reminders unavailable")?;
            let list = &mut inner.stored.reminders;
            match list.iter_mut().find(|x| !r.id.is_empty() && x.id == r.id) {
                Some(slot) => *slot = r.clone(),
                None => {
                    if list.len() >= MAX_REMINDERS {
                        return Err("too many reminders".into());
                    }
                    r.id = format!("r{:x}{}", now as u64, list.len());
                    list.push(r.clone());
                }
            }
            inner.stored.fired.insert(r.id.clone(), now);
            inner.anchors.insert(r.id.clone(), now);
            tracing::info!(id = %r.id, kind = kind_of(&r.when), card = r.card, "reminder saved");
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake();
        self.emit();
        self.state()
    }

    pub fn remove(&self, id: &str) -> Result<ReminderState, String> {
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "reminders unavailable")?;
            inner.stored.reminders.retain(|r| r.id != id);
            inner.stored.fired.remove(id);
            if inner.card_skip.as_ref().is_some_and(|k| k.id == id) {
                inner.card_skip = None;
            }
            inner.anchors.remove(id);
            inner.stored.clone()
        };
        tracing::info!(%id, "reminder removed");
        self.save(&stored)?;
        self.wake();
        self.emit();
        self.state()
    }

    /// "Dene": the wave in this colour (and the sound), now.
    pub fn try_light(&self, color: [u8; 3], sound: bool) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.wave = Some((color, now_ms()));
        }
        if sound {
            play_sound();
        }
        self.wake();
    }

    /// The UI handed in a card to try on the keyboard: it goes now (counted
    /// like a manual upload) and the own picture comes back after a minute.
    pub fn try_card(&self) {
        tracing::info!("reminder card tried on the keyboard");
        let _ = self.live.send_now();
        self.live.return_after(SOURCE, TRY_CARD_BACK);
    }

    // ----------------------------------------------------------- ticking

    /// Let go of the screen: what was under the card comes back by itself
    /// (a meeting's card, the timer's, the song), or else the own picture.
    fn release(&self, inner: &mut Inner, live_on: bool) {
        if inner.showing.take().is_none() {
            return;
        }
        self.live.set_hold(SOURCE, false);
        self.live.drop_pending(SOURCE);
        let song = self.live.track_playing() && self.live.now_playing_wanted();
        if live_on && !self.live.held() && !song {
            self.live.return_after(SOURCE, BACK_AFTER);
        }
    }

    fn tick(&self, device: &DeviceHandle) -> Duration {
        let now = now_ms();
        let live_on = self.live.enabled();
        let idle = kb_winsys::idle_ms();
        let Ok(mut guard) = self.inner.lock() else { return TICK };
        let inner = &mut *guard;
        let s = inner.stored.settings.clone();

        if !std::mem::replace(&mut inner.startup_checked, true) && live_on && self.live.has_card_from(SOURCE) {
            // A reminder's card left from before the app started.
            tracing::info!("a reminder card from before the start is on the screen");
            self.live.return_after(SOURCE, BACK_AFTER);
        }

        // Nobody at the computer: repeating reminders count from the return.
        let away = idle.is_some_and(|ms| ms >= AWAY_MS);
        if away != inner.away {
            tracing::debug!(away, "at the computer");
            inner.away = away;
        }
        if away && s.away_wait {
            for r in inner.stored.reminders.iter().filter(|r| matches!(r.when, ReminderWhen::Every { .. })) {
                inner.anchors.insert(r.id.clone(), now);
            }
        }

        // What goes off now.
        let mut fired: Vec<(Reminder, f64)> = Vec::new();
        let mut changed = false;
        if s.enabled {
            let (stored, anchors) = (&mut inner.stored, &mut inner.anchors);
            for r in stored.reminders.iter_mut().filter(|r| r.enabled) {
                let last = stored.fired.get(&r.id).copied().unwrap_or(0.0);
                let anchor = *anchors.entry(r.id.clone()).or_insert(now);
                let (t, went_off) = match check(&r.when, &Local, now, last, anchor) {
                    Check::Quiet => continue,
                    Check::Fire(t) => (t, true),
                    Check::Missed(t) => {
                        tracing::info!(id = %r.id, late_min = ((now - t) / MINUTE_MS).round(), "reminder missed");
                        (t, false)
                    }
                };
                if went_off {
                    fired.push((r.clone(), t));
                }
                stored.fired.insert(r.id.clone(), t);
                // A repeating one counts on from this time; one missed long
                // ago counts from now.
                anchors.insert(r.id.clone(), if went_off { t } else { now });
                if matches!(r.when, ReminderWhen::Once { .. }) {
                    // A one-time reminder is done once its time has gone by.
                    r.enabled = false;
                }
                changed = true;
            }
        }
        if changed {
            let stored = inner.stored.clone();
            if let Err(e) = self.save(&stored) {
                tracing::warn!("reminders not saved: {e}");
            }
        }

        // The keys light up only on the USB cable; otherwise a notification
        // says it instead.
        let map = self.key_map(inner, device);
        let notify = s.notify || map.is_none();
        for (r, t) in &fired {
            tracing::info!(id = %r.id, card = r.card, sound = r.sound, notify, late_s = ((now - t) / 1000.0).round(), "reminder went off");
            inner.wave = Some((r.color, now));
            if r.sound {
                play_sound();
            }
            if notify {
                self.notify(r);
            }
        }
        if let Some((r, t)) = fired.iter().rev().find(|(r, _)| r.card) {
            let why_not = card_skip_reason(
                s.screen,
                live_on,
                map.is_some(),
                self.live.held_by(crate::calendar::SOURCE),
                self.live.budget_ready(),
            );
            match why_not {
                Some(reason) => {
                    tracing::info!(id = %r.id, ?reason, "reminder card skipped");
                    inner.card_skip = Some(CardSkip { id: r.id.clone(), reason, at_ms: *t });
                }
                None => {
                    inner.card_skip = None;
                    let card =
                        ReminderCard { reminder: r.clone(), at_ms: *t, next_ms: next_after(&r.when, &Local, *t) };
                    self.live.set_hold(SOURCE, true);
                    self.live.cancel_return(SOURCE);
                    let _ = self.app.emit(EVENT_REMINDER_CARD, card);
                    inner.showing = Some((r.id.clone(), now + f64::from(s.stay_min) * MINUTE_MS));
                }
            }
        }

        // The card's time is over, or it should not be there any more.
        if let Some((id, until)) = inner.showing.clone() {
            let still = inner.stored.reminders.iter().any(|r| r.id == id);
            if now >= until || !live_on || !s.screen || !s.enabled || !still {
                self.release(inner, live_on);
            }
        }

        // The wave.
        let frame = match (inner.wave, &map) {
            (Some((color, from)), Some(map)) => wave_frame(map, color, now - from),
            _ => None,
        };
        if frame.is_none() {
            inner.wave = None;
        }
        if frame != inner.wave_sent {
            if frame.is_some() != inner.wave_sent.is_some() {
                tracing::debug!(on = frame.is_some(), "reminder wave");
            }
            let _ = device.send(Request::SetLayer(
                LightLayer::Nudges,
                frame.clone().map(|colors| LiveFrame { colors, level: level_factor(5) }),
            ));
            inner.wave_sent = frame;
        }
        let waving = inner.wave_sent.is_some();

        let state = Self::state_of(inner, &Local);
        if inner.last_state.as_ref() != Some(&state) {
            inner.last_state = Some(state.clone());
            let _ = self.app.emit(EVENT_REMINDERS, state);
        }

        if waving || inner.wave.is_some() {
            TICK_WAVE
        } else {
            TICK
        }
    }

    /// A Windows notification with the reminder's title and note, off this
    /// thread (the first one can take a moment).
    fn notify(&self, r: &Reminder) {
        let (app_id, title, note, id) = (self.app_id.clone(), r.title.clone(), r.note.clone(), r.id.clone());
        let spawned = std::thread::Builder::new().name("reminder-notification".into()).spawn(move || {
            if let Err(e) = notify::show(&app_id, &title, &note) {
                tracing::warn!(%id, "reminder notification not shown: {e}");
            }
        });
        if let Err(e) = spawned {
            tracing::warn!("reminder notification not shown: {e}");
        }
    }

    /// Key layout of the keyboard on the USB cable, cached per model.
    fn key_map(&self, inner: &mut Inner, device: &DeviceHandle) -> Option<KeyMap> {
        let model_id = device.snapshot.lock().ok().and_then(|s| {
            s.keyboards
                .iter()
                .find(|k| k.connection == Connection::Usb && k.has_command_channel)
                .map(|k| k.model_id.clone())
        })?;
        if inner.model.as_ref().is_none_or(|(id, _)| *id != model_id) {
            let model = kb_devices::catalogue().ok()?.iter().find(|m| m.id == model_id)?;
            inner.model = Some((model_id, KeyMap::of(model)));
        }
        inner.model.as_ref().map(|(_, m)| m.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn ist() -> FixedOffset {
        FixedOffset::east_opt(3 * 3600).unwrap()
    }

    /// November 2026, Istanbul time. The 11th is a Wednesday.
    fn at(day: u32, h: u32, m: u32) -> f64 {
        ist().with_ymd_and_hms(2026, 11, day, h, m, 0).unwrap().timestamp_millis() as f64
    }

    const WEEKDAYS: u8 = 0b001_1111;

    #[test]
    fn a_skipped_card_says_why() {
        use CardSkipReason::*;
        assert_eq!(card_skip_reason(true, true, true, false, true), None);
        assert_eq!(card_skip_reason(false, true, true, false, true), Some(CardsOff));
        assert_eq!(card_skip_reason(true, false, true, false, true), Some(LiveOff));
        assert_eq!(card_skip_reason(true, true, false, false, true), Some(NoCable));
        assert_eq!(card_skip_reason(true, true, true, true, true), Some(Meeting));
        assert_eq!(card_skip_reason(true, true, true, false, false), Some(Budget));
    }

    #[test]
    fn a_daily_reminder_goes_off_once_at_its_time_on_its_days() {
        let w = ReminderWhen::Daily { minute: 14 * 60, days: WEEKDAYS };
        let tz = ist();
        let saved = at(11, 10, 0);
        assert_eq!(check(&w, &tz, at(11, 13, 59), saved, saved), Check::Quiet);
        assert_eq!(check(&w, &tz, at(11, 14, 0), saved, saved), Check::Fire(at(11, 14, 0)));
        // Once it went off, not again that day.
        assert_eq!(check(&w, &tz, at(11, 14, 1), at(11, 14, 0), saved), Check::Quiet);
        // A few minutes late (the app just opened) it still goes off; much later it is missed.
        assert_eq!(check(&w, &tz, at(11, 14, 9), saved, saved), Check::Fire(at(11, 14, 0)));
        assert_eq!(check(&w, &tz, at(11, 14, 30), saved, saved), Check::Missed(at(11, 14, 0)));
        // Saved after its time today: from tomorrow on.
        assert_eq!(check(&w, &tz, at(11, 14, 5), at(11, 14, 3), at(11, 14, 3)), Check::Quiet);
        assert_eq!(next_timed(&w, &tz, at(11, 14, 3)), Some(at(12, 14, 0)));
        // Friday afternoon, then Monday (the weekend is off).
        assert_eq!(next_timed(&w, &tz, at(13, 15, 0)), Some(at(16, 14, 0)));
    }

    #[test]
    fn a_repeating_reminder_counts_from_the_last_time_inside_its_hours() {
        let w = ReminderWhen::Every { every: 45, from: 9 * 60, to: 18 * 60, days: WEEKDAYS };
        let tz = ist();
        // Before the hours: the first is 45 minutes after they start.
        assert_eq!(next_every(&w, &tz, at(11, 7, 0)), Some(at(11, 9, 45)));
        // Counted from the last time, or from the user's return.
        assert_eq!(next_every(&w, &tz, at(11, 9, 45)), Some(at(11, 10, 30)));
        assert_eq!(next_every(&w, &tz, at(11, 10, 10)), Some(at(11, 10, 55)));
        // The last one fits by 18:00; after it, the next working day.
        assert_eq!(next_every(&w, &tz, at(11, 17, 15)), Some(at(11, 18, 0)));
        assert_eq!(next_every(&w, &tz, at(11, 17, 16)), Some(at(12, 9, 45)));
        assert_eq!(next_every(&w, &tz, at(13, 17, 30)), Some(at(16, 9, 45)));
        // Due at its time; missed when the PC slept through it.
        assert_eq!(check(&w, &tz, at(11, 10, 29), 0.0, at(11, 9, 45)), Check::Quiet);
        assert_eq!(check(&w, &tz, at(11, 10, 30), 0.0, at(11, 9, 45)), Check::Fire(at(11, 10, 30)));
        assert_eq!(check(&w, &tz, at(11, 12, 0), 0.0, at(11, 9, 45)), Check::Missed(at(11, 10, 30)));
        assert_eq!(next_after(&w, &tz, at(11, 10, 30)), Some(at(11, 11, 15)));
    }

    #[test]
    fn a_one_time_reminder_goes_off_once() {
        let w = ReminderWhen::Once { at: at(11, 19, 30) };
        let tz = ist();
        let saved = at(11, 12, 0);
        assert_eq!(check(&w, &tz, at(11, 19, 29), saved, saved), Check::Quiet);
        assert_eq!(check(&w, &tz, at(11, 19, 30), saved, saved), Check::Fire(at(11, 19, 30)));
        assert_eq!(check(&w, &tz, at(11, 20, 0), saved, saved), Check::Missed(at(11, 19, 30)));
        assert_eq!(next_timed(&w, &tz, at(11, 19, 31)), None);
        assert_eq!(next_after(&w, &tz, at(11, 19, 30)), None);
    }

    #[test]
    fn reminders_are_checked_before_they_are_kept() {
        let now = at(11, 12, 0);
        let r = |title: &str, when| Reminder { title: title.into(), when, ..Reminder::default() };
        assert_eq!(clean(r("  ", ReminderWhen::default()), now), Err("title empty".into()));
        assert_eq!(clean(r("İlacını al", ReminderWhen::Once { at: at(11, 11, 0) }), now), Err("time passed".into()));
        let bad = [
            ReminderWhen::Every { every: 3, from: 540, to: 1080, days: WEEKDAYS },
            ReminderWhen::Every { every: 60, from: 1080, to: 540, days: WEEKDAYS },
            ReminderWhen::Daily { minute: 600, days: 0x80 },
            ReminderWhen::Daily { minute: 1440, days: WEEKDAYS },
        ];
        for when in bad {
            assert_eq!(clean(r("Su iç", when), now), Err("bad time".into()));
        }
        let long = clean(r(&"ş".repeat(80), ReminderWhen::default()), now).unwrap();
        assert_eq!(long.title.chars().count(), MAX_TITLE);
        // A one-time reminder that went by may stay in the list, turned off.
        let done = Reminder { enabled: false, ..r("Annemi ara", ReminderWhen::Once { at: at(11, 11, 0) }) };
        assert!(clean(done, now).is_ok());
    }

    #[test]
    fn the_wave_sweeps_left_to_right_twice_then_ends() {
        let map = KeyMap { keys: vec![(1, 0.0, 0.0), (2, 0.5, 0.0), (3, 1.0, 0.0)] };
        let lit = |since: f64| wave_frame(&map, [255, 0, 0], since).map(|f| f.iter().map(|k| k.0).collect::<Vec<_>>());
        // The band is at since / PASS_MS * 1.4 - 0.2 across the keyboard.
        assert_eq!(lit(PASS_MS * 0.2 / 1.4), Some(vec![1]));
        assert_eq!(lit(PASS_MS * 0.7 / 1.4), Some(vec![2]));
        assert_eq!(lit(PASS_MS + PASS_MS * 1.2 / 1.4), Some(vec![3]));
        assert_eq!(lit(PASS_MS * PASSES), None);
        // Full colour at the band's centre.
        assert_eq!(wave_frame(&map, [200, 100, 0], PASS_MS * 0.7 / 1.4).unwrap()[0].1, [200, 100, 0]);
    }
}
