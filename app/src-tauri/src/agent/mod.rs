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
//! * Beyond lights (plan §12.1 rows 10–11, §12.4 F): the focus timer
//!   (`focus`, its own permission), profiles (`profile list`, `show` in
//!   working memory, `apply` the lighting part to the keyboard: its own
//!   permission, `--write-flash`, at most 10 writes in any 24 hours) and
//!   screen cards (`card`: its own permission, `--write-flash`, the user's
//!   hourly number, the live mode's budget; [`cards`]). The numbers are for
//!   all agents together and survive a restart ([`limits`]): an agent's
//!   name is what it says it is. Agents never turn these permissions on:
//!   only the page does. A card needs the page: it draws it, and says when
//!   it can (`agents_card_drawer_ready`).
//! * An agent's own animations (§12.1 last row, §12.5): `scene` plays a
//!   timeline of steps (keys, colour, effect, when, how long) and may keep
//!   it under a name, `pattern` lists, plays and forgets those, and `spell`
//!   writes a word on its letter keys. RAM only, like every light; the
//!   patterns are kept in `agent-patterns.json`, per agent.

mod apps;
mod cards;
mod config;
mod effects;
mod engine;
mod keys;
mod limits;

pub use cards::SOURCE as CARD_SOURCE;

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Quiet, Request};
use crate::dto::{
    AgentApp, AgentAppPlan, AgentCard, AgentCardImage, AgentCardKind, AgentConnectOutcome, AgentConnectPlan,
    AgentConnections, AgentDisconnectOutcome, AgentLights, AgentLook, AgentNightMode, AgentServer, AgentSettings,
    AgentsState, Connection, Feature, FlashWrite, KeyboardInfo,
};
use crate::profiles::{Current, Profile, ProfileLighting};
use crate::settings;
use cards::{agent_of, CardBook, OnScreen, Stage};
use effects::FlashGuard;
use engine::{in_night, Engine, Now};
use kb_agent::protocol::{
    CardArgs, FocusAction, FocusArgs, PatternAction, PatternArgs, ProfileAction, ProfileArgs, Scene, SceneArgs,
    SceneStep, SpellArgs,
};
use kb_agent::{AgentError, Call, Channel, Command, ErrorCode, KeySpec, ServerInfo, SourceName};
use kb_protocol::sonix::level_factor;
use kb_protocol::sonix::screen::ScreenAnimation;
use keys::KeyIndex;
use limits::{Limits, DAY_MS, HOUR_MS};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as AtomicOrdering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Condvar;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const EVENT_AGENTS: &str = "agents";
/// An agent's screen card for the page to draw (`dto::AgentCard`).
pub const EVENT_AGENT_CARD: &str = "agent-card";
const FILE: &str = "agents.json";
const VERSION: u32 = 1;
/// A frame while lights are on: 20 a second, the app's live rate.
const TICK: Duration = Duration::from_millis(50);
/// While only lights kept for the user's return wait: a look at the
/// user's input now and then.
const WATCH: Duration = Duration::from_millis(250);
/// Otherwise a look now and then (the pipe server, the page).
const IDLE: Duration = Duration::from_secs(1);
/// The page hears of changes at most this often.
const EMIT_EVERY_MS: u64 = 250;
/// A pipe someone else holds is tried again this often.
const SERVER_RETRY_MS: u64 = 30_000;
/// The layer's brightness, in the keyboard's own steps.
const LEVEL: u8 = 5;
/// Night quiet's "faint": the colours at this share (about a fifth of the
/// light: the keyboard's LEDs are linear, the colours are not).
const NIGHT_DIM: f32 = 0.5;
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
    /// An agent's one-off block of this many minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub once: Option<u16>,
}

/// What an agent's `focus` did.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusDone {
    /// `start` found it running (nothing changed).
    pub already_running: bool,
    /// `start` went on after a pause.
    pub resumed: bool,
    /// Where it stands now; `None` when it is idle.
    pub now: Option<FocusNow>,
}

/// The user's profiles and the keyboard on the cable.
#[derive(Debug, Clone, Default)]
pub struct ProfilesNow {
    /// The built-in one first, then the user's.
    pub profiles: Vec<Profile>,
    /// What the keyboard on the cable has, as this computer recorded it;
    /// `None` without one.
    pub current: Option<Current>,
}

/// The screen, for cards.
#[derive(Debug, Clone)]
pub struct ScreenNow {
    /// A keyboard with a screen is on the cable, or why not.
    pub ready: Result<(), AgentError>,
    /// The live mode is on: cards may go up by themselves.
    pub live: bool,
}

/// Profile writes by all agents together in any 24 hours.
pub const PROFILE_WRITES_PER_DAY: u32 = limits::PROFILE_WRITES as u32;
/// How often the worker looks for an agent's card left on the screen with
/// nothing to take it back, when no card is on its way.
const LOOK_FOR_LEFT_CARDS_MS: u64 = 2_000;

/// Why a profile's lighting was not written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// The write guard refused it (several keyboards, another program,
    /// untested firmware): why.
    Blocked(String),
    /// No keyboard on the USB cable.
    NoUsb,
    /// The write failed: why.
    Failed(String),
}

/// Patterns one agent keeps.
pub const MAX_PATTERNS: usize = 32;
/// Agents with patterns.
const MAX_PATTERN_AGENTS: usize = 32;
const PATTERNS_FILE: &str = "agent-patterns.json";

/// The agents' saved scenes: agent → name → scene.
type Patterns = std::collections::BTreeMap<String, std::collections::BTreeMap<String, SceneArgs>>;

/// `spell`'s timing at each speed (1–5): how long a letter shows.
const SPELL_LETTER_MS: [u64; 5] = [900, 700, 500, 380, 280];
/// The dark gap when the same key comes twice in a row.
const SPELL_GAP_MS: u64 = 200;

/// The key a character is written on, by the model files' ids.
fn spell_key(c: char) -> Option<String> {
    if c.is_ascii_alphanumeric() {
        return Some(c.to_ascii_lowercase().to_string());
    }
    let id = match c {
        '-' => "minus",
        '=' => "equal",
        ',' => "comma",
        '.' => "period",
        '/' => "slash",
        ';' => "semicolon",
        '\'' => "quote",
        '[' => "lbracket",
        ']' => "rbracket",
        '\\' => "backslash",
        '`' => "grave",
        _ => return None,
    };
    Some(id.to_string())
}

/// A word as a scene: each character on its key, one after another; a
/// space a short pause; the same key twice in a row dark in between; the
/// word again after a pause, `times` times. The characters written and
/// those this keyboard has no key for.
fn spell_scene(
    spell: &kb_agent::protocol::Spell,
    has: impl Fn(&str) -> bool,
) -> Result<(Scene, String, Vec<String>), AgentError> {
    let on = SPELL_LETTER_MS[(spell.speed.clamp(1, 5) - 1) as usize];
    let mut steps = Vec::new();
    let mut written = String::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut t = 0u64;
    let mut last: Option<String> = None;
    for c in spell.text.chars() {
        if c.is_whitespace() {
            t += on;
            last = None;
            written.push(' ');
            continue;
        }
        let Some(id) = spell_key(c).filter(|id| has(id)) else {
            if !skipped.contains(&c.to_string()) {
                skipped.push(c.to_string());
            }
            continue;
        };
        if last.as_deref() == Some(id.as_str()) {
            t += SPELL_GAP_MS;
        }
        let keys = KeySpec::parse(&id)?;
        steps.push(SceneStep { keys, color: spell.color, effect: kb_agent::Effect::solid(), at_ms: t, for_ms: on });
        written.push(c);
        t += on;
        last = Some(id);
    }
    if steps.is_empty() {
        return Err(AgentError::new(ErrorCode::BadValue, "None of these characters has a key on this keyboard.")
            .with_hint("Letters a-z, digits and - = , . / ; ' [ ] \\ ` can be written."));
    }
    // Written again after a pause of two letters; the light ends with the
    // last letter.
    let pause = if spell.times > 1 { 2 * on } else { 0 };
    let length = t + pause;
    let total = (length * u64::from(spell.times) - pause).min(kb_agent::values::MAX_SIGNAL_DURATION.as_millis() as u64);
    let scene = Scene {
        steps,
        length_ms: length,
        repeat: spell.times,
        duration: Duration::from_millis(total),
        id: spell.id.clone(),
        save: None,
        play: true,
    };
    Ok((scene, written.trim().to_string(), skipped))
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
    /// Tell the coding agents' hooks whether they may start Keymotion
    /// (`kb_hooks::setup::mark_stopped`): `Some` when the user quit it or
    /// turned agent access off, `None` when they may again.
    fn note_stopped(&self, why: Option<kb_hooks::setup::Stopped>);
    /// The focus timer, for an agent: `action`, and on `start` one block
    /// of `minutes` instead of the user's sequence.
    fn focus_control(&self, action: FocusAction, minutes: Option<u16>) -> Result<FocusDone, String>;
    fn profiles(&self) -> Result<ProfilesNow, String>;
    /// Write `profile`'s lighting to the keyboard on the cable (the write
    /// guard applies). Whether it was written (`false`: it had it).
    fn write_lighting(&self, profile: &Profile) -> Result<bool, WriteError>;
    /// A JSON file of the agents' in the app's settings folder, as last
    /// saved; and saving one.
    fn load_json(&self, file: &str) -> Option<Value>;
    fn save_json(&self, file: &str, value: &Value) -> Result<(), String>;
    fn screen(&self) -> ScreenNow;
    /// Ask the page to draw an agent's card.
    fn request_card(&self, card: &AgentCard);
    /// Hand a drawn card to the live mode.
    fn submit_card(&self, anim: ScreenAnimation, name: &str) -> Result<(), String>;
    /// Agents' cards in the live mode now.
    fn cards_on_screen(&self) -> OnScreen;
    /// The user's own picture comes back after `ms`, unless another card
    /// comes first.
    fn card_return_after(&self, ms: u64);
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
    /// Frames the flash guard changed since it was last logged, and when.
    held: u32,
    held_logged: Option<u64>,
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
    /// Held while `agents.json` is written, so two saves never cross and
    /// the last one written is the newest.
    saving: Mutex<()>,
    /// Open the user's pipe (not in tests, which serve their own).
    serve_pipe: bool,
    wake: Mutex<Option<Sender<()>>>,
    /// Agents' screen cards; `cards_moved` wakes a `card --wait`.
    cards: Mutex<CardBook>,
    cards_moved: Condvar,
    /// Profile writes and cards in their windows, all agents together.
    limits: Mutex<Limits>,
    /// Numbers the slots of profile writes (to give one back).
    write_seq: AtomicU64,
    /// The page listens for `agent-card` and can draw a card.
    drawer: AtomicBool,
    /// When the worker next looks for a card left on the screen.
    next_look: AtomicU64,
    /// The agents' saved scenes.
    patterns: Mutex<Patterns>,
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

/// A write the device refused or that failed, as an agent's error: the
/// write guard's refusals (`device::WriteBlock`) are `write_blocked`.
fn write_error(e: WriteError) -> AgentError {
    match e {
        WriteError::Blocked(why) => {
            AgentError::new(ErrorCode::WriteBlocked, format!("Keymotion did not write the keyboard: {why}."))
                .with_hint("The user can sort it out in Keymotion; do not retry until then.")
        }
        WriteError::NoUsb => needs_usb(),
        WriteError::Failed(why) => AgentError::new(ErrorCode::Internal, format!("The keyboard was not written: {why}")),
    }
}

/// The page cannot draw cards now.
fn no_drawer() -> AgentError {
    AgentError::new(ErrorCode::Unsupported, "Keymotion's window has not loaded, so it cannot draw a card now.")
        .with_hint("Ask the user to open Keymotion's window once; cards work from then on.")
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
        let limits: Limits =
            host.load_json(limits::FILE).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            host,
            info,
            engine: Mutex::new(Engine::new(settings)),
            index: Mutex::new(None),
            server: Mutex::new(ServerSlot::default()),
            saving: Mutex::new(()),
            serve_pipe,
            wake: Mutex::new(None),
            cards: Mutex::new(CardBook::default()),
            cards_moved: Condvar::new(),
            limits: Mutex::new(limits),
            write_seq: AtomicU64::new(0),
            drawer: AtomicBool::new(false),
            next_look: AtomicU64::new(0),
            patterns: Mutex::new(Patterns::new()),
        })
        .with_patterns()
    }

    /// The saved patterns, from the host.
    fn with_patterns(self: Arc<Self>) -> Arc<Self> {
        let loaded = self.host.load_json(PATTERNS_FILE).and_then(|v| serde_json::from_value::<Patterns>(v).ok());
        if let Some(p) = loaded {
            *lock(&self.patterns) = p;
        }
        self
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

    /// The agents' lights can be seen on the keys now (dimmed for the night
    /// or not).
    fn shows(&self, s: &AgentSettings) -> bool {
        matches!(self.lights_now(s), AgentLights::Shown | AgentLights::NightDim)
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
        let label = match &command {
            Command::Hook(args) => args.label.clone(),
            _ => None,
        };
        let result = match command {
            Command::Status(_) => Ok(self.status(call.peer.protocol)),
            Command::Keys(_) => self.keys(),
            Command::Hello(_) => Err(AgentError::new(ErrorCode::Internal, "hello was not answered by the session.")),
            Command::Focus(args) => self.focus(call, &args),
            Command::Profile(args) => self.profile(call, &args),
            Command::Card(args) => self.card(call, &args),
            Command::Scene(args) => self.scene(call, &args),
            Command::Pattern(args) => self.pattern(call, &args),
            Command::Spell(args) => self.spell(call, &args),
            other => self.change(call, other).inspect(|data| {
                // F: a long task done puts a card up, when the user lets it.
                if let Some(ms) = data.get("turn_ms").and_then(Value::as_u64) {
                    self.long_task(call.source, ms, label);
                }
            }),
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
        let visible = lights.is_ok() && self.shows(&self.settings());
        let (result, effects, settings) = {
            let mut e = lock(&self.engine);
            e.set_visible(visible, now.mono);
            e.observe_idle(idle, now.mono);
            e.prune(now.mono);
            let (result, effects) = e.command(call.source, command, lights.as_deref().map_err(Clone::clone), now);
            (result, effects, *e.settings())
        };
        if effects.settings {
            self.persist();
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

    // --------------------------------------- focus, profiles and the screen

    /// Run a request the app carries out itself: access and the rate first,
    /// then `work`; the outcome goes in the log.
    fn carried(
        &self,
        call: Call<'_>,
        name: &'static str,
        work: impl FnOnce() -> Result<Value, AgentError>,
    ) -> Result<Value, AgentError> {
        self.carried_as(call, name, true, work)
    }

    /// [`Self::carried`]; `count: false` when `work` counts the request
    /// itself (a light the engine puts up).
    fn carried_as(
        &self,
        call: Call<'_>,
        name: &'static str,
        count: bool,
        work: impl FnOnce() -> Result<Value, AgentError>,
    ) -> Result<Value, AgentError> {
        let now = self.now();
        let gate = {
            let mut e = lock(&self.engine);
            if count {
                e.gate(call.source, now)
            } else {
                e.allowed()
            }
        };
        let result = gate.and_then(|()| work());
        let id = result.as_ref().ok().and_then(|d| d.get("id")).and_then(Value::as_str).map(str::to_string);
        lock(&self.engine).note(call.source, name, result.as_ref().err().map(|e| e.code), id, now);
        self.wake();
        result
    }

    fn permission_off(what: &str, switch: &str) -> AgentError {
        AgentError::new(ErrorCode::PermissionOff, format!("The user has not let agents {what}.")).with_hint(format!(
            "Only the user can allow it, in Keymotion › Tools › AI agents › {switch}. Do not retry."
        ))
    }

    fn write_flash_missing(what: &str) -> AgentError {
        AgentError::new(ErrorCode::WriteFlashFlagMissing, format!("This {what} writes the keyboard's memory."))
            .with_hint("Add --write-flash: the user allowed it, and you know it writes the memory.")
    }

    /// `focus`: the focus timer. `status` reads; the rest needs the user's
    /// focus permission.
    fn focus(&self, call: Call<'_>, args: &FocusArgs) -> Result<Value, AgentError> {
        let f = args.validate()?;
        if !f.action.changes() {
            return Ok(json!({ "focus": self.host.focus() }));
        }
        self.carried(call, "focus", || {
            if !self.settings().focus {
                return Err(Self::permission_off("use the focus timer", "Focus timer"));
            }
            let done = self.host.focus_control(f.action, f.minutes).map_err(|e| {
                AgentError::new(ErrorCode::Unsupported, format!("The focus timer did not take it: {e}"))
            })?;
            // `--for` starts a block; a timer already going keeps its own.
            let kept = done.already_running || done.resumed;
            let mut answer = json!({
                "action": f.action.as_str(),
                "already_running": done.already_running,
                "resumed": done.resumed,
                "minutes": if kept { None } else { f.minutes },
                "focus": done.now,
            });
            if kept && f.minutes.is_some() {
                answer["minutes_not_applied"] = json!(if done.resumed {
                    "the timer was paused: it went on with what it had"
                } else {
                    "the timer was running: it keeps what it has"
                });
            }
            Ok(answer)
        })
    }

    /// The profile `name` names: its id, or its name in any case; the
    /// built-in one is `default` (its id) or `Default`.
    fn find_profile<'a>(list: &'a [Profile], name: &str) -> Result<&'a Profile, AgentError> {
        let lower = name.to_lowercase();
        list.iter()
            .find(|p| p.id == name)
            .or_else(|| list.iter().find(|p| Self::profile_name(p).to_lowercase() == lower))
            .ok_or_else(|| {
                AgentError::new(
                    ErrorCode::BadValue,
                    format!("There is no profile '{}'.", kb_agent::values::shown(name)),
                )
                .with_hint("keymotion-cli profile list shows them.")
            })
    }

    fn profile_name(p: &Profile) -> String {
        if p.id == crate::profiles::DEFAULT_ID {
            "Default".to_string()
        } else {
            p.name.clone()
        }
    }

    /// `profile list|show|apply`.
    fn profile(&self, call: Call<'_>, args: &ProfileArgs) -> Result<Value, AgentError> {
        let p = args.validate()?;
        let all = self.host.profiles().map_err(|e| AgentError::new(ErrorCode::Internal, e))?;
        match p.action {
            ProfileAction::List => {
                let model = all.current.as_ref().map(|c| c.model_id.clone());
                let list: Vec<Value> = all
                    .profiles
                    .iter()
                    .map(|pr| {
                        let lighting = match &pr.lighting {
                            Some(ProfileLighting::Effect { .. }) => json!("effect"),
                            Some(ProfileLighting::PerKey { .. }) => json!("per_key"),
                            None => Value::Null,
                        };
                        let on_keyboard = all.current.as_ref().is_some_and(|c| {
                            pr.fits(&c.model_id)
                                && pr.lighting.is_some()
                                && pr.part_matches(crate::profiles::ProfilePart::Lighting, c)
                        });
                        json!({
                            "id": pr.id,
                            "name": Self::profile_name(pr),
                            "built_in": pr.id == crate::profiles::DEFAULT_ID,
                            "parts": pr.parts(),
                            "lighting": lighting,
                            "fits": model.as_deref().is_none_or(|m| pr.fits(m)),
                            "lighting_on_keyboard": on_keyboard,
                        })
                    })
                    .collect();
                Ok(json!({ "profiles": list, "model": model }))
            }
            ProfileAction::Show => self.carried_as(call, "profile", false, || {
                let name = p.name.as_deref().unwrap_or_default();
                let profile = Self::find_profile(&all.profiles, name)?;
                let index = self.lights()?;
                if !profile.fits(&index.model_id) {
                    return Err(AgentError::new(ErrorCode::Unsupported, "This profile is for another keyboard model."));
                }
                let colors = match &profile.lighting {
                    Some(l @ ProfileLighting::PerKey { .. }) => l.per_key().map(|(keys, _)| keys).unwrap_or_default(),
                    Some(ProfileLighting::Effect { .. }) => {
                        return Err(AgentError::new(
                            ErrorCode::Unsupported,
                            "This profile's lighting is a built-in effect; it shows only once written.",
                        )
                        .with_hint("profile apply NAME --write-flash writes it, if the user allows profile writes."))
                    }
                    None => return Err(AgentError::new(ErrorCode::BadValue, "This profile has no lighting part.")),
                };
                let places = index.resolve(&KeySpec::all())?;
                let keys: Vec<(u8, effects::Place, kb_protocol::sonix::Rgb)> = places
                    .iter()
                    .map(|&(led, place)| {
                        let color = colors.iter().find(|(l, _)| *l == led).map_or([0, 0, 0], |(_, c)| *c);
                        (led, place, color)
                    })
                    .collect();
                let id = format!("{}:profile", call.source.as_str());
                let for_ms = p.duration.map_or(engine::LIGHT_DEFAULT_MS, kb_agent::values::duration_ms);
                let now = self.now();
                let mut answer = lock(&self.engine).paint(call.source, &id, keys, &index.model_id, for_ms, now)?;
                answer["profile"] = json!(Self::profile_name(profile));
                Ok(answer)
            }),
            ProfileAction::Apply => self.carried(call, "profile", || {
                if !self.settings().profiles {
                    return Err(Self::permission_off("write profiles", "Profile lighting"));
                }
                if !p.write_flash {
                    return Err(Self::write_flash_missing("command"));
                }
                let name = p.name.as_deref().unwrap_or_default();
                let profile = Self::find_profile(&all.profiles, name)?;
                let current = match (&all.current, self.host.cable()) {
                    (Some(c), _) => c.clone(),
                    (None, Cable::NotUsb) => return Err(needs_usb()),
                    (None, _) => return Err(no_keyboard()),
                };
                if !profile.fits(&current.model_id) {
                    return Err(AgentError::new(ErrorCode::Unsupported, "This profile is for another keyboard model."));
                }
                if profile.lighting.is_none() {
                    return Err(AgentError::new(ErrorCode::BadValue, "This profile has no lighting part."));
                }
                let id = format!("profile:{}", profile.id);
                if profile.part_matches(crate::profiles::ProfilePart::Lighting, &current) {
                    return Ok(json!({ "id": id, "profile": Self::profile_name(profile), "changed": false }));
                }
                if self.host.flash_write().is_some() {
                    return Err(AgentError::new(ErrorCode::BusyFlashWrite, "The keyboard's memory is being written.")
                        .with_retry_after(Duration::from_secs(5)));
                }
                let agent = agent_of(call.source.as_str()).to_string();
                // The slot is taken before the write, under the lock, and
                // given back when nothing was written.
                let slot = format!("w{}", self.write_seq.fetch_add(1, AtomicOrdering::SeqCst));
                let wall = self.wall();
                {
                    let mut l = lock(&self.limits);
                    if let Err(wait) = l.profile_writes.room(limits::PROFILE_WRITES, wall, DAY_MS) {
                        return Err(AgentError::new(
                            ErrorCode::RateLimited,
                            format!(
                                "Agents wrote profiles {PROFILE_WRITES_PER_DAY} times in the last 24 hours; the keyboard's memory is spared for now."
                            ),
                        )
                        .with_retry_after(Duration::from_millis(wait)));
                    }
                    l.profile_writes.take(&agent, &slot, wall);
                    self.store_limits(&l);
                }
                let result = self.host.write_lighting(profile);
                let l = {
                    let mut l = lock(&self.limits);
                    if !matches!(result, Ok(true)) {
                        l.profile_writes.give_back(&slot);
                        self.store_limits(&l);
                    }
                    l.clone()
                };
                let written = result.map_err(write_error)?;
                Ok(json!({
                    "id": id,
                    "profile": Self::profile_name(profile),
                    "changed": written,
                    "writes_in_24h": l.profile_writes.count(wall, DAY_MS),
                    "agent_writes_in_24h": l.profile_writes.count_for(&agent, wall, DAY_MS),
                    "writes_per_24h": PROFILE_WRITES_PER_DAY,
                }))
            }),
        }
    }

    /// Now, as the limits count it: Unix ms.
    fn wall(&self) -> u64 {
        self.host.wall_ms().max(0.0) as u64
    }

    /// Keep the limits' times on disk (after every change: a few an hour).
    fn store_limits(&self, l: &Limits) {
        let saved =
            serde_json::to_value(l).map_err(|e| e.to_string()).and_then(|v| self.host.save_json(limits::FILE, &v));
        if let Err(e) = saved {
            tracing::warn!("agents' limits not saved: {e}");
        }
    }

    /// Give card `id`'s count back: it never reached the screen.
    fn give_card_back(&self, id: &str) {
        let mut l = lock(&self.limits);
        if l.cards.give_back(id) {
            self.store_limits(&l);
        }
    }

    /// The page listens for cards (`true`) or went away (`false`).
    pub fn set_card_drawer(&self, ready: bool) {
        if self.drawer.swap(ready, AtomicOrdering::SeqCst) != ready {
            tracing::debug!(ready, "agent card drawer");
        }
    }

    // ------------------------------------------- scenes, patterns, spelling

    /// What a light just put up is hidden by, as `light` says it.
    fn hidden(&self) -> Value {
        match self.lights_now(&self.settings()) {
            AgentLights::Game => json!("game"),
            AgentLights::NightOff => json!("night"),
            _ => Value::Null,
        }
    }

    /// Play `scene` as the source's light `id`.
    fn play(&self, call: Call<'_>, scene: &Scene, id: &str) -> Result<Value, AgentError> {
        let index = self.lights()?;
        let now = self.now();
        let idle = self.host.idle_ms();
        let visible = self.shows(&self.settings());
        let mut answer = {
            let mut e = lock(&self.engine);
            e.set_visible(visible, now.mono);
            e.observe_idle(idle, now.mono);
            e.prune(now.mono);
            e.scene(call.source, id, scene, &index, now)?
        };
        answer["hidden"] = self.hidden();
        Ok(answer)
    }

    /// Keep `args` as `agent`'s pattern `name`.
    fn keep_pattern(&self, agent: &str, name: &str, args: &SceneArgs) -> Result<bool, AgentError> {
        let snapshot = {
            let mut all = lock(&self.patterns);
            if !all.contains_key(agent) && all.len() >= MAX_PATTERN_AGENTS {
                return Err(AgentError::new(ErrorCode::RateLimited, "Too many agents keep patterns already."));
            }
            let mine = all.entry(agent.to_string()).or_default();
            if !mine.contains_key(name) && mine.len() >= MAX_PATTERNS {
                return Err(AgentError::new(
                    ErrorCode::BadArgument,
                    format!("{agent} keeps {MAX_PATTERNS} patterns already; delete one first."),
                )
                .with_hint("keymotion-cli pattern list, then pattern delete NAME."));
            }
            let replaced = mine.insert(name.to_string(), args.to_pattern()).is_some();
            (serde_json::to_value(&*all), replaced)
        };
        let (value, replaced) = snapshot;
        let value = value.map_err(|e| AgentError::new(ErrorCode::Internal, format!("Patterns not written: {e}")))?;
        self.host
            .save_json(PATTERNS_FILE, &value)
            .map_err(|e| AgentError::new(ErrorCode::Internal, format!("Patterns not saved: {e}")))?;
        Ok(replaced)
    }

    /// `scene`: play the agent's timeline; with `save`, keep it.
    fn scene(&self, call: Call<'_>, args: &SceneArgs) -> Result<Value, AgentError> {
        let scene = args.validate()?;
        self.carried_as(call, "scene", !scene.play, || {
            let agent = agent_of(call.source.as_str()).to_string();
            let mut answer = if scene.play {
                let id = scene
                    .id
                    .as_ref()
                    .map_or_else(|| format!("{}:scene", call.source.as_str()), |i| i.as_str().to_string());
                self.play(call, &scene, &id)?
            } else {
                json!({})
            };
            if let Some(name) = &scene.save {
                let replaced = self.keep_pattern(&agent, name, args)?;
                answer["saved"] = json!(name);
                answer["replaced_pattern"] = json!(replaced);
            }
            Ok(answer)
        })
    }

    /// `pattern list|play|delete`: the agent's saved scenes.
    fn pattern(&self, call: Call<'_>, args: &PatternArgs) -> Result<Value, AgentError> {
        let p = args.validate()?;
        let agent = agent_of(call.source.as_str()).to_string();
        let name = p.name.clone().unwrap_or_default();
        match p.action {
            PatternAction::List => {
                let all = lock(&self.patterns);
                let list: Vec<Value> = all
                    .get(&agent)
                    .into_iter()
                    .flatten()
                    .map(|(name, a)| {
                        let length = a.validate().map(|s| s.length_ms).unwrap_or(0);
                        json!({ "name": name, "steps": a.steps.len(), "length_ms": length, "repeat": a.repeat.unwrap_or(1) })
                    })
                    .collect();
                Ok(json!({ "agent": agent, "patterns": list, "max": MAX_PATTERNS }))
            }
            PatternAction::Play => self.carried_as(call, "pattern", false, || {
                let saved = lock(&self.patterns).get(&agent).and_then(|m| m.get(&name)).cloned().ok_or_else(|| {
                    AgentError::new(ErrorCode::BadValue, format!("{agent} has no pattern '{name}'."))
                        .with_hint("keymotion-cli pattern list shows yours; scene FILE --save NAME keeps one.")
                })?;
                let args = SceneArgs {
                    repeat: p.repeat.or(saved.repeat),
                    for_ms: p.for_ms.or(if p.repeat.is_some() { None } else { saved.for_ms }),
                    ..saved
                };
                let scene = args.validate()?;
                let id =
                    p.id.as_ref()
                        .map_or_else(|| format!("{}:pattern-{name}", call.source.as_str()), |i| i.as_str().to_string());
                let mut answer = self.play(call, &scene, &id)?;
                answer["pattern"] = json!(name);
                Ok(answer)
            }),
            PatternAction::Delete => self.carried(call, "pattern", || {
                let value = {
                    let mut all = lock(&self.patterns);
                    let gone = all.get_mut(&agent).and_then(|m| m.remove(&name)).is_some();
                    if !gone {
                        return Ok(json!({ "deleted": false, "pattern": name }));
                    }
                    all.retain(|_, m| !m.is_empty());
                    serde_json::to_value(&*all)
                };
                let value =
                    value.map_err(|e| AgentError::new(ErrorCode::Internal, format!("Patterns not written: {e}")))?;
                self.host
                    .save_json(PATTERNS_FILE, &value)
                    .map_err(|e| AgentError::new(ErrorCode::Internal, format!("Patterns not saved: {e}")))?;
                Ok(json!({ "deleted": true, "pattern": name }))
            }),
        }
    }

    /// `spell`: a word on its letter keys.
    fn spell(&self, call: Call<'_>, args: &SpellArgs) -> Result<Value, AgentError> {
        let spell = args.validate()?;
        self.carried_as(call, "spell", false, || {
            let index = self.lights()?;
            let (scene, written, skipped) =
                spell_scene(&spell, |id| KeySpec::parse(id).is_ok_and(|k| index.resolve(&k).is_ok()))?;
            let id =
                spell.id.as_ref().map_or_else(|| format!("{}:spell", call.source.as_str()), |i| i.as_str().to_string());
            let mut answer = self.play(call, &scene, &id)?;
            answer["written"] = json!(written);
            answer["skipped"] = json!(skipped);
            Ok(answer)
        })
    }

    /// The live mode's name for a card: its title, else its first words.
    fn card_name(title: Option<&str>, text: Option<&str>) -> String {
        let name = title.or_else(|| text.and_then(|t| t.lines().next())).unwrap_or("Picture");
        let short: String = name.chars().take(32).collect();
        format!("Agent: {short}")
    }

    /// Whether the screen may take an agent's card now.
    fn screen_ready(&self) -> Result<(), AgentError> {
        let screen = self.host.screen();
        screen.ready?;
        if !screen.live {
            return Err(AgentError::new(ErrorCode::PermissionOff, "The live screen is off in Keymotion.")
                .with_hint("Only the user can turn it on, in Keymotion › Screen › Live screen. Do not retry."));
        }
        Ok(())
    }

    /// `card`: a card on the keyboard's screen.
    fn card(&self, call: Call<'_>, args: &CardArgs) -> Result<Value, AgentError> {
        let c = args.validate()?;
        let result = self.carried(call, "card", || {
            let s = self.settings();
            if !s.screen {
                return Err(Self::permission_off("put cards on the screen", "Screen cards"));
            }
            if !c.write_flash {
                return Err(Self::write_flash_missing("card"));
            }
            self.screen_ready()?;
            if !self.drawer.load(AtomicOrdering::SeqCst) {
                return Err(no_drawer());
            }
            let color = c.color.map(|k| [k.r, k.g, k.b]);
            let image_bytes = c.image.as_ref().map(|(b, _)| b.as_slice()).unwrap_or_default();
            let content = cards::content_id(&[
                c.title.as_deref().unwrap_or_default().as_bytes(),
                c.text.as_deref().unwrap_or_default().as_bytes(),
                image_bytes,
                &color.unwrap_or([0; 3]),
            ]);
            let agent = agent_of(call.source.as_str()).to_string();
            let now = self.host.mono_ms();
            let mut book = lock(&self.cards);
            if let Some(id) = book.same(content) {
                let stage = book.stage(&id).unwrap_or(Stage::Live);
                return Ok(json!({ "id": id, "skipped": "same_content", "state": stage.name() }));
            }
            let wall = self.wall();
            let (id, sent, mine) = {
                let mut l = lock(&self.limits);
                if let Err(wait) = l.cards.room(usize::from(s.screen_per_hour), wall, HOUR_MS) {
                    return Err(AgentError::new(
                        ErrorCode::RateLimited,
                        format!(
                            "Agents put {} cards on the screen this hour, as many as the user allows.",
                            s.screen_per_hour
                        ),
                    )
                    .with_retry_after(Duration::from_millis(wait)));
                }
                let show_ms = kb_agent::values::duration_ms(c.duration);
                let name = Self::card_name(c.title.as_deref(), c.text.as_deref());
                let id = book.start(content, show_ms, &name, now);
                l.cards.take(&agent, &id, wall);
                self.store_limits(&l);
                (id, l.cards.count(wall, HOUR_MS), l.cards.count_for(&agent, wall, HOUR_MS))
            };
            drop(book);
            // A `--wait` on the card this one replaced answers now.
            self.cards_moved.notify_all();
            let show_ms = kb_agent::values::duration_ms(c.duration);
            let image = c.image.as_ref().map(|(bytes, info)| AgentCardImage {
                mime: info.format.mime().to_string(),
                data: kb_agent::image::encode(bytes),
                width: info.width,
                height: info.height,
                animated: info.animated,
            });
            self.host.request_card(&AgentCard {
                id: id.clone(),
                kind: AgentCardKind::Text,
                agent: agent.clone(),
                title: c.title.clone(),
                text: c.text.clone(),
                image,
                color,
                minutes: None,
                label: None,
                show_s: (show_ms / 1000) as u32,
            });
            tracing::info!(%agent, %id, picture = c.image.is_some(), "agent card asked of the page");
            Ok(json!({
                "id": id,
                "state": "drawing",
                "sent_this_hour": sent,
                "agent_sent_this_hour": mine,
                "per_hour": s.screen_per_hour,
            }))
        })?;
        let (Some(wait), Some(id)) = (c.wait, result.get("id").and_then(Value::as_str).map(str::to_string)) else {
            return Ok(self.card_answer(result));
        };
        if result.get("skipped").is_some() {
            return Ok(self.card_answer(result));
        }
        // `--wait`: until it is on the screen, or failed, or the time is up.
        let deadline = Instant::now() + wait;
        let mut book = lock(&self.cards);
        loop {
            let stage = book.stage(&id);
            if stage.as_ref().is_none_or(Stage::settled) {
                break;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            book = self.cards_moved.wait_timeout(book, left).map(|(g, _)| g).unwrap_or_else(|p| p.into_inner().0);
        }
        let stage = book.stage(&id);
        drop(book);
        let mut data = result;
        match stage {
            Some(Stage::Failed(why)) => Err(AgentError::new(ErrorCode::Unsupported, why)),
            Some(stage) => {
                data["state"] = json!(stage.name());
                Ok(self.card_answer(data))
            }
            // Settled long ago, past what is remembered.
            None => {
                data["state"] = json!("unknown");
                Ok(self.card_answer(data))
            }
        }
    }

    /// A card's answer: where it is, and the screen's budget.
    fn card_answer(&self, mut data: Value) -> Value {
        let state = data["state"].as_str().unwrap_or_default().to_string();
        data["shown"] = json!(state == "shown");
        data["queued"] = json!(matches!(state.as_str(), "drawing" | "waiting"));
        data["screen_budget"] = json!(self.host.screen_budget());
        data
    }

    /// F: a session's turn of `ms` ended with "done": a card "Done · N min"
    /// when the user lets agents use the screen and keeps F on, the turn
    /// took at least the threshold, and the agent has room this hour.
    fn long_task(&self, source: &SourceName, ms: u64, label: Option<String>) {
        let s = self.settings();
        if !s.enabled || !s.screen || !s.long_task.on || ms < u64::from(s.long_task.after_s) * 1000 {
            return;
        }
        if self.screen_ready().is_err() || !self.drawer.load(AtomicOrdering::SeqCst) {
            return;
        }
        let agent = agent_of(source.as_str()).to_string();
        let now = self.host.mono_ms();
        let wall = self.wall();
        let minutes = (ms as f64 / 60_000.0).round() as u32;
        let show_ms = u64::from(s.long_task.show_s) * 1000;
        let id = {
            let mut book = lock(&self.cards);
            let mut l = lock(&self.limits);
            if l.cards.room(usize::from(s.screen_per_hour), wall, HOUR_MS).is_err() {
                tracing::debug!(%agent, "long task done: no card, the hour's cards are used");
                return;
            }
            let content = cards::content_id(&[b"long", agent.as_bytes(), &minutes.to_le_bytes(), &now.to_le_bytes()]);
            let id = book.start(content, show_ms, &format!("Agent: done in {minutes} min"), now);
            l.cards.take(&agent, &id, wall);
            self.store_limits(&l);
            id
        };
        self.cards_moved.notify_all();
        self.host.request_card(&AgentCard {
            id: id.clone(),
            kind: AgentCardKind::LongTask,
            agent: agent.clone(),
            title: None,
            text: None,
            image: None,
            color: None,
            minutes: Some(minutes),
            label,
            show_s: s.long_task.show_s,
        });
        lock(&self.engine).note(source, "card", None, Some(id), self.now());
        tracing::info!(%agent, minutes, "long task done: card asked of the page");
    }

    /// The page drew card `id`: to the live mode.
    pub fn card_drawn(&self, id: &str, anim: ScreenAnimation) -> Result<(), String> {
        let now = self.host.mono_ms();
        let name = lock(&self.cards).drawn(id, now)?;
        let sent = self.host.submit_card(anim, &name);
        if let Err(e) = &sent {
            if lock(&self.cards).failed(id, &format!("The live screen did not take it: {e}")) {
                self.give_card_back(id);
            }
        }
        self.cards_moved.notify_all();
        self.wake();
        sent
    }

    /// The page could not draw card `id`.
    pub fn card_failed(&self, id: &str, why: &str) {
        let why: String = why.chars().filter(|c| !c.is_control()).take(200).collect();
        let failed = lock(&self.cards).failed(id, &format!("The card could not be drawn: {why}"));
        if failed {
            tracing::warn!(%id, "agent card not drawn: {why}");
            self.give_card_back(id);
        }
        self.cards_moved.notify_all();
    }

    /// The worker's look at agents' cards: every pass while one is on its
    /// way, and now and then otherwise, for a card left on the screen with
    /// nothing to take it back (one from before a restart).
    fn watch_cards(&self) {
        let now = self.host.mono_ms();
        if !lock(&self.cards).busy() {
            if now < self.next_look.load(AtomicOrdering::SeqCst) {
                return;
            }
            self.next_look.store(now + LOOK_FOR_LEFT_CARDS_MS, AtomicOrdering::SeqCst);
        }
        let on = self.host.cards_on_screen();
        let due = lock(&self.cards).tick(now, on);
        for id in &due.failed {
            self.give_card_back(id);
        }
        if let Some(ms) = due.return_after_ms {
            self.host.card_return_after(ms);
        }
        if due.changed {
            self.cards_moved.notify_all();
        }
    }

    /// Write the newest settings: read from the engine while no other save
    /// runs, so an older copy is never written last.
    fn persist(&self) {
        let _saving = lock(&self.saving);
        let settings = self.settings();
        if let Err(e) = self.host.save(&settings) {
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
        let (signals, sessions): (Vec<Value>, Vec<Value>) = {
            let index = self.lights().ok();
            let mut e = lock(&self.engine);
            if let Some(index) = &index {
                e.refresh_keys(index);
            }
            let sessions = e
                .sessions(now.mono)
                .into_iter()
                .map(|s| {
                    json!({
                        "number": s.number,
                        "source": s.source,
                        "agent": s.agent,
                        "label": s.label,
                        "waiting": s.waiting,
                    })
                })
                .collect();
            let signals = e
                .infos(now.mono, index.is_some())
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
                .collect();
            (signals, sessions)
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
            "sessions": sessions,
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
            let stopped = {
                let mut slot = lock(&self.server);
                if slot.pipe.as_ref().is_some_and(|p| !p.accepting()) {
                    tracing::warn!("agent pipe stopped accepting; opening it again");
                    slot.retry_at = 0;
                    slot.pipe.take()
                } else {
                    if slot.pipe.is_some() || now < slot.retry_at {
                        return;
                    }
                    None
                }
            };
            // Its connections end (a few seconds at most) outside the lock.
            drop(stopped);
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
        let (settings, signals, sessions, log) = {
            let mut e = lock(&self.engine);
            if let Some(index) = &index {
                e.refresh_keys(index);
            }
            (*e.settings(), e.infos(now, index.is_some()), e.sessions(now), e.log_entries())
        };
        AgentsState {
            lights: self.lights_now(&settings),
            settings,
            signals,
            sessions,
            log,
            server: self.server_state(),
        }
    }

    /// The page saved its settings (any of them). They are written first
    /// and only then used: a save that fails changes nothing.
    pub fn save(&self, settings: AgentSettings) -> Result<AgentsState, String> {
        self.save_with(|_| settings)
    }

    /// Turn agent access on or off, and nothing else: read and written
    /// while no other save runs, so a setting the page or an agent changed
    /// meanwhile is never put back.
    pub fn set_enabled(&self, on: bool) -> Result<AgentsState, String> {
        self.save_with(|now| AgentSettings { enabled: on, ..now })
    }

    /// Save what `change` makes of the settings in force, under the save
    /// lock. Written first and only then used: a save that fails changes
    /// nothing.
    fn save_with(&self, change: impl FnOnce(AgentSettings) -> AgentSettings) -> Result<AgentsState, String> {
        let written = {
            let _saving = lock(&self.saving);
            let settings = change(self.settings()).sanitized();
            self.host.save(&settings).map(|()| {
                let mut e = lock(&self.engine);
                let toggled = e.settings().enabled != settings.enabled;
                if toggled {
                    tracing::info!(on = settings.enabled, "agent access");
                }
                e.set_settings(settings);
                (toggled, settings.enabled)
            })
        };
        if let Ok((true, enabled)) = written {
            self.host.note_stopped((!enabled).then_some(kb_hooks::setup::Stopped::AccessOff));
        }
        self.wake();
        written.map(|_| self.state())
    }

    /// The app starts: with access on, hooks may start it again; with it
    /// off, they may not.
    pub fn started(&self) {
        let on = self.settings().enabled;
        self.host.note_stopped((!on).then_some(kb_hooks::setup::Stopped::AccessOff));
    }

    /// The user chose "Quit" (`true`) or stayed after all (`false`): hooks
    /// leave a Keymotion the user quit closed. Nothing changes while access
    /// is off (that note stays).
    pub fn quitting(&self, quit: bool) {
        if self.settings().enabled {
            self.host.note_stopped(quit.then_some(kb_hooks::setup::Stopped::Quit));
        }
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

    /// "Undo" on a log entry. Settings changed again since stay as they
    /// are; the entry lists them (`kept`).
    pub fn undo(&self, seq: u32) -> Result<AgentsState, String> {
        let kept = lock(&self.engine).undo(seq)?;
        tracing::info!(seq, kept = kept.len(), "agent change undone");
        self.persist();
        self.wake();
        Ok(self.state())
    }

    // ----------------------------------------------------------- the worker

    /// One pass: watch the user's input, end lights whose time is up, send
    /// the frame, tell the page. Returns how long to sleep.
    pub fn tick(&self, w: &mut Worker) -> Duration {
        let now = self.host.mono_ms();
        self.ensure_server(now);
        self.watch_cards();
        let idle = self.host.idle_ms();
        let index = self.lights().ok();
        let settings = self.settings();
        // Hidden while a game wants the keyboard left alone: no frames at all.
        // Night quiet dims every light but the page's own try (the engine).
        let (hidden, night) = match self.lights_now(&settings) {
            AgentLights::Game => (true, 1.0),
            AgentLights::NightDim => (false, NIGHT_DIM),
            AgentLights::NightOff => (false, 0.0),
            _ => (false, 1.0),
        };
        let (frame, lit, waiting, dirty) = {
            let mut e = lock(&self.engine);
            e.set_visible(index.is_some() && !hidden && night > 0.0, now);
            e.observe_idle(idle, now);
            e.prune(now);
            e.apply_queued(now);
            let frame = match &index {
                Some(index) if !hidden => {
                    e.refresh_keys(index);
                    e.frame(now, night)
                }
                _ => Vec::new(),
            };
            (frame, e.lit(), e.waiting(), e.take_dirty())
        };
        let in_quiet = !settings.hide_in_game;
        if w.in_quiet != Some(in_quiet) {
            self.host.send(Request::SetAgentInQuiet(in_quiet));
            w.in_quiet = Some(in_quiet);
        }
        let wanted = frame.clone();
        let colors = w.guard.check(now, frame);
        if colors != wanted {
            w.held += 1;
        }
        if w.held > 0 && w.held_logged.is_none_or(|t| now.saturating_sub(t) >= 1000) {
            tracing::debug!(frames = w.held, "flash guard held keys that changed too fast");
            w.held = 0;
            w.held_logged = Some(now);
        }
        let out = (!colors.is_empty()).then(|| LiveFrame { colors, level: level_factor(LEVEL) });
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
        if lit || w.sent.is_some() || w.emit_pending {
            TICK
        } else if waiting {
            WATCH
        } else {
            IDLE
        }
    }

    /// The worker's pass, kept alive through a panic: the agents' layer is
    /// taken off the keys (no frame stays stuck), and the next pass starts
    /// afresh.
    pub fn tick_safely(&self, w: &mut Worker) -> Duration {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.tick(w))) {
            Ok(wait) => wait,
            Err(_) => {
                tracing::error!("agent lights: the worker failed; their layer is cleared");
                self.host.send(Request::SetLayer(LightLayer::Agent, None));
                *w = Worker::default();
                IDLE
            }
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
            once: state.once,
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

    fn load_json(&self, file: &str) -> Option<Value> {
        settings::load::<Value>(self.config_dir.as_deref()?, file, VERSION)
    }

    fn save_json(&self, file: &str, value: &Value) -> Result<(), String> {
        match &self.config_dir {
            Some(dir) => settings::save(dir, file, VERSION, value).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    fn publish(&self, state: &AgentsState) {
        let _ = self.app.emit(EVENT_AGENTS, state);
    }

    fn note_stopped(&self, why: Option<kb_hooks::setup::Stopped>) {
        let Some(homes) = homes() else { return };
        let done = match why {
            Some(why) => {
                let logon = kb_winsys::pipe::current_logon_id().ok();
                kb_hooks::setup::mark_stopped(&homes, why, logon)
            }
            None => kb_hooks::setup::clear_stopped(&homes),
        };
        if !done.is_empty() {
            tracing::info!(stopped = why.is_some(), files = done.len(), "agent hooks told whether to start Keymotion");
        }
    }

    fn focus_control(&self, action: FocusAction, minutes: Option<u16>) -> Result<FocusDone, String> {
        use crate::pomodoro::{PomodoroAction, PomodoroHandle};
        use tauri::Manager;
        let pomodoro = self.app.try_state::<Arc<PomodoroHandle>>().ok_or("the focus timer is not running")?;
        let action = match action {
            FocusAction::Start => PomodoroAction::Start,
            FocusAction::Pause => PomodoroAction::Pause,
            FocusAction::Skip => PomodoroAction::Skip,
            FocusAction::Stop => PomodoroAction::Stop,
            FocusAction::Status => return Ok(FocusDone { already_running: false, resumed: false, now: self.focus() }),
        };
        let done = pomodoro.agent_control(action, minutes)?;
        Ok(FocusDone { already_running: done.already_running, resumed: done.resumed, now: self.focus() })
    }

    fn profiles(&self) -> Result<ProfilesNow, String> {
        use tauri::Manager;
        let store = self.app.try_state::<Arc<Mutex<crate::profiles::ProfileStore>>>().ok_or("profiles unavailable")?;
        let mut profiles = vec![crate::profiles::default_profile()];
        profiles.extend(crate::lock_profiles(&store)?.profiles().iter().cloned());
        let current = crate::usb_model(&self.app).and_then(|m| crate::profile_current(&self.app, &m).ok());
        Ok(ProfilesNow { profiles, current })
    }

    fn write_lighting(&self, profile: &Profile) -> Result<bool, WriteError> {
        let model = crate::usb_model(&self.app).ok_or(WriteError::NoUsb)?;
        let current = crate::profile_current(&self.app, &model).map_err(WriteError::Failed)?;
        // The write guard's answer as it is (the write would say it again,
        // in words).
        if let Some(block) = self.device.call(Request::WriteBlocked).map_err(WriteError::Failed)? {
            return Err(WriteError::Blocked(block.to_string()));
        }
        let mut writer = crate::DeviceWriter { device: &self.device, config_dir: self.config_dir.clone() };
        let loaded = crate::profiles::load_part(profile, crate::profiles::ProfilePart::Lighting, &current, &mut writer)
            .map_err(WriteError::Failed)?;
        let written = loaded == crate::profiles::PartLoaded::Written;
        if written {
            tracing::info!(profile = %profile.id, "an agent wrote a profile's lighting");
            // The Lighting page reads the keyboard's record again.
            let _ = self.app.emit(crate::autoprofile::EVENT_AUTO_PROFILE_WRITTEN, ());
            crate::tray::refresh(&self.app);
        }
        Ok(written)
    }

    fn screen(&self) -> ScreenNow {
        use tauri::Manager;
        let keyboards = self.keyboards();
        let with_screen: Vec<&KeyboardInfo> =
            keyboards.iter().filter(|k| k.features.contains(&Feature::Screen)).collect();
        let ready = if with_screen.iter().any(|k| k.connection == Connection::Usb && k.has_command_channel) {
            Ok(())
        } else if !with_screen.is_empty() {
            Err(AgentError::new(ErrorCode::NeedsUsb, "The keyboard's screen works only over the USB cable.")
                .with_hint("Connect the keyboard with its USB cable."))
        } else if keyboards.is_empty() {
            Err(no_keyboard())
        } else {
            Err(AgentError::new(ErrorCode::Unsupported, "This keyboard has no screen."))
        };
        let live = self.app.try_state::<Arc<crate::live::LiveHandle>>().is_some_and(|l| l.enabled());
        ScreenNow { ready, live }
    }

    fn request_card(&self, card: &AgentCard) {
        let _ = self.app.emit(EVENT_AGENT_CARD, card);
    }

    fn submit_card(&self, anim: ScreenAnimation, name: &str) -> Result<(), String> {
        use tauri::Manager;
        let live = self.app.try_state::<Arc<crate::live::LiveHandle>>().ok_or("the live screen is not running")?;
        live.submit(anim, CARD_SOURCE, name).map(|_| ())
    }

    fn cards_on_screen(&self) -> OnScreen {
        use tauri::Manager;
        match self.app.try_state::<Arc<crate::live::LiveHandle>>() {
            Some(live) => {
                let (shown, waiting, back_planned) = live.card_status(CARD_SOURCE);
                OnScreen { shown, waiting, back_planned }
            }
            None => OnScreen::default(),
        }
    }

    fn card_return_after(&self, ms: u64) {
        use tauri::Manager;
        if let Some(live) = self.app.try_state::<Arc<crate::live::LiveHandle>>() {
            live.return_after(CARD_SOURCE, Duration::from_millis(ms));
        }
    }
}

/// Development builds only: the coding apps' folders under this folder
/// instead of the user's (`.claude`, `.codex`, `.agents`; `CLAUDE_CONFIG_DIR`
/// and `CODEX_HOME` count only inside it), as keymotion-cli's own test
/// variable. Trying the AI agents page then never touches the settings of
/// the Claude Code or Codex one works in.
pub const TEST_HOME_ENV: &str = "KEYMOTION_CLI_TEST_HOME";

/// [`TEST_HOME_ENV`], read only by a development build run against the
/// dev server (`tauri dev`, or the debug program with `cargo build`): never
/// by an installed copy, a `tauri build --debug` one included, which could
/// otherwise write the real PATH beside a test folder's settings.
pub fn test_home() -> Option<PathBuf> {
    if !(cfg!(debug_assertions) && tauri::is_dev()) {
        return None;
    }
    std::env::var_os(TEST_HOME_ENV).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The coding apps' folders: the user's (a development build: see
/// [`test_home`]).
pub fn homes() -> Option<kb_hooks::setup::Homes> {
    if let Some(home) = test_home() {
        let inside = |name: &str| {
            std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty() && p.starts_with(&home))
        };
        return Some(kb_hooks::setup::Homes::with(&home, inside("CLAUDE_CONFIG_DIR"), inside("CODEX_HOME")));
    }
    kb_hooks::setup::Homes::from_env()
}

/// At start, off the main thread: the skill (and Codex rule) Keymotion put
/// in a connected agent's folders is brought up to this version's, if it is
/// older (`kb_hooks::setup::refresh`: only our own files, only where they
/// are); and the hooks learn whether they may start Keymotion.
fn on_start(core: Arc<Core>) {
    let spawned = std::thread::Builder::new().name("agents-start".into()).spawn(move || {
        if let Some(homes) = homes() {
            for path in kb_hooks::setup::refresh(&homes) {
                tracing::info!(file = %path.display(), "agent skill brought up to date");
            }
        }
        core.started();
    });
    if let Err(e) = spawned {
        tracing::warn!("agent start check not started: {e}");
    }
}

/// The agents (managed Tauri state).
pub struct AgentHandle {
    core: Arc<Core>,
    /// Where this copy's keymotion-cli and the coding apps' folders are.
    place: apps::Place,
    user_path: Box<dyn apps::UserPath>,
    /// One connect or disconnect at a time.
    setup: Mutex<()>,
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
        let place = apps::Place::for_channel(channel);
        let info = ServerInfo { version: app.package_info().version.to_string(), channel };
        let host = Arc::new(AppHost { app, device, config_dir, started: Instant::now(), conflict: Mutex::new(None) });
        let core = Core::new(host, settings, info, true);
        on_start(core.clone());
        let (tx, rx) = mpsc::channel();
        *lock(&core.wake) = Some(tx);
        let worker = core.clone();
        std::thread::Builder::new()
            .name("agents".into())
            .spawn(move || {
                let mut w = Worker::default();
                loop {
                    let wait = worker.tick_safely(&mut w);
                    match rx.recv_timeout(wait) {
                        Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn agents thread");
        Arc::new(Self { core, place, user_path: Box::new(apps::WindowsPath), setup: Mutex::new(()) })
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

    /// "Quit" in the tray (`true`), or the user stayed after all (`false`).
    pub fn quitting(&self, quit: bool) {
        self.core.quitting(quit);
    }

    /// The page drew an agent's card: to the live mode.
    pub fn card_drawn(&self, id: &str, anim: ScreenAnimation) -> Result<(), String> {
        self.core.card_drawn(id, anim)
    }

    /// The page could not draw an agent's card.
    pub fn card_failed(&self, id: &str, why: &str) {
        self.core.card_failed(id, why);
    }

    /// The page listens for `agent-card` and can draw cards (`true`), or
    /// it went away (`false`: reloaded, its window closed).
    pub fn card_drawer(&self, ready: bool) {
        self.core.set_card_drawer(ready);
    }

    /// Claude Code and Codex: here, connected; keymotion-cli; PATH.
    pub fn connections(&self) -> AgentConnections {
        apps::connections(&self.place, self.user_path.as_ref())
    }

    /// What turning agent access on would add (`apps`: which coding apps,
    /// default both).
    pub fn connect_plan(&self, apps: Option<&[AgentApp]>) -> AgentConnectPlan {
        apps::plan_connect(&self.place, self.user_path.as_ref(), apps)
    }

    /// The user approved the plan of `token`: connect the coding apps, put
    /// the `bin` folder on PATH (GitHub copy), turn agent access on. When
    /// the plan no longer holds, nothing changes and the new one comes back.
    pub fn connect(&self, token: &str, apps: Option<&[AgentApp]>) -> AgentConnectOutcome {
        let done = {
            let _one = lock(&self.setup);
            apps::connect(&self.place, self.user_path.as_ref(), token, apps)
        };
        let done = match done {
            Ok(done) => done,
            Err(plan) => {
                tracing::info!("agents: the connect plan changed before it was approved; shown again");
                return AgentConnectOutcome {
                    applied: false,
                    plan: Some(*plan),
                    connected: Vec::new(),
                    failed: Vec::new(),
                    path_added: false,
                    path_error: None,
                    save_error: None,
                    connections: self.connections(),
                    state: self.core.state(),
                };
            }
        };
        tracing::info!(
            connected = done.connected.len(),
            failed = done.failed.len(),
            path_added = done.path_added,
            "agents: coding apps connected"
        );
        if done.path_added {
            self.user_path.announce();
        }
        if let Some(e) = &done.path_error {
            tracing::warn!("agents: keymotion-cli's folder not put on PATH: {e}");
        }
        // Every coding app failed: access stays as it was, the page says why.
        let save_error = if apps::access_follows(&done) { self.core.set_enabled(true).err() } else { None };
        if let Some(e) = &save_error {
            tracing::warn!("agents: access not saved as on: {e}");
        }
        // Newly connected apps learn whether they may start Keymotion.
        self.core.started();
        AgentConnectOutcome {
            applied: true,
            plan: None,
            connected: done.connected,
            failed: done.failed,
            path_added: done.path_added,
            path_error: done.path_error,
            save_error,
            connections: self.connections(),
            state: self.core.state(),
        }
    }

    /// What disconnecting `app` would take away.
    pub fn disconnect_plan(&self, app: AgentApp) -> AgentAppPlan {
        apps::plan_disconnect(&self.place, app)
    }

    /// Take Keymotion's hooks, skill and rule away from `app`. Agent access
    /// and PATH stay as they are.
    pub fn disconnect(&self, app: AgentApp) -> AgentDisconnectOutcome {
        let _one = lock(&self.setup);
        let plan = apps::disconnect(&self.place, app);
        match &plan.problem {
            None => tracing::info!(?app, files = plan.changes.len(), "agents: coding app disconnected"),
            Some(p) => tracing::warn!(?app, "agents: coding app not disconnected: {}", p.message),
        }
        AgentDisconnectOutcome { plan, connections: self.connections() }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::dto::AgentEffect;
    use kb_agent::protocol::{ClearArgs, LightArgs, SettingsSetArgs, SignalArgs, StatusArgs};
    use kb_agent::transport::Client;
    use kb_agent::SourceName;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    /// A host with a keyboard on the cable and a device that only records.
    struct Fake {
        clock: AtomicU64,
        cable: Mutex<Cable>,
        quiet: Mutex<Quiet>,
        minute: Mutex<u16>,
        frames: Mutex<Vec<Option<LiveFrame>>>,
        in_quiet: Mutex<Vec<bool>>,
        saved: Mutex<Vec<AgentSettings>>,
        /// What the hooks were told (`note_stopped`).
        stopped: Mutex<Vec<Option<kb_hooks::setup::Stopped>>>,
        published: AtomicU64,
        idle: Mutex<Option<u64>>,
        /// Saving fails (a full disk).
        save_fails: AtomicBool,
        /// Asking for the cable panics (a bug somewhere in the worker's pass).
        cable_panics: AtomicBool,
        /// What the focus timer was asked.
        focus_calls: Mutex<Vec<(FocusAction, Option<u16>)>>,
        profiles: Mutex<ProfilesNow>,
        /// Profiles whose lighting was written.
        wrote: Mutex<Vec<String>>,
        /// The screen: ready, and the live mode on.
        screen_ok: AtomicBool,
        live_on: AtomicBool,
        cards: Mutex<Vec<AgentCard>>,
        submitted: Mutex<Vec<String>>,
        on_screen: Mutex<OnScreen>,
        returns: Mutex<Vec<u64>>,
        /// The agents' files (`load_json`, `save_json`): kept across a
        /// "restart" (a new Core on the same host).
        files: Mutex<HashMap<String, Value>>,
        /// The next profile writes fail so.
        write_fails: Mutex<Option<WriteError>>,
        /// A profile write takes this long.
        write_ms: AtomicU64,
        /// The focus timer is running already.
        focus_running: AtomicBool,
    }

    /// A per-key profile for the keyboard on the cable: Esc red, F1 blue.
    fn per_key_profile() -> Profile {
        let colors = [("1".to_string(), "#ff0000".to_string()), ("2".to_string(), "#0000ff".to_string())];
        Profile {
            id: "p1".into(),
            name: "Night Owl".into(),
            created_ms: 0.0,
            updated_ms: 0.0,
            model_id: "ajazz-ak980-pro".into(),
            lighting: Some(ProfileLighting::PerKey { colors: colors.into_iter().collect(), brightness: 4 }),
            keymap: None,
            macros: None,
            screen: None,
        }
    }

    fn current(lighting: Option<ProfileLighting>) -> Current {
        Current {
            model_id: "ajazz-ak980-pro".into(),
            lighting,
            keymap: crate::profiles::ProfileKeymap::default(),
            macros: Vec::new(),
            screen: None,
        }
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
                stopped: Mutex::new(Vec::new()),
                published: AtomicU64::new(0),
                idle: Mutex::new(Some(0)),
                save_fails: AtomicBool::new(false),
                cable_panics: AtomicBool::new(false),
                focus_calls: Mutex::new(Vec::new()),
                profiles: Mutex::new(ProfilesNow {
                    profiles: vec![crate::profiles::default_profile(), per_key_profile()],
                    current: Some(current(Some(ProfileLighting::factory()))),
                }),
                wrote: Mutex::new(Vec::new()),
                screen_ok: AtomicBool::new(true),
                live_on: AtomicBool::new(true),
                cards: Mutex::new(Vec::new()),
                submitted: Mutex::new(Vec::new()),
                on_screen: Mutex::new(OnScreen::default()),
                returns: Mutex::new(Vec::new()),
                files: Mutex::new(HashMap::new()),
                write_fails: Mutex::new(None),
                write_ms: AtomicU64::new(0),
                focus_running: AtomicBool::new(false),
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
            assert!(!self.cable_panics.load(Ordering::SeqCst), "the cable panicked (expected in this test)");
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
            *lock(&self.idle)
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
            if self.save_fails.load(Ordering::SeqCst) {
                return Err("disk full".into());
            }
            lock(&self.saved).push(*settings);
            Ok(())
        }
        fn publish(&self, _: &AgentsState) {
            self.published.fetch_add(1, Ordering::SeqCst);
        }
        fn note_stopped(&self, why: Option<kb_hooks::setup::Stopped>) {
            lock(&self.stopped).push(why);
        }
        fn focus_control(&self, action: FocusAction, minutes: Option<u16>) -> Result<FocusDone, String> {
            lock(&self.focus_calls).push((action, minutes));
            let running = self.focus_running.load(Ordering::SeqCst);
            let once = if running { None } else { minutes };
            let now = FocusNow { phase: "focus", status: "running", remaining_s: 60, once };
            Ok(FocusDone { already_running: running, resumed: false, now: Some(now) })
        }
        fn profiles(&self) -> Result<ProfilesNow, String> {
            Ok(lock(&self.profiles).clone())
        }
        fn write_lighting(&self, profile: &Profile) -> Result<bool, WriteError> {
            std::thread::sleep(Duration::from_millis(self.write_ms.load(Ordering::SeqCst)));
            if let Some(e) = lock(&self.write_fails).clone() {
                return Err(e);
            }
            lock(&self.wrote).push(profile.id.clone());
            if let Some(c) = lock(&self.profiles).current.as_mut() {
                c.lighting.clone_from(&profile.lighting);
            }
            Ok(true)
        }
        fn screen(&self) -> ScreenNow {
            let ready = if self.screen_ok.load(Ordering::SeqCst) { Ok(()) } else { Err(needs_usb()) };
            ScreenNow { ready, live: self.live_on.load(Ordering::SeqCst) }
        }
        fn request_card(&self, card: &AgentCard) {
            lock(&self.cards).push(card.clone());
        }
        fn submit_card(&self, _anim: ScreenAnimation, name: &str) -> Result<(), String> {
            lock(&self.submitted).push(name.to_string());
            let mut on = lock(&self.on_screen);
            on.waiting = true;
            Ok(())
        }
        fn cards_on_screen(&self) -> OnScreen {
            *lock(&self.on_screen)
        }
        fn card_return_after(&self, ms: u64) {
            lock(&self.returns).push(ms);
            lock(&self.on_screen).back_planned = true;
        }
        fn load_json(&self, file: &str) -> Option<Value> {
            lock(&self.files).get(file).cloned()
        }
        fn save_json(&self, file: &str, value: &Value) -> Result<(), String> {
            lock(&self.files).insert(file.to_string(), value.clone());
            Ok(())
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

    fn scale_rgb(c: [u8; 3], k: f32) -> [u8; 3] {
        effects::scale(c, k)
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
        // What the keys show: the device composes what the agents sent with
        // its own quiet rule.
        let on_keys = |fake: &Fake| {
            let layers = crate::device::Layers { agent: fake.last_frame(), ..Default::default() };
            let in_quiet = lock(&fake.in_quiet).last().copied().unwrap_or(false);
            crate::device::compose(None, &crate::device::quiet_layers(&layers, Quiet::All, false, in_quiet))
        };
        let mut w = Worker::default();
        for _ in 0..5 {
            fake.advance(50);
            core.tick(&mut w);
        }
        assert_eq!(*lock(&fake.in_quiet), [false], "the device hides the layer");
        assert!(lock(&fake.frames).is_empty(), "and no frames go out while hidden");
        assert_eq!(on_keys(&fake), None);
        // The user turns "hide while playing" off: the question shows over
        // the game.
        core.save(AgentSettings { enabled: true, hide_in_game: false, ..AgentSettings::default() }).unwrap();
        core.tick(&mut w);
        assert_eq!(*lock(&fake.in_quiet), [false, true]);
        assert_eq!(core.state().lights, AgentLights::Shown);
        let shown = on_keys(&fake).expect("the agents' keys show while the game is quiet");
        assert_eq!(shown.len(), KeyIndex::for_model("ajazz-ak980-pro").unwrap().keys().len());
        // On again: the layer goes off the keys.
        core.save(AgentSettings { enabled: true, ..AgentSettings::default() }).unwrap();
        core.tick(&mut w);
        assert_eq!(fake.last_frame(), None);
        assert_eq!(on_keys(&fake), None);
    }

    /// The times key `led` changed colour in the frames the device got.
    fn key_changes(fake: &Fake, times: &[u64], led: u8) -> Vec<u64> {
        let frames = lock(&fake.frames).clone();
        let mut last = None;
        let mut out = Vec::new();
        for (frame, &t) in frames.iter().zip(times) {
            let now = frame.as_ref().and_then(|f| f.colors.iter().find(|k| k.0 == led).map(|k| k.1));
            if last.is_some() && last != Some(now) {
                out.push(t);
            }
            last = Some(now);
        }
        out
    }

    #[test]
    fn an_agent_switching_colours_fast_never_flashes_the_keys_too_fast() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        let mut w = Worker::default();
        let mut times = Vec::new();
        // 20 requests a second for five seconds, white and grey in turn,
        // then red and blue; a frame after each.
        for i in 0..200u64 {
            let color = match (i / 100, i % 2) {
                (0, 0) => "#ffffff",
                (0, _) => "#404040",
                (_, 0) => "red",
                _ => "blue",
            };
            let light =
                LightArgs { id: Some("x".into()), color: color.into(), keys: Some("esc".into()), ..Default::default() };
            cli.call(&Command::Light(light)).unwrap();
            fake.advance(50);
            let before = lock(&fake.frames).len();
            core.tick(&mut w);
            if lock(&fake.frames).len() > before {
                times.push(fake.mono_ms());
            }
        }
        let changes = key_changes(&fake, &times, 1);
        let most = changes.iter().map(|&t| changes.iter().filter(|&&u| u >= t && u < t + 1000).count()).max();
        assert!(most.unwrap_or(0) <= effects::MAX_CHANGES, "{changes:?}");
        assert!(changes.len() >= 4 * 9, "it still changes at the safe rate: {changes:?}");
    }

    #[test]
    fn replacements_past_the_rate_wait_for_the_worker() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        // 60 updates of one light at once (the fake clock stands still).
        for i in 0..60u8 {
            let color = format!("#0000{i:02x}");
            let light = LightArgs { id: Some("bar".into()), color, keys: Some("esc".into()), ..Default::default() };
            let answer = cli.call(&Command::Light(light)).unwrap();
            assert_eq!(answer["queued"], i >= 20, "{i}");
        }
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(fake.last_frame().unwrap().colors, [(1, [0, 0, 19])], "the newest put up so far");
        fake.advance(1_000);
        core.tick(&mut w);
        assert_eq!(fake.last_frame().unwrap().colors, [(1, [0, 0, 59])], "then the newest that waited");
    }

    #[test]
    fn busy_without_the_cable_still_clears() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        cli.call(&signal("attention")).unwrap();
        *lock(&fake.cable) = Cable::NotUsb;
        assert_eq!(cli.call(&signal("busy")).unwrap()["cleared"], 1);
        // Plugged in again: the question does not come back.
        *lock(&fake.cable) = Cable::Usb("ajazz-ak980-pro".into());
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(fake.last_frame(), None);
        assert!(core.state().signals.is_empty());
    }

    #[test]
    fn a_key_pressed_once_the_question_showed_puts_it_out() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        *lock(&fake.idle) = Some(60_000);
        cli.call(&signal("attention")).unwrap();
        let mut w = Worker::default();
        core.tick(&mut w);
        fake.advance(5_000);
        core.tick(&mut w);
        assert!(fake.last_frame().is_some(), "nobody touched anything");
        // Hidden while a game runs: a key pressed then is no answer.
        *lock(&fake.quiet) = Quiet::All;
        *lock(&fake.idle) = Some(0);
        fake.advance(100);
        core.tick(&mut w);
        *lock(&fake.idle) = Some(60_000);
        *lock(&fake.quiet) = Quiet::Off;
        fake.advance(100);
        core.tick(&mut w);
        assert!(!core.state().signals.is_empty());
        // Shown again for a while, a key pressed: it goes.
        fake.advance(5_000);
        core.tick(&mut w);
        *lock(&fake.idle) = Some(0);
        fake.advance(100);
        core.tick(&mut w);
        assert!(core.state().signals.is_empty());
        assert_eq!(fake.last_frame(), None);
    }

    fn call(core: &Core, source: &str, command: Command) -> Result<Value, AgentError> {
        let peer = kb_agent::Peer { client: "tests".into(), version: "1".into(), protocol: 1 };
        let source = SourceName::parse(source).unwrap();
        core.handle(Call { peer: &peer, source: &source, id: "1" }, command)
    }

    /// A Core on `fake` whose page can draw cards.
    fn core_with(fake: &Arc<Fake>, settings: AgentSettings) -> Arc<Core> {
        let info = ServerInfo { version: "1.2.0".into(), channel: Channel::Dev };
        let core = Core::new(fake.clone(), settings, info, false);
        core.set_card_drawer(true);
        core
    }

    fn focus(action: &str, minutes: Option<u64>) -> Command {
        Command::Focus(FocusArgs { action: action.into(), for_ms: minutes.map(|m| m * 60_000) })
    }

    #[test]
    fn the_focus_timer_needs_its_own_permission_to_change() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        assert!(call(&core, "tests", focus("status", None)).is_ok(), "reading needs no permission");
        let e = call(&core, "tests", focus("start", Some(50))).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::PermissionOff, 77));
        assert!(e.hint.unwrap().contains("Focus timer"));
        assert!(lock(&fake.focus_calls).is_empty());
        let mut s = AgentSettings { enabled: true, focus: true, ..AgentSettings::default() };
        core.save(s).unwrap();
        let answer = call(&core, "tests", focus("start", Some(50))).unwrap();
        assert_eq!((answer["minutes"].clone(), answer["focus"]["once"].clone()), (json!(50), json!(50)));
        call(&core, "tests", focus("stop", None)).unwrap();
        assert_eq!(*lock(&fake.focus_calls), [(FocusAction::Start, Some(50)), (FocusAction::Stop, None)]);
        // A timer already running keeps what it has: the answer says so.
        fake.focus_running.store(true, Ordering::SeqCst);
        let kept = call(&core, "tests", focus("start", Some(25))).unwrap();
        assert_eq!((kept["minutes"].clone(), kept["already_running"].clone()), (Value::Null, json!(true)));
        assert!(kept["minutes_not_applied"].as_str().unwrap().contains("running"));
        fake.focus_running.store(false, Ordering::SeqCst);
        assert!(core.state().log.iter().any(|l| l.command == "focus" && l.error.is_none()));
        // Access off: nothing.
        s.enabled = false;
        core.save(s).unwrap();
        assert_eq!(call(&core, "tests", focus("pause", None)).unwrap_err().code, ErrorCode::AgentAccessOff);
    }

    fn profile(action: &str, name: Option<&str>, write_flash: bool) -> Command {
        Command::Profile(ProfileArgs {
            action: action.into(),
            name: name.map(str::to_string),
            for_ms: None,
            write_flash,
        })
    }

    #[test]
    fn profiles_list_show_in_memory_and_apply_only_the_lighting_with_leave() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        let list = call(&core, "tests", profile("list", None, false)).unwrap();
        let names: Vec<&str> =
            list["profiles"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Default", "Night Owl"]);
        assert_eq!(list["profiles"][0]["lighting_on_keyboard"], true);
        assert_eq!(list["profiles"][1]["lighting"], "per_key");

        // Show: its colours in working memory, nothing written.
        let shown = call(&core, "tests", profile("show", Some("night owl"), false)).unwrap();
        assert_eq!((shown["profile"].clone(), shown["shown"].clone()), (json!("Night Owl"), json!(true)));
        let mut w = Worker::default();
        core.tick(&mut w);
        let frame = fake.last_frame().unwrap();
        assert!(frame.colors.contains(&(1, [255, 0, 0])) && frame.colors.contains(&(2, [0, 0, 255])));
        assert!(lock(&fake.wrote).is_empty());
        let e = call(&core, "tests", profile("show", Some("Default"), false)).unwrap_err();
        assert_eq!(e.code, ErrorCode::Unsupported, "an effect shows only once written");
        assert_eq!(call(&core, "tests", profile("show", Some("nope"), false)).unwrap_err().code, ErrorCode::BadValue);

        // Apply: the permission, then --write-flash, then the write.
        let e = call(&core, "tests", profile("apply", Some("Night Owl"), true)).unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionOff);
        core.save(AgentSettings { enabled: true, profiles: true, ..AgentSettings::default() }).unwrap();
        let e = call(&core, "tests", profile("apply", Some("Night Owl"), false)).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::WriteFlashFlagMissing, 77));
        let done = call(&core, "tests", profile("apply", Some("p1"), true)).unwrap();
        assert_eq!((done["changed"].clone(), done["writes_in_24h"].clone()), (json!(true), json!(1)));
        let again = call(&core, "tests", profile("apply", Some("p1"), true)).unwrap();
        assert_eq!(again["changed"], false, "it has it: nothing written");
        assert_eq!(*lock(&fake.wrote), ["p1"]);
        // Refused by the write guard, or no cable: said as such, and the
        // slot is given back.
        *lock(&fake.write_fails) = Some(WriteError::Blocked("another program is using the keyboard (x)".into()));
        let e = call(&core, "tests", profile("apply", Some("default"), true)).unwrap_err();
        assert_eq!(e.code, ErrorCode::WriteBlocked);
        *lock(&fake.write_fails) = Some(WriteError::NoUsb);
        assert_eq!(
            call(&core, "tests", profile("apply", Some("default"), true)).unwrap_err().code,
            ErrorCode::NeedsUsb
        );
        *lock(&fake.write_fails) = None;
        let second = call(&core, "claude-code:1", profile("apply", Some("default"), true)).unwrap();
        assert_eq!((second["writes_in_24h"].clone(), second["agent_writes_in_24h"].clone()), (json!(2), json!(1)));
        // Ten in any 24 hours for all agents together: a new name each time
        // gets no count of its own.
        for i in 2..PROFILE_WRITES_PER_DAY - 1 {
            let back = if i % 2 == 0 { "p1" } else { "default" };
            let answer = call(&core, &format!("agent{i}:s"), profile("apply", Some(back), true)).unwrap();
            assert_eq!(answer["changed"], true);
        }
        // Nine written, the keyboard has p1. Two at once for the last slot:
        // it is taken before the write, so only one of them writes.
        fake.write_ms.store(150, Ordering::SeqCst);
        let both: Vec<_> = ["one:s", "two:s"]
            .map(|source| {
                let core = core.clone();
                std::thread::spawn(move || call(&core, source, profile("apply", Some("default"), true)))
            })
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect();
        fake.write_ms.store(0, Ordering::SeqCst);
        assert_eq!(both.iter().filter(|r| r.is_ok()).count(), 1, "{both:?}");
        assert!(both.iter().any(|r| r.as_ref().is_err_and(|e| e.code == ErrorCode::RateLimited)));
        let e = call(&core, "codex:1", profile("apply", Some("p1"), true)).unwrap_err();
        assert_eq!(
            (e.code, e.retry_after_ms),
            (ErrorCode::RateLimited, Some(limits::DAY_MS)),
            "until the first is a day old"
        );
        // A restart does not start the count again.
        let restarted = core_with(&fake, AgentSettings { enabled: true, profiles: true, ..AgentSettings::default() });
        assert_eq!(
            call(&restarted, "codex:1", profile("apply", Some("p1"), true)).unwrap_err().code,
            ErrorCode::RateLimited
        );
        // A day after the first write: one again.
        fake.advance(limits::DAY_MS);
        assert_eq!(call(&restarted, "codex:1", profile("apply", Some("p1"), true)).unwrap()["changed"], true);
    }

    fn card(text: &str, write_flash: bool) -> Command {
        Command::Card(CardArgs { text: Some(text.into()), write_flash, ..Default::default() })
    }

    fn anim() -> ScreenAnimation {
        ScreenAnimation::still(kb_protocol::sonix::screen::Frame::filled(0))
    }

    #[test]
    fn a_card_needs_the_screen_permission_the_flag_and_room_this_hour() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        assert_eq!(call(&core, "tests", card("Build passed", true)).unwrap_err().code, ErrorCode::PermissionOff);
        core.save(AgentSettings { enabled: true, screen: true, screen_per_hour: 2, ..AgentSettings::default() })
            .unwrap();
        assert_eq!(
            call(&core, "tests", card("Build passed", false)).unwrap_err().code,
            ErrorCode::WriteFlashFlagMissing
        );
        fake.live_on.store(false, Ordering::SeqCst);
        let e = call(&core, "tests", card("Build passed", true)).unwrap_err();
        assert!(e.message.contains("live screen"), "{e:?}");
        fake.live_on.store(true, Ordering::SeqCst);
        fake.screen_ok.store(false, Ordering::SeqCst);
        assert_eq!(call(&core, "tests", card("Build passed", true)).unwrap_err().code, ErrorCode::NeedsUsb);
        fake.screen_ok.store(true, Ordering::SeqCst);
        // The page has not said it can draw cards (its window never loaded).
        core.set_card_drawer(false);
        let e = call(&core, "tests", card("Build passed", true)).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::Unsupported, 69));
        assert!(e.hint.unwrap().contains("open Keymotion's window"));
        core.set_card_drawer(true);

        // A picture and text: asked of the page.
        let png = kb_agent::image::encode(TEST_PNG);
        let with_picture = Command::Card(CardArgs {
            text: Some("Build passed".into()),
            title: Some("CI".into()),
            image: Some(png),
            color: Some("green".into()),
            write_flash: true,
            ..Default::default()
        });
        let answer = call(&core, "tests", with_picture.clone()).unwrap();
        assert_eq!((answer["state"].clone(), answer["queued"].clone()), (json!("drawing"), json!(true)));
        let asked = lock(&fake.cards)[0].clone();
        assert_eq!(
            (asked.kind, asked.title.as_deref(), asked.color),
            (AgentCardKind::Text, Some("CI"), Some([0, 255, 0]))
        );
        assert_eq!(asked.image.as_ref().map(|i| i.mime.as_str()), Some("image/png"));
        assert_eq!(asked.show_s, 300);
        // The same card again: nothing to send.
        assert_eq!(call(&core, "tests", with_picture).unwrap()["skipped"], "same_content");
        assert_eq!(lock(&fake.cards).len(), 1);
        // Another one from another name, then the hour is full (two
        // allowed, for all agents together).
        let second = call(&core, "other-agent:s", card("Second", true)).unwrap();
        assert_eq!((second["sent_this_hour"].clone(), second["agent_sent_this_hour"].clone()), (json!(2), json!(1)));
        let e = call(&core, "a-third-name", card("Third", true)).unwrap_err();
        assert_eq!(e.code, ErrorCode::RateLimited);
        assert_eq!(e.retry_after_ms, Some(HOUR_MS), "until the first is an hour old");
        // A restart does not start the count again.
        let restarted = core_with(
            &fake,
            AgentSettings { enabled: true, screen: true, screen_per_hour: 2, ..AgentSettings::default() },
        );
        assert_eq!(call(&restarted, "tests", card("Fourth", true)).unwrap_err().code, ErrorCode::RateLimited);
    }

    /// A PNG header of 16x16 pixels (signature, IHDR, an empty IDAT): what
    /// the checks read.
    const TEST_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00,
        0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x91, 0x68, 0x36, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x44, 0x41, 0x54, 0xAE, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn a_drawn_card_goes_to_the_live_mode_and_back_after_its_time() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, screen: true, ..AgentSettings::default() });
        let command = Command::Card(CardArgs {
            text: Some("Deploy done".into()),
            for_ms: Some(60_000),
            write_flash: true,
            wait_ms: Some(5_000),
            ..Default::default()
        });
        // The page draws it while the agent waits; the live mode puts it up.
        let page = {
            let (fake, core) = (fake.clone(), core.clone());
            std::thread::spawn(move || {
                let id = loop {
                    if let Some(c) = lock(&fake.cards).first() {
                        break c.id.clone();
                    }
                    std::thread::sleep(Duration::from_millis(5));
                };
                core.card_drawn(&id, anim()).unwrap();
                *lock(&fake.on_screen) = OnScreen { shown: true, waiting: false, back_planned: false };
                let mut w = Worker::default();
                core.tick(&mut w);
            })
        };
        let answer = call(&core, "tests", command).unwrap();
        page.join().unwrap();
        assert_eq!((answer["state"].clone(), answer["shown"].clone()), (json!("shown"), json!(true)));
        assert_eq!(lock(&fake.submitted).len(), 1);
        assert_eq!(*lock(&fake.returns), [60_000], "the user's picture comes back after its time");
        // A card the page never draws fails, and does not count.
        let late = call(&core, "tests", card("Never drawn", true)).unwrap();
        let id = late["id"].as_str().unwrap().to_string();
        fake.advance(cards::DRAW_WAIT_MS);
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(core.card_drawn(&id, anim()).unwrap_err(), format!("card {id} is not on its way any more"));
        assert_eq!(lock(&core.limits).cards.count(core.wall(), HOUR_MS), 1, "the one never drawn gave its count back");
        assert_eq!(*lock(&fake.returns), [60_000]);
    }

    #[test]
    fn a_card_replaced_while_going_up_still_gives_the_screen_back() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, screen: true, ..AgentSettings::default() });
        let mut w = Worker::default();
        // Card 1 drawn: the live mode uploads it.
        let first = call(&core, "tests", card("One", true)).unwrap()["id"].as_str().unwrap().to_string();
        core.card_drawn(&first, anim()).unwrap();
        // Card 2 replaces it, and the page cannot draw it.
        let second = call(&core, "tests", card("Two", true)).unwrap()["id"].as_str().unwrap().to_string();
        core.card_failed(&second, "a broken picture");
        assert_eq!(lock(&core.limits).cards.count(core.wall(), HOUR_MS), 1, "card 2 gave its count back");
        core.tick(&mut w);
        assert!(lock(&fake.returns).is_empty(), "card 1 still uploading");
        // Card 1 reaches the screen all the same: it goes back after its time.
        *lock(&fake.on_screen) = OnScreen { shown: true, waiting: false, back_planned: false };
        fake.advance(100);
        core.tick(&mut w);
        assert_eq!(*lock(&fake.returns), [kb_agent::protocol::DEFAULT_CARD_MS]);
        fake.advance(LOOK_FOR_LEFT_CARDS_MS);
        core.tick(&mut w);
        assert_eq!(lock(&fake.returns).len(), 1, "once");
    }

    #[test]
    fn a_card_left_on_the_screen_before_a_restart_goes_back() {
        let fake = Fake::new();
        *lock(&fake.on_screen) = OnScreen { shown: true, waiting: false, back_planned: false };
        let core = core_with(&fake, AgentSettings { enabled: true, screen: true, ..AgentSettings::default() });
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(*lock(&fake.returns), [cards::LEFT_BEHIND_MS]);
        fake.advance(LOOK_FOR_LEFT_CARDS_MS);
        core.tick(&mut w);
        assert_eq!(lock(&fake.returns).len(), 1, "planned: not again");
    }

    #[test]
    fn a_long_task_done_puts_a_card_up_when_the_user_lets_it() {
        let fake = Fake::new();
        let hook = |event: &str| {
            Command::Hook(kb_agent::protocol::HookArgs {
                event: event.into(),
                agent: Some("claude-code".into()),
                label: Some("klavye".into()),
                calls: Vec::new(),
            })
        };
        // Without the screen permission F does nothing.
        let core = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        call(&core, "claude-code:a1", hook("busy")).unwrap();
        fake.advance(11 * 60_000);
        call(&core, "claude-code:a1", hook("done")).unwrap();
        assert!(lock(&fake.cards).is_empty());
        // With it: a card for a turn of ten minutes or more.
        core.save(AgentSettings { enabled: true, screen: true, ..AgentSettings::default() }).unwrap();
        call(&core, "claude-code:a1", hook("busy")).unwrap();
        fake.advance(9 * 60_000);
        call(&core, "claude-code:a1", hook("done")).unwrap();
        assert!(lock(&fake.cards).is_empty(), "nine minutes is no long task");
        call(&core, "claude-code:a1", hook("busy")).unwrap();
        fake.advance(14 * 60_000 + 20_000);
        call(&core, "claude-code:a1", hook("done")).unwrap();
        let cards = lock(&fake.cards).clone();
        assert_eq!(cards.len(), 1);
        let c = &cards[0];
        assert_eq!(
            (c.kind, c.minutes, c.label.as_deref(), c.show_s),
            (AgentCardKind::LongTask, Some(14), Some("klavye"), 300)
        );
        assert_eq!(
            lock(&core.limits).cards.count_for("claude-code", core.wall(), HOUR_MS),
            1,
            "it counts for the hour"
        );
        // No page to draw it: no card, nothing counted.
        core.set_card_drawer(false);
        call(&core, "claude-code:a1", hook("busy")).unwrap();
        fake.advance(12 * 60_000);
        call(&core, "claude-code:a1", hook("done")).unwrap();
        assert_eq!(lock(&fake.cards).len(), 1);
        assert_eq!(lock(&core.limits).cards.count(core.wall(), HOUR_MS), 1);
    }

    fn frame_has(fake: &Fake, key: &str) -> Option<kb_protocol::sonix::Rgb> {
        let led = KeyIndex::for_model("ajazz-ak980-pro").unwrap().keys().iter().find(|k| k.id == key).unwrap().led;
        fake.last_frame().and_then(|f| f.colors.iter().find(|c| c.0 == led).map(|c| c.1))
    }

    #[test]
    fn a_word_is_spelled_key_after_key_and_what_is_missing_is_said() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        let spell = |text: &str| {
            Command::Spell(SpellArgs { text: text.into(), color: Some("green".into()), ..Default::default() })
        };
        let answer = call(&core, "tests", spell("Gül 2")).unwrap();
        assert_eq!(answer["written"], "Gl 2");
        assert_eq!(answer["skipped"], json!(["ü"]));
        // G, then L, a pause for the space, then 2: half a second each.
        assert_eq!(answer["length_ms"], 2_000);
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(frame_has(&fake, "g"), Some([0, 255, 0]));
        assert_eq!(frame_has(&fake, "l"), None);
        fake.advance(600);
        core.tick(&mut w);
        assert_eq!((frame_has(&fake, "g"), frame_has(&fake, "l")), (None, Some([0, 255, 0])));
        fake.advance(1_000);
        core.tick(&mut w);
        assert_eq!(frame_has(&fake, "2"), Some([0, 255, 0]));
        assert_eq!(call(&core, "tests", spell("çğü")).unwrap_err().code, ErrorCode::BadValue);
    }

    #[test]
    fn the_same_key_twice_goes_dark_in_between() {
        let s = kb_agent::protocol::SpellArgs { text: "OO".into(), speed: Some(3), ..Default::default() }
            .validate()
            .unwrap();
        let (scene, written, skipped) = spell_scene(&s, |_| true).unwrap();
        let times: Vec<(u64, u64)> = scene.steps.iter().map(|x| (x.at_ms, x.for_ms)).collect();
        assert_eq!(times, [(0, 500), (500 + SPELL_GAP_MS, 500)]);
        assert_eq!((written.as_str(), skipped.len()), ("OO", 0));
        // Twice: a pause of two letters between the words.
        let twice =
            kb_agent::protocol::SpellArgs { text: "OK".into(), times: Some(2), speed: Some(5), ..Default::default() }
                .validate()
                .unwrap();
        let (scene, _, _) = spell_scene(&twice, |_| true).unwrap();
        assert_eq!((scene.length_ms, scene.repeat), (2 * 280 + 2 * 280, 2));
        assert_eq!(scene.duration, Duration::from_millis(2 * 1_120 - 560), "it ends with the last letter");
    }

    fn scene_args(save: Option<&str>, play: bool) -> SceneArgs {
        SceneArgs {
            steps: vec![
                kb_agent::protocol::SceneStepArgs {
                    keys: Some("f1".into()),
                    color: "red".into(),
                    ..Default::default()
                },
                kb_agent::protocol::SceneStepArgs {
                    keys: Some("f2".into()),
                    color: "blue".into(),
                    ..Default::default()
                },
            ],
            save: save.map(str::to_string),
            play: Some(play),
            ..Default::default()
        }
    }

    #[test]
    fn an_agent_saves_its_own_patterns_and_plays_them_by_name() {
        let fake = Fake::new();
        let core = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        let pattern = |action: &str, name: Option<&str>| {
            Command::Pattern(PatternArgs {
                action: action.into(),
                name: name.map(str::to_string),
                ..Default::default()
            })
        };
        // Played and kept under a name.
        let played = call(&core, "claude-code:s1", Command::Scene(scene_args(Some("Deploy"), true))).unwrap();
        assert_eq!((played["saved"].clone(), played["length_ms"].clone()), (json!("deploy"), json!(1000)));
        // Only kept.
        call(&core, "claude-code:s1", Command::Scene(scene_args(Some("quiet"), false))).unwrap();
        let list = call(&core, "claude-code:s2", pattern("list", None)).unwrap();
        let names: Vec<&str> =
            list["patterns"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["deploy", "quiet"], "every session of the agent sees them");
        assert_eq!(
            call(&core, "codex:x", pattern("list", None)).unwrap()["patterns"],
            json!([]),
            "not another agent's"
        );
        // Played by name, from another session.
        let again = call(&core, "claude-code:s2", pattern("play", Some("deploy"))).unwrap();
        assert_eq!(
            (again["pattern"].clone(), again["id"].clone()),
            (json!("deploy"), json!("claude-code:s2:pattern-deploy"))
        );
        let mut w = Worker::default();
        core.tick(&mut w);
        assert_eq!(frame_has(&fake, "f1"), Some([255, 0, 0]));
        assert_eq!(call(&core, "codex:x", pattern("play", Some("deploy"))).unwrap_err().code, ErrorCode::BadValue);
        // Kept across a restart: the host has them.
        let core2 = core_with(&fake, AgentSettings { enabled: true, ..AgentSettings::default() });
        assert_eq!(
            call(&core2, "claude-code:s3", pattern("list", None)).unwrap()["patterns"].as_array().unwrap().len(),
            2
        );
        assert_eq!(call(&core2, "claude-code:s3", pattern("delete", Some("quiet"))).unwrap()["deleted"], true);
        assert_eq!(call(&core2, "claude-code:s3", pattern("delete", Some("quiet"))).unwrap()["deleted"], false);
        // At most 32 each.
        for i in 0..31 {
            fake.advance(60);
            call(&core2, "cli", Command::Scene(scene_args(Some(&format!("p{i}")), false))).unwrap();
        }
        call(&core2, "cli", Command::Scene(scene_args(Some("p31"), false))).unwrap();
        let e = call(&core2, "cli", Command::Scene(scene_args(Some("p32"), false))).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadArgument);
        assert!(call(&core2, "cli", Command::Scene(scene_args(Some("p31"), false))).is_ok(), "replacing one is fine");
        // Access off: nothing plays, nothing is kept.
        core2.save(AgentSettings::default()).unwrap();
        assert_eq!(
            call(&core2, "cli", Command::Scene(scene_args(None, true))).unwrap_err().code,
            ErrorCode::AgentAccessOff
        );
    }

    #[test]
    fn turning_access_on_changes_nothing_else() {
        let fake = Fake::new();
        let info = ServerInfo { version: "1.2.0".into(), channel: Channel::Dev };
        let core = Core::new(fake.clone(), AgentSettings::default(), info, false);
        // The page (or an agent) changed a look just before.
        let mut s = core.state().settings;
        s.helper.color = [1, 2, 3];
        core.save(s).unwrap();
        let state = core.set_enabled(true).unwrap();
        assert!(state.settings.enabled);
        assert_eq!(state.settings.helper.color, [1, 2, 3], "kept, not put back");
        assert_eq!(*lock(&fake.stopped), [None], "the hooks may start Keymotion again");
        fake.save_fails.store(true, Ordering::SeqCst);
        assert!(core.set_enabled(false).is_err());
        assert!(core.state().settings.enabled, "a save that fails changes nothing");
    }

    #[test]
    fn the_hooks_learn_when_keymotion_must_stay_closed() {
        use kb_hooks::setup::Stopped;
        let fake = Fake::new();
        let info = ServerInfo { version: "1.2.0".into(), channel: Channel::Dev };
        let core = Core::new(fake.clone(), AgentSettings::default(), info, false);
        core.started();
        core.quitting(true);
        let on = AgentSettings { enabled: true, ..AgentSettings::default() };
        core.save(on).unwrap();
        core.save(on).unwrap();
        core.quitting(true);
        core.quitting(false);
        core.save(AgentSettings::default()).unwrap();
        assert_eq!(
            *lock(&fake.stopped),
            [Some(Stopped::AccessOff), None, Some(Stopped::Quit), None, Some(Stopped::AccessOff)],
            "access off at start; quitting with access off changes nothing; saving without a change says nothing"
        );
    }

    #[test]
    fn a_failed_save_changes_nothing_and_concurrent_saves_end_with_the_newest() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let (tx, rx) = mpsc::channel();
        *lock(&core.wake) = Some(tx);
        fake.save_fails.store(true, Ordering::SeqCst);
        let off = AgentSettings { enabled: false, ..AgentSettings::default() };
        assert_eq!(core.save(off).unwrap_err(), "disk full");
        assert!(core.state().settings.enabled, "the engine kept what is on disk");
        assert!(rx.try_recv().is_ok(), "the worker was woken anyway");
        fake.save_fails.store(false, Ordering::SeqCst);

        // Agents and the page saving at once: what is on disk at the end is
        // what the app uses.
        let colors = ["red", "green", "blue", "white", "amber", "cyan", "pink", "purple"];
        let threads: Vec<_> = (0..8)
            .map(|n| {
                let name = name.clone();
                std::thread::spawn(move || {
                    let mut cli = client(&name, &format!("agent{n}"));
                    for i in 0..10 {
                        let key = ["signals.done.color", "signals.error.color", "progress.color"][(n + i) % 3];
                        let values = [(key.to_string(), json!(colors[(n + i) % 8]))].into_iter().collect();
                        cli.call(&Command::SettingsSet(SettingsSetArgs { values })).unwrap();
                    }
                })
            })
            .collect();
        for i in 0..10u8 {
            let mut s = core.state().settings;
            s.helper.color = [i, i, i];
            core.save(s).unwrap();
        }
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(*lock(&fake.saved).last().unwrap(), core.state().settings);
    }

    #[test]
    fn a_panic_in_the_worker_takes_the_agents_layer_off_and_it_goes_on() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        cli.call(&Command::Light(LightArgs { color: "red".into(), keys: Some("esc".into()), ..Default::default() }))
            .unwrap();
        let mut w = Worker::default();
        core.tick_safely(&mut w);
        assert!(fake.last_frame().is_some());
        fake.cable_panics.store(true, Ordering::SeqCst);
        core.tick_safely(&mut w);
        assert_eq!(*lock(&fake.frames).last().unwrap(), None, "no frame stays stuck on the keys");
        fake.cable_panics.store(false, Ordering::SeqCst);
        fake.advance(50);
        core.tick_safely(&mut w);
        assert_eq!(fake.last_frame().unwrap().colors, [(1, [255, 0, 0])], "the next pass draws again");
    }

    #[test]
    fn the_worker_only_watches_while_lights_wait_for_the_users_return() {
        let (fake, core, _server, name) = setup(AgentSettings { enabled: true, ..AgentSettings::default() });
        let mut cli = client(&name, "tests");
        let mut w = Worker::default();
        *lock(&fake.idle) = Some(60 * 60_000);
        core.tick(&mut w);
        cli.call(&signal("done")).unwrap();
        assert_eq!(core.tick(&mut w), TICK);
        fake.advance(3 * 60_000);
        let mut wait = core.tick(&mut w);
        for _ in 0..10 {
            wait = core.tick(&mut w);
        }
        assert_eq!(fake.last_frame(), None);
        assert_eq!(wait, WATCH, "only a look at the user's input now and then");
        // Back: the done flashes once.
        *lock(&fake.idle) = Some(0);
        assert_eq!(core.tick(&mut w), TICK);
        assert_eq!(
            fake.last_frame().unwrap().colors.len(),
            KeyIndex::for_model("ajazz-ak980-pro").unwrap().keys().len()
        );
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
        // The page's try on F1 at the same time is not dimmed.
        core.try_look(look, Some("f1".into())).unwrap();
        core.tick(&mut w);
        let frame = fake.last_frame().unwrap();
        let faint = scale_rgb([255, 255, 255], NIGHT_DIM);
        assert_eq!(frame.colors, [(2, [10, 20, 30]), (1, faint)]);
        assert_eq!(frame.level, level_factor(LEVEL));
        assert_eq!(core.state().lights, AgentLights::NightDim);
        fake.advance(engine::TRY_MS);
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
