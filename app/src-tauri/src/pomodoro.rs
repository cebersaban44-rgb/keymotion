//! Odak zamanlayıcısı (the Pomodoro technique; phase C, second source):
//! a focus/break timer.
//!
//! * The timer runs here, not in the webview, so it keeps time while the
//!   window is hidden (hidden pages get their timers throttled).
//! * The whole keyboard shows the phase as a background live layer (`04 20`,
//!   RAM only, no flash write): it is full in the phase colour when a phase
//!   starts and empties in the chosen direction as time runs out, so the
//!   light left is the time left (owner's design, 2026-11-10). The light
//!   left breathes in the last minute; a paused timer dims and breathes
//!   slowly; a phase starts with two blinks or a wave of the new colour.
//!   It is the timer layer (`device::LightLayer::Timer`): above the system
//!   gauges, which wait while it shows, and below their alerts (volume,
//!   microphone) and the preview the user edits on the Lighting page. When
//!   the timer stops, the gauges come back, or else the keyboard's saved
//!   effect. "Klavyede dene" plays a short run.
//! * When a phase starts the UI is asked for a card (`pomodoro-card`
//!   event), which goes through the live mode like any other source:
//!   budget, newest card wins. While the timer runs it holds the screen, so
//!   "Çalan şarkı" waits. When it ends or is stopped, the song that plays
//!   comes back; otherwise the user's own picture comes back after
//!   `BACK_AFTER`, or a summary card stays, as the user chose
//!   (`PomodoroEndScreen`; owner's request, 2026-11-10).

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{
    Connection, PomodoroCard, PomodoroCardKind, PomodoroEndScreen, PomodoroMotion, PomodoroPhase, PomodoroSettings,
    PomodoroState, PomodoroStatus, PomodoroSummary, PomodoroTransition,
};
use crate::live::{now_ms, LiveHandle};
use crate::settings;
use kb_devices::Model;
use kb_protocol::sonix::{level_factor, Rgb};
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
/// Light frames are worked out this often while they show (the device
/// sends one every 50 ms).
const TICK: Duration = Duration::from_millis(50);
const IDLE: Duration = Duration::from_secs(60);
/// The blink or wave when a phase starts; the card is asked for after it,
/// so the upload (which pauses the live layer) does not cut it short.
const FLASH_MS: f64 = 1_000.0;
/// After the timer stops, wait this long before the user's own picture goes
/// back up: starting again at once costs no upload.
const BACK_AFTER: Duration = Duration::from_secs(15);

/// Keys the light has left: barely lit, so the empty part stays visible.
/// 8 % looked like the light was still waiting there (owner, 2026-11-10).
const TRACK: f32 = 0.03;
/// Width of the edge between lit and dark keys, as a share of the keyboard:
/// the key on the edge dims slowly instead of switching off.
const EDGE: f32 = 0.04;
/// Paused: the whole keyboard dims and breathes slowly between these.
const PAUSE_LOW: f32 = 0.15;
const PAUSE_HIGH: f32 = 0.35;
const PAUSE_PERIOD_MS: f64 = 3_000.0;
/// Last minute: the whole keyboard breathes. The light left dips to this
/// share and the dim part rises to `PULSE_TRACK`, in step, so the pulse is
/// seen even when only a few keys are still lit.
const PULSE_LOW: f32 = 0.45;
const PULSE_TRACK: f32 = 0.22;
const PULSE_PERIOD_MS: f64 = 1_600.0;
/// Light levels change in this many steps, so frames go to the keyboard
/// only when something visible changed.
const STEPS: f32 = 32.0;
/// "Klavyede dene": a focus phase and a break, much shorter.
const TRY_FOCUS_MS: f64 = 8_000.0;
const TRY_BREAK_MS: f64 = 3_000.0;

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
    /// Show a short run on the keys (only while idle).
    Try,
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
            trying: false,
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

/// Where each key sits, as a share (0–1) of the span of the key centres.
#[derive(Debug, Clone, Default, PartialEq)]
struct KeyMap {
    keys: Vec<(u8, f32, f32)>,
}

impl KeyMap {
    fn of(model: &Model) -> Self {
        let Some(g) = &model.geometry else { return Self::default() };
        let centres: Vec<(u8, f32, f32)> = g.keys.iter().map(|k| (k.led, k.x + k.w / 2.0, k.y + k.h / 2.0)).collect();
        let span = |pick: fn(&(u8, f32, f32)) -> f32| {
            let lo = centres.iter().map(pick).fold(f32::INFINITY, f32::min);
            let hi = centres.iter().map(pick).fold(f32::NEG_INFINITY, f32::max);
            (lo, (hi - lo).max(f32::EPSILON))
        };
        let (x0, xs) = span(|k| k.1);
        let (y0, ys) = span(|k| k.2);
        Self { keys: centres.iter().map(|&(l, x, y)| (l, (x - x0) / xs, (y - y0) / ys)).collect() }
    }
}

/// Where a key sits along the drain: keys with a higher value go dark first.
fn drain_order(motion: PomodoroMotion, x: f32, y: f32) -> f32 {
    match motion {
        PomodoroMotion::RightToLeft => x,
        PomodoroMotion::LeftToRight => 1.0 - x,
        PomodoroMotion::TopToBottom => 1.0 - y,
        PomodoroMotion::ToCenter => (x - 0.5).abs() * 2.0,
        PomodoroMotion::Fade => 0.0,
    }
}

/// Light (0–1) of a key at `order` when `left` (0–1) of the phase is left.
fn lit(motion: PomodoroMotion, order: f32, left: f32) -> f32 {
    let left = left.clamp(0.0, 1.0);
    if motion == PomodoroMotion::Fade {
        return left;
    }
    let edge = left * (1.0 + 2.0 * EDGE) - EDGE;
    ((edge - order) / EDGE + 0.5).clamp(0.0, 1.0)
}

/// The last minute of a phase, or its last 15 % when it is short.
fn pulse_window(phase_ms: f64) -> f64 {
    (phase_ms * 0.15).min(60_000.0)
}

/// 0–1, one cycle every `period` ms.
fn breathe(now: f64, period: f64) -> f32 {
    (0.5 + 0.5 * (std::f64::consts::TAU * now / period).cos()) as f32
}

fn scale(c: Rgb, k: f32) -> Rgb {
    let k = ((k.clamp(0.0, 1.0)) * STEPS).round() / STEPS;
    c.map(|v| (f32::from(v) * k).round() as u8)
}

/// The first second of a phase: two blinks of the whole keyboard, or the
/// new colour washing in from the side that stays lit longest.
fn transition_frame(map: &KeyMap, s: &PomodoroSettings, color: Rgb, since: f64) -> Vec<(u8, Rgb)> {
    match s.transition {
        PomodoroTransition::Blink => {
            let on = since < 220.0 || (420.0..640.0).contains(&since);
            map.keys.iter().map(|&(l, _, _)| (l, if on { color } else { [0, 0, 0] })).collect()
        }
        PomodoroTransition::Wave => {
            let fill = (since / FLASH_MS) as f32;
            map.keys
                .iter()
                .map(|&(l, x, y)| {
                    let on = lit(s.motion, drain_order(s.motion, x, y), fill);
                    (l, scale(color, TRACK + (1.0 - TRACK) * on))
                })
                .collect()
        }
    }
}

/// Key colours for the timer at `now` (UI colours, before calibration and
/// brightness). Empty when nothing should show.
fn light_frame(map: &KeyMap, t: &Timer, s: &PomodoroSettings, now: f64) -> Vec<(u8, Rgb)> {
    if t.status == PomodoroStatus::Idle {
        return Vec::new();
    }
    let color = s.color(t.phase);
    let since = now - t.started;
    if (0.0..FLASH_MS).contains(&since) {
        return transition_frame(map, s, color, since);
    }
    let elapsed = t.elapsed(now);
    let left = if t.phase_ms > 0.0 { (1.0 - elapsed / t.phase_ms) as f32 } else { 0.0 };
    let pulsing = s.pulse && t.status == PomodoroStatus::Running && t.phase_ms - elapsed <= pulse_window(t.phase_ms);
    map.keys
        .iter()
        .map(|&(led, x, y)| {
            let on = lit(s.motion, drain_order(s.motion, x, y), left);
            let level = if t.status == PomodoroStatus::Paused {
                (TRACK + (1.0 - TRACK) * on) * (PAUSE_LOW + (PAUSE_HIGH - PAUSE_LOW) * breathe(now, PAUSE_PERIOD_MS))
            } else if pulsing {
                // Dark keys rise and lit keys dip in step: one breath.
                let b = breathe(now, PULSE_PERIOD_MS);
                let track = TRACK + (PULSE_TRACK - TRACK) * b;
                let level = track + (1.0 - track) * on;
                if on > 0.5 {
                    level * (PULSE_LOW + (1.0 - PULSE_LOW) * b)
                } else {
                    level
                }
            } else {
                TRACK + (1.0 - TRACK) * on
            };
            (led, scale(color, level))
        })
        .collect()
}

/// After the series: two blinks in the short break's colour, then the
/// layer stops.
fn done_frame(map: &KeyMap, s: &PomodoroSettings, ended: f64, now: f64) -> Option<Vec<(u8, Rgb)>> {
    let since = now - ended;
    if !(0.0..FLASH_MS).contains(&since) {
        return None;
    }
    let on = since < 220.0 || (420.0..640.0).contains(&since);
    Some(map.keys.iter().map(|&(l, _, _)| (l, if on { s.short_color } else { [0, 0, 0] })).collect())
}

/// "Klavyede dene": the same light as a real series, but a focus phase of
/// 8 s and a break of 3 s. `None` once it is over.
fn try_frame(map: &KeyMap, s: &PomodoroSettings, start: f64, now: f64) -> Option<Vec<(u8, Rgb)>> {
    let t = now - start;
    let phase = |phase, at: f64, len: f64| Timer {
        status: PomodoroStatus::Running,
        phase,
        phase_ms: len,
        anchor: at,
        started: at,
        ..Timer::default()
    };
    if t < 0.0 {
        None
    } else if t < TRY_FOCUS_MS {
        Some(light_frame(map, &phase(PomodoroPhase::Focus, start, TRY_FOCUS_MS), s, now))
    } else if t < TRY_FOCUS_MS + TRY_BREAK_MS {
        Some(light_frame(map, &phase(PomodoroPhase::ShortBreak, start + TRY_FOCUS_MS, TRY_BREAK_MS), s, now))
    } else {
        done_frame(map, s, start + TRY_FOCUS_MS + TRY_BREAK_MS, now)
    }
}

/// The Windows notification sound, without waiting for it.
fn play_sound() {
    std::thread::Builder::new()
        .name("pomodoro-sound".into())
        .spawn(|| {
            if let Err(e) = sound::play() {
                tracing::debug!("notification sound not played: {e}");
            }
        })
        .ok();
}

#[cfg(windows)]
mod sound {
    use windows::core::HSTRING;
    use windows::Foundation::Uri;
    use windows::Media::Core::MediaSource;
    use windows::Media::Playback::MediaPlayer;

    pub fn play() -> windows::core::Result<()> {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let media = std::path::Path::new(&root).join("Media");
        let file = ["Windows Notify System Generic.wav", "Windows Notify.wav"]
            .iter()
            .map(|f| media.join(f))
            .find(|p| p.exists())
            .ok_or_else(|| windows::core::Error::from(windows::core::HRESULT(0x8007_0002_u32 as i32)))?;
        let url = format!("file:///{}", file.to_string_lossy().replace('\\', "/").replace(' ', "%20"));
        let player = MediaPlayer::new()?;
        player.SetSource(&MediaSource::CreateFromUri(&Uri::CreateUri(&HSTRING::from(url))?)?)?;
        player.Play()?;
        // Keep the player alive while the sound plays.
        std::thread::sleep(std::time::Duration::from_secs(3));
        Ok(())
    }
}

#[cfg(not(windows))]
mod sound {
    pub fn play() -> Result<(), String> {
        Err("no notification sound on this system".into())
    }
}

// ----------------------------------------------------------------- worker

struct Inner {
    settings: PomodoroSettings,
    timer: Timer,
    /// A phase card is due at this time (after the blink).
    card_at: Option<f64>,
    /// Cards were wanted on the last tick (live mode on, screen switch on).
    cards_wanted: bool,
    /// The first tick looked for a card left on the screen from before the
    /// app started.
    startup_checked: bool,
    /// The calendar held the screen on the last tick (a meeting's card goes
    /// over the timer's; when it lets go, the timer's card comes back).
    cal_held: bool,
    /// The series ended at this time: the green blink shows until then + FLASH_MS.
    done_at: Option<f64>,
    /// "Klavyede dene" started at this time.
    trying: Option<f64>,
    /// The sound of the try's phase change was played.
    try_sounded: bool,
    /// Last frame (and brightness) handed to the device, to send only changes.
    sent: Option<(Vec<(u8, Rgb)>, u8)>,
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
            startup_checked: false,
            cal_held: false,
            done_at: None,
            trying: None,
            try_sounded: false,
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
        let mut state = inner.timer.state(inner.settings, now_ms());
        state.trying = inner.trying.is_some();
        Ok(state)
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
            if action != PomodoroAction::Try {
                inner.trying = None;
            }
            match action {
                PomodoroAction::Start => {
                    // Started again soon after a stop: the screen stays ours.
                    self.live.cancel_return(SOURCE);
                    inner.timer.start(&s, now)
                }
                PomodoroAction::Pause => {
                    inner.timer.pause(now);
                    None
                }
                PomodoroAction::Skip => inner.timer.advance(&s, now),
                PomodoroAction::Stop => inner.timer.stop(now),
                PomodoroAction::Try => {
                    if inner.timer.status == PomodoroStatus::Idle {
                        inner.trying = Some(now);
                        inner.try_sounded = false;
                        inner.done_at = None;
                    }
                    None
                }
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
                let end = inner.settings.end_screen;
                drop(inner);
                tracing::info!(complete = summary.complete, rounds = summary.focus_done, "pomodoro series ended");
                // Leave the screen true: the song that plays comes back by
                // itself once the hold ends (its card cancels the return if
                // it can go by then); otherwise the user's own picture, or a
                // summary card.
                self.live.drop_pending(SOURCE);
                if !cards || !self.live.has_card_from(SOURCE) {
                    return;
                }
                if end == PomodoroEndScreen::Back || self.live.track_playing() {
                    self.live.return_after(SOURCE, BACK_AFTER);
                } else {
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
        let (step, sound) = {
            let Ok(mut inner) = self.inner.lock() else { return IDLE };
            let s = inner.settings;
            (inner.timer.tick(&s, now), s.sound)
        };
        if let Some(step) = step {
            // The phase ended by itself: the sound is for this moment only
            // (not for buttons the user pressed).
            if sound {
                play_sound();
            }
            self.stepped(step, now);
            self.emit();
        }

        let Ok(mut inner) = self.inner.lock() else { return IDLE };
        let s = inner.settings;
        let running = inner.timer.status != PomodoroStatus::Idle;

        // A phase card left on the screen from before the app started (it
        // was closed while the timer ran, or before the return): the timer
        // does not run now, so the card is untrue. The scheduled return is
        // not kept across restarts, so set it again.
        if !std::mem::replace(&mut inner.startup_checked, true)
            && !running
            && live_on
            && s.end_screen == PomodoroEndScreen::Back
            && self.live.has_card_from(SOURCE)
        {
            tracing::info!("a timer card from before the start is on the screen");
            self.live.return_after(SOURCE, BACK_AFTER);
        }

        // Screen: hold it while the timer runs with cards on, and ask for the
        // current phase's card when it starts or when cards were just turned on.
        let wanted = running && s.screen && live_on;
        if wanted && !inner.cards_wanted && inner.card_at.is_none() {
            inner.card_at = Some(now);
        }
        if !wanted && inner.cards_wanted && running && live_on {
            // Cards turned off while the timer runs: its card leaves the
            // screen like at the end (the song that plays comes back).
            self.live.drop_pending(SOURCE);
            self.live.return_after(SOURCE, BACK_AFTER);
        }
        inner.cards_wanted = wanted;
        self.live.set_hold(SOURCE, wanted);
        // A meeting's card goes over the timer's: phase cards wait while the
        // calendar holds the screen, and the current one comes back after.
        let cal_holds = self.live.held_by(crate::calendar::SOURCE);
        if inner.cal_held && !cal_holds && wanted && inner.card_at.is_none() {
            inner.card_at = Some(now);
        }
        inner.cal_held = cal_holds;
        if let Some(at) = inner.card_at {
            if !wanted {
                inner.card_at = None;
            } else if now >= at && !cal_holds {
                inner.card_at = None;
                let card = inner.timer.card(&s);
                tracing::debug!(phase = ?card.phase, round = card.round, "pomodoro card wanted");
                let _ = self.app.emit(EVENT_POMODORO_CARD, card);
            }
        }

        // Lights.
        let map = self.key_map(&mut inner, device);
        let mut try_ended = false;
        let frame = match (&map, s.lights) {
            (Some(map), true) => {
                if let Some(at) = inner.trying {
                    if s.sound && !inner.try_sounded && now - at >= TRY_FOCUS_MS {
                        inner.try_sounded = true;
                        play_sound();
                    }
                    let f = try_frame(map, &s, at, now);
                    if f.is_none() {
                        inner.trying = None;
                        try_ended = true;
                    }
                    f
                } else if running {
                    Some(light_frame(map, &inner.timer, &s, now))
                } else {
                    inner.done_at.and_then(|at| done_frame(map, &s, at, now))
                }
            }
            _ => {
                // No keyboard on the cable, or lights off: a try has nothing to show.
                try_ended = inner.trying.take().is_some();
                None
            }
        }
        .filter(|f| !f.is_empty())
        .map(|f| (f, s.brightness));
        if frame.is_none() && inner.done_at.is_some_and(|at| now - at >= FLASH_MS) {
            inner.done_at = None;
        }
        if frame != inner.sent {
            let _ = device.send(Request::SetLayer(
                LightLayer::Timer,
                frame.clone().map(|(colors, b)| LiveFrame { colors, level: level_factor(b) }),
            ));
            inner.sent = frame;
        }
        if try_ended {
            drop(inner);
            self.emit();
            return self.tick_wait();
        }

        Self::wait_for(&inner, now)
    }

    fn tick_wait(&self) -> Duration {
        self.inner.lock().map(|inner| Self::wait_for(&inner, now_ms())).unwrap_or(IDLE)
    }

    /// How long the worker may sleep.
    fn wait_for(inner: &Inner, now: f64) -> Duration {
        let lights_on = inner.sent.is_some() || inner.trying.is_some();
        let mut wait = if lights_on { TICK } else { IDLE };
        if inner.timer.status == PomodoroStatus::Running {
            let left = inner.timer.phase_ms - (now - inner.timer.anchor);
            wait = wait.min(Duration::from_millis(left.clamp(0.0, 1_000.0) as u64));
        }
        if let Some(at) = inner.card_at {
            // A due card waiting for the calendar to let go: look again soon.
            let floor = if inner.cal_held { 1_000.0 } else { 0.0 };
            wait = wait.min(Duration::from_millis((at - now).clamp(floor, 60_000.0) as u64));
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
        PomodoroSettings { rounds: 2, ..PomodoroSettings::default() }
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

    fn color(f: &[(u8, Rgb)], led: u8) -> Rgb {
        f.iter().find(|(l, _)| *l == led).unwrap().1
    }

    /// LEDs of the AK980 PRO: Esc 1 (top left), F12 13, Del 119 (top right
    /// of the main block), Num Lock 32, keypad + 123 (right edge).
    const ESC: u8 = 1;
    const KP_PLUS: u8 = 123;

    #[test]
    fn every_key_takes_part() {
        let map = ak980();
        assert_eq!(map.keys.len(), 97);
        assert!(map.keys.iter().all(|&(_, x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)));
    }

    #[test]
    fn a_phase_starts_full_and_empties_in_the_chosen_direction() {
        let map = ak980();
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        // Just after the blink: the whole keyboard in the focus colour.
        let f = light_frame(&map, &t, &s, 2_000.0);
        assert_eq!(f.len(), 97);
        assert!(f.iter().all(|(_, c)| *c == s.focus_color), "full at the start");
        // Right to left (default): half way, the left is lit and the right is dim.
        let f = light_frame(&map, &t, &s, 12.5 * M);
        assert_eq!(color(&f, ESC), s.focus_color);
        assert_eq!(color(&f, KP_PLUS), scale(s.focus_color, TRACK));
        // Left to right: the other way round.
        let ltr = PomodoroSettings { motion: PomodoroMotion::LeftToRight, ..s };
        let f = light_frame(&map, &t, &ltr, 12.5 * M);
        assert_eq!(color(&f, ESC), scale(s.focus_color, TRACK));
        assert_eq!(color(&f, KP_PLUS), s.focus_color);
        // Fade: every key at the same level.
        let fade = PomodoroSettings { motion: PomodoroMotion::Fade, ..s };
        let f = light_frame(&map, &t, &fade, 12.5 * M);
        assert!(f.iter().all(|(_, c)| *c == f[0].1));
        // Nearly over (pulse off): only the dim track is left.
        let still = PomodoroSettings { pulse: false, ..s };
        let f = light_frame(&map, &t, &still, 25.0 * M - 1.0);
        assert!(f.iter().all(|(_, c)| *c == scale(s.focus_color, TRACK)));
    }

    #[test]
    fn top_to_bottom_and_to_center_empty_the_right_keys_first() {
        let map = ak980();
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        let at = 15.0 * M; // 40 % left
        let ttb = PomodoroSettings { motion: PomodoroMotion::TopToBottom, ..s };
        let f = light_frame(&map, &t, &ttb, at);
        let space = map.keys.iter().find(|k| k.2 > 0.99).unwrap().0; // bottom row
        assert_eq!(color(&f, ESC), scale(s.focus_color, TRACK), "top goes dark first");
        assert_eq!(color(&f, space), s.focus_color);
        let mid = PomodoroSettings { motion: PomodoroMotion::ToCenter, ..s };
        let f = light_frame(&map, &t, &mid, at);
        let centre = map.keys.iter().min_by(|a, b| (a.1 - 0.5).abs().total_cmp(&(b.1 - 0.5).abs())).unwrap().0;
        assert_eq!(color(&f, ESC), scale(s.focus_color, TRACK), "edges go dark first");
        assert_eq!(color(&f, centre), s.focus_color);
    }

    #[test]
    fn breaks_use_their_own_colour() {
        let map = ak980();
        let s = PomodoroSettings { short_color: [10, 20, 200], ..s() };
        let mut t = Timer::default();
        t.start(&s, 0.0);
        t.tick(&s, 25.0 * M);
        let f = light_frame(&map, &t, &s, 25.0 * M + 2_000.0);
        assert!(f.iter().all(|(_, c)| *c == [10, 20, 200]));
    }

    #[test]
    fn the_last_minute_breathes_and_a_pause_dims() {
        let map = ak980();
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        // Last minute: the whole keyboard breathes, the dark part too.
        let a = light_frame(&map, &t, &s, 24.5 * M);
        let b = light_frame(&map, &t, &s, 24.5 * M + PULSE_PERIOD_MS / 4.0);
        assert_ne!(color(&a, KP_PLUS), color(&b, KP_PLUS));
        // With the pulse off it does not.
        let still = PomodoroSettings { pulse: false, ..s };
        let a = light_frame(&map, &t, &still, 24.5 * M);
        let b = light_frame(&map, &t, &still, 24.5 * M + PULSE_PERIOD_MS / 4.0);
        assert_eq!(color(&a, KP_PLUS), color(&b, KP_PLUS));
        // Earlier the frame stays the same within a step.
        let a = light_frame(&map, &t, &s, 5.0 * M);
        let b = light_frame(&map, &t, &s, 5.0 * M + 100.0);
        assert_eq!(a, b);
        // Paused: never brighter than PAUSE_HIGH of the colour.
        t.pause(5.0 * M);
        let f = light_frame(&map, &t, &s, 6.0 * M);
        let max = f32::from(s.focus_color[0]) * PAUSE_HIGH + 1.0;
        assert!(f.iter().all(|(_, c)| f32::from(c[0]) <= max));
    }

    #[test]
    fn a_phase_starts_with_a_blink_or_a_wave() {
        let map = ak980();
        let s = s();
        let mut t = Timer::default();
        t.start(&s, 0.0);
        assert!(light_frame(&map, &t, &s, 100.0).iter().all(|(_, c)| *c == s.focus_color));
        assert!(light_frame(&map, &t, &s, 300.0).iter().all(|(_, c)| *c == [0, 0, 0]));
        // Wave (right to left): the left fills first.
        let wave = PomodoroSettings { transition: PomodoroTransition::Wave, ..s };
        let f = light_frame(&map, &t, &wave, 400.0);
        assert_eq!(color(&f, ESC), s.focus_color);
        assert_eq!(color(&f, KP_PLUS), scale(s.focus_color, TRACK));
    }

    #[test]
    fn idle_shows_nothing_and_the_series_ends_with_two_blinks() {
        let map = ak980();
        let s = s();
        assert!(light_frame(&map, &Timer::default(), &s, 0.0).is_empty());
        assert!(done_frame(&map, &s, 0.0, 100.0).unwrap().iter().all(|(_, c)| *c == s.short_color));
        assert!(done_frame(&map, &s, 0.0, 300.0).unwrap().iter().all(|(_, c)| *c == [0, 0, 0]));
        assert!(done_frame(&map, &s, 0.0, FLASH_MS).is_none());
    }

    #[test]
    fn the_try_runs_focus_then_break_then_stops() {
        let map = ak980();
        let s = s();
        assert!(try_frame(&map, &s, 0.0, 500.0).unwrap().iter().all(|(_, c)| *c == s.focus_color));
        let f = try_frame(&map, &s, 0.0, TRY_FOCUS_MS + 1_500.0).unwrap();
        assert_eq!(color(&f, ESC), s.short_color);
        assert!(try_frame(&map, &s, 0.0, TRY_FOCUS_MS + TRY_BREAK_MS + 100.0).is_some(), "end blink");
        assert!(try_frame(&map, &s, 0.0, TRY_FOCUS_MS + TRY_BREAK_MS + FLASH_MS).is_none());
    }

    #[test]
    fn old_settings_files_get_the_new_light_settings() {
        let old = r#"{"focusMin":30,"shortMin":5,"longMin":15,"rounds":4,"lights":true,"screen":false}"#;
        let s: PomodoroSettings = serde_json::from_str(old).unwrap();
        assert_eq!(s.focus_min, 30);
        assert!(!s.screen);
        assert_eq!(s.motion, PomodoroMotion::RightToLeft);
        assert_eq!(s.focus_color, PomodoroSettings::default().focus_color);
        assert_eq!(s.brightness, 4);
    }
}
