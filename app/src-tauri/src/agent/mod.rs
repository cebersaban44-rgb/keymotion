//! AI agents and scripts on the keyboard (plan: `ajan-cli-plan.md`).
//!
//! `keymotion-cli` never opens the keyboard: it talks to this module over
//! a named pipe only the current Windows user can open
//! (`kb_agent::transport`), and the device actor stays the keyboard's
//! only owner. What agents ask for becomes one light layer,
//! `LightLayer::Agent`: over the focus timer, the game, the music, the
//! gauges and the per-app profile; under the alerts, the reminders and the
//! nudges (owner's order, 2026-11-22). RAM only, USB only.
//!
//! * The pipe server starts with the app, always: with access off an agent
//!   still gets a clear answer (`agent_access_off`), and `status`, `keys`,
//!   `log` and `settings_get` still work. If another program holds the
//!   pipe's name, the app says so on the page and tries again now and then.
//! * [`engine`]: the lights per source, their time, ranks, the rate limit,
//!   the log and "Undo". [`effects`]: how they move, at most three flashes
//!   a second. [`keys`]: key names to LEDs on the keyboard in use.
//!   [`config`]: the settings as keys and values for agents.
//! * Settings in `agents.json`: access and the permissions off until the
//!   user turns them on; the looks the owner approved (2026-11-22).
//! * A game that wants the keyboard left alone hides the layer while "hide
//!   while playing" is on (the device actor does it). Night quiet dims it
//!   or takes it away.
//! * The app's own log gets the source, the command and the result of each
//!   request; nothing an agent's user typed reaches the app at all.

mod config;
mod effects;
mod engine;
mod keys;

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Quiet, Request};
use crate::dto::{
    AgentLights, AgentLook, AgentNightMode, AgentServer, AgentSettings, AgentsState, Connection, Feature, FlashWrite,
    KeyboardInfo,
};
use crate::settings;
use effects::FlashGuard;
use engine::{in_night, Engine, Now};
use kb_agent::{AgentError, Call, Channel, Command, ErrorCode, KeySpec, ServerInfo};
use kb_protocol::sonix::level_factor;
use keys::KeyIndex;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const EVENT_AGENTS: &str = "agents";
const FILE: &str = "agents.json";
const VERSION: u32 = 1;
/// A frame while lights are on: 20 a second, the app's live rate.
const TICK: Duration = Duration::from_millis(50);
/// Otherwise a look now and then (the pipe server, the page).
const IDLE: Duration = Duration::from_secs(1);
/// The page hears of changes at most this often.
const EMIT_EVERY_MS: u64 = 250;
/// A pipe someone else holds is tried again this often.
const SERVER_RETRY_MS: u64 = 30_000;
/// The layer's brightness, in the keyboard's own steps.
const LEVEL: u8 = 5;
/// Night quiet's "faint": this share of it.
const NIGHT_DIM: f32 = 0.2;
/// The conflicting-program check lists every process: kept this long.
const CONFLICT_KEEP: Duration = Duration::from_secs(5);

/// Where the keyboard for lights stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cable {
    /// On the USB cable: its model's id.
    Usb(String),
    /// Connected, but not over USB.
    NotUsb,
    /// No keyboard.
    None,
}

/// The focus timer, for `status`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FocusNow {
    pub phase: &'static str,
    pub status: &'static str,
    pub remaining_s: u64,
}

/// The screen's upload budget, for `status`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BudgetNow {
    pub live: bool,
    pub left: u32,
    pub next_in_s: Option<u64>,
}

/// What the agents need from the rest of the app.
pub trait Host: Send + Sync {
    /// Steady milliseconds.
    fn mono_ms(&self) -> u64;
    /// Unix milliseconds.
    fn wall_ms(&self) -> f64;
    fn cable(&self) -> Cable;
    fn keyboards(&self) -> Vec<KeyboardInfo>;
    fn quiet(&self) -> Quiet;
    fn focus(&self) -> Option<FocusNow>;
    fn screen_budget(&self) -> Option<BudgetNow>;
    fn flash_write(&self) -> Option<FlashWrite>;
    /// A known program that talks to the keyboard too.
    fn conflict(&self) -> Option<&'static str>;
    /// Milliseconds since the user's last input.
    fn idle_ms(&self) -> Option<u64>;
    /// Local time, in minutes after midnight.
    fn minute_of_day(&self) -> u16;
    fn send(&self, request: Request);
    fn save(&self, settings: &AgentSettings) -> Result<(), String>;
    fn publish(&self, state: &AgentsState);
}

#[derive(Default)]
struct ServerSlot {
    #[cfg(windows)]
    pipe: Option<kb_winsys::pipe::PipeServer>,
    error: Option<String>,
    /// When to try starting it again.
    retry_at: u64,
    /// The page has not heard of a change yet.
    changed: bool,
}

/// What the worker thread keeps between frames.
#[derive(Default)]
pub struct Worker {
    sent: Option<LiveFrame>,
    guard: FlashGuard,
    in_quiet: Option<bool>,
    emit_pending: bool,
    last_emit: Option<u64>,
}

/// The agents' state and the pipe's handler, shared by the pipe's
/// connections, the worker and the page.
pub struct Core {
    me: Weak<Core>,
    host: Arc<dyn Host>,
    info: ServerInfo,
    engine: Mutex<Engine>,
    /// The key layout of the model on the cable.
    index: Mutex<Option<Arc<KeyIndex>>>,
    server: Mutex<ServerSlot>,
    /// Open the user's pipe (not in tests, which serve their own).
    serve_pipe: bool,
    wake: Mutex<Option<Sender<()>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn needs_usb() -> AgentError {
    AgentError::new(ErrorCode::NeedsUsb, "The keyboard's lights work only over the USB cable.")
        .with_hint("Connect the keyboard with its USB cable.")
}

fn no_keyboard() -> AgentError {
    AgentError::new(ErrorCode::NoKeyboard, "No keyboard is connected.")
}

fn lights_name(l: AgentLights) -> &'static str {
    match l {
        AgentLights::Shown => "shown",
        AgentLights::NoKeyboard => "no_keyboard",
        AgentLights::NeedsUsb => "needs_usb",
        AgentLights::Game => "game",
        AgentLights::NightDim => "night_dim",
        AgentLights::NightOff => "night_off",
    }
}

impl Core {
    pub fn new(host: Arc<dyn Host>, settings: AgentSettings, info: ServerInfo, serve_pipe: bool) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            host,
            info,
            engine: Mutex::new(Engine::new(settings)),
            index: Mutex::new(None),
            server: Mutex::new(ServerSlot::default()),
            serve_pipe,
            wake: Mutex::new(None),
        })
    }

    fn now(&self) -> Now {
        Now { mono: self.host.mono_ms(), wall: self.host.wall_ms() }
    }

    fn wake(&self) {
        if let Some(tx) = lock(&self.wake).as_ref() {
            let _ = tx.send(());
        }
    }

    fn settings(&self) -> AgentSettings {
        *lock(&self.engine).settings()
    }

    /// The key layout of the keyboard on the cable, or why lights cannot
    /// show.
    fn lights(&self) -> Result<Arc<KeyIndex>, AgentError> {
        let model = match self.host.cable() {
            Cable::Usb(model) => model,
            Cable::NotUsb => return Err(needs_usb()),
            Cable::None => return Err(no_keyboard()),
        };
        let mut cached = lock(&self.index);
        if let Some(index) = cached.as_ref().filter(|i| i.model_id == model) {
            return Ok(index.clone());
        }
        let index = KeyIndex::for_model(&model).map(Arc::new).ok_or_else(|| {
            AgentError::new(ErrorCode::Unsupported, "This keyboard's key layout is not known to Keymotion.")
        })?;
        *cached = Some(index.clone());
        Ok(index)
    }

    /// How the lights show now, as far as the keyboard, a game and the
    /// night go.
    fn lights_now(&self, s: &AgentSettings) -> AgentLights {
        match self.host.cable() {
            Cable::None => return AgentLights::NoKeyboard,
            Cable::NotUsb => return AgentLights::NeedsUsb,
            Cable::Usb(_) => {}
        }
        if s.hide_in_game && self.host.quiet() != Quiet::Off {
            return AgentLights::Game;
        }
        if s.night.on && in_night(s.night.from_min, s.night.to_min, self.host.minute_of_day()) {
            return match s.night.mode {
                AgentNightMode::Dim => AgentLights::NightDim,
                AgentNightMode::Off => AgentLights::NightOff,
            };
        }
        AgentLights::Shown
    }

    // ------------------------------------------------------------ the pipe

    /// Answer one request from the pipe (after `hello`).
    pub fn handle(&self, call: Call<'_>, command: Command) -> Result<Value, AgentError> {
        let name = command.name();
        let result = match command {
            Command::Status(_) => Ok(self.status(call.peer.protocol)),
            Command::Keys(_) => self.keys(),
            Command::Hello(_) => Err(AgentError::new(ErrorCode::Internal, "hello was not answered by the session.")),
            other => self.change(call, other),
        };
        tracing::debug!(
            source = call.source.as_str(),
            command = name,
            result = result.as_ref().err().map_or("ok", |e| e.code.as_str()),
            "agent request"
        );
        result
    }

    fn change(&self, call: Call<'_>, command: Command) -> Result<Value, AgentError> {
        let name = command.name();
        let now = self.now();
        let lights = self.lights();
        let idle = self.host.idle_ms();
        let (result, effects, settings) = {
            let mut e = lock(&self.engine);
            e.observe_idle(idle, now.mono);
            e.prune(now.mono);
            let (result, effects) = e.command(call.source, command, lights.as_deref().map_err(Clone::clone), now);
            (result, effects, *e.settings())
        };
        if effects.settings {
            self.persist(&settings);
        }
        self.wake();
        let mut data = result?;
        if matches!(name, "signal" | "light" | "progress") {
            data["hidden"] = match self.lights_now(&settings) {
                AgentLights::Game => json!("game"),
                AgentLights::NightOff => json!("night"),
                _ => Value::Null,
            };
        }
        Ok(data)
    }

    fn persist(&self, settings: &AgentSettings) {
        if let Err(e) = self.host.save(settings) {
            tracing::warn!("agent settings not saved: {e}");
        }
    }

    /// `status`: the app, the agents' switches, the keyboards, what holds
    /// the keyboard, and every agent's lights.
    fn status(&self, protocol: u32) -> Value {
        let now = self.now();
        let s = self.settings();
        let keyboards: Vec<Value> = self
            .host
            .keyboards()
            .iter()
            .map(|k| {
                let battery = k.battery.as_ref().map(|b| {
                    let age_ms = now.wall - b.read_at_ms as f64;
                    let stale = k.connection != Connection::Bluetooth || age_ms > 10.0 * 60_000.0;
                    json!({ "percent": b.percent, "read_at_ms": b.read_at_ms, "stale": stale })
                });
                let lights = k.connection == Connection::Usb && k.features.contains(&Feature::PerKeyRgb);
                json!({
                    "model": k.model_id,
                    "name": k.model_name,
                    "connection": k.connection,
                    "battery": battery,
                    "lights": lights,
                    "screen": k.features.contains(&Feature::Screen),
                })
            })
            .collect();
        let quiet = match self.host.quiet() {
            Quiet::Off => "off",
            Quiet::ButReminders => "but_reminders",
            Quiet::All => "all",
        };
        let signals: Vec<Value> = {
            let index = self.lights().ok();
            let e = lock(&self.engine);
            e.infos(now.mono, index.as_deref())
                .into_iter()
                .filter(|i| i.kind != "try")
                .map(|i| {
                    json!({
                        "id": i.id,
                        "source": i.source,
                        "kind": i.kind,
                        "keys": i.keys,
                        "shown": i.shown,
                        "expires_in_ms": i.expires_in_ms as u64,
                        "percent": i.percent,
                    })
                })
                .collect()
        };
        json!({
            "app": { "running": true, "version": self.info.version, "channel": self.info.channel, "protocol": protocol },
            "agents": {
                "enabled": s.enabled,
                "screen": s.screen,
                "focus": s.focus,
                "profiles": s.profiles,
                "agents_may_edit": s.agents_may_edit,
                "hide_in_game": s.hide_in_game,
            },
            "keyboards": keyboards,
            "lights": lights_name(self.lights_now(&s)),
            "quiet": quiet,
            "focus": self.host.focus(),
            "signals": signals,
            "screen_budget": self.host.screen_budget(),
            "flash_write": self.host.flash_write(),
            "conflict": self.host.conflict(),
        })
    }

    /// `keys`: the keyboard on the cable, else any connected one.
    fn keys(&self) -> Result<Value, AgentError> {
        let index = match self.lights() {
            Ok(index) => index,
            Err(e) if e.code == ErrorCode::NeedsUsb => {
                let model = self.host.keyboards().first().map(|k| k.model_id.clone()).ok_or_else(no_keyboard)?;
                Arc::new(KeyIndex::for_model(&model).ok_or_else(|| {
                    AgentError::new(ErrorCode::Unsupported, "This keyboard's key layout is not known to Keymotion.")
                })?)
            }
            Err(e) => return Err(e),
        };
        serde_json::to_value(index.describe())
            .map_err(|e| AgentError::new(ErrorCode::Internal, format!("The key list could not be written: {e}")))
    }

    /// Serve the pipe `name`. Fails with `AddrInUse` when another process
    /// holds it.
    #[cfg(windows)]
    pub fn serve(&self, name: &str) -> std::io::Result<kb_winsys::pipe::PipeServer> {
        let me = self.me.clone();
        kb_agent::transport::serve(name, self.info.clone(), move |call, command| match me.upgrade() {
            Some(core) => core.handle(call, command),
            None => Err(AgentError::new(ErrorCode::ConnectionLost, "Keymotion is closing.")),
        })
    }

    /// Start the pipe server if it is not running and it is time to try.
    fn ensure_server(&self, now: u64) {
        #[cfg(windows)]
        {
            if !self.serve_pipe {
                return;
            }
            {
                let slot = lock(&self.server);
                if slot.pipe.is_some() || now < slot.retry_at {
                    return;
                }
            }
            let started = kb_agent::transport::current_pipe_name().and_then(|name| self.serve(&name));
            let mut slot = lock(&self.server);
            match started {
                Ok(pipe) => {
                    tracing::info!("agent pipe open");
                    slot.pipe = Some(pipe);
                    slot.error = None;
                    slot.changed = true;
                }
                Err(e) => {
                    let error = if e.kind() == std::io::ErrorKind::AddrInUse {
                        "another program holds the agents' pipe".to_string()
                    } else {
                        e.to_string()
                    };
                    if slot.error.as_deref() != Some(error.as_str()) {
                        tracing::warn!("agent pipe not open: {error}");
                        slot.error = Some(error);
                        slot.changed = true;
                    }
                    slot.retry_at = now + SERVER_RETRY_MS;
                }
            }
        }
        #[cfg(not(windows))]
        let _ = now;
    }

    fn server_state(&self) -> AgentServer {
        let slot = lock(&self.server);
        #[cfg(windows)]
        let (running, connections) = (slot.pipe.is_some(), slot.pipe.as_ref().map_or(0, |p| p.connections()));
        #[cfg(not(windows))]
        let (running, connections) = (false, 0);
        AgentServer { running, error: slot.error.clone(), connections }
    }

    // ------------------------------------------------------------- the page

    pub fn state(&self) -> AgentsState {
        let now = self.host.mono_ms();
        let index = self.lights().ok();
        let (settings, signals, log) = {
            let e = lock(&self.engine);
            (*e.settings(), e.infos(now, index.as_deref()), e.log_entries())
        };
        AgentsState { lights: self.lights_now(&settings), settings, signals, log, server: self.server_state() }
    }

    /// The page saved its settings (any of them).
    pub fn save(&self, settings: AgentSettings) -> Result<AgentsState, String> {
        let settings = settings.sanitized();
        {
            let mut e = lock(&self.engine);
            if e.settings().enabled != settings.enabled {
                tracing::info!(on = settings.enabled, "agent access");
            }
            e.set_settings(settings);
        }
        self.host.save(&settings)?;
        self.wake();
        Ok(self.state())
    }

    /// "Try on the keyboard": `look` on `keys` (default every key).
    pub fn try_look(&self, look: AgentLook, keys: Option<String>) -> Result<AgentsState, String> {
        let spec = match keys.as_deref() {
            Some(k) => KeySpec::parse(k).map_err(|e| e.message)?,
            None => KeySpec::all(),
        };
        let index = self.lights().map_err(|e| match e.code {
            ErrorCode::NeedsUsb | ErrorCode::NoKeyboard => "no keyboard on the USB cable".to_string(),
            _ => e.message,
        })?;
        let now = self.host.mono_ms();
        lock(&self.engine).try_look(&look.sanitized(), spec, &index, now).map_err(|e| e.message)?;
        self.wake();
        Ok(self.state())
    }

    /// "Turn off every agent light".
    pub fn clear_all(&self) -> AgentsState {
        let gone = lock(&self.engine).clear_agents();
        tracing::info!(lights = gone, "every agent light turned off");
        self.wake();
        self.state()
    }

    /// "Undo" on a log entry.
    pub fn undo(&self, seq: u32) -> Result<AgentsState, String> {
        let settings = {
            let mut e = lock(&self.engine);
            e.undo(seq)?;
            *e.settings()
        };
        tracing::info!(seq, "agent change undone");
        self.persist(&settings);
        self.wake();
        Ok(self.state())
    }

    // ----------------------------------------------------------- the worker

    /// One pass: watch the user's input, end lights whose time is up, send
    /// the frame, tell the page. Returns how long to sleep.
    pub fn tick(&self, w: &mut Worker) -> Duration {
        let now = self.host.mono_ms();
        self.ensure_server(now);
        let idle = self.host.idle_ms();
        let index = self.lights().ok();
        let (frame, busy, dirty, settings, trying) = {
            let mut e = lock(&self.engine);
            e.observe_idle(idle, now);
            e.prune(now);
            let frame = index.as_deref().map(|i| e.frame(now, i)).unwrap_or_default();
            (frame, e.busy(), e.take_dirty(), *e.settings(), e.trying(now))
        };
        let in_quiet = !settings.hide_in_game;
        if w.in_quiet != Some(in_quiet) {
            self.host.send(Request::SetAgentInQuiet(in_quiet));
            w.in_quiet = Some(in_quiet);
        }
        // Night quiet leaves the page's own try alone.
        let night = match self.lights_now(&settings) {
            AgentLights::NightDim if !trying => NIGHT_DIM,
            AgentLights::NightOff if !trying => 0.0,
            _ => 1.0,
        };
        let colors = w.guard.check(now, if night > 0.0 { frame } else { Vec::new() });
        let out = (!colors.is_empty()).then(|| LiveFrame { colors, level: level_factor(LEVEL) * night });
        if out != w.sent {
            if out.is_some() != w.sent.is_some() {
                tracing::debug!(on = out.is_some(), "agent lights");
            }
            self.host.send(Request::SetLayer(LightLayer::Agent, out.clone()));
            w.sent = out;
        }
        let server_changed = std::mem::take(&mut lock(&self.server).changed);
        if dirty || server_changed {
            w.emit_pending = true;
        }
        if w.emit_pending && w.last_emit.is_none_or(|t| now.saturating_sub(t) >= EMIT_EVERY_MS) {
            self.host.publish(&self.state());
            w.emit_pending = false;
            w.last_emit = Some(now);
        }
        if busy || w.sent.is_some() || w.emit_pending {
            TICK
        } else {
            IDLE
        }
    }
}

// ------------------------------------------------------------------- app

/// The rest of the app, as the agents see it.
struct AppHost {
    app: AppHandle,
    device: Arc<DeviceHandle>,
    config_dir: Option<PathBuf>,
    started: Instant,
    conflict: Mutex<Option<(Instant, Option<&'static str>)>>,
}

impl Host for AppHost {
    fn mono_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn wall_ms(&self) -> f64 {
        crate::live::now_ms()
    }

    fn cable(&self) -> Cable {
        let Ok(snap) = self.device.snapshot.lock() else { return Cable::None };
        let usb = snap.keyboards.iter().find(|k| {
            k.connection == Connection::Usb && k.has_command_channel && k.features.contains(&Feature::PerKeyRgb)
        });
        match usb {
            Some(k) => Cable::Usb(k.model_id.clone()),
            None if snap.keyboards.is_empty() => Cable::None,
            None => Cable::NotUsb,
        }
    }

    fn keyboards(&self) -> Vec<KeyboardInfo> {
        self.device.snapshot.lock().map(|s| s.keyboards.clone()).unwrap_or_default()
    }

    fn quiet(&self) -> Quiet {
        self.device.snapshot.lock().map(|s| s.quiet).unwrap_or_default()
    }

    fn focus(&self) -> Option<FocusNow> {
        use crate::dto::{PomodoroPhase, PomodoroStatus};
        use tauri::Manager;
        let state = self.app.try_state::<Arc<crate::pomodoro::PomodoroHandle>>()?.state().ok()?;
        let running = match state.status {
            PomodoroStatus::Idle => return None,
            PomodoroStatus::Running => true,
            PomodoroStatus::Paused => false,
        };
        let left =
            if running { state.remaining_ms - (crate::live::now_ms() - state.at_ms) } else { state.remaining_ms };
        Some(FocusNow {
            phase: match state.phase {
                PomodoroPhase::Focus => "focus",
                PomodoroPhase::Break => "break",
            },
            status: if running { "running" } else { "paused" },
            remaining_s: (left.max(0.0) / 1000.0).round() as u64,
        })
    }

    fn screen_budget(&self) -> Option<BudgetNow> {
        use tauri::Manager;
        let state = self.app.try_state::<Arc<crate::live::LiveHandle>>()?.state().ok()?;
        Some(BudgetNow {
            live: state.enabled,
            left: state.tokens.floor().max(0.0) as u32,
            next_in_s: state.next_token_ms.map(|ms| (ms.max(0.0) / 1000.0).ceil() as u64),
        })
    }

    fn flash_write(&self) -> Option<FlashWrite> {
        self.device.flash.current()
    }

    fn conflict(&self) -> Option<&'static str> {
        let mut kept = lock(&self.conflict);
        if let Some((at, found)) = *kept {
            if at.elapsed() < CONFLICT_KEEP {
                return found;
            }
        }
        let programs = kb_winsys::running_programs();
        let found = kb_core::conflicts::find_conflict(
            programs.iter().map(|p| (p.pid, p.name.as_str(), p.path.as_deref())),
            std::process::id(),
        )
        .map(|c| c.id());
        *kept = Some((Instant::now(), found));
        found
    }

    fn idle_ms(&self) -> Option<u64> {
        kb_winsys::idle_ms()
    }

    fn minute_of_day(&self) -> u16 {
        let now = crate::zones::now();
        now.hour() as u16 * 60 + now.minute() as u16
    }

    fn send(&self, request: Request) {
        let _ = self.device.send(request);
    }

    fn save(&self, settings: &AgentSettings) -> Result<(), String> {
        match &self.config_dir {
            Some(dir) => settings::save(dir, FILE, VERSION, settings).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    fn publish(&self, state: &AgentsState) {
        let _ = self.app.emit(EVENT_AGENTS, state);
    }
}

/// The agents (managed Tauri state).
pub struct AgentHandle {
    core: Arc<Core>,
}

impl AgentHandle {
    /// Load the settings, start the worker and the pipe server.
    pub fn spawn(app: AppHandle, device: Arc<DeviceHandle>, config_dir: Option<PathBuf>) -> Arc<Self> {
        let settings = config_dir
            .as_deref()
            .and_then(|d| settings::load::<AgentSettings>(d, FILE, VERSION))
            .unwrap_or_default()
            .sanitized();
        tracing::info!(access = settings.enabled, "agents");
        let channel = if crate::channel::store() {
            Channel::Store
        } else if tauri::is_dev() {
            Channel::Dev
        } else {
            Channel::Github
        };
        let info = ServerInfo { version: app.package_info().version.to_string(), channel };
        let host = Arc::new(AppHost { app, device, config_dir, started: Instant::now(), conflict: Mutex::new(None) });
        let core = Core::new(host, settings, info, true);
        let (tx, rx) = mpsc::channel();
        *lock(&core.wake) = Some(tx);
        let worker = core.clone();
        std::thread::Builder::new()
            .name("agents".into())
            .spawn(move || {
                let mut w = Worker::default();
                loop {
                    let wait = worker.tick(&mut w);
                    match rx.recv_timeout(wait) {
                        Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn agents thread");
        Arc::new(Self { core })
    }

    pub fn state(&self) -> AgentsState {
        self.core.state()
    }

    pub fn save(&self, settings: AgentSettings) -> Result<AgentsState, String> {
        self.core.save(settings)
    }

    pub fn try_look(&self, look: AgentLook, keys: Option<String>) -> Result<AgentsState, String> {
        self.core.try_look(look, keys)
    }

    pub fn clear_all(&self) -> AgentsState {
        self.core.clear_all()
    }

    pub fn undo(&self, seq: u32) -> Result<AgentsState, String> {
        self.core.undo(seq)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::dto::AgentEffect;
    use kb_agent::protocol::{ClearArgs, LightArgs, SettingsSetArgs, SignalArgs, StatusArgs};
    use kb_agent::transport::Client;
    use kb_agent::SourceName;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A host with a keyboard on the cable and a device that only records.
    struct Fake {
        clock: AtomicU64,
        cable: Mutex<Cable>,
        quiet: Mutex<Quiet>,
        minute: Mutex<u16>,
        frames: Mutex<Vec<Option<LiveFrame>>>,
        in_quiet: Mutex<Vec<bool>>,
        saved: Mutex<Vec<AgentSettings>>,
        published: AtomicU64,
    }

    impl Fake {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                clock: AtomicU64::new(1_000),
                cable: Mutex::new(Cable::Usb("ajazz-ak980-pro".into())),
                quiet: Mutex::new(Quiet::Off),
                minute: Mutex::new(12 * 60),
                frames: Mutex::new(Vec::new()),
                in_quiet: Mutex::new(Vec::new()),
                saved: Mutex::new(Vec::new()),
                published: AtomicU64::new(0),
            })
        }

        fn advance(&self, ms: u64) {
            self.clock.fetch_add(ms, Ordering::SeqCst);
        }

        fn last_frame(&self) -> Option<LiveFrame> {
            lock(&self.frames).last().cloned().flatten()
        }
    }

    impl Host for Fake {
        fn mono_ms(&self) -> u64 {
            self.clock.load(Ordering::SeqCst)
        }
        fn wall_ms(&self) -> f64 {
            1_800_000_000_000.0 + self.mono_ms() as f64
        }
        fn cable(&self) -> Cable {
            lock(&self.cable).clone()
        }
        fn keyboards(&self) -> Vec<KeyboardInfo> {
            Vec::new()
        }
        fn quiet(&self) -> Quiet {
            *lock(&self.quiet)
        }
        fn focus(&self) -> Option<FocusNow> {
            None
        }
        fn screen_budget(&self) -> Option<BudgetNow> {
            None
        }
        fn flash_write(&self) -> Option<FlashWrite> {
            None
        }
        fn conflict(&self) -> Option<&'static str> {
            None
        }
        fn idle_ms(&self) -> Option<u64> {
            Some(0)
        }
        fn minute_of_day(&self) -> u16 {
            *lock(&self.minute)
        }
        fn send(&self, request: Request) {
            match request {
                Request::SetLayer(LightLayer::Agent, frame) => lock(&self.frames).push(frame),
                Request::SetAgentInQuiet(on) => lock(&self.in_quiet).push(on),
                _ => panic!("the agents sent something else to the device"),
            }
        }
        fn save(&self, settings: &AgentSettings) -> Result<(), String> {
            lock(&self.saved).push(*settings);
            Ok(())
        }
        fn publish(&self, _: &AgentsState) {
            self.published.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn unique_pipe() -> String {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        format!(r"\\.\pipe\keymotion-app-test-{}-{}-{nanos}", std::process::id(), COUNT.fetch_add(1, Ordering::SeqCst))
    }

    fn setup(settings: AgentSettings) -> (Arc<Fake>, Arc<Core>, kb_winsys::pipe::PipeServer, String) {
        let fake = Fake::new();
        let info = ServerInfo { version: "1.2.0".into(), channel: Channel::Dev };
        let core = Core::new(fake.clone(), settings, info, false);
        let name = unique_pipe();
        let server = core.serve(&name).unwrap();
        (fake, core, server, name)
    }

    fn client(name: &str, source: &str) -> Client {
        Client::connect(name, SourceName::parse(source).unwrap(), "tests", "1.2.0", Duration::from_secs(5)).unwrap()
    }

    fn signal(name: &str) -> Command {
        Command::Signal(SignalArgs { name: name.into(), ..Default::default() })
    }

    #[test]
    fn through_the_pipe_access_off_then_on_lights_the_keyboard() {
        let (fake, core, _server, name) = setup(AgentSettings::default());
        let mut cli = client(&name, "tests");

        // Access off: reading works, lighting is refused with 77.
        let status = cli.call(&Command::Status(StatusArgs {})).unwrap();
        assert_eq!(status["app"]["channel"], "dev");
        assert_eq!(status["app"]["protocol"], 1);
        assert_eq!(status["agents"]["enabled"], false);
        assert_eq!(status["lights"], "shown");
        let e = cli.call(&signal("done")).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AgentAccessOff, 77));
        let keys = cli.call(&Command::Keys(Default::default())).unwrap();
        assert_eq!(keys["model"], "ajazz-ak980-pro");
        assert!(keys["keys"].as_array().unwrap().len() > 90);
        assert_eq!(cli.call(&Command::Log(Default::default())).unwrap()["entries"][0]["result"], "agent_access_off");

        // The user turns access on: the signal shows on the keys.
        core.save(AgentSettings { enabled: true, ..AgentSettings::default() }).unwrap();
        assert!(lock(&fake.saved).last().unwrap().enabled);
        let answer = cli.call(&signal("done")).unwrap();
        assert_eq!(answer["id"], "tests:done");
        assert_eq!((answer["shown"].clone(), answer["hidden"].clone()), (json!(true), Value::Null));
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(*lock(&fake.in_quiet), [false], "hide while playing: on");
        fake.advance(2_000);
        core.tick(&mut w);
        let frame = fake.last_frame().expect("a frame");
        assert_eq!(frame.colors.len(), KeyIndex::for_model("ajazz-ak980-pro").unwrap().keys().len());
        assert!(frame.colors.iter().all(|(_, c)| *c == [0, 255, 0]));
        let status = cli.call(&Command::Status(StatusArgs {})).unwrap();
        assert_eq!(status["signals"][0]["id"], "tests:done");

        // Clearing takes it off the keys.
        assert_eq!(cli.call(&Command::Clear(ClearArgs::default())).unwrap()["cleared"], 1);
        core.tick(&mut w);
        assert_eq!(*lock(&fake.frames).last().unwrap(), None);
        assert!(fake.published.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn through_the_pipe_lights_need_the_cable_and_protected_settings_stay() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "claude-code:3f9a");

        *lock(&fake.cable) = Cable::NotUsb;
        let e = cli.call(&Command::Light(LightArgs { color: "red".into(), ..Default::default() })).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::NeedsUsb, 69));
        *lock(&fake.cable) = Cable::None;
        assert_eq!(cli.call(&signal("done")).unwrap_err().code, ErrorCode::NoKeyboard);
        *lock(&fake.cable) = Cable::Usb("ajazz-ak980-pro".into());

        let set = |key: &str, v: Value| {
            Command::SettingsSet(SettingsSetArgs { values: [(key.to_string(), v)].into_iter().collect() })
        };
        let e = cli.call(&set("enabled", json!(false))).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::PermissionOff, 77));
        assert!(lock(&fake.saved).is_empty());
        let changed = cli.call(&set("signals.attention.color", json!("purple"))).unwrap();
        assert_eq!(changed["changed"][0]["after"], "#8000ff");
        assert_eq!(lock(&fake.saved).last().unwrap().signals.attention.color, [0x80, 0, 0xFF]);
        let got = cli
            .call(&Command::SettingsGet(kb_agent::protocol::SettingsGetArgs { key: Some("signals.attention".into()) }))
            .unwrap();
        assert_eq!(got["settings"]["signals.attention.color"], "#8000ff");
        assert_eq!(got["settings"].as_object().unwrap().len(), 5);

        // The page puts it back.
        let seq = core.state().log.iter().find(|l| !l.changes.is_empty()).unwrap().seq;
        core.undo(seq).unwrap();
        assert_eq!(core.state().settings.signals.attention.color, [255, 128, 0]);
    }

    #[test]
    fn a_game_that_wants_quiet_hides_the_agents_unless_the_user_lets_them() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        *lock(&fake.quiet) = Quiet::All;
        let answer = cli.call(&signal("attention")).unwrap();
        assert_eq!(answer["hidden"], "game");
        assert_eq!(core.state().lights, AgentLights::Game);
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(*lock(&fake.in_quiet), [false], "the device hides the layer");
        // The user turns "hide while playing" off: the device is told.
        core.save(AgentSettings { enabled: true, hide_in_game: false, ..AgentSettings::default() }).unwrap();
        core.tick(&mut w);
        assert_eq!(*lock(&fake.in_quiet), [false, true]);
        assert_eq!(core.state().lights, AgentLights::Shown);
    }

    #[test]
    fn a_try_works_with_access_off_and_night_quiet_dims_the_rest() {
        let fake = Fake::new();
        let info = ServerInfo { version: "1.2.0".into(), channel: Channel::Dev };
        let core = Core::new(fake.clone(), AgentSettings::default(), info, false);
        let look = AgentLook::new([10, 20, 30], AgentEffect::Solid, 3, 60);
        core.try_look(look, Some("esc".into())).unwrap();
        let mut w = Worker::default();
        core.tick(&mut w);
        let frame = fake.last_frame().unwrap();
        assert_eq!(frame.colors, [(1, [10, 20, 30])]);
        assert_eq!(frame.level, level_factor(LEVEL));
        assert!(core.try_look(look, Some("f13".into())).is_err());
        *lock(&fake.cable) = Cable::NotUsb;
        assert_eq!(core.try_look(look, None).unwrap_err(), "no keyboard on the USB cable");
        *lock(&fake.cable) = Cable::Usb("ajazz-ak980-pro".into());

        // Night quiet, 23:00 to 08:00: faint at half past eleven, the try
        // aside; nothing at all when the user chose "off".
        let mut night = AgentSettings { enabled: true, ..AgentSettings::default() };
        night.night.on = true;
        core.save(night).unwrap();
        *lock(&fake.minute) = 23 * 60 + 30;
        fake.advance(engine::TRY_MS);
        let peer = kb_agent::Peer { client: "tests".into(), version: "1".into(), protocol: 1 };
        let (tests, other) = (SourceName::parse("tests").unwrap(), SourceName::parse("other").unwrap());
        let light = Command::Light(LightArgs { color: "white".into(), keys: Some("esc".into()), ..Default::default() });
        let answer = core.handle(Call { peer: &peer, source: &tests, id: "1" }, light.clone()).unwrap();
        assert_eq!(answer["hidden"], Value::Null, "faint is not hidden");
        core.tick(&mut w);
        let frame = fake.last_frame().unwrap();
        assert_eq!(frame.colors, [(1, [255, 255, 255])]);
        assert!((frame.level - level_factor(LEVEL) * NIGHT_DIM).abs() < 1e-6);
        assert_eq!(core.state().lights, AgentLights::NightDim);
        night.night.mode = AgentNightMode::Off;
        core.save(night).unwrap();
        let answer = core.handle(Call { peer: &peer, source: &other, id: "2" }, light).unwrap();
        assert_eq!(answer["hidden"], "night");
        core.tick(&mut w);
        assert_eq!(fake.last_frame(), None);
        // In the morning they show again.
        *lock(&fake.minute) = 8 * 60;
        core.tick(&mut w);
        assert_eq!(fake.last_frame().unwrap().level, level_factor(LEVEL));
    }
}
