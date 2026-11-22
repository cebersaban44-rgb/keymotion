//! Running in the background.
//!
//! Closing the window hides it, and the app goes on beside the clock: the
//! focus timer, reminders, calendar cards, the live screen and the key
//! gauges only work while it runs. The tray icon brings the window back
//! (left click) and has a menu:
//!
//! * while a new version waits, "Update available: Keymotion 1.1.0" (or,
//!   downloaded by "update by itself", "Install now: …"), which opens the
//!   update dialog; the icon then carries a dot (updates.rs);
//! * the keyboard and its connection (and battery, without the cable);
//! * Open Keymotion;
//! * Focus timer ▸ the phase and the minutes left, start / pause / resume,
//!   skip to the next phase, stop;
//! * Reminders ▸ the next one, on / off;
//! * System gauges on the keys (on / off; RAM only, no flash write);
//! * Profiles ▸ each profile, the keyboard's own ticked;
//! * Quit.
//!
//! "Quit" ends the app the way closing the window did before: it asks
//! about unsaved changes and waits for a flash write. A profile chosen in
//! the menu is not loaded from here: the window opens on the Profiles page
//! with that profile's confirmation, since every flash write needs the
//! user's yes.
//!
//! The menu's words come from the UI (`tray_texts`) in its language,
//! English until the UI has sent them. The menu is made once; later only
//! its items change (texts, ticks, the profile list), so an open menu stays
//! open and its minutes count down.

use crate::device::{DeviceHandle, EVENT_KEYBOARDS_CHANGED};
use crate::dto::{
    BackgroundSettings, Connection, KeyboardInfo, PomodoroPhase, PomodoroState, PomodoroStatus, ReminderState,
    TrayTexts,
};
use crate::games::{GamesHandle, EVENT_GAMES};
use crate::gauges::{GaugesHandle, EVENT_GAUGES};
use crate::pomodoro::{PomodoroAction, PomodoroHandle, EVENT_POMODORO};
use crate::reminders::{RemindersHandle, EVENT_REMINDERS};
use crate::updates::{Updates, EVENT_UPDATE};
use crate::{notify, settings};
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager, Wry};

const TRAY_ID: &str = "main";
/// Windows starts the app with this when it starts with Windows: the
/// window stays hidden, only the tray icon shows.
pub const ARG_HIDDEN: &str = "--hidden";
/// The value's name in the `Run` key.
pub const RUN_NAME: &str = "Keymotion";
const FILE: &str = "background.json";
const VERSION: u32 = 1;

/// The UI opens the Profiles page on this profile's confirmation.
pub const EVENT_TRAY_PROFILE: &str = "tray-profile";
/// The window was shown (`true`) or hidden (`false`).
pub const EVENT_WINDOW_SHOWN: &str = "window-shown";

const PROFILE_PREFIX: &str = "profile:";
/// How often the minutes are brought up to date while the focus timer
/// runs; otherwise the menu changes when something happens (and hourly,
/// for the reminder's "tomorrow").
const TICK_RUNNING: Duration = Duration::from_secs(10);
const TICK_IDLE: Duration = Duration::from_secs(600);
/// Changes that come together (a profile load writes several parts) are
/// shown at once.
const SETTLE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Stored {
    close_to_tray: bool,
    // Older files also hold `noticeShown` (the notice used to come once per
    // computer); it is read and ignored.
}

impl Default for Stored {
    fn default() -> Self {
        Self { close_to_tray: true }
    }
}

/// Background settings and the tray's state (managed Tauri state).
pub struct Background {
    stored: Mutex<Stored>,
    texts: Mutex<TrayTexts>,
    dir: Option<PathBuf>,
    /// "Quit" was chosen: the next close ends the app instead of hiding it.
    quitting: AtomicBool,
    /// The "still running" notice was shown in this run of the app.
    notice_given: AtomicBool,
    /// The UI has sent its words (until then they are English).
    texts_received: AtomicBool,
    refresh: Mutex<Option<Sender<()>>>,
}

impl Background {
    pub fn open(dir: Option<PathBuf>) -> Arc<Self> {
        let stored = dir.as_deref().and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).unwrap_or_default();
        Arc::new(Self {
            stored: Mutex::new(stored),
            texts: Mutex::new(TrayTexts::default()),
            dir,
            quitting: AtomicBool::new(false),
            notice_given: AtomicBool::new(false),
            texts_received: AtomicBool::new(false),
            refresh: Mutex::new(None),
        })
    }

    fn stored(&self) -> Stored {
        self.stored.lock().map(|s| *s).unwrap_or_default()
    }

    fn save(&self, next: Stored) -> Result<(), String> {
        if let Some(dir) = &self.dir {
            settings::save(dir, FILE, VERSION, next).map_err(|e| format!("settings not saved: {e}"))?;
        }
        if let Ok(mut s) = self.stored.lock() {
            *s = next;
        }
        Ok(())
    }

    pub fn settings(&self) -> BackgroundSettings {
        let close_to_tray = self.stored().close_to_tray;
        if crate::channel::store() {
            use kb_winsys::startup::TaskState;
            let state = kb_winsys::startup::task_state(crate::channel::STARTUP_TASK)
                .map_err(|e| tracing::warn!("startup task not read: {e}"))
                .ok();
            return BackgroundSettings {
                close_to_tray,
                start_with_windows: state.is_some_and(TaskState::starts),
                start_turned_off: false,
                start_locked: matches!(
                    state,
                    Some(TaskState::OffByUser | TaskState::OffByPolicy | TaskState::OnByPolicy)
                ),
            };
        }
        let entry = kb_winsys::startup::entry(RUN_NAME);
        BackgroundSettings {
            close_to_tray,
            start_with_windows: entry.is_some(),
            start_turned_off: entry.is_some_and(|e| e.disabled),
            start_locked: false,
        }
    }

    /// The Store build's "start with Windows": its package's startup task.
    fn configure_task(&self, now: BackgroundSettings, wanted: bool) -> Result<(), String> {
        if wanted == now.start_with_windows || now.start_locked {
            return Ok(());
        }
        if wanted {
            let state = kb_winsys::startup::task_enable(crate::channel::STARTUP_TASK)?;
            tracing::info!(?state, "startup task");
        } else {
            kb_winsys::startup::task_disable(crate::channel::STARTUP_TASK)?;
            tracing::info!("startup task turned off");
        }
        Ok(())
    }

    /// The GitHub copy handed its start with Windows to this one (the Store
    /// build; handoff.rs): turn the startup task on, unless the user or a
    /// policy decides it.
    pub fn take_startup(&self) {
        if !crate::channel::store() {
            return;
        }
        match self.configure_task(self.settings(), true) {
            Ok(()) => tracing::info!(
                start_with_windows = self.settings().start_with_windows,
                "start with Windows taken over from the GitHub copy"
            ),
            Err(e) => tracing::warn!("start with Windows not taken over: {e}"),
        }
    }

    /// Store the choices; starting with Windows writes or removes the `Run`
    /// entry (for this exe, started hidden), or in the Store build turns
    /// the package's startup task on or off.
    pub fn configure(&self, wanted: BackgroundSettings) -> Result<BackgroundSettings, String> {
        let now = self.settings();
        if wanted.close_to_tray != now.close_to_tray {
            self.save(Stored { close_to_tray: wanted.close_to_tray })?;
            tracing::info!(close_to_tray = wanted.close_to_tray, "background setting");
        }
        if crate::channel::store() {
            self.configure_task(now, wanted.start_with_windows)?;
            return Ok(self.settings());
        }
        let fix = wanted.start_with_windows && now.start_turned_off;
        if wanted.start_with_windows != now.start_with_windows || fix {
            if wanted.start_with_windows {
                let exe = std::env::current_exe().map_err(|e| format!("program path unknown: {e}"))?;
                kb_winsys::startup::set(RUN_NAME, &kb_winsys::startup::command_line(&exe, &[ARG_HIDDEN]))?;
            } else {
                kb_winsys::startup::remove(RUN_NAME)?;
            }
            tracing::info!(start_with_windows = wanted.start_with_windows, "background setting");
        }
        Ok(self.settings())
    }

    pub fn set_texts(&self, texts: TrayTexts) {
        if let Ok(mut t) = self.texts.lock() {
            if t.locale != texts.locale {
                tracing::info!(locale = %texts.locale, "tray menu language");
            }
            *t = texts;
        }
        self.texts_received.store(true, Ordering::SeqCst);
    }

    pub fn texts_received(&self) -> bool {
        self.texts_received.load(Ordering::SeqCst)
    }

    pub fn texts(&self) -> TrayTexts {
        self.texts.lock().map(|t| t.clone()).unwrap_or_default()
    }

    pub fn quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }

    /// The user stayed after "Quit" asked about unsaved changes.
    pub fn quit_cancelled(&self) {
        self.quitting.store(false, Ordering::SeqCst);
    }

    /// True once per run of the app: the first time the window is hidden
    /// (owner's decision, 2026-11-12). Started hidden with Windows, the
    /// window is not closed until the user opens it, so no notice comes then.
    fn take_notice(&self) -> bool {
        !self.notice_given.swap(true, Ordering::SeqCst)
    }
}

/// Bring the main window back: shown, restored, in front.
pub fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else { return };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit(EVENT_WINDOW_SHOWN, true);
}

/// The window's close button (or Alt+F4): hide the window instead, when the
/// user keeps the app in the tray and has not chosen "Quit". Returns
/// whether it was hidden.
pub fn hide_on_close(window: &tauri::Window) -> bool {
    let app = window.app_handle();
    let Some(bg) = app.try_state::<Arc<Background>>() else { return false };
    if bg.quitting() || !bg.stored().close_to_tray {
        return false;
    }
    if let Err(e) = window.hide() {
        tracing::warn!("window not hidden: {e}");
        return false;
    }
    let _ = app.emit(EVENT_WINDOW_SHOWN, false);
    tracing::info!("window hidden; running in the tray");
    if bg.take_notice() {
        let texts = bg.texts();
        let app_id = notify::app_id(&app.config().identifier);
        std::thread::spawn(move || {
            if let Err(e) = notify::show(&app_id, &texts.hidden_title, &texts.hidden_body) {
                tracing::warn!("background notice not shown: {e}");
            }
        });
    }
    true
}

/// "Quit": close the window for real. Closing asks first when something is
/// unsaved or a flash write runs (`lib.rs`), so the window is shown then.
fn quit(app: &AppHandle) {
    let bg = app.state::<Arc<Background>>();
    bg.quitting.store(true, Ordering::SeqCst);
    tracing::info!("quit chosen in the tray");
    let Some(window) = app.get_webview_window("main") else {
        app.exit(0);
        return;
    };
    if crate::close_needs_asking(app) {
        show_main(app);
    }
    if let Err(e) = window.close() {
        tracing::warn!("window not closed: {e}");
        app.exit(0);
    }
}

/// Bring the menu up to date soon (the profiles, the focus timer or the
/// words changed).
pub fn refresh(app: &AppHandle) {
    let Some(bg) = app.try_state::<Arc<Background>>() else { return };
    let tx = bg.refresh.lock().ok().and_then(|tx| tx.clone());
    if let Some(tx) = tx {
        let _ = tx.send(());
    }
}

// ------------------------------------------------------------------- view

/// The keyboard line's facts.
#[derive(Debug, Clone, PartialEq)]
struct KeyboardLine {
    name: String,
    connection: Connection,
    battery: Option<u8>,
}

/// What the menu is made from.
#[derive(Debug, Clone, Default)]
struct Facts {
    keyboard: Option<KeyboardLine>,
    pomodoro: Option<PomodoroState>,
    /// Reminders on, and the next one (when, title).
    reminders: Option<(bool, Option<(f64, String)>)>,
    gauges: Option<bool>,
    /// Id, name (empty: the built-in one) and whether the keyboard has it.
    profiles: Vec<(String, String, bool)>,
    /// A new version waits: its number, and whether it is downloaded.
    update: Option<(String, bool)>,
    /// A game session runs: the game and when it started.
    game: Option<(String, f64)>,
    /// Per-app profiles run (on and not paused).
    auto: bool,
}

/// Everything the menu shows, as text: what changed is set on its item.
#[derive(Debug, Clone, PartialEq, Default)]
struct View {
    /// The first line while a new version waits.
    update: Option<String>,
    keyboard: String,
    /// The line under the keyboard's while a game session runs.
    game: Option<String>,
    open: String,
    focus: String,
    focus_status: String,
    focus_toggle: String,
    /// Skip and stop are usable (the timer runs or is paused).
    focus_active: bool,
    focus_skip: String,
    focus_stop: String,
    reminders: String,
    reminders_next: String,
    reminders_on_label: String,
    reminders_on: bool,
    gauges_label: String,
    gauges_on: bool,
    profiles_label: String,
    profiles: Vec<(String, String, bool)>,
    auto_label: String,
    auto_on: bool,
    quit: String,
    tooltip: String,
}

fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    pairs.iter().fold(template.to_string(), |s, (k, v)| s.replace(&format!("{{{k}}}"), v))
}

/// Minutes left in the phase, rounded up, at least 1 while it runs.
fn minutes_left(p: &PomodoroState, now_ms: f64) -> u32 {
    let left = if p.status == PomodoroStatus::Running { p.remaining_ms - (now_ms - p.at_ms) } else { p.remaining_ms };
    (left.max(0.0) / 60_000.0).ceil().max(1.0) as u32
}

/// "Focus · Block 3/6 · 18 min left", "Paused · Break · 3 min left",
/// "Ready: 6 blocks, 100 min".
fn focus_status(t: &TrayTexts, p: &PomodoroState, now_ms: f64) -> String {
    if p.status == PomodoroStatus::Idle {
        let (blocks, min) = (p.settings.blocks.len().to_string(), p.settings.total_minutes().to_string());
        return fill(&t.ready, &[("blocks", &blocks), ("min", &min)]);
    }
    let mut parts = Vec::new();
    if p.status == PomodoroStatus::Paused {
        parts.push(t.paused.clone());
    }
    parts.push(match p.phase {
        PomodoroPhase::Focus => t.phase_focus.clone(),
        PomodoroPhase::Break => t.phase_break.clone(),
    });
    if p.settings.blocks.len() > 1 {
        let (n, total) = (p.block.to_string(), p.settings.blocks.len().to_string());
        parts.push(fill(&t.round, &[("n", &n), ("total", &total)]));
    }
    parts.push(fill(&t.left, &[("min", &minutes_left(p, now_ms).to_string())]));
    parts.join(" · ")
}

/// "14:30" today, "tomorrow 09:00", "14.11 09:00" (Turkish) or
/// "Nov 14 09:00" later.
fn when(t: &TrayTexts, at_ms: f64, now_ms: f64, tz: &TimeZone) -> String {
    let zoned = |ms: f64| -> Option<Zoned> { Some(Timestamp::from_millisecond(ms as i64).ok()?.to_zoned(tz.clone())) };
    let (Some(at), Some(now)) = (zoned(at_ms), zoned(now_ms)) else { return String::new() };
    let time = at.strftime("%H:%M").to_string();
    let days = at.date().since(now.date()).map_or(0, |s| s.get_days());
    match days {
        ..=0 => time,
        1 => format!("{} {time}", t.tomorrow),
        _ if t.locale == "tr" => format!("{} {time}", at.strftime("%d.%m")),
        _ => format!("{} {time}", at.strftime("%b %-d")),
    }
}

fn connection_word(t: &TrayTexts, c: Connection) -> &str {
    match c {
        Connection::Usb => &t.conn_usb,
        Connection::Dongle => &t.conn_dongle,
        Connection::Bluetooth => &t.conn_bluetooth,
    }
}

fn view_of(t: &TrayTexts, f: &Facts, now_ms: f64, tz: &TimeZone) -> View {
    let keyboard = match &f.keyboard {
        None => t.no_keyboard.clone(),
        Some(k) => {
            let mut parts = vec![k.name.clone(), connection_word(t, k.connection).to_string()];
            if let Some(n) = k.battery.filter(|_| k.connection != Connection::Usb) {
                parts.push(fill(&t.battery, &[("n", &n.to_string())]));
            }
            parts.join(" · ")
        }
    };
    let status = f.pomodoro.as_ref().map(|p| p.status).unwrap_or_default();
    let focus_line = f.pomodoro.as_ref().map(|p| focus_status(t, p, now_ms)).unwrap_or_default();
    let (reminders_on, next) = f.reminders.clone().unwrap_or((false, None));
    let reminders_next = match (reminders_on, next) {
        (false, _) => t.reminders_off.clone(),
        (true, None) => t.reminders_none.clone(),
        (true, Some((at, title))) => {
            let title = if title.trim().is_empty() { "—".to_string() } else { title };
            fill(&t.reminders_next, &[("time", &when(t, at, now_ms, tz)), ("title", &title)])
        }
    };
    let update = f.update.as_ref().map(|(version, ready)| {
        fill(if *ready { &t.update_ready } else { &t.update_available }, &[("version", version)])
    });
    let game = f.game.as_ref().map(|(name, started)| {
        // As on the page: "1 h 5 min", "42 min", "under 1 min".
        let ms = (now_ms - started).max(0.0);
        let total = (ms / 60_000.0).floor() as u64;
        let (h, min) = ((total / 60).to_string(), (total % 60).to_string());
        let time = if ms < 60_000.0 {
            t.game_less.clone()
        } else if total >= 60 {
            fill(&t.game_hours, &[("h", &h), ("min", &min)])
        } else {
            fill(&t.game_minutes, &[("min", &min)])
        };
        fill(&t.game, &[("name", name), ("time", &time)])
    });
    let mut tooltip =
        if status == PomodoroStatus::Idle { "Keymotion".to_string() } else { format!("Keymotion — {focus_line}") };
    if let Some(g) = &game {
        tooltip = format!("{tooltip} · {g}");
    }
    if f.update.is_some() {
        tooltip = format!("{tooltip} · {}", t.update_tooltip);
    }
    View {
        update,
        keyboard,
        game,
        open: t.open.clone(),
        focus: t.focus.clone(),
        focus_status: focus_line.clone(),
        focus_toggle: match status {
            PomodoroStatus::Idle => t.focus_start.clone(),
            PomodoroStatus::Running => t.focus_pause.clone(),
            PomodoroStatus::Paused => t.focus_resume.clone(),
        },
        focus_active: status != PomodoroStatus::Idle,
        focus_skip: t.focus_skip.clone(),
        focus_stop: t.focus_stop.clone(),
        reminders: t.reminders.clone(),
        reminders_next,
        reminders_on_label: t.reminders_on.clone(),
        reminders_on,
        gauges_label: t.gauges.clone(),
        gauges_on: f.gauges.unwrap_or(false),
        profiles_label: t.profiles.clone(),
        profiles: f
            .profiles
            .iter()
            .map(|(id, name, on)| {
                (id.clone(), if name.is_empty() { t.default_profile.clone() } else { name.clone() }, *on)
            })
            .collect(),
        auto_label: t.auto_profile.clone(),
        auto_on: f.auto,
        quit: t.quit.clone(),
        tooltip,
    }
}

/// The keyboard the UI shows (USB first, as `useActiveKeyboard`).
fn keyboard_line(app: &AppHandle) -> Option<KeyboardLine> {
    let device = app.try_state::<Arc<DeviceHandle>>()?;
    let snap = device.snapshot.lock().ok()?;
    let rank = |k: &&KeyboardInfo| match k.connection {
        Connection::Usb => 0,
        Connection::Dongle => 1,
        Connection::Bluetooth => 2,
    };
    let k = snap.keyboards.iter().min_by_key(rank)?;
    let name = if k.model_name.starts_with(&k.vendor) {
        k.model_name.clone()
    } else {
        format!("{} {}", k.vendor, k.model_name)
    };
    Some(KeyboardLine { name, connection: k.connection, battery: k.battery.as_ref().map(|b| b.percent) })
}

fn next_reminder(s: &ReminderState) -> Option<(f64, String)> {
    s.reminders
        .iter()
        .filter(|r| r.reminder.enabled)
        .filter_map(|r| r.next_ms.map(|at| (at, r.reminder.title.clone())))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

/// The new version waiting, if any (updates.rs).
fn update_line(app: &AppHandle) -> Option<(String, bool)> {
    match app.try_state::<Arc<Updates>>()?.status() {
        crate::dto::UpdateStatus::Available { version, .. } => Some((version, false)),
        crate::dto::UpdateStatus::Ready { version, .. } => Some((version, true)),
        _ => None,
    }
}

fn facts(app: &AppHandle) -> Facts {
    Facts {
        update: update_line(app),
        game: app.try_state::<Arc<GamesHandle>>().and_then(|g| g.session()),
        auto: app.try_state::<Arc<crate::autoprofile::AutoProfiles>>().and_then(|a| a.running()).unwrap_or(false),
        keyboard: keyboard_line(app),
        pomodoro: app.try_state::<Arc<PomodoroHandle>>().and_then(|p| p.state().ok()),
        reminders: app
            .try_state::<Arc<RemindersHandle>>()
            .and_then(|r| r.state().ok())
            .map(|s| (s.settings.enabled, next_reminder(&s))),
        gauges: app.try_state::<Arc<GaugesHandle>>().and_then(|g| g.state().ok()).map(|s| s.settings.enabled),
        profiles: crate::tray_profiles(app)
            .map(|(state, connected)| {
                std::iter::once(state.factory)
                    .chain(state.profiles)
                    .map(|p| (p.id, if p.built_in { String::new() } else { p.name }, connected && p.on_keyboard))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// ------------------------------------------------------------------- menu

/// `&` would underline the next letter in a Windows menu.
fn menu_text(s: &str) -> String {
    s.replace('&', "&&")
}

/// The menu's items, kept to change them in place.
struct Items {
    menu: Menu<Wry>,
    /// The update line and the line under it: in the menu only while a
    /// new version waits.
    update: MenuItem<Wry>,
    update_sep: PredefinedMenuItem<Wry>,
    keyboard: MenuItem<Wry>,
    /// Under the keyboard's line while a game session runs.
    game: MenuItem<Wry>,
    open: MenuItem<Wry>,
    focus: Submenu<Wry>,
    focus_status: MenuItem<Wry>,
    focus_toggle: MenuItem<Wry>,
    focus_skip: MenuItem<Wry>,
    focus_stop: MenuItem<Wry>,
    reminders: Submenu<Wry>,
    reminders_next: MenuItem<Wry>,
    reminders_on: CheckMenuItem<Wry>,
    gauges: CheckMenuItem<Wry>,
    profiles: Submenu<Wry>,
    /// First in Profiles: the per-app profiles' switch.
    auto: CheckMenuItem<Wry>,
    profile_items: Vec<CheckMenuItem<Wry>>,
    quit: MenuItem<Wry>,
}

fn item(app: &AppHandle, id: &str, text: &str, enabled: bool) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, id, menu_text(text), enabled, None::<&str>)
}

fn profile_items(app: &AppHandle, v: &View) -> tauri::Result<Vec<CheckMenuItem<Wry>>> {
    v.profiles
        .iter()
        .map(|(id, name, on)| {
            CheckMenuItem::with_id(app, format!("{PROFILE_PREFIX}{id}"), menu_text(name), true, *on, None::<&str>)
        })
        .collect()
}

fn build(app: &AppHandle, v: &View) -> tauri::Result<(Menu<Wry>, Items)> {
    let focus_status = item(app, "focus.status", &v.focus_status, false)?;
    let focus_toggle = item(app, "focus.toggle", &v.focus_toggle, true)?;
    let focus_skip = item(app, "focus.skip", &v.focus_skip, v.focus_active)?;
    let focus_stop = item(app, "focus.stop", &v.focus_stop, v.focus_active)?;
    let focus = Submenu::with_items(
        app,
        menu_text(&v.focus),
        true,
        &[&focus_status, &PredefinedMenuItem::separator(app)?, &focus_toggle, &focus_skip, &focus_stop],
    )?;
    let reminders_next = item(app, "reminders.next", &v.reminders_next, false)?;
    let reminders_on = CheckMenuItem::with_id(
        app,
        "reminders.on",
        menu_text(&v.reminders_on_label),
        true,
        v.reminders_on,
        None::<&str>,
    )?;
    let reminders = Submenu::with_items(
        app,
        menu_text(&v.reminders),
        true,
        &[&reminders_next, &PredefinedMenuItem::separator(app)?, &reminders_on],
    )?;
    let gauges = CheckMenuItem::with_id(app, "gauges", menu_text(&v.gauges_label), true, v.gauges_on, None::<&str>)?;
    let items = profile_items(app, v)?;
    let auto = CheckMenuItem::with_id(app, "auto", menu_text(&v.auto_label), true, v.auto_on, None::<&str>)?;
    let auto_sep = PredefinedMenuItem::separator(app)?;
    let mut refs: Vec<&dyn IsMenuItem<Wry>> = vec![&auto, &auto_sep];
    refs.extend(items.iter().map(|i| i as &dyn IsMenuItem<Wry>));
    let profiles = Submenu::with_items(app, menu_text(&v.profiles_label), true, &refs)?;
    let keyboard = item(app, "keyboard", &v.keyboard, false)?;
    let open = item(app, "open", &v.open, true)?;
    let quit = item(app, "quit", &v.quit, true)?;
    let update = item(app, "update", v.update.as_deref().unwrap_or_default(), true)?;
    let update_sep = PredefinedMenuItem::separator(app)?;
    let game = item(app, "game", v.game.as_deref().unwrap_or_default(), false)?;
    let menu = Menu::with_items(
        app,
        &[
            &keyboard,
            &open,
            &PredefinedMenuItem::separator(app)?,
            &focus,
            &reminders,
            &gauges,
            &profiles,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    if v.game.is_some() {
        menu.insert(&game, 1)?;
    }
    if v.update.is_some() {
        menu.insert(&update, 0)?;
        menu.insert(&update_sep, 1)?;
    }
    Ok((
        menu.clone(),
        Items {
            menu,
            update,
            update_sep,
            keyboard,
            game,
            open,
            focus,
            focus_status,
            focus_toggle,
            focus_skip,
            focus_stop,
            reminders,
            reminders_next,
            reminders_on,
            gauges,
            profiles,
            auto,
            profile_items: items,
            quit,
        },
    ))
}

/// Change the items whose text or state differs from what is shown. Ticks
/// are set every time: a check item ticks itself when clicked.
fn apply(app: &AppHandle, items: &mut Items, old: &View, new: &View) -> tauri::Result<()> {
    macro_rules! text {
        ($item:expr, $field:ident) => {
            if old.$field != new.$field {
                $item.set_text(menu_text(&new.$field))?;
            }
        };
    }
    match (&old.update, &new.update) {
        (None, Some(line)) => {
            items.update.set_text(menu_text(line))?;
            items.menu.insert(&items.update, 0)?;
            items.menu.insert(&items.update_sep, 1)?;
        }
        (Some(_), None) => {
            items.menu.remove(&items.update)?;
            items.menu.remove(&items.update_sep)?;
        }
        (Some(a), Some(b)) if a != b => items.update.set_text(menu_text(b))?,
        _ => {}
    }
    match (&old.game, &new.game) {
        (None, Some(line)) => {
            items.game.set_text(menu_text(line))?;
            // Under the keyboard's line, which follows the update lines.
            items.menu.insert(&items.game, if new.update.is_some() { 3 } else { 1 })?;
        }
        (Some(_), None) => items.menu.remove(&items.game)?,
        (Some(a), Some(b)) if a != b => items.game.set_text(menu_text(b))?,
        _ => {}
    }
    text!(items.keyboard, keyboard);
    text!(items.open, open);
    text!(items.focus, focus);
    text!(items.focus_status, focus_status);
    text!(items.focus_toggle, focus_toggle);
    text!(items.focus_skip, focus_skip);
    text!(items.focus_stop, focus_stop);
    text!(items.reminders, reminders);
    text!(items.reminders_next, reminders_next);
    text!(items.reminders_on, reminders_on_label);
    text!(items.gauges, gauges_label);
    text!(items.profiles, profiles_label);
    text!(items.auto, auto_label);
    text!(items.quit, quit);
    if old.focus_active != new.focus_active {
        items.focus_skip.set_enabled(new.focus_active)?;
        items.focus_stop.set_enabled(new.focus_active)?;
    }
    items.reminders_on.set_checked(new.reminders_on)?;
    items.gauges.set_checked(new.gauges_on)?;
    items.auto.set_checked(new.auto_on)?;
    let same_list = old.profiles.len() == new.profiles.len()
        && old.profiles.iter().zip(&new.profiles).all(|(a, b)| a.0 == b.0 && a.1 == b.1);
    if same_list {
        for (i, p) in items.profile_items.iter().zip(&new.profiles) {
            i.set_checked(p.2)?;
        }
    } else {
        for i in items.profile_items.drain(..) {
            items.profiles.remove(&i)?;
        }
        items.profile_items = profile_items(app, new)?;
        for i in &items.profile_items {
            items.profiles.append(i)?;
        }
    }
    Ok(())
}

fn focus_control(app: &AppHandle, action: PomodoroAction) {
    if let Err(e) = app.state::<Arc<PomodoroHandle>>().control(action) {
        tracing::warn!("focus timer from the tray: {e}");
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main(app),
        "update" => {
            if let Some(updates) = app.try_state::<Arc<Updates>>() {
                updates.open_dialog();
            }
        }
        "quit" => quit(app),
        "focus.toggle" => {
            let running = app.state::<Arc<PomodoroHandle>>().state().is_ok_and(|s| s.status == PomodoroStatus::Running);
            focus_control(app, if running { PomodoroAction::Pause } else { PomodoroAction::Start });
        }
        "focus.skip" => focus_control(app, PomodoroAction::Skip),
        "focus.stop" => focus_control(app, PomodoroAction::Stop),
        "reminders.on" => {
            let reminders = app.state::<Arc<RemindersHandle>>();
            let result = reminders.state().and_then(|s| {
                let mut settings = s.settings;
                settings.enabled = !settings.enabled;
                tracing::info!(on = settings.enabled, "reminders from the tray");
                reminders.configure(settings)
            });
            if let Err(e) = result {
                tracing::warn!("reminders from the tray: {e}");
            }
        }
        "auto" => {
            if let Some(auto) = app.try_state::<Arc<crate::autoprofile::AutoProfiles>>() {
                let on = !auto.running().unwrap_or(false);
                tracing::info!(on, "per-app profiles from the tray");
                auto.set_running(on);
            }
        }
        "gauges" => {
            let gauges = app.state::<Arc<GaugesHandle>>();
            let result = gauges.state().and_then(|s| {
                let mut settings = s.settings;
                settings.enabled = !settings.enabled;
                gauges.configure(settings)
            });
            match result {
                // The UI's switch follows (it listens only while its page is open).
                Ok(state) => {
                    let _ = app.emit(EVENT_GAUGES, state);
                }
                Err(e) => tracing::warn!("gauges from the tray: {e}"),
            }
        }
        _ => {
            if let Some(profile) = id.strip_prefix(PROFILE_PREFIX) {
                tracing::info!("profile chosen in the tray");
                show_main(app);
                let _ = app.emit(EVENT_TRAY_PROFILE, profile);
            }
        }
    }
    refresh(app);
}

/// Keep the menu and the tooltip true: on each nudge, and every few
/// seconds while the focus timer runs.
/// The tray icon, and the same with a dot while a new version waits.
struct Icons {
    plain: Option<Image<'static>>,
    dotted: Option<Image<'static>>,
}

impl Icons {
    fn of(&self, update: bool) -> Option<Image<'static>> {
        if update {
            self.dotted.clone().or_else(|| self.plain.clone())
        } else {
            self.plain.clone()
        }
    }
}

/// The dot's colour (the app's green, as in the approved preview) and the
/// ring that sets it off from the icon (the icon's own dark background).
const DOT: [u8; 3] = [0x5f, 0xd3, 0x9a];
const DOT_RING: [u8; 3] = [0x16, 0x18, 0x1c];

/// `rgba` (width × height) with a dot in its bottom-right corner, two
/// fifths of the icon across, in a ring of the icon's background colour.
fn with_dot(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let size = width.min(height) as f32;
    let radius = size * 0.2;
    let ring = radius + (size * 0.07).max(1.0);
    let (cx, cy) = (width as f32 - ring, height as f32 - ring);
    for y in 0..height {
        for x in 0..width {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let color = if d <= radius {
                DOT
            } else if d <= ring {
                DOT_RING
            } else {
                continue;
            };
            let i = ((y * width + x) * 4) as usize;
            if let Some(px) = out.get_mut(i..i + 4) {
                px.copy_from_slice(&[color[0], color[1], color[2], 0xff]);
            }
        }
    }
    out
}

fn icons(app: &AppHandle) -> Icons {
    let Some(icon) = app.default_window_icon() else { return Icons { plain: None, dotted: None } };
    let (w, h) = (icon.width(), icon.height());
    Icons {
        plain: Some(Image::new_owned(icon.rgba().to_vec(), w, h)),
        dotted: Some(Image::new_owned(with_dot(icon.rgba(), w, h), w, h)),
    }
}

fn keep_up(
    app: AppHandle,
    bg: Arc<Background>,
    tray: TrayIcon<Wry>,
    icons: Icons,
    mut items: Items,
    mut shown: View,
    rx: mpsc::Receiver<()>,
) {
    loop {
        match rx.recv_timeout(if shown.focus_active { TICK_RUNNING } else { TICK_IDLE }) {
            Ok(()) => {
                std::thread::sleep(SETTLE);
                while rx.try_recv().is_ok() {}
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let view = view_of(&bg.texts(), &facts(&app), crate::now_ms(), &crate::zones::pc_zone());
        if let Err(e) = apply(&app, &mut items, &shown, &view) {
            tracing::warn!("tray menu not updated: {e}");
        }
        if view.tooltip != shown.tooltip {
            let _ = tray.set_tooltip(Some(&view.tooltip));
        }
        if view.update.is_some() != shown.update.is_some() {
            if let Err(e) = tray.set_icon(icons.of(view.update.is_some())) {
                tracing::warn!("tray icon not changed: {e}");
            }
        }
        shown = view;
    }
}

/// Put the icon in the tray and keep its menu and tooltip up to date.
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let bg = app.state::<Arc<Background>>().inner().clone();
    let first = view_of(&bg.texts(), &facts(app), crate::now_ms(), &crate::zones::pc_zone());
    let (menu, items) = build(app, &first)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(&first.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    let icons = icons(app);
    if let Some(icon) = icons.of(first.update.is_some()) {
        builder = builder.icon(icon);
    }
    let tray = builder.build(app)?;

    let (tx, rx) = mpsc::channel::<()>();
    if let Ok(mut r) = bg.refresh.lock() {
        *r = Some(tx);
    }
    for event in [
        EVENT_POMODORO,
        EVENT_KEYBOARDS_CHANGED,
        EVENT_REMINDERS,
        EVENT_UPDATE,
        EVENT_GAMES,
        crate::autoprofile::EVENT_AUTO_PROFILE,
    ] {
        let app = app.clone();
        app.clone().listen_any(event, move |_| refresh(&app));
    }
    let app = app.clone();
    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || keep_up(app, bg, tray, icons, items, first, rx))
        .map_err(|e| tauri::Error::Anyhow(e.into()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    /// The classic sequence (6 blocks, 100 minutes), at block `block`.
    fn pomodoro(status: PomodoroStatus, phase: PomodoroPhase, block: u8, remaining_ms: f64) -> PomodoroState {
        PomodoroState {
            settings: crate::dto::PomodoroSettings::default(),
            status,
            phase,
            block,
            round: 1,
            cycle: 1,
            phase_ms: 25.0 * 60_000.0,
            remaining_ms,
            at_ms: 1_000.0,
            focus_done: 0,
            last: None,
            trying: false,
        }
    }

    fn tr() -> TrayTexts {
        let s = |t: &str| t.to_string();
        TrayTexts {
            locale: s("tr"),
            tomorrow: s("yarın"),
            round: s("Blok {n}/{total}"),
            left: s("{min} dk kaldı"),
            ..TrayTexts::default()
        }
    }

    const UTC3: i8 = 3;

    #[test]
    fn focus_line_shows_phase_block_and_minutes() {
        let t = TrayTexts::default();
        let idle = pomodoro(PomodoroStatus::Idle, PomodoroPhase::Focus, 1, 0.0);
        assert_eq!(focus_status(&t, &idle, 0.0), "Ready: 6 blocks, 100 min");
        // 18 min 40 s left at 1 s; 20 s later 18 min 20 s: 19 (rounded up).
        let p = pomodoro(PomodoroStatus::Running, PomodoroPhase::Focus, 3, 18.0 * 60_000.0 + 40_000.0);
        assert_eq!(focus_status(&t, &p, 21_000.0), "Focus · Block 3/6 · 19 min left");
        let p = pomodoro(PomodoroStatus::Running, PomodoroPhase::Break, 2, 30_000.0);
        assert_eq!(focus_status(&t, &p, 1_000.0), "Break · Block 2/6 · 1 min left");
        // Ran past its end (the worker had not ticked yet): 1, not 0.
        assert_eq!(focus_status(&t, &p, 90_000.0), "Break · Block 2/6 · 1 min left");
        // Paused: the time does not run down.
        let p = pomodoro(PomodoroStatus::Paused, PomodoroPhase::Focus, 3, 12.0 * 60_000.0);
        assert_eq!(focus_status(&t, &p, 9e9), "Paused · Focus · Block 3/6 · 12 min left");
    }

    #[test]
    fn view_reads_like_the_menu() {
        let t = tr();
        let tz = TimeZone::fixed(jiff::tz::offset(UTC3));
        let ms = |d: i8, h: i8, m: i8| {
            date(2026, 11, d).at(h, m, 0, 0).to_zoned(tz.clone()).unwrap().timestamp().as_millisecond() as f64
        };
        // 2026-11-12 10:00 local.
        let now = ms(12, 10, 0);
        let at = ms;
        let mut f = Facts {
            keyboard: Some(KeyboardLine {
                name: "AJAZZ AK980 PRO".into(),
                connection: Connection::Bluetooth,
                battery: Some(80),
            }),
            pomodoro: Some(pomodoro(PomodoroStatus::Running, PomodoroPhase::Focus, 1, 5.0 * 60_000.0)),
            reminders: Some((true, Some((at(12, 14, 30), "Su iç".into())))),
            gauges: Some(true),
            profiles: vec![("default".into(), String::new(), false), ("p1".into(), "Oyun & iş".into(), true)],
            update: None,
            game: None,
            auto: false,
        };
        let v = view_of(&t, &f, now, &tz);
        assert_eq!(v.keyboard, "AJAZZ AK980 PRO · Bluetooth · battery 80%");
        assert_eq!(v.focus_toggle, "Pause");
        assert!(v.focus_active);
        assert_eq!(v.reminders_next, "Next: 14:30 · Su iç");
        assert!(v.gauges_on);
        assert_eq!(v.profiles[0].1, "Default");
        assert_eq!(v.tooltip, "Keymotion — Focus · Blok 1/6 · 1 dk kaldı");

        f.reminders = Some((true, Some((at(13, 9, 0), String::new()))));
        assert_eq!(view_of(&t, &f, now, &tz).reminders_next, "Next: yarın 09:00 · —");
        f.reminders = Some((true, Some((at(14, 9, 0), "x".into()))));
        assert_eq!(view_of(&t, &f, now, &tz).reminders_next, "Next: 14.11 09:00 · x");
        f.reminders = Some((false, None));
        assert_eq!(view_of(&t, &f, now, &tz).reminders_next, "Reminders off");

        // The cable: no battery; no keyboard; the timer idle.
        f.keyboard =
            Some(KeyboardLine { name: "AJAZZ AK980 PRO".into(), connection: Connection::Usb, battery: Some(80) });
        f.pomodoro = Some(pomodoro(PomodoroStatus::Idle, PomodoroPhase::Focus, 1, 0.0));
        let v = view_of(&t, &f, now, &tz);
        assert_eq!(v.keyboard, "AJAZZ AK980 PRO · USB cable");
        assert_eq!((v.focus_toggle.as_str(), v.focus_active, v.tooltip.as_str()), ("Start", false, "Keymotion"));
        f.keyboard = None;
        assert_eq!(view_of(&t, &f, now, &tz).keyboard, "No keyboard connected");
        assert_eq!(view_of(&t, &f, now, &tz).update, None);

        // A new version waits: the first line, and a word in the tooltip.
        f.update = Some(("1.1.0".into(), false));
        let v = view_of(&t, &f, now, &tz);
        assert_eq!(v.update.as_deref(), Some("Update available: Keymotion 1.1.0"));
        assert_eq!(v.tooltip, "Keymotion · update available");
        f.update = Some(("1.1.0".into(), true));
        assert_eq!(view_of(&t, &f, now, &tz).update.as_deref(), Some("Install now: Keymotion 1.1.0"));

        // A game session: a line under the keyboard's, and in the tooltip.
        f.update = None;
        f.game = Some(("League of Legends".into(), now - 42.5 * 60_000.0));
        let v = view_of(&t, &f, now, &tz);
        assert_eq!(v.game.as_deref(), Some("Playing League of Legends · 42 min"));
        assert_eq!(v.tooltip, "Keymotion · Playing League of Legends · 42 min");
        // Hours, and under a minute, as on the page.
        f.game = Some(("League of Legends".into(), now - 135.0 * 60_000.0));
        assert_eq!(view_of(&t, &f, now, &tz).game.as_deref(), Some("Playing League of Legends · 2 h 15 min"));
        f.game = Some(("League of Legends".into(), now - 20_000.0));
        assert_eq!(view_of(&t, &f, now, &tz).game.as_deref(), Some("Playing League of Legends · under 1 min"));
    }

    #[test]
    fn the_update_dot_sits_in_the_bottom_right_corner() {
        // A 10 × 10 transparent icon.
        let rgba = vec![0u8; 10 * 10 * 4];
        let out = with_dot(&rgba, 10, 10);
        let px = |x: usize, y: usize| &out[(y * 10 + x) * 4..(y * 10 + x) * 4 + 4];
        assert_eq!(px(7, 7), &[DOT[0], DOT[1], DOT[2], 0xff]);
        assert_eq!(px(0, 0), &[0, 0, 0, 0]);
        assert_eq!(px(9, 0), &[0, 0, 0, 0]);
        assert_eq!(out.len(), rgba.len());
    }

    #[test]
    fn ampersands_stay_visible_in_the_menu() {
        assert_eq!(menu_text("Rock & Roll"), "Rock && Roll");
    }

    #[test]
    fn stored_defaults_keep_the_app_in_the_tray() {
        let s: Stored = serde_json::from_str("{}").unwrap();
        assert!(s.close_to_tray);
    }

    #[test]
    fn older_settings_with_the_notice_flag_still_load() {
        let s: Stored = serde_json::from_str(r#"{"closeToTray":false,"noticeShown":true}"#).unwrap();
        assert!(!s.close_to_tray);
    }

    #[test]
    fn the_notice_comes_once_per_run() {
        let bg = Background::open(None);
        assert!(bg.take_notice());
        assert!(!bg.take_notice());
        assert!(!bg.take_notice());
        // A new run of the app (a new Background) shows it again.
        assert!(Background::open(None).take_notice());
    }
}
