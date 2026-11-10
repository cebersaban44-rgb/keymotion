//! Pomodoro (phase C, second source): a focus/break timer.
//!
//! * The timer runs here, not in the webview, so it keeps time while the
//!   window is hidden (hidden pages get their timers throttled).
//! * Progress shows on the key lights as a background live layer (`04 20`,
//!   RAM only, no flash write): F1–F12 fill up in the phase colour, the
//!   digit keys count the finished rounds, Esc shows the phase, and the
//!   whole board blinks twice when a phase starts. A preview the user edits
//!   on the Lighting page goes on top (`device::Request::SetAmbient`). When
//!   the timer stops, the keyboard goes back to its saved effect.
//! * When a phase starts the UI is asked for a card (`pomodoro-card`
//!   event), which goes through the live mode like any other source:
//!   budget, newest card wins. While the timer runs it holds the screen, so
//!   "Çalan şarkı" waits; when it stops, the song that plays comes back, or
//!   a summary card replaces the phase card so the screen stays true.

use crate::device::{DeviceHandle, LiveFrame, Request};
use crate::dto::{
    Connection, PomodoroCard, PomodoroCardKind, PomodoroPhase, PomodoroSettings, PomodoroState, PomodoroStatus,
    PomodoroSummary,
};
use crate::live::{now_ms, LiveHandle};
use crate::settings;
use kb_devices::Model;
use kb_protocol::sonix::Rgb;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

pub const EVENT_POMODORO: &str = "pomodoro";
pub const EVENT_POMODORO_CARD: &str = "pomodoro-card";
/// Live mode source id of the Pomodoro cards.
pub const SOURCE: &str = "pomodoro";
const FILE: &str = "pomodoro.json";
const VERSION: u32 = 1;
const MINUTE_MS: f64 = 60_000.0;
/// Light frames are worked out this often while they show.
const TICK: Duration = Duration::from_millis(100);
const IDLE: Duration = Duration::from_secs(60);
/// The blink when a phase starts; the card is asked for after it, so the
/// upload (which pauses the live layer) does not cut it short.
const FLASH_MS: f64 = 1_000.0;

const FOCUS: Rgb = [255, 80, 20];
const SHORT: Rgb = [30, 220, 100];
const LONG: Rgb = [40, 140, 255];
/// Unfilled part of the bar, so the track stays visible.
const TRACK: f32 = 0.08;
/// Finished rounds on the digit keys.
const ROUND: f32 = 0.45;
/// Bar while paused, at the low point of its blink.
const PAUSE_LOW: f32 = 0.2;
const PAUSE_PERIOD_MS: f64 = 1_200.0;
/// The bar's partly filled key changes in this many steps, so frames go
/// to the keyboard only when something visible changed.
const STEPS: f32 = 32.0;

/// What the UI asks the timer to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroAction {
    /// Start a new series (idle) or go on (paused).
    Start,
    Pause,
    /// End the current phase now.
    Skip,
    Stop,
}

fn phase_color(p: PomodoroPhase) -> Rgb {
    match p {
        PomodoroPhase::Focus => FOCUS,
        PomodoroPhase::ShortBreak => SHORT,
        PomodoroPhase::LongBreak => LONG,
    }
}

fn scale(c: Rgb, k: f32) -> Rgb {
    c.map(|v| (f32::from(v) * k.clamp(0.0, 1.0)).round() as u8)
}

// ------------------------------------------------------------------ timer

/// What a change of the timer led to.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    /// A phase started (the series too, when it is the first).
    Phase,
    /// The series ended (finished or stopped).
    Done(PomodoroSummary),
}

#[derive(Debug, Clone, PartialEq)]
struct Timer {
    status: PomodoroStatus,
    phase: PomodoroPhase,
    round: u8,
    phase_ms: f64,
    /// Running: when the phase would have started had it never paused.
    /// Paused: time spent in the phase so far.
    anchor: f64,
    focus_done: u8,
    /// Time spent in finished focus phases.
    focus_ms: f64,
    /// When the current phase started (for the blink).
    started: f64,
    last: Option<PomodoroSummary>,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            status: PomodoroStatus::Idle,
            phase: PomodoroPhase::Focus,
            round: 1,
            phase_ms: 0.0,
            anchor: 0.0,
            focus_done: 0,
            focus_ms: 0.0,
            started: 0.0,
            last: None,
        }
    }
}

impl Timer {
    fn elapsed(&self, now: f64) -> f64 {
        match self.status {
            PomodoroStatus::Running => (now - self.anchor).clamp(0.0, self.phase_ms),
            PomodoroStatus::Paused => self.anchor.min(self.phase_ms),
            PomodoroStatus::Idle => 0.0,
        }
    }

    fn begin(&mut self, phase: PomodoroPhase, s: &PomodoroSettings, now: f64) -> Step {
        self.phase = phase;
        self.phase_ms = f64::from(s.minutes(phase)) * MINUTE_MS;
        self.anchor = now;
        self.started = now;
        self.status = PomodoroStatus::Running;
        Step::Phase
    }

    /// Start a series (idle) or go on (paused).
    fn start(&mut self, s: &PomodoroSettings, now: f64) -> Option<Step> {
        match self.status {
            PomodoroStatus::Idle => {
                *self = Self { last: self.last, ..Self::default() };
                Some(self.begin(PomodoroPhase::Focus, s, now))
            }
            PomodoroStatus::Paused => {
                self.anchor = now - self.anchor;
                self.status = PomodoroStatus::Running;
                None
            }
            PomodoroStatus::Running => None,
        }
    }

    fn pause(&mut self, now: f64) {
        if self.status == PomodoroStatus::Running {
            self.anchor = self.elapsed(now);
            self.status = PomodoroStatus::Paused;
        }
    }

    fn summary(&self, now: f64, complete: bool) -> PomodoroSummary {
        let current = if self.phase == PomodoroPhase::Focus { self.elapsed(now) } else { 0.0 };
        PomodoroSummary {
            focus_done: self.focus_done,
            focus_min: ((self.focus_ms + current) / MINUTE_MS).round() as u32,
            complete,
            ended_ms: now,
        }
    }

    fn stop(&mut self, now: f64) -> Option<Step> {
        if self.status == PomodoroStatus::Idle {
            return None;
        }
        let summary = self.summary(now, false);
        *self = Self { last: Some(summary), ..Self::default() };
        Some(Step::Done(summary))
    }

    /// End the current phase (time is up, or skipped) and start the next.
    fn advance(&mut self, s: &PomodoroSettings, now: f64) -> Option<Step> {
        if self.status == PomodoroStatus::Idle {
            return None;
        }
        match self.phase {
            PomodoroPhase::Focus => {
                self.focus_ms += self.elapsed(now);
                self.focus_done += 1;
                let next =
                    if self.focus_done >= s.rounds { PomodoroPhase::LongBreak } else { PomodoroPhase::ShortBreak };
                Some(self.begin(next, s, now))
            }
            PomodoroPhase::ShortBreak => {
                self.round = (self.round + 1).min(s.rounds);
                Some(self.begin(PomodoroPhase::Focus, s, now))
            }
            PomodoroPhase::LongBreak => {
                let summary = self.summary(now, true);
                *self = Self { last: Some(summary), ..Self::default() };
                Some(Step::Done(summary))
            }
        }
    }

    /// Time is up: the next phase starts now. A long sleep of the PC moves
    /// one phase on, not through every phase that would have passed.
    fn tick(&mut self, s: &PomodoroSettings, now: f64) -> Option<Step> {
        if self.status == PomodoroStatus::Running && now - self.anchor >= self.phase_ms {
            self.advance(s, now)
        } else {
            None
        }
    }

    /// New settings: the current phase takes its new length at once.
    fn settings_changed(&mut self, s: &PomodoroSettings) {
        if self.status != PomodoroStatus::Idle {
            self.phase_ms = f64::from(s.minutes(self.phase)) * MINUTE_MS;
            self.round = self.round.min(s.rounds);
        }
    }

    fn state(&self, settings: PomodoroSettings, now: f64) -> PomodoroState {
        PomodoroState {
            settings,
            status: self.status,
            phase: self.phase,
            round: self.round,
            phase_ms: self.phase_ms,
            remaining_ms: (self.phase_ms - self.elapsed(now)).max(0.0),
            at_ms: now,
            focus_done: self.focus_done,
            last: self.last,
        }
    }

    fn card(&self, s: &PomodoroSettings) -> PomodoroCard {
        PomodoroCard {
            kind: PomodoroCardKind::Phase,
            phase: self.phase,
            round: self.round,
            rounds: s.rounds,
            minutes: s.minutes(self.phase),
            focus_done: self.focus_done,
            focus_min: 0,
            complete: false,
            ended_ms: 0.0,
        }
    }
}

// ----------------------------------------------------------------- lights

/// Which LEDs the Pomodoro uses on a model.
#[derive(Debug, Clone, Default, PartialEq)]
struct KeyMap {
    /// Progress bar, left to right (F1–F12, or the digit row).
    bar: Vec<u8>,
    /// Finished rounds: the digit keys 1–8.
    rounds: Vec<u8>,
    /// Shows the phase colour.
    phase: Option<u8>,
    /// Every key, for the blink.
    all: Vec<u8>,
}

impl KeyMap {
    fn of(model: &Model) -> Self {
        let Some(g) = &model.geometry else { return Self::default() };
        let find = |id: &str| g.keys.iter().find(|k| k.id == id);
        let row = |ids: &[&str]| {
            let mut keys: Vec<_> = ids.iter().filter_map(|id| find(id)).collect();
            keys.sort_by(|a, b| a.x.total_cmp(&b.x));
            keys.into_iter().map(|k| k.led).collect::<Vec<u8>>()
        };
        let digits = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
        let fkeys = ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];
        let fbar = row(&fkeys);
        let (bar, rounds) = if fbar.len() >= 4 {
            (fbar, digits[..8].iter().filter_map(|id| find(id)).map(|k| k.led).collect())
        } else {
            // No function row: the digits carry the bar and the rounds go.
            (row(&digits), Vec::new())
        };
        Self { bar, rounds, phase: find("esc").map(|k| k.led), all: g.keys.iter().map(|k| k.led).collect() }
    }
}

/// The key colours for the timer at `now` (UI colours, before calibration).
/// Empty when nothing should show.
fn light_frame(map: &KeyMap, t: &Timer, rounds: u8, now: f64) -> Vec<(u8, Rgb)> {
    if t.status == PomodoroStatus::Idle {
        return Vec::new();
    }
    let color = phase_color(t.phase);
    let since = now - t.started;
    if (0.0..FLASH_MS).contains(&since) {
        // Two blinks of the whole board in the new phase's colour.
        let on = since < 220.0 || (420.0..640.0).contains(&since);
        return map.all.iter().map(|&l| (l, if on { color } else { [0, 0, 0] })).collect();
    }
    let blink = if t.status == PomodoroStatus::Paused {
        let in_period = (now - t.started).rem_euclid(PAUSE_PERIOD_MS);
        if in_period < PAUSE_PERIOD_MS * 0.6 {
            1.0
        } else {
            PAUSE_LOW
        }
    } else {
        1.0
    };
    let progress = if t.phase_ms > 0.0 { (t.elapsed(now) / t.phase_ms) as f32 } else { 0.0 };
    let filled = progress.clamp(0.0, 1.0) * map.bar.len() as f32;
    let mut out: Vec<(u8, Rgb)> = map.all.iter().map(|&l| (l, [0, 0, 0])).collect();
    let mut set = |led: u8, c: Rgb| {
        if let Some(slot) = out.iter_mut().find(|(l, _)| *l == led) {
            slot.1 = c;
        } else {
            out.push((led, c));
        }
    };
    for (i, &led) in map.bar.iter().enumerate() {
        let part = (filled - i as f32).clamp(0.0, 1.0);
        let part = (part * STEPS).floor() / STEPS;
        set(led, scale(color, (TRACK + (1.0 - TRACK) * part) * blink));
    }
    for (i, &led) in map.rounds.iter().enumerate().take(usize::from(rounds)) {
        let done = i < usize::from(t.focus_done);
        set(led, if done { scale(FOCUS, ROUND) } else { scale(FOCUS, TRACK) });
    }
    if let Some(led) = map.phase {
        set(led, color);
    }
    out
}

/// After the series: the same two blinks in green, then the layer stops.
fn done_frame(map: &KeyMap, ended: f64, now: f64) -> Option<Vec<(u8, Rgb)>> {
    let since = now - ended;
    if !(0.0..FLASH_MS).contains(&since) {
        return None;
    }
    let on = since < 220.0 || (420.0..640.0).contains(&since);
    Some(map.all.iter().map(|&l| (l, if on { SHORT } else { [0, 0, 0] })).collect())
}

// ----------------------------------------------------------------- worker

struct Inner {
    settings: PomodoroSettings,
    timer: Timer,
    /// A phase card is due at this time (after the blink).
    card_at: Option<f64>,
    /// Cards were wanted on the last tick (live mode on, screen switch on).
    cards_wanted: bool,
    /// The series ended at this time: the green blink shows until then + FLASH_MS.
    done_at: Option<f64>,
    /// Last frame handed to the device, to send only changes.
    sent: Option<Vec<(u8, Rgb)>>,
    model: Option<(String, KeyMap)>,
}

enum Msg {
    Wake,
}

/// Handle to the Pomodoro worker (managed Tauri state).
pub struct PomodoroHandle {
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    live: Arc<LiveHandle>,
    config_dir: Option<PathBuf>,
}

impl PomodoroHandle {
    pub fn spawn(
        app: AppHandle,
        live: Arc<LiveHandle>,
        device: Arc<DeviceHandle>,
        config_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let settings = config_dir
            .as_deref()
            .and_then(|d| settings::load::<PomodoroSettings>(d, FILE, VERSION))
            .unwrap_or_default()
            .sanitized();
        let inner = Arc::new(Mutex::new(Inner {
            settings,
            timer: Timer::default(),
            card_at: None,
            cards_wanted: false,
            done_at: None,
            sent: None,
            model: None,
        }));
        let (tx, rx) = mpsc::channel();
        let handle = Arc::new(Self { tx, inner, app, live, config_dir });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("pomodoro".into())
            .spawn(move || loop {
                let wait = worker.tick(&device);
                match rx.recv_timeout(wait) {
                    Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            })
            .expect("spawn pomodoro thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    pub fn state(&self) -> Result<PomodoroState, String> {
        let inner = self.inner.lock().map_err(|_| "pomodoro unavailable")?;
        Ok(inner.timer.state(inner.settings, now_ms()))
    }

    fn emit(&self) {
        if let Ok(s) = self.state() {
            let _ = self.app.emit(EVENT_POMODORO, s);
        }
    }

    pub fn control(&self, action: PomodoroAction) -> Result<PomodoroState, String> {
        let now = now_ms();
        let step = {
            let mut inner = self.inner.lock().map_err(|_| "pomodoro unavailable")?;
            let s = inner.settings;
            match action {
                PomodoroAction::Start => inner.timer.start(&s, now),
                PomodoroAction::Pause => {
                    inner.timer.pause(now);
                    None
                }
                PomodoroAction::Skip => inner.timer.advance(&s, now),
                PomodoroAction::Stop => inner.timer.stop(now),
            }
        };
        tracing::info!(?action, "pomodoro");
        if let Some(step) = step {
            self.stepped(step, now);
        }
        self.wake();
        self.emit();
        self.state()
    }

    pub fn configure(&self, settings: PomodoroSettings) -> Result<PomodoroState, String> {
        let settings = settings.sanitized();
        {
            let mut inner = self.inner.lock().map_err(|_| "pomodoro unavailable")?;
            inner.settings = settings;
            inner.timer.settings_changed(&settings);
        }
        if let Some(dir) = &self.config_dir {
            settings::save(dir, FILE, VERSION, settings).map_err(|e| e.to_string())?;
        }
        self.wake();
        self.emit();
        self.state()
    }

    /// Bookkeeping after the timer moved on.
    fn stepped(&self, step: Step, now: f64) {
        let Ok(mut inner) = self.inner.lock() else { return };
        match step {
            Step::Phase => {
                inner.done_at = None;
                inner.card_at = Some(now + FLASH_MS);
            }
            Step::Done(summary) => {
                inner.card_at = None;
                inner.done_at = Some(now);
                let cards = inner.settings.screen && self.live.enabled();
                drop(inner);
                tracing::info!(complete = summary.complete, rounds = summary.focus_done, "pomodoro series ended");
                // Leave the screen true: the song that plays comes back by
                // itself once the hold ends; otherwise a summary card.
                self.live.drop_pending(SOURCE);
                if cards && self.live.has_card_from(SOURCE) && !self.live.track_playing() {
                    let card = PomodoroCard {
                        kind: PomodoroCardKind::Done,
                        phase: PomodoroPhase::Focus,
                        round: summary.focus_done,
                        rounds: summary.focus_done,
                        minutes: 0,
                        focus_done: summary.focus_done,
                        focus_min: summary.focus_min,
                        complete: summary.complete,
                        ended_ms: summary.ended_ms,
                    };
                    let _ = self.app.emit(EVENT_POMODORO_CARD, card);
                }
            }
        }
    }

    /// One pass of the worker; returns how long to sleep.
    fn tick(&self, device: &DeviceHandle) -> Duration {
        let now = now_ms();
        let live_on = self.live.enabled();
        let step = {
            let Ok(mut inner) = self.inner.lock() else { return IDLE };
            let s = inner.settings;
            inner.timer.tick(&s, now)
        };
        if let Some(step) = step {
            self.stepped(step, now);
            self.emit();
        }

        let Ok(mut inner) = self.inner.lock() else { return IDLE };
        let s = inner.settings;
        let running = inner.timer.status != PomodoroStatus::Idle;

        // Screen: hold it while the timer runs with cards on, and ask for the
        // current phase's card when it starts or when cards were just turned on.
        let wanted = running && s.screen && live_on;
        if wanted && !inner.cards_wanted && inner.card_at.is_none() {
            inner.card_at = Some(now);
        }
        inner.cards_wanted = wanted;
        self.live.set_held(wanted);
        if let Some(at) = inner.card_at {
            if !wanted {
                inner.card_at = None;
            } else if now >= at {
                inner.card_at = None;
                let card = inner.timer.card(&s);
                tracing::debug!(phase = ?card.phase, round = card.round, "pomodoro card wanted");
                let _ = self.app.emit(EVENT_POMODORO_CARD, card);
            }
        }

        // Lights.
        let map = self.key_map(&mut inner, device);
        let frame = match (&map, s.lights) {
            (Some(map), true) if running => Some(light_frame(map, &inner.timer, s.rounds, now)),
            (Some(map), true) => inner.done_at.and_then(|at| done_frame(map, at, now)),
            _ => None,
        }
        .filter(|f| !f.is_empty());
        if frame.is_none() && inner.done_at.is_some_and(|at| now - at >= FLASH_MS) {
            inner.done_at = None;
        }
        if frame != inner.sent {
            let _ = device.send(Request::SetAmbient(frame.clone().map(|colors| LiveFrame { colors, level: 1.0 })));
            inner.sent = frame;
        }

        let lights_on = inner.sent.is_some();
        let mut wait = if lights_on { TICK } else { IDLE };
        if inner.timer.status == PomodoroStatus::Running {
            let left = inner.timer.phase_ms - (now - inner.timer.anchor);
            wait = wait.min(Duration::from_millis(left.clamp(0.0, 1_000.0) as u64));
        }
        if let Some(at) = inner.card_at {
            wait = wait.min(Duration::from_millis((at - now).clamp(0.0, 60_000.0) as u64));
        }
        if inner.done_at.is_some() {
            wait = wait.min(TICK);
        }
        wait
    }

    /// Key layout of the keyboard on the USB cable (the only connection that
    /// takes live frames), cached per model.
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

    fn s() -> PomodoroSettings {
        PomodoroSettings { focus_min: 25, short_min: 5, long_min: 15, rounds: 2, lights: true, screen: true }
    }

    const M: f64 = MINUTE_MS;

    #[test]
    fn a_series_runs_focus_break_focus_long_break() {
        let s = s();
        let mut t = Timer::default();
        assert_eq!(t.start(&s, 0.0), Some(Step::Phase));
        assert_eq!((t.phase, t.round), (PomodoroPhase::Focus, 1));
        assert_eq!(t.tick(&s, 24.0 * M), None);
        assert_eq!(t.tick(&s, 25.0 * M), Some(Step::Phase));
        assert_eq!((t.phase, t.focus_done), (PomodoroPhase::ShortBreak, 1));
        assert_eq!(t.tick(&s, 30.0 * M), Some(Step::Phase));
        assert_eq!((t.phase, t.round), (PomodoroPhase::Focus, 2));
        assert_eq!(t.tick(&s, 55.0 * M), Some(Step::Phase));
        assert_eq!((t.phase, t.focus_done), (PomodoroPhase::LongBreak, 2));
        let Some(Step::Done(sum)) = t.tick(&s, 70.0 * M) else { panic!("series should end") };
        assert_eq!((sum.focus_done, sum.focus_min, sum.complete), (2, 50, true));
        assert_eq!(t.status, PomodoroStatus::Idle);
        assert_eq!(t.last, Some(sum));
    }

    #[test]
    fn pause_keeps_the_time_left() {
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        t.pause(10.0 * M);
        assert_eq!(t.state(s, 40.0 * M).remaining_ms, 15.0 * M);
        assert_eq!(t.tick(&s, 40.0 * M), None);
        assert_eq!(t.start(&s, 40.0 * M), None);
        assert_eq!(t.state(s, 41.0 * M).remaining_ms, 14.0 * M);
        assert_eq!(t.tick(&s, 55.0 * M), Some(Step::Phase));
    }

    #[test]
    fn stop_counts_the_focus_spent_so_far() {
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        t.tick(&s, 25.0 * M);
        t.tick(&s, 30.0 * M);
        let Some(Step::Done(sum)) = t.stop(40.0 * M) else { panic!("stop ends the series") };
        assert_eq!((sum.focus_done, sum.focus_min, sum.complete), (1, 35, false));
        assert_eq!(t.stop(41.0 * M), None);
    }

    #[test]
    fn skipped_focus_counts_only_its_time() {
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        assert_eq!(t.advance(&s, 5.0 * M), Some(Step::Phase));
        assert_eq!((t.phase, t.focus_done), (PomodoroPhase::ShortBreak, 1));
        assert_eq!(t.summary(5.0 * M, false).focus_min, 5);
    }

    #[test]
    fn a_long_sleep_moves_one_phase_on() {
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        assert_eq!(t.tick(&s, 300.0 * M), Some(Step::Phase));
        assert_eq!(t.phase, PomodoroPhase::ShortBreak);
        assert_eq!(t.state(s, 300.0 * M).remaining_ms, 5.0 * M);
    }

    #[test]
    fn new_lengths_apply_to_the_running_phase() {
        let mut s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        s.focus_min = 1;
        t.settings_changed(&s);
        assert_eq!(t.tick(&s, 1.0 * M), Some(Step::Phase));
    }

    #[test]
    fn settings_are_kept_in_range() {
        let s = PomodoroSettings { focus_min: 0, short_min: 500, long_min: 0, rounds: 20, ..s() }.sanitized();
        assert_eq!((s.focus_min, s.short_min, s.long_min, s.rounds), (1, 60, 1, 8));
    }

    fn ak980() -> KeyMap {
        let model = kb_devices::catalogue().unwrap().iter().find(|m| m.id.contains("ak980")).unwrap();
        KeyMap::of(model)
    }

    #[test]
    fn the_bar_is_the_function_row() {
        let map = ak980();
        assert_eq!(map.bar, (2..=13).collect::<Vec<u8>>(), "F1–F12 are LEDs 2–13");
        assert_eq!(map.phase, Some(1));
        assert_eq!(map.rounds.len(), 8);
        assert!(map.all.len() > 90);
    }

    #[test]
    fn the_bar_fills_with_the_phase() {
        let map = ak980();
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        let color = |f: &[(u8, Rgb)], led: u8| f.iter().find(|(l, _)| *l == led).unwrap().1;
        // During the blink the whole board shows the phase colour.
        let f = light_frame(&map, &t, s.rounds, 100.0);
        assert!(f.iter().all(|(_, c)| *c == FOCUS));
        assert_eq!(f.len(), map.all.len());
        // Half way: F1–F6 full, F7–F12 dim, Esc in the phase colour.
        let f = light_frame(&map, &t, s.rounds, 12.5 * M);
        assert_eq!(color(&f, 2), FOCUS);
        assert_eq!(color(&f, 7), FOCUS);
        assert_eq!(color(&f, 8), scale(FOCUS, TRACK));
        assert_eq!(color(&f, 13), scale(FOCUS, TRACK));
        assert_eq!(color(&f, 1), FOCUS);
        // No round finished yet: the digit keys are dim.
        let one = map.rounds[0];
        assert_eq!(color(&f, one), scale(FOCUS, TRACK));
        // In the break the first round is lit and the bar is green.
        t.tick(&s, 25.0 * M);
        let f = light_frame(&map, &t, s.rounds, 26.0 * M);
        assert_eq!(color(&f, one), scale(FOCUS, ROUND));
        assert_eq!(color(&f, 1), SHORT);
        // Idle: nothing.
        t.stop(27.0 * M);
        assert!(light_frame(&map, &t, s.rounds, 27.0 * M).is_empty());
    }

    #[test]
    fn a_paused_bar_blinks_and_a_still_one_does_not_change() {
        let map = ak980();
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        let a = light_frame(&map, &t, s.rounds, 60_000.0);
        let b = light_frame(&map, &t, s.rounds, 60_100.0);
        assert_eq!(a, b, "the same frame within one step");
        t.pause(60_000.0);
        let high = light_frame(&map, &t, s.rounds, 60_000.0 + 100.0);
        let low = light_frame(&map, &t, s.rounds, 60_000.0 + 1_000.0);
        assert_ne!(high, low);
    }

    #[test]
    fn the_series_ends_with_a_green_blink() {
        let map = ak980();
        assert!(done_frame(&map, 0.0, 100.0).unwrap().iter().all(|(_, c)| *c == SHORT));
        assert!(done_frame(&map, 0.0, 300.0).unwrap().iter().all(|(_, c)| *c == [0, 0, 0]));
        assert!(done_frame(&map, 0.0, FLASH_MS).is_none());
    }
}
