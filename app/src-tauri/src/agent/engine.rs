//! The agents' lights: who asked for what, until when, and what shows.
//!
//! * Each source (`claude-code:3f9a`, `cli`, `tests`) has its own lights,
//!   at most [`MAX_PER_SOURCE`]. A light's id (default `<source>:<signal>`)
//!   replaces the source's earlier light with that id; no source can touch
//!   another's.
//! * Every light ends: its time is up, it ran its cycles, or it is cleared.
//!   `busy` takes the source's earlier lights away.
//! * On a key named by several lights, the highest rank shows: a question
//!   for the user (`attention`) over an `error` over the newest. Lights on
//!   different keys show together.
//! * A source may make [`RATE_PER_SECOND`] requests a second. Beyond that a
//!   light that replaces one of its own waits in a queue, only the newest
//!   per id, and the worker puts it up when the source is within the rate
//!   again ([`Engine::apply_queued`]); anything else is `rate_limited`.
//! * "While you were away" (C): a signal that came while nobody was at the
//!   computer flashes once when the user is back, even if its time ran out
//!   meanwhile (for up to [`MISSED_KEEP_MS`]). "Unanswered" (D): a question
//!   flashes again every few minutes while it waits.
//! * The log keeps the last [`LOG_SIZE`] requests that changed something or
//!   were refused: source, command, result, time, the light's id, and for
//!   settings what they were before (for "Undo"). Refusals go in at most
//!   once a second per source, a light updated again and again keeps one
//!   entry, and entries that can still be undone go last when it is full.
//!   Never what an agent's user typed: the protocol carries no such text.
//! * Each light's LEDs are worked out once, when it comes, and again only
//!   when another keyboard model is in use ([`Engine::refresh_keys`]).
//! * Coding agents' hooks (`hook`): each session is a source of its own
//!   (`claude-code:3f9a0c1d`) and gets a number, 1 to 9, in the order it
//!   first reached Keymotion (a number no live session holds is free again).
//!   A question (`attention`) stays until it is answered when its look says
//!   so (`until_answered`, the default): the tool calls it waits for are
//!   kept, and it goes when the last one is answered, or when the turn
//!   ends. With two or more live sessions, a waiting session's number lights
//!   on its number key (B). A usage limit has its own look (A), and a helper
//!   agent's end a short wave on the F row (H).
//! * A question goes out as soon as the user touches the keyboard or the
//!   mouse after its first flashes showed (the owner's decision,
//!   2026-11-23): whoever is at the computer has seen it. Only while the
//!   lights show (not hidden by a game, not off for the night). Its
//!   session's number (B) stays on until the question is answered, so the
//!   user still sees which session waits.
//!
//! Time is in milliseconds of a steady clock (`Now::mono`); the log shows
//! the wall clock.

use crate::agent::config;
use crate::agent::effects::{burst_level, scale, Look, Motion, Place, BURST_MS};
use crate::agent::keys::KeyIndex;
use crate::dto::{
    AgentLogEntry, AgentLook, AgentProgressKeys, AgentSessionInfo, AgentSettingChange, AgentSettings, AgentSignalInfo,
};
use kb_agent::protocol::{ClearArgs, HookArgs, LightArgs, ProgressArgs, SettingsSetArgs, SignalArgs};
use kb_agent::values::duration_ms;
use kb_agent::{AgentError, Command, ErrorCode, HookEvent, KeyGroup, KeySpec, SignalId, SignalName, SourceName};
use kb_protocol::sonix::Rgb;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub const MAX_PER_SOURCE: usize = 8;
/// Sources with lights at once.
pub const MAX_SOURCES: usize = 64;
pub const RATE_PER_SECOND: usize = 20;
pub const LOG_SIZE: usize = 200;
/// A `light` without a duration or a cycle count stays this long.
pub const LIGHT_DEFAULT_MS: u64 = 60_000;
/// A progress bar stays this long after its last update.
pub const PROGRESS_DEFAULT_MS: u64 = 10 * 60_000;
/// "Try on the keyboard" on the page.
pub const TRY_MS: u64 = 6_000;
/// The page's tries. Not a valid source name, so no agent can take it.
pub const TRY_SOURCE: &str = "~try";
/// Lights that ended while the user was away, kept for their return…
const MAX_MISSED: usize = 8;
/// …for at most this long: a day-old "done" means nothing.
pub const MISSED_KEEP_MS: u64 = 12 * 60 * 60 * 1000;
/// A source's refusals go in the log at most this often.
const REFUSAL_LOG_EVERY_MS: u64 = 1000;
/// Coding agents' sessions known at once (B)…
pub const MAX_SESSIONS: usize = 32;
/// …and the numbers they get: one per number key.
pub const SESSION_NUMBERS: u8 = 9;
/// A session not heard of for this long is forgotten, and its number goes
/// to the next one (a session that crashed never says it ended).
pub const SESSION_KEEP_MS: u64 = 12 * 60 * 60 * 1000;
/// Tool calls one session may have waiting at once.
const MAX_PENDING: usize = 16;
/// A session counts as live (for its number and for B) this long after it
/// was last heard of, or while its question is open.
pub const SESSION_LIVE_MS: u64 = 30 * 60_000;
/// Input sooner than this after a question showed (and before its opening
/// flashes ended) was already on its way: it does not put the question out.
pub const ACK_GRACE_MS: u64 = 1_000;
/// A helper agent's wave (H) runs once across the F row at this speed.
const HELPER_SPEED: u32 = 4;

/// Now, on both clocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Now {
    /// Steady milliseconds.
    pub mono: u64,
    /// Unix milliseconds, for the log.
    pub wall: f64,
}

/// What a light is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Signal(SignalName),
    Light,
    Progress,
    /// The page's "Try on the keyboard".
    Try,
    /// A signal that ended while the user was away, flashing once on their
    /// return.
    Back,
    /// A coding agent stopped at its usage limit (A).
    UsageLimit,
    /// A coding agent's helper agent finished (H).
    Helper,
}

impl Kind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Signal(name) => name.as_str(),
            Self::Light => "light",
            Self::Progress => "progress",
            Self::Try => "try",
            Self::Back => "back",
            Self::UsageLimit => "usage_limit",
            Self::Helper => "helper",
        }
    }

    /// Higher shows over lower on a key both name.
    fn rank(&self) -> u8 {
        match self {
            Self::Try => 5,
            Self::Signal(SignalName::Attention) => 4,
            Self::Back => 3,
            Self::Signal(SignalName::Error) | Self::UsageLimit => 2,
            _ => 1,
        }
    }
}

/// One light.
#[derive(Debug, Clone, PartialEq)]
pub struct Lamp {
    pub source: String,
    pub id: String,
    pub kind: Kind,
    pub look: Look,
    pub keys: KeySpec,
    /// When it came (its look starts over then).
    pub since: u64,
    pub until: u64,
    /// Counts up with every light placed: higher is newer.
    order: u64,
    pub percent: Option<f64>,
    pub label: Option<String>,
    /// It came while nobody was at the computer.
    while_away: bool,
    /// When it flashes again (the user came back).
    burst: Option<u64>,
    /// Its LEDs and places, worked out for this model.
    leds: Vec<(u8, Place)>,
    leds_for: Option<String>,
    /// A picture of its own: one colour per LED of `leds`, in the same
    /// order (a profile's per-key design); `look.color` otherwise.
    paint: Option<Vec<Rgb>>,
}

impl Lamp {
    /// The colour of its `i`-th LED.
    fn color_at(&self, i: usize) -> Rgb {
        self.paint.as_ref().and_then(|p| p.get(i)).copied().unwrap_or(self.look.color)
    }
}

#[derive(Debug, Default)]
struct Source {
    lamps: Vec<Lamp>,
    /// Replacements that came past the rate, the newest per id, in the
    /// order they came.
    queued: Vec<Lamp>,
    /// When its counted requests came, in the last second.
    requests: VecDeque<u64>,
}

/// A coding agent's session, known from its hooks.
#[derive(Debug, Clone, PartialEq)]
struct Session {
    agent: Option<String>,
    /// Its project folder's name.
    label: Option<String>,
    /// 1 to [`SESSION_NUMBERS`], in the order sessions first came; `None`
    /// when every number was taken.
    number: Option<u8>,
    last_seen: u64,
    /// What its question waits for: one entry per tool call, each by the
    /// keys that name it (its id, its content, its turn).
    pending: Vec<BTreeSet<String>>,
    /// A question with no call of its own waits: any answer ends it.
    open: bool,
    /// The keys of the turn whose failure shows: the agent going on with
    /// that turn (Codex retrying a request) takes the failure away.
    failed: Vec<String>,
    /// The user's input put its question out before it was answered: the
    /// session still waits (its number stays on, B) until this time, the
    /// question's own end.
    acked_until: Option<u64>,
    /// Its number key's LEDs, for this model.
    digit: Vec<(u8, Place)>,
    digit_for: Option<String>,
    /// When its turn began (the user's prompt), for "a long task done" (F).
    turn_since: Option<u64>,
}

/// How a request goes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Admit {
    Now,
    /// Past the rate, replacing one of the source's lights: it waits.
    Queue,
}

/// A setting an agent changed.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub key: String,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, PartialEq)]
struct Entry {
    seq: u32,
    at_ms: f64,
    source: String,
    command: &'static str,
    error: Option<ErrorCode>,
    id: Option<String>,
    changes: Vec<Change>,
    undone: bool,
    /// Keys "Undo" left alone: they changed again since.
    kept: Vec<String>,
}

impl Entry {
    fn undoable(&self) -> bool {
        !self.changes.is_empty() && !self.undone
    }
}

/// What an operation did besides its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Effects {
    /// The settings changed (save them).
    pub settings: bool,
}

#[derive(Debug, Default)]
pub struct Engine {
    settings: AgentSettings,
    sources: BTreeMap<String, Source>,
    log: VecDeque<Entry>,
    next_seq: u32,
    next_order: u64,
    /// When each source's last refusal went in the log.
    refusals: HashMap<String, u64>,
    /// Nobody has touched the computer for `away.after`.
    away: bool,
    missed: Vec<Lamp>,
    /// Coding agents' sessions, by source.
    sessions: BTreeMap<String, Session>,
    /// When the user last touched the keyboard or the mouse.
    last_input: Option<u64>,
    /// Since when the lights show (not hidden by a game, the night, a
    /// missing cable); `None` while they do not.
    visible_since: Option<u64>,
    /// Something the page shows changed.
    dirty: bool,
}

fn rgb(c: kb_agent::Color) -> Rgb {
    [c.r, c.g, c.b]
}

/// Lights cannot show for want of a keyboard on the cable, not for anything
/// wrong with the request.
fn is_unlit(e: &AgentError) -> bool {
    matches!(e.code, ErrorCode::NeedsUsb | ErrorCode::NoKeyboard | ErrorCode::Unsupported)
}

impl Engine {
    pub fn new(settings: AgentSettings) -> Self {
        Self { settings, next_seq: 1, ..Self::default() }
    }

    pub fn settings(&self) -> &AgentSettings {
        &self.settings
    }

    /// The user saved the page. Turning access off takes every agent's
    /// lights away.
    pub fn set_settings(&mut self, settings: AgentSettings) {
        if self.settings.enabled && !settings.enabled {
            self.clear_agents();
        }
        self.settings = settings;
        self.dirty = true;
    }

    /// Something the page shows changed since the last call.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Lights are on or queued: the worker draws 20 times a second.
    pub fn lit(&self) -> bool {
        self.sources.values().any(|s| !s.lamps.is_empty() || !s.queued.is_empty())
    }

    /// Only lights kept for the user's return: the worker just watches.
    pub fn waiting(&self) -> bool {
        !self.missed.is_empty()
    }

    #[cfg(test)]
    pub fn busy(&self) -> bool {
        self.lit() || self.waiting()
    }

    /// A try from the page is on.
    #[cfg(test)]
    pub fn trying(&self, now: u64) -> bool {
        self.sources.get(TRY_SOURCE).is_some_and(|s| s.lamps.iter().any(|l| l.until > now))
    }

    fn lamps(&self) -> impl Iterator<Item = &Lamp> {
        self.sources.values().flat_map(|s| s.lamps.iter())
    }

    // ------------------------------------------------------------ requests

    /// Carry out a command that is not `hello`, `status` or `keys` (the
    /// caller answers those). `lights` is the keyboard on the cable, or why
    /// there is none.
    pub fn command(
        &mut self,
        source: &SourceName,
        command: Command,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
    ) -> (Result<Value, AgentError>, Effects) {
        let name = command.name();
        match command {
            Command::Log(args) => (args.validate().map(|limit| self.log_view(limit)), Effects::default()),
            Command::SettingsGet(args) => {
                (args.validate().and_then(|key| self.settings_get(key.as_deref())), Effects::default())
            }
            Command::Hello(_) | Command::Status(_) | Command::Keys(_) => (
                Err(AgentError::new(ErrorCode::Internal, format!("{name} is answered elsewhere."))),
                Effects::default(),
            ),
            changing => {
                let mut effects = Effects::default();
                let mut id = None;
                let mut changes = Vec::new();
                let result = self.change(source, changing, lights, now, &mut id, &mut changes, &mut effects);
                let log = match &result {
                    // Queued past the rate: the entry of the light it replaces
                    // stands for it.
                    Ok(data) if data["queued"] == json!(true) => false,
                    // Nothing to clear (a hook that clears after every step).
                    Ok(data) if data["cleared"] == json!(0) && name == "clear" => false,
                    // A hook event that changed no light: a session starting, an
                    // answer with nothing waiting, a tool call after another.
                    Ok(data) if name == "hook" && data["lit"] != json!(true) && data["cleared"] == json!(0) => false,
                    Ok(_) => true,
                    // Refusals at most once a second per source: an agent whose
                    // access is off, or that keeps sending a bad request, does
                    // not push everything else out of the log.
                    Err(_) => {
                        let last = self.refusals.get(source.as_str()).copied();
                        let log = last.is_none_or(|t| now.mono.saturating_sub(t) >= REFUSAL_LOG_EVERY_MS);
                        if log {
                            self.refusals.insert(source.as_str().to_string(), now.mono);
                        }
                        log
                    }
                };
                if log {
                    self.record(source.as_str(), name, result.as_ref().err().map(|e| e.code), id, changes, now);
                }
                (result, effects)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn change(
        &mut self,
        source: &SourceName,
        command: Command,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id: &mut Option<String>,
        changes: &mut Vec<Change>,
        effects: &mut Effects,
    ) -> Result<Value, AgentError> {
        if !self.settings.enabled {
            return Err(AgentError::new(ErrorCode::AgentAccessOff, "Agent access is off in Keymotion.")
                .with_hint("The user can turn it on in Keymotion › Tools › AI agents."));
        }
        match command {
            Command::Signal(args) => self.signal(source, &args, lights, now, id),
            Command::Light(args) => self.light(source, &args, lights, now, id),
            Command::Progress(args) => self.progress(source, &args, lights, now, id),
            Command::Clear(args) => self.clear(source, &args, id),
            Command::Hook(args) => self.hook(source, &args, lights, now, id),
            Command::SettingsSet(args) => {
                let result = self.settings_set(source, &args, now, changes);
                effects.settings = result.is_ok() && !changes.is_empty();
                result
            }
            other => Err(AgentError::new(ErrorCode::Internal, format!("{} is not a change.", other.name()))),
        }
    }

    /// Count a request of `source`. Past the rate, one that replaces the
    /// source's light `id` waits in the queue.
    fn admit(&mut self, source: &str, id: Option<&str>, now: u64) -> Result<Admit, AgentError> {
        if !self.sources.contains_key(source) {
            if self.sources.len() >= MAX_SOURCES {
                self.sources.retain(|_, s| !s.lamps.is_empty() || !s.queued.is_empty());
            }
            if self.sources.len() >= MAX_SOURCES {
                return Err(AgentError::new(ErrorCode::RateLimited, "Too many agents have lights at once.")
                    .with_retry_after(std::time::Duration::from_secs(10)));
            }
        }
        let s = self.sources.entry(source.to_string()).or_default();
        if s.take(now) {
            return Ok(Admit::Now);
        }
        if id.is_some_and(|id| s.lamps.iter().chain(&s.queued).any(|l| l.id == id)) {
            return Ok(Admit::Queue);
        }
        let wait = 1000 - now.saturating_sub(*s.requests.front().unwrap_or(&now)).min(1000);
        Err(AgentError::new(ErrorCode::RateLimited, format!("More than {RATE_PER_SECOND} requests a second."))
            .with_hint("Send fewer, or reuse one id so newer lights replace older ones.")
            .with_retry_after(std::time::Duration::from_millis(wait.max(1))))
    }

    fn signal(
        &mut self,
        source: &SourceName,
        args: &SignalArgs,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let s = args.validate()?;
        let id = s.id.clone().unwrap_or_else(|| SignalId::default_for(source, s.name)).as_str().to_string();
        *id_out = Some(id.clone());
        let admit = self.admit(source.as_str(), Some(&id), now.mono)?;
        let chosen = *self.signal_look(s.name);
        let color = s.color.map(rgb).unwrap_or(chosen.color);
        let look = match &s.effect {
            Some(effect) => Some(Look::plain(color, Motion::of(effect))),
            None => Look::of_effect(color, chosen.effect, chosen.speed)
                // A colour for a look the user turned off: steady in it.
                .or_else(|| s.color.map(|_| Look::plain(color, Motion::Solid))),
        };
        // `busy` takes the source's lights away first, cable or not: a
        // question must not come back when the keyboard is plugged in again.
        let cleared = if s.name == SignalName::Busy { self.remove(source.as_str(), |_| true) } else { 0 };
        let Some(look) = look else {
            return Ok(json!({ "id": id, "shown": false, "lit": false, "cleared": cleared }));
        };
        let keys = s.keys.clone().unwrap_or_default();
        let index = lights?;
        let leds = index.resolve(&keys)?;
        let for_ms = match (s.duration, s.effect.as_ref().and_then(|e| e.run_time())) {
            (Some(d), _) | (None, Some(d)) => duration_ms(d),
            (None, None) => u64::from(chosen.duration_s) * 1000,
        };
        let mut lamp = self.lamp(source.as_str(), &id, Kind::Signal(s.name), look, keys, now.mono, for_ms);
        lamp.leds = leds;
        lamp.leds_for = Some(index.model_id.clone());
        let mut answer = self.put(lamp, admit, now.mono);
        if s.name == SignalName::Busy {
            answer["cleared"] = json!(cleared);
        }
        Ok(answer)
    }

    fn light(
        &mut self,
        source: &SourceName,
        args: &LightArgs,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let l = args.validate()?;
        let id = l.id.as_ref().map_or_else(|| format!("{}:light", source.as_str()), |id| id.as_str().to_string());
        *id_out = Some(id.clone());
        let admit = self.admit(source.as_str(), Some(&id), now.mono)?;
        let index = lights?;
        let leds = index.resolve(&l.keys)?;
        let for_ms = match (l.duration, l.effect.run_time()) {
            (Some(d), _) | (None, Some(d)) => duration_ms(d),
            (None, None) => LIGHT_DEFAULT_MS,
        };
        let look = Look::plain(rgb(l.color), Motion::of(&l.effect));
        let mut lamp = self.lamp(source.as_str(), &id, Kind::Light, look, l.keys, now.mono, for_ms);
        lamp.leds = leds;
        lamp.leds_for = Some(index.model_id.clone());
        Ok(self.put(lamp, admit, now.mono))
    }

    fn progress(
        &mut self,
        source: &SourceName,
        args: &ProgressArgs,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let p = args.validate()?;
        let id = p.id.as_ref().map_or_else(|| format!("{}:progress", source.as_str()), |id| id.as_str().to_string());
        *id_out = Some(id.clone());
        let admit = self.admit(source.as_str(), Some(&id), now.mono)?;
        let keys = p.keys.clone().unwrap_or_else(|| {
            let row = match self.settings.progress.keys {
                AgentProgressKeys::FRow => KeyGroup::FRow,
                AgentProgressKeys::Numbers => KeyGroup::Numbers,
            };
            KeySpec::parse(row.as_str()).unwrap_or_default()
        });
        let index = lights?;
        let leds = index.resolve(&keys)?;
        let color = p.color.map(rgb).unwrap_or(self.settings.progress.color);
        let look = Look::plain(color, Motion::Bar { fraction: (p.percent / 100.0) as f32 });
        let for_ms = p.duration.map_or(PROGRESS_DEFAULT_MS, duration_ms);
        let mut lamp = self.lamp(source.as_str(), &id, Kind::Progress, look, keys, now.mono, for_ms);
        lamp.percent = Some(p.percent);
        lamp.label = p.label;
        lamp.leds = leds;
        lamp.leds_for = Some(index.model_id.clone());
        let mut answer = self.put(lamp, admit, now.mono);
        answer["percent"] = json!(p.percent);
        Ok(answer)
    }

    // --------------------------------------------------------------- hooks

    /// A coding agent's hook event, from the session `source`.
    fn hook(
        &mut self,
        source: &SourceName,
        args: &HookArgs,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let hook = args.validate()?;
        let name = source.as_str();
        if hook.event == HookEvent::SessionEnd {
            // Nobody is left to answer its question; what it finished or
            // where it failed stays its time (a headless run ends its
            // session right after its last turn).
            let cleared = self.answer_all(name);
            if self.sessions.remove(name).is_some() {
                self.dirty = true;
            }
            return Ok(json!({ "lit": false, "cleared": cleared }));
        }
        let known = self.sessions.contains_key(name);
        let session = self.session(name, now.mono)?;
        if let Some(agent) = hook.agent.as_ref().map(|a| a.as_str().to_string()) {
            session.agent = Some(agent);
        }
        let relabeled = hook.label.is_some() && hook.label != session.label;
        if relabeled {
            session.label.clone_from(&hook.label);
        }
        if relabeled || !known {
            self.dirty = true;
        }
        let number = self.sessions.get(name).and_then(|s| s.number);
        let mut answer = match hook.event {
            HookEvent::SessionStart | HookEvent::SessionEnd => json!({ "lit": false, "cleared": 0 }),
            // The user stopped the turn: its question and its "working"
            // light go; what came before stays.
            HookEvent::Interrupted => {
                if let Some(s) = self.sessions.get_mut(name) {
                    s.turn_since = None;
                }
                let cleared = self.answer_all(name) + self.remove(name, |l| l.kind == Kind::Signal(SignalName::Busy));
                json!({ "lit": false, "cleared": cleared })
            }
            HookEvent::Busy => {
                if let Some(s) = self.sessions.get_mut(name) {
                    s.turn_since = Some(now.mono);
                }
                let asked = self.answer_all(name);
                let mut a = self.signal(
                    source,
                    &SignalArgs { name: "busy".into(), ..Default::default() },
                    lights,
                    now,
                    id_out,
                )?;
                a["cleared"] = json!(asked + a["cleared"].as_u64().unwrap_or(0) as usize);
                a
            }
            HookEvent::Attention => self.ask(source, &hook.calls, lights, now, id_out)?,
            HookEvent::Answered => {
                let mut cleared = self.answer(name, &hook.calls);
                let retried = self.sessions.get_mut(name).is_some_and(|s| {
                    hook.calls.iter().any(|c| s.failed.contains(c)) && !std::mem::take(&mut s.failed).is_empty()
                });
                if retried {
                    cleared +=
                        self.remove(name, |l| matches!(l.kind, Kind::Signal(SignalName::Error) | Kind::UsageLimit));
                }
                json!({ "lit": false, "cleared": cleared, "waiting": self.waiting_calls(name) })
            }
            // The same turn failing again (Codex trying a request again and
            // again): the failure that shows goes on as it was, with no new
            // blinks.
            HookEvent::Error | HookEvent::UsageLimit if self.fails_again(name, &hook.calls, now.mono) => {
                json!({ "lit": true, "kept": true, "cleared": 0 })
            }
            HookEvent::Done | HookEvent::Error => {
                let cleared = self.answer_all(name);
                let mut turn = None;
                if let Some(s) = self.sessions.get_mut(name) {
                    s.failed = if hook.event == HookEvent::Error { hook.calls.clone() } else { Vec::new() };
                    turn = s.turn_since.take().map(|t| now.mono.saturating_sub(t));
                }
                let signal = if hook.event == HookEvent::Done { "done" } else { "error" };
                // Lit or not (no cable), the turn's length is the app's to
                // know: a long task done may still put a card up (F).
                let lit =
                    self.signal(source, &SignalArgs { name: signal.into(), ..Default::default() }, lights, now, id_out);
                let mut a = match lit {
                    Ok(a) => a,
                    Err(e) if hook.event == HookEvent::Done && turn.is_some() && is_unlit(&e) => {
                        json!({ "lit": false, "shown": false, "unlit": e.code.as_str() })
                    }
                    Err(e) => return Err(e),
                };
                a["cleared"] = json!(cleared);
                if hook.event == HookEvent::Done {
                    a["turn_ms"] = json!(turn);
                }
                a
            }
            HookEvent::UsageLimit => {
                let cleared = self.answer_all(name);
                if let Some(s) = self.sessions.get_mut(name) {
                    s.failed.clone_from(&hook.calls);
                    s.turn_since = None;
                }
                let mut a = if self.settings.usage_limit.on {
                    self.usage_limit(source, lights, now, id_out)?
                } else {
                    self.signal(
                        source,
                        &SignalArgs { name: "error".into(), ..Default::default() },
                        lights,
                        now,
                        id_out,
                    )?
                };
                a["cleared"] = json!(cleared);
                a
            }
            HookEvent::Helper => self.helper(source, lights, now, id_out)?,
        };
        answer["session"] = json!(number);
        Ok(answer)
    }

    /// `name`'s session, known from now on: it gets the lowest free number.
    fn session(&mut self, name: &str, now: u64) -> Result<&mut Session, AgentError> {
        if !self.sessions.contains_key(name) {
            if self.sessions.len() >= MAX_SESSIONS {
                // The one heard of longest ago, unless it has lights.
                let oldest = self
                    .sessions
                    .iter()
                    .filter(|(source, _)| self.sources.get(*source).is_none_or(|s| s.lamps.is_empty()))
                    .min_by_key(|(_, s)| s.last_seen)
                    .map(|(source, _)| source.clone());
                match oldest {
                    Some(source) => {
                        self.sessions.remove(&source);
                    }
                    None => {
                        return Err(AgentError::new(ErrorCode::RateLimited, "Too many agent sessions at once.")
                            .with_retry_after(std::time::Duration::from_secs(10)))
                    }
                }
            }
            let number = self.free_number(name, now);
            self.sessions.insert(
                name.to_string(),
                Session {
                    agent: None,
                    label: None,
                    number,
                    last_seen: now,
                    pending: Vec::new(),
                    open: false,
                    failed: Vec::new(),
                    acked_until: None,
                    digit: Vec::new(),
                    digit_for: None,
                    turn_since: None,
                },
            );
        }
        if self.sessions.get(name).is_some_and(|s| s.number.is_none()) {
            let number = self.free_number(name, now);
            if let Some(s) = self.sessions.get_mut(name) {
                s.number = number;
                s.digit_for = None;
            }
        }
        let session = self.sessions.get_mut(name).expect("just made sure it is there");
        session.last_seen = now;
        Ok(session)
    }

    /// The lowest number no live session (but `name`) holds; a session that
    /// is not live any more gives its number up for it.
    fn free_number(&mut self, name: &str, now: u64) -> Option<u8> {
        let held: HashSet<u8> = self
            .sessions
            .iter()
            .filter(|(source, s)| source.as_str() != name && self.live(source, s, now))
            .filter_map(|(_, s)| s.number)
            .collect();
        let number = (1..=SESSION_NUMBERS).find(|n| !held.contains(n))?;
        for (source, s) in &mut self.sessions {
            if source != name && s.number == Some(number) {
                s.number = None;
                s.digit.clear();
                s.digit_for = None;
            }
        }
        Some(number)
    }

    /// Heard of lately, or its question is open.
    fn live(&self, source: &str, s: &Session, now: u64) -> bool {
        now.saturating_sub(s.last_seen) < SESSION_LIVE_MS || self.waits(source, now)
    }

    /// The session waits for the user: its calls are kept, and its question
    /// lights, unless it is up already (it then goes on as it was, with no
    /// new flashes).
    fn ask(
        &mut self,
        source: &SourceName,
        calls: &[String],
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let name = source.as_str();
        let id = SignalId::default_for(source, SignalName::Attention).as_str().to_string();
        let mut seen = None;
        if let Some(s) = self.sessions.get_mut(name) {
            let keys: BTreeSet<String> = calls.iter().cloned().collect();
            let new = if keys.is_empty() {
                // A question with no call of its own, unless it is the
                // notification that backs up a call's question already here.
                let new = s.pending.is_empty() && !s.open;
                if s.pending.is_empty() {
                    s.open = true;
                }
                new
            } else if !s.pending.contains(&keys) {
                if s.pending.len() >= MAX_PENDING {
                    s.pending.remove(0);
                }
                s.pending.push(keys);
                true
            } else {
                false
            };
            if new {
                s.acked_until = None;
            } else {
                // The user's input put this question out already: asked
                // again (the notification that backs it up), it stays out.
                seen = s.acked_until.filter(|until| now.mono < *until);
            }
            if let (Some(n), Ok(index)) = (s.number, lights.as_ref()) {
                if s.digit_for.as_deref() != Some(index.model_id.as_str()) {
                    s.digit = KeySpec::parse(&n.to_string()).map(|k| index.resolve_lenient(&k)).unwrap_or_default();
                    s.digit_for = Some(index.model_id.clone());
                }
            }
        }
        if let Some(until) = seen {
            *id_out = Some(id.clone());
            let expires = until - now.mono;
            return Ok(
                json!({ "id": id, "shown": false, "lit": false, "kept": true, "expires_in_ms": expires, "cleared": 0 }),
            );
        }
        let up = self.sources.get(name).and_then(|s| s.lamps.iter().find(|l| l.id == id && l.until > now.mono));
        if let Some(lamp) = up {
            let expires = lamp.until.saturating_sub(now.mono);
            *id_out = Some(id.clone());
            let shown = self.shown(now.mono).contains(&(name.to_string(), id.clone()));
            self.dirty = true;
            return Ok(
                json!({ "id": id, "shown": shown, "lit": true, "kept": true, "expires_in_ms": expires, "cleared": 0 }),
            );
        }
        let mut answer =
            self.signal(source, &SignalArgs { name: "attention".into(), ..Default::default() }, lights, now, id_out)?;
        answer["cleared"] = json!(0);
        Ok(answer)
    }

    /// What `calls` name was answered: each waiting call one of these keys
    /// names. None given: everything the session waits for, its questions
    /// with no call of their own too (an MCP form, a notification's
    /// question); a call's answer does not end those (a helper agent's tool
    /// call finishing says nothing of a form that waits). When nothing waits
    /// any more, the session's question goes, if its look stays until
    /// answered. How many lights went.
    fn answer(&mut self, name: &str, calls: &[String]) -> usize {
        let Some(s) = self.sessions.get_mut(name) else { return 0 };
        if calls.is_empty() {
            s.pending.clear();
            s.open = false;
        } else {
            s.pending.retain(|keys| !calls.iter().any(|c| keys.contains(c)));
        }
        if !s.pending.is_empty() || s.open {
            return 0;
        }
        s.acked_until = None;
        if !self.settings.signals.attention.until_answered {
            return 0;
        }
        self.remove(name, |l| l.kind == Kind::Signal(SignalName::Attention))
    }

    /// The turn ended (or a new one began): nothing of `name`'s session waits
    /// any more, and its question goes whatever its look.
    fn answer_all(&mut self, name: &str) -> usize {
        if let Some(s) = self.sessions.get_mut(name) {
            s.pending.clear();
            s.open = false;
            s.failed.clear();
            s.acked_until = None;
        }
        self.remove(name, |l| l.kind == Kind::Signal(SignalName::Attention))
    }

    /// `name`'s session fails again where its failure shows: the same keys,
    /// and its error or usage-limit light still up.
    fn fails_again(&self, name: &str, calls: &[String], now: u64) -> bool {
        !calls.is_empty()
            && self.sessions.get(name).is_some_and(|s| s.failed == calls)
            && self.sources.get(name).is_some_and(|s| {
                s.lamps
                    .iter()
                    .any(|l| l.until > now && matches!(l.kind, Kind::Signal(SignalName::Error) | Kind::UsageLimit))
            })
    }

    fn waiting_calls(&self, name: &str) -> usize {
        self.sessions.get(name).map_or(0, |s| s.pending.len() + usize::from(s.open))
    }

    /// A: the usage limit's own look.
    fn usage_limit(
        &mut self,
        source: &SourceName,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let id = format!("{}:usage_limit", source.as_str());
        *id_out = Some(id.clone());
        let chosen = self.settings.usage_limit.look;
        let admit = self.admit(source.as_str(), Some(&id), now.mono)?;
        let Some(look) = Look::of_effect(chosen.color, chosen.effect, chosen.speed) else {
            return Ok(json!({ "id": id, "shown": false, "lit": false }));
        };
        let index = lights?;
        let keys = KeySpec::all();
        let leds = index.resolve(&keys)?;
        let for_ms = u64::from(chosen.duration_s) * 1000;
        let mut lamp = self.lamp(source.as_str(), &id, Kind::UsageLimit, look, keys, now.mono, for_ms);
        lamp.leds = leds;
        lamp.leds_for = Some(index.model_id.clone());
        Ok(self.put(lamp, admit, now.mono))
    }

    /// H: one short wave across the F row, when the user turned it on.
    fn helper(
        &mut self,
        source: &SourceName,
        lights: Result<&KeyIndex, AgentError>,
        now: Now,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let id = format!("{}:helper", source.as_str());
        if !self.settings.helper.on {
            return Ok(json!({ "id": id, "shown": false, "lit": false, "cleared": 0 }));
        }
        *id_out = Some(id.clone());
        let admit = self.admit(source.as_str(), Some(&id), now.mono)?;
        let index = lights?;
        let keys = KeySpec::parse(KeyGroup::FRow.as_str()).unwrap_or_default();
        let leds = index.resolve(&keys)?;
        let effect = kb_agent::Effect::new(kb_agent::EffectKind::Wave, None, Some(HELPER_SPEED), Some(1))?;
        let for_ms = effect.run_time().map_or(2_000, duration_ms);
        let look = Look::plain(self.settings.helper.color, Motion::of(&effect));
        let mut lamp = self.lamp(source.as_str(), &id, Kind::Helper, look, keys, now.mono, for_ms);
        lamp.leds = leds;
        lamp.leds_for = Some(index.model_id.clone());
        let mut answer = self.put(lamp, admit, now.mono);
        answer["cleared"] = json!(0);
        Ok(answer)
    }

    /// The sessions for the page and `status`, by number.
    pub fn sessions(&self, now: u64) -> Vec<AgentSessionInfo> {
        let mut out: Vec<AgentSessionInfo> = self
            .sessions
            .iter()
            .map(|(source, s)| AgentSessionInfo {
                number: s.number,
                source: source.clone(),
                agent: s.agent.clone(),
                label: s.label.clone(),
                waiting: self.waits(source, now),
            })
            .collect();
        out.sort_by_key(|s| (s.number.is_none(), s.number));
        out
    }

    /// `source`'s question is up.
    fn asks(&self, source: &str, now: u64) -> bool {
        self.sources
            .get(source)
            .is_some_and(|s| s.lamps.iter().any(|l| l.kind == Kind::Signal(SignalName::Attention) && l.until > now))
    }

    /// `source`'s session waits for the user: its question is up, or the
    /// user's input put it out before it was answered (until the question's
    /// own end).
    fn waits(&self, source: &str, now: u64) -> bool {
        self.asks(source, now)
            || self
                .sessions
                .get(source)
                .is_some_and(|s| s.acked_until.is_some_and(|until| now < until) && (!s.pending.is_empty() || s.open))
    }

    /// B: the number keys of the sessions that wait, when two or more
    /// sessions are live and the user wants them.
    fn session_digits(&self, now: u64) -> Vec<(u8, Place)> {
        let live = self.sessions.iter().filter(|(source, s)| s.number.is_some() && self.live(source, s, now)).count();
        if !self.settings.sessions.on || live < 2 {
            return Vec::new();
        }
        self.sessions
            .iter()
            .filter(|(source, s)| s.number.is_some() && self.waits(source, now))
            .flat_map(|(_, s)| s.digit.iter().copied())
            .collect()
    }

    fn clear(
        &mut self,
        source: &SourceName,
        args: &ClearArgs,
        id_out: &mut Option<String>,
    ) -> Result<Value, AgentError> {
        let id = args.validate()?;
        *id_out = id.as_ref().map(|id| id.as_str().to_string());
        let cleared = match &id {
            Some(id) => self.remove(source.as_str(), |l| l.id == id.as_str()),
            None => self.remove(source.as_str(), |_| true),
        };
        Ok(json!({ "cleared": cleared }))
    }

    /// Take away `source`'s lights that `which` picks, with what waits in
    /// its queue and what was kept of them for the user's return. How many
    /// lights went.
    fn remove(&mut self, source: &str, which: impl Fn(&Lamp) -> bool) -> usize {
        let before = self.missed.len();
        self.missed.retain(|l| !(l.source == source && which(l)));
        let gone_missed = before - self.missed.len();
        let Some(s) = self.sources.get_mut(source) else { return 0 };
        s.queued.retain(|l| !which(l));
        let before = s.lamps.len();
        s.lamps.retain(|l| !which(l));
        let gone = before - s.lamps.len();
        if gone + gone_missed > 0 {
            self.dirty = true;
        }
        gone
    }

    /// Every agent's lights go (the page's "Turn off every agent light", or
    /// access turned off). The page's own try stays.
    pub fn clear_agents(&mut self) -> usize {
        let mut gone = 0;
        for (name, s) in &mut self.sources {
            if name != TRY_SOURCE {
                gone += s.lamps.len();
                s.lamps.clear();
                s.queued.clear();
            }
        }
        self.missed.clear();
        for s in self.sessions.values_mut() {
            s.pending.clear();
            s.open = false;
        }
        self.dirty = true;
        gone
    }

    fn signal_look(&self, name: SignalName) -> &AgentLook {
        let s = &self.settings.signals;
        match name {
            SignalName::Done => &s.done,
            SignalName::Attention => &s.attention,
            SignalName::Error => &s.error,
            SignalName::Busy => &s.busy,
            SignalName::Info => &s.info,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lamp(&self, source: &str, id: &str, kind: Kind, look: Look, keys: KeySpec, now: u64, for_ms: u64) -> Lamp {
        let away = self.away && self.settings.away.on && matches!(kind, Kind::Signal(_));
        Lamp {
            source: source.to_string(),
            id: id.to_string(),
            kind,
            look,
            keys,
            since: now,
            until: now.saturating_add(for_ms.max(1)),
            order: 0,
            percent: None,
            label: None,
            while_away: away,
            burst: None,
            leds: Vec::new(),
            leds_for: None,
            paint: None,
        }
    }

    /// Put `lamp` up now, or in its source's queue. The answer: its id,
    /// whether it shows now and for how long, what it replaced or pushed
    /// out.
    fn put(&mut self, lamp: Lamp, admit: Admit, now: u64) -> Value {
        let (source, id, expires) = (lamp.source.clone(), lamp.id.clone(), lamp.until.saturating_sub(now));
        let queued = admit == Admit::Queue;
        let (replaced, evicted) = if queued {
            let s = self.sources.entry(source.clone()).or_default();
            s.queued.retain(|l| l.id != lamp.id);
            s.queued.push(lamp);
            (true, Vec::new())
        } else {
            self.place(lamp)
        };
        let shown = self.shown(now).contains(&(source, id.clone()));
        json!({
            "id": id,
            "shown": shown,
            "lit": true,
            "expires_in_ms": expires,
            "replaced": replaced,
            "evicted": evicted,
            "coalesced": queued,
            "queued": queued,
        })
    }

    /// Put `lamp` in its source's place: over the light with its id, or as
    /// a new one (the oldest goes when the source has [`MAX_PER_SOURCE`]).
    /// Whether it replaced one, and the ids it pushed out.
    fn place(&mut self, mut lamp: Lamp) -> (bool, Vec<String>) {
        lamp.order = self.next_order;
        self.next_order += 1;
        self.missed.retain(|m| !(m.source == lamp.source && m.id == lamp.id));
        let s = self.sources.entry(lamp.source.clone()).or_default();
        // An older replacement still waiting must not land over this one.
        s.queued.retain(|l| l.id != lamp.id);
        let mut evicted = Vec::new();
        let replaced = match s.lamps.iter_mut().find(|l| l.id == lamp.id) {
            Some(old) => {
                *old = lamp;
                true
            }
            None => {
                while s.lamps.len() >= MAX_PER_SOURCE {
                    let oldest = s.lamps.iter().enumerate().min_by_key(|(_, l)| l.order).map(|(i, _)| i).unwrap_or(0);
                    evicted.push(s.lamps.remove(oldest).id);
                }
                s.lamps.push(lamp);
                false
            }
        };
        self.dirty = true;
        (replaced, evicted)
    }

    /// The worker's turn: queued replacements go up, as many as each
    /// source's rate allows now (so at most [`RATE_PER_SECOND`] a second,
    /// together with its other requests). How many went up.
    pub fn apply_queued(&mut self, now: u64) -> usize {
        let mut ready = Vec::new();
        for s in self.sources.values_mut() {
            while !s.queued.is_empty() && s.take(now) {
                ready.push(s.queued.remove(0));
            }
        }
        let applied = ready.len();
        for lamp in ready {
            if lamp.until > now {
                self.place(lamp);
            }
        }
        applied
    }

    // ------------------------------------------- requests the app carries out

    /// Before a request the app carries out itself (`focus`, `profile`,
    /// `card`) changes anything: access is on, and the source is within its
    /// rate.
    pub fn gate(&mut self, source: &SourceName, now: Now) -> Result<(), AgentError> {
        if !self.settings.enabled {
            return Err(AgentError::new(ErrorCode::AgentAccessOff, "Agent access is off in Keymotion.")
                .with_hint("The user can turn it on in Keymotion › Tools › AI agents."));
        }
        self.admit(source.as_str(), None, now.mono).map(|_| ())
    }

    /// Log such a request, as `command` logs the others: a refusal at most
    /// once a second per source.
    pub fn note(
        &mut self,
        source: &SourceName,
        command: &'static str,
        error: Option<ErrorCode>,
        id: Option<String>,
        now: Now,
    ) {
        if error.is_some() {
            let last = self.refusals.get(source.as_str()).copied();
            if last.is_some_and(|t| now.mono.saturating_sub(t) < REFUSAL_LOG_EVERY_MS) {
                return;
            }
            self.refusals.insert(source.as_str().to_string(), now.mono);
        }
        self.record(source.as_str(), command, error, id, Vec::new(), now);
    }

    /// A picture of the source's own on the keys (a profile's per-key
    /// design): `keys` with their places and colours, on the keyboard of
    /// `model`. It replaces the source's light `id`.
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        source: &SourceName,
        id: &str,
        keys: Vec<(u8, Place, Rgb)>,
        model: &str,
        for_ms: u64,
        now: Now,
    ) -> Result<Value, AgentError> {
        let admit = self.admit(source.as_str(), Some(id), now.mono)?;
        let color = keys.first().map_or([255, 255, 255], |k| k.2);
        let mut lamp = self.lamp(
            source.as_str(),
            id,
            Kind::Light,
            Look::plain(color, Motion::Solid),
            KeySpec::all(),
            now.mono,
            for_ms,
        );
        lamp.leds = keys.iter().map(|&(led, place, _)| (led, place)).collect();
        lamp.paint = Some(keys.iter().map(|k| k.2).collect());
        lamp.leds_for = Some(model.to_string());
        Ok(self.put(lamp, admit, now.mono))
    }

    // ------------------------------------------------------------ settings

    fn settings_set(
        &mut self,
        source: &SourceName,
        args: &SettingsSetArgs,
        now: Now,
        changes: &mut Vec<Change>,
    ) -> Result<Value, AgentError> {
        let pairs = args.validate()?;
        self.admit(source.as_str(), None, now.mono)?;
        if !self.settings.agents_may_edit {
            return Err(AgentError::new(
                ErrorCode::PermissionOff,
                "The user has not let agents change Keymotion's settings.",
            )
            .with_hint("The user can allow it in Keymotion › Tools › AI agents."));
        }
        let mut next = self.settings;
        let mut found = Vec::new();
        for (key, value) in &pairs {
            let Some(before) = config::get(&next, key) else { return Err(config::unknown(key)) };
            if !config::agent_may_set(key) {
                return Err(AgentError::new(
                    ErrorCode::PermissionOff,
                    format!("Only the user can change '{key}', in Keymotion."),
                ));
            }
            config::set(&mut next, key, value)?;
            let after = config::get(&next, key).unwrap_or(Value::Null);
            if before != after {
                found.push(Change { key: key.clone(), before, after });
            }
        }
        self.settings = next.sanitized();
        if !found.is_empty() {
            self.dirty = true;
        }
        let changed: Vec<Value> =
            found.iter().map(|c| json!({ "key": c.key, "before": c.before, "after": c.after })).collect();
        *changes = found;
        Ok(json!({ "changed": changed, "log_entry": self.next_seq }))
    }

    fn settings_get(&self, key: Option<&str>) -> Result<Value, AgentError> {
        let all = config::keys();
        let picked: Vec<&String> = match key {
            None => all.iter().collect(),
            Some(key) => {
                let under = format!("{key}.");
                let picked: Vec<&String> = all.iter().filter(|k| *k == key || k.starts_with(&under)).collect();
                if picked.is_empty() {
                    return Err(config::unknown(key));
                }
                picked
            }
        };
        let mut settings = serde_json::Map::new();
        for k in picked {
            if let Some(v) = config::get(&self.settings, k) {
                settings.insert(k.clone(), v);
            }
        }
        Ok(json!({
            "settings": settings,
            "agents_may_edit": self.settings.agents_may_edit,
            "user_only": config::USER_ONLY,
        }))
    }

    /// "Undo" on the page: what log entry `seq` changed goes back, key by
    /// key, where it is still what that change left. The keys changed again
    /// since (by the user or an agent) stay as they are; they are returned
    /// and kept on the entry.
    pub fn undo(&mut self, seq: u32) -> Result<Vec<String>, String> {
        let entry = self.log.iter_mut().find(|e| e.seq == seq).ok_or("log entry not found")?;
        if !entry.undoable() {
            return Err("nothing to undo".into());
        }
        let mut next = self.settings;
        let mut kept = Vec::new();
        for c in entry.changes.iter().rev() {
            if config::get(&next, &c.key).as_ref() == Some(&c.after) {
                config::set(&mut next, &c.key, &c.before).map_err(|e| e.message)?;
            } else {
                kept.push(c.key.clone());
            }
        }
        if kept.len() == entry.changes.len() {
            return Err("these settings changed again since; nothing to undo".into());
        }
        kept.reverse();
        entry.undone = true;
        entry.kept.clone_from(&kept);
        self.settings = next.sanitized();
        self.dirty = true;
        Ok(kept)
    }

    // ----------------------------------------------------------------- log

    fn record(
        &mut self,
        source: &str,
        command: &'static str,
        error: Option<ErrorCode>,
        id: Option<String>,
        changes: Vec<Change>,
        now: Now,
    ) {
        // The same request again and again (a progress bar's updates) keeps
        // one entry, the newest, rather than filling the log.
        if let Some(last) = self.log.back_mut() {
            let again = last.source == source
                && last.command == command
                && last.id == id
                && last.error.is_none()
                && error.is_none()
                && last.changes.is_empty()
                && changes.is_empty();
            if again {
                last.at_ms = now.wall;
                self.dirty = true;
                return;
            }
        }
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1).max(1);
        if self.log.len() >= LOG_SIZE {
            // What can still be undone goes last.
            match self.log.iter().position(|e| !e.undoable()) {
                Some(i) => {
                    self.log.remove(i);
                }
                None => {
                    self.log.pop_front();
                }
            }
        }
        self.log.push_back(Entry {
            seq,
            at_ms: now.wall,
            source: source.to_string(),
            command,
            error,
            id,
            changes,
            undone: false,
            kept: Vec::new(),
        });
        self.dirty = true;
    }

    fn log_view(&self, limit: u32) -> Value {
        let entries: Vec<Value> = self
            .log
            .iter()
            .rev()
            .take(limit as usize)
            .map(|e| {
                let mut v = json!({
                    "seq": e.seq,
                    "at_ms": e.at_ms,
                    "source": e.source,
                    "command": e.command,
                    "result": e.error.map_or("ok", |c| c.as_str()),
                });
                if let Some(id) = &e.id {
                    v["id"] = json!(id);
                }
                if !e.changes.is_empty() {
                    v["changes"] = e
                        .changes
                        .iter()
                        .map(|c| json!({ "key": c.key, "before": c.before, "after": c.after }))
                        .collect();
                    v["undone"] = json!(e.undone);
                }
                v
            })
            .collect();
        json!({ "entries": entries })
    }

    /// The log for the page, newest first.
    pub fn log_entries(&self) -> Vec<AgentLogEntry> {
        self.log
            .iter()
            .rev()
            .map(|e| AgentLogEntry {
                seq: e.seq,
                at_ms: e.at_ms,
                source: e.source.clone(),
                command: e.command.to_string(),
                error: e.error.map(|c| c.as_str().to_string()),
                id: e.id.clone(),
                changes: e
                    .changes
                    .iter()
                    .map(|c| AgentSettingChange {
                        key: c.key.clone(),
                        before: config::value_text(&c.before),
                        after: config::value_text(&c.after),
                    })
                    .collect(),
                undone: e.undone,
                kept: e.kept.clone(),
            })
            .collect()
    }

    // ---------------------------------------------------------------- tries

    /// "Try on the keyboard": `look` on `keys` (default all) for a few
    /// seconds, over every agent's light. Works with access off.
    pub fn try_look(&mut self, look: &AgentLook, keys: KeySpec, index: &KeyIndex, now: u64) -> Result<(), AgentError> {
        let leds = index.resolve(&keys)?;
        let Some(drawn) = Look::of_effect(look.color, look.effect, look.speed) else {
            self.remove(TRY_SOURCE, |_| true);
            return Ok(());
        };
        let mut lamp = self.lamp(TRY_SOURCE, "try", Kind::Try, drawn, keys, now, TRY_MS);
        lamp.leds = leds;
        lamp.leds_for = Some(index.model_id.clone());
        self.place(lamp);
        Ok(())
    }

    // ------------------------------------------------------------ the clock

    /// Lights whose time is up go. One that came while the user was away
    /// is kept for their return, for up to [`MISSED_KEEP_MS`].
    pub fn prune(&mut self, now: u64) {
        let keep_missed = self.away && self.settings.away.on;
        let mut missed = Vec::new();
        for s in self.sources.values_mut() {
            s.lamps.retain(|l| {
                if l.until > now {
                    return true;
                }
                if keep_missed && l.while_away {
                    missed.push(l.clone());
                }
                false
            });
            s.queued.retain(|l| l.until > now);
            if s.lamps.is_empty() && s.requests.back().is_none_or(|&t| now.saturating_sub(t) >= 1000) {
                s.requests.clear();
            }
        }
        let before = self.missed.len();
        self.missed.retain(|l| now.saturating_sub(l.until) <= MISSED_KEEP_MS);
        if !missed.is_empty() || before != self.missed.len() {
            self.dirty = true;
        }
        self.missed.extend(missed);
        if self.missed.len() > MAX_MISSED {
            let extra = self.missed.len() - MAX_MISSED;
            self.missed.drain(..extra);
        }
        let before = self.sources.len();
        self.sources.retain(|_, s| !s.lamps.is_empty() || !s.queued.is_empty() || !s.requests.is_empty());
        if before != self.sources.len() {
            self.dirty = true;
        }
        self.refusals.retain(|_, t| now.saturating_sub(*t) < REFUSAL_LOG_EVERY_MS);
        let before = self.sessions.len();
        let sources = &self.sources;
        self.sessions.retain(|name, s| {
            now.saturating_sub(s.last_seen) < SESSION_KEEP_MS || sources.get(name).is_some_and(|x| !x.lamps.is_empty())
        });
        if before != self.sessions.len() {
            self.dirty = true;
        }
    }

    /// Whether the lights show now (a game may hide them, the night put
    /// them off, the cable be missing): input counts against a question only
    /// while they do.
    pub fn set_visible(&mut self, visible: bool, now: u64) {
        match (visible, self.visible_since) {
            (true, None) => self.visible_since = Some(now),
            (false, Some(_)) => self.visible_since = None,
            _ => {}
        }
    }

    /// The user's input (`idle_ms` since the last one). Coming back flashes
    /// what came meanwhile, once. Input after a question showed puts it out
    /// ([`Engine::acknowledge`]).
    pub fn observe_idle(&mut self, idle_ms: Option<u64>, now: u64) {
        if let Some(ms) = idle_ms {
            self.last_input = Some(now.saturating_sub(ms));
        }
        self.see_away(idle_ms, now);
        self.acknowledge(now);
    }

    /// Questions the user's input answered: input once the question showed
    /// on the keys (the lights visible, its opening flashes over, the flash
    /// of the user's return over) and [`ACK_GRACE_MS`] passed. Its session
    /// keeps waiting (B) until it is answered.
    fn acknowledge(&mut self, now: u64) {
        let (Some(input), Some(visible)) = (self.last_input, self.visible_since) else { return };
        let mut seen = Vec::new();
        for s in self.sources.values() {
            for l in s.lamps.iter().filter(|l| l.kind == Kind::Signal(SignalName::Attention) && l.until > now) {
                let shown = l.since.max(visible).max(l.burst.unwrap_or(0));
                let grace = ACK_GRACE_MS.max(l.look.intro_ms()) + l.burst.map_or(0, |_| BURST_MS);
                if !l.while_away && input > shown.saturating_add(grace) {
                    seen.push((l.source.clone(), l.until));
                }
            }
        }
        for (source, until) in seen {
            self.remove(&source, |l| l.kind == Kind::Signal(SignalName::Attention));
            if let Some(s) = self.sessions.get_mut(&source) {
                if !s.pending.is_empty() || s.open {
                    s.acked_until = Some(until);
                }
            }
            self.dirty = true;
        }
    }

    fn see_away(&mut self, idle_ms: Option<u64>, now: u64) {
        let away = idle_ms.is_some_and(|ms| ms >= u64::from(self.settings.away.after_s) * 1000);
        if away == self.away {
            return;
        }
        self.away = away;
        if away {
            return;
        }
        let flash = self.settings.away.on;
        for l in self.sources.values_mut().flat_map(|s| s.lamps.iter_mut()) {
            if std::mem::take(&mut l.while_away) && flash {
                l.burst = Some(now);
            }
        }
        let missed = std::mem::take(&mut self.missed);
        if flash {
            for m in missed {
                let look = Look::plain(m.look.color, Motion::Solid);
                let mut back = self.lamp(&m.source, &m.id, Kind::Back, look, m.keys, now, BURST_MS);
                back.burst = Some(now);
                back.while_away = false;
                back.leds = m.leds;
                back.leds_for = m.leds_for;
                self.place(back);
            }
        }
        self.dirty = true;
    }

    /// Nobody has been at the computer for `away.after`.
    #[cfg(test)]
    pub fn away(&self) -> bool {
        self.away
    }

    // ------------------------------------------------------------- drawing

    /// Work the lights' LEDs out again for `index`'s model, where they were
    /// worked out for another one (or none). Nothing to do otherwise.
    pub fn refresh_keys(&mut self, index: &KeyIndex) {
        let lamps = self.sources.values_mut().flat_map(|s| s.lamps.iter_mut().chain(s.queued.iter_mut()));
        for lamp in lamps.chain(self.missed.iter_mut()) {
            if lamp.leds_for.as_deref() != Some(index.model_id.as_str()) {
                if lamp.paint.is_some() {
                    // A picture made for another keyboard's LEDs.
                    lamp.leds.clear();
                    lamp.paint = Some(Vec::new());
                } else {
                    lamp.leds = index.resolve_lenient(&lamp.keys);
                }
                lamp.leds_for = Some(index.model_id.clone());
            }
        }
        for s in self.sessions.values_mut() {
            if let Some(n) = s.number.filter(|_| s.digit_for.as_deref() != Some(index.model_id.as_str())) {
                s.digit = KeySpec::parse(&n.to_string()).map(|k| index.resolve_lenient(&k)).unwrap_or_default();
                s.digit_for = Some(index.model_id.clone());
            }
        }
    }

    /// The brightness of `lamp` on the key at `place` at `now`.
    fn level(&self, lamp: &Lamp, now: u64, place: Place) -> f32 {
        if let Some(level) = lamp.burst.filter(|&b| now >= b).and_then(|b| burst_level(now - b)) {
            return level;
        }
        let t = now.saturating_sub(lamp.since);
        let d = &self.settings.unanswered;
        if d.on && lamp.kind == Kind::Signal(SignalName::Attention) {
            let every = u64::from(d.every_s).max(1) * 1000;
            let k = t / every;
            if k >= 1 && k * every <= u64::from(d.up_to_s) * 1000 {
                if let Some(level) = burst_level(t - k * every) {
                    return level;
                }
            }
        }
        lamp.look.level(t, place)
    }

    /// The lights in the order they claim keys: highest rank, then newest.
    fn ranked(&self, now: u64) -> Vec<&Lamp> {
        let mut lamps: Vec<&Lamp> = self.lamps().filter(|l| l.until > now).collect();
        lamps.sort_by(|a, b| b.kind.rank().cmp(&a.kind.rank()).then(b.order.cmp(&a.order)));
        lamps
    }

    /// What the agents' layer shows at `now` (UI colours, before the
    /// brightness level): each key the lights name, in the colour of the
    /// one that ranks highest there. Keys no light names are left out, so
    /// the layers under it show there. `night` (0–1) dims every light but
    /// the page's try; at 0 they are left out.
    pub fn frame(&self, now: u64, night: f32) -> Vec<(u8, Rgb)> {
        let mut taken = HashSet::new();
        let mut out = Vec::new();
        let ranked = self.ranked(now);
        // The page's try first, then the waiting sessions' numbers (B), then
        // the rest by rank.
        let tries = ranked.iter().take_while(|l| l.kind == Kind::Try).count();
        for lamp in &ranked[..tries] {
            for (i, &(led, place)) in lamp.leds.iter().enumerate() {
                if taken.insert(led) {
                    out.push((led, scale(lamp.color_at(i), self.level(lamp, now, place))));
                }
            }
        }
        if night > 0.0 {
            for (led, _) in self.session_digits(now) {
                if taken.insert(led) {
                    out.push((led, scale(self.settings.sessions.color, night)));
                }
            }
        }
        for lamp in &ranked[tries..] {
            let dim = if lamp.kind == Kind::Try { 1.0 } else { night };
            if dim <= 0.0 {
                continue;
            }
            for (i, &(led, place)) in lamp.leds.iter().enumerate() {
                if taken.insert(led) {
                    out.push((led, scale(lamp.color_at(i), self.level(lamp, now, place) * dim)));
                }
            }
        }
        out
    }

    /// The lights that show on at least one key at `now`: (source, id).
    pub fn shown(&self, now: u64) -> HashSet<(String, String)> {
        let mut taken = HashSet::new();
        let mut out = HashSet::new();
        for lamp in self.ranked(now) {
            let mut any = false;
            for (led, _) in &lamp.leds {
                any |= taken.insert(*led);
            }
            if any {
                out.insert((lamp.source.clone(), lamp.id.clone()));
            }
        }
        out
    }

    /// Every light for the page and `status`, highest rank first. Without
    /// a keyboard on the cable none shows.
    pub fn infos(&self, now: u64, keyboard: bool) -> Vec<AgentSignalInfo> {
        let shown = if keyboard { self.shown(now) } else { HashSet::new() };
        self.ranked(now)
            .into_iter()
            .map(|l| AgentSignalInfo {
                source: if l.source == TRY_SOURCE { String::new() } else { l.source.clone() },
                id: l.id.clone(),
                kind: l.kind.as_str().to_string(),
                color: l.look.color,
                keys: l.keys.to_string(),
                shown: shown.contains(&(l.source.clone(), l.id.clone())),
                expires_in_ms: l.until.saturating_sub(now) as f64,
                percent: l.percent,
                label: l.label.clone(),
            })
            .collect()
    }
}

impl Source {
    /// Count a request now if the source is within its rate.
    fn take(&mut self, now: u64) -> bool {
        while self.requests.front().is_some_and(|&t| now.saturating_sub(t) >= 1000) {
            self.requests.pop_front();
        }
        if self.requests.len() < RATE_PER_SECOND {
            self.requests.push_back(now);
            true
        } else {
            false
        }
    }
}

/// Whether `minute` (after midnight) falls in the night from `from` to
/// `to`, which may run over midnight. The same two ends mean no night.
pub fn in_night(from: u16, to: u16, minute: u16) -> bool {
    match from.cmp(&to) {
        std::cmp::Ordering::Less => (from..to).contains(&minute),
        std::cmp::Ordering::Greater => minute >= from || minute < to,
        std::cmp::Ordering::Equal => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::effects::flash_period_ms;
    use crate::agent::keys::Key;
    use crate::dto::AgentEffect;
    use kb_agent::protocol::LogArgs;

    fn index() -> KeyIndex {
        KeyIndex::for_model("ajazz-ak980-pro").unwrap()
    }

    fn on() -> Engine {
        Engine::new(AgentSettings { enabled: true, ..AgentSettings::default() })
    }

    fn src(name: &str) -> SourceName {
        SourceName::parse(name).unwrap()
    }

    fn at(ms: u64) -> Now {
        Now { mono: ms, wall: 1_800_000_000_000.0 + ms as f64 }
    }

    fn usb() -> AgentError {
        AgentError::new(ErrorCode::NeedsUsb, "USB only.")
    }

    fn signal(name: &str) -> Command {
        Command::Signal(SignalArgs { name: name.into(), ..Default::default() })
    }

    fn light(id: &str, color: &str) -> Command {
        Command::Light(LightArgs { id: Some(id.into()), color: color.into(), ..Default::default() })
    }

    fn set(pairs: &[(&str, Value)]) -> Command {
        Command::SettingsSet(SettingsSetArgs {
            values: pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        })
    }

    fn run(e: &mut Engine, source: &str, command: Command, ms: u64) -> Result<Value, AgentError> {
        let index = index();
        e.command(&src(source), command, Ok(&index), at(ms)).0
    }

    fn led(id: &str) -> u8 {
        index().keys().iter().find(|k| k.id == id).unwrap().led
    }

    /// The colour key `id` shows at `ms`, if any light names it.
    fn key_at(e: &Engine, id: &str, ms: u64) -> Option<Rgb> {
        e.frame(ms, 1.0).into_iter().find(|k| k.0 == led(id)).map(|k| k.1)
    }

    fn ids(e: &Engine, ms: u64) -> Vec<String> {
        e.infos(ms, true).into_iter().map(|i| i.id).collect()
    }

    #[test]
    fn access_off_refuses_every_change_and_says_how_to_allow_it() {
        let mut e = Engine::new(AgentSettings::default());
        let changes = [
            signal("done"),
            light("x", "red"),
            Command::Clear(ClearArgs::default()),
            Command::Progress(ProgressArgs { percent: Some(5.0), ..Default::default() }),
            set(&[("night.on", json!(true))]),
        ];
        for (i, command) in changes.into_iter().enumerate() {
            let name = command.name();
            let err = run(&mut e, "cli", command, i as u64 * 1000).unwrap_err();
            assert_eq!((err.code, err.exit_code()), (ErrorCode::AgentAccessOff, 77), "{name}");
            assert!(err.hint.is_some());
        }
        assert!(!e.busy());
        assert_eq!(e.log_entries().len(), 5, "a second apart, each refusal is logged");
        assert!(e.log_entries().iter().all(|l| l.error.as_deref() == Some("agent_access_off")));
        // Reading still works.
        assert!(run(&mut e, "cli", Command::Log(LogArgs::default()), 0).is_ok());
        assert!(run(&mut e, "cli", Command::SettingsGet(Default::default()), 0).is_ok());
    }

    #[test]
    fn refusals_go_in_the_log_once_a_second_per_source() {
        // A hook firing on every step while access is off, or a script that
        // keeps sending a bad colour, must not push the rest out of the log.
        let mut e = Engine::new(AgentSettings::default());
        for i in 0..500u64 {
            let _ = run(&mut e, "claude-code:1", signal("done"), i * 10);
        }
        assert_eq!(e.log_entries().len(), 5, "one a second for five seconds");
        let mut e = on();
        for i in 0..100u64 {
            let _ = run(&mut e, "a", light("x", "teal"), i * 5);
            let _ = run(&mut e, "b", light("y", "teal"), i * 5);
        }
        let log = e.log_entries();
        assert_eq!(log.len(), 2, "{log:?}");
        assert!(log.iter().all(|l| l.error.as_deref() == Some("bad_color")));
    }

    #[test]
    fn a_signal_shows_its_look_until_its_time_is_up() {
        let mut e = on();
        let answer = run(&mut e, "claude-code:3f9a", signal("done"), 1_000).unwrap();
        assert_eq!(answer["id"], "claude-code:3f9a:done");
        assert_eq!((answer["shown"].clone(), answer["expires_in_ms"].clone()), (json!(true), json!(120_000)));
        // Done: a wave first, then steady green on every key.
        assert_eq!(key_at(&e, "esc", 1_000 + 2_000), Some([0, 255, 0]));
        assert_eq!(e.frame(3_000, 1.0).len(), index().keys().len());
        // Two minutes later it is gone, and the keys show what is under it.
        e.prune(1_000 + 120_000);
        assert!(e.frame(121_000, 1.0).is_empty());
        assert!(!e.busy());
    }

    #[test]
    fn the_same_id_replaces_and_sources_stay_apart() {
        let mut e = on();
        run(&mut e, "a", signal("done"), 0).unwrap();
        let blue = SignalArgs { name: "done".into(), color: Some("blue".into()), ..Default::default() };
        let again = run(&mut e, "a", Command::Signal(blue), 10).unwrap();
        assert_eq!(again["replaced"], true);
        assert_eq!(ids(&e, 10), ["a:done"]);
        // Another source's done is its own; clearing one leaves the other.
        run(&mut e, "b", signal("done"), 20).unwrap();
        assert_eq!(ids(&e, 20).len(), 2);
        let cleared = run(&mut e, "a", Command::Clear(ClearArgs::default()), 30).unwrap();
        assert_eq!(cleared["cleared"], 1);
        assert_eq!(ids(&e, 30), ["b:done"]);
        // `a` cannot clear `b`'s light, even by its id.
        let none = run(&mut e, "a", Command::Clear(ClearArgs { id: Some("b:done".into()) }), 40).unwrap();
        assert_eq!(none["cleared"], 0);
        assert_eq!(ids(&e, 40), ["b:done"]);
        // A chosen id.
        let build = SignalArgs { name: "info".into(), id: Some("build".into()), ..Default::default() };
        run(&mut e, "b", Command::Signal(build), 50).unwrap();
        assert!(ids(&e, 50).contains(&"build".to_string()));
        assert_eq!(run(&mut e, "b", Command::Clear(ClearArgs { id: Some("build".into()) }), 60).unwrap()["cleared"], 1);
    }

    #[test]
    fn attention_covers_error_covers_the_newest_and_disjoint_keys_share() {
        let mut e = on();
        let with_keys = |name: &str, keys: &str| {
            Command::Signal(SignalArgs {
                name: name.into(),
                keys: Some(keys.into()),
                effect: Some("solid".into()),
                ..Default::default()
            })
        };
        run(&mut e, "a", with_keys("attention", "f1-f4"), 0).unwrap();
        run(&mut e, "b", with_keys("error", "f1-f12"), 10).unwrap();
        run(&mut e, "c", with_keys("done", "all"), 20).unwrap();
        let orange = e.settings().signals.attention.color;
        let red = e.settings().signals.error.color;
        let green = e.settings().signals.done.color;
        // The question on F1–F4 over the error, the error on the rest of the
        // F row over the newer done, done elsewhere.
        assert_eq!(key_at(&e, "f2", 100), Some(orange));
        assert_eq!(key_at(&e, "f8", 100), Some(red));
        assert_eq!(key_at(&e, "esc", 100), Some(green));
        let infos = e.infos(100, true);
        assert_eq!(infos.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(), ["attention", "error", "done"]);
        assert!(infos.iter().all(|i| i.shown));
        assert!(e.infos(100, false).iter().all(|i| !i.shown), "without a keyboard nothing shows");
        // Among equals the newest wins.
        run(&mut e, "d", with_keys("info", "esc"), 30).unwrap();
        assert_eq!(key_at(&e, "esc", 100), Some(e.settings().signals.info.color));
        // A done covering only F1 is not shown at all under the question.
        run(&mut e, "c", with_keys("done", "f1"), 40).unwrap();
        let done = e.infos(100, true).into_iter().find(|i| i.id == "c:done").unwrap();
        assert!(!done.shown);
    }

    #[test]
    fn each_source_keeps_its_eight_newest() {
        let mut e = on();
        for i in 0..MAX_PER_SOURCE {
            let one = LightArgs {
                id: Some(format!("l{i}")),
                color: "red".into(),
                keys: Some("esc".into()),
                ..Default::default()
            };
            let answer = run(&mut e, "a", Command::Light(one), i as u64 * 60).unwrap();
            assert_eq!(answer["evicted"], json!([]));
        }
        let answer = run(&mut e, "a", light("l8", "red"), 1_000).unwrap();
        assert_eq!(answer["evicted"], json!(["l0"]));
        let mine: Vec<String> = ids(&e, 1_000);
        assert_eq!(mine.len(), MAX_PER_SOURCE);
        assert!(!mine.contains(&"l0".to_string()));
        // Another source has its own eight.
        run(&mut e, "b", Command::Light(LightArgs { color: "red".into(), ..Default::default() }), 1_000).unwrap();
        assert_eq!(ids(&e, 1_000).len(), MAX_PER_SOURCE + 1);
    }

    #[test]
    fn past_twenty_a_second_replacements_wait_and_the_rest_is_refused() {
        let mut e = on();
        for i in 0..RATE_PER_SECOND {
            assert_eq!(run(&mut e, "a", light("bar", "red"), i as u64 * 10).unwrap()["queued"], false);
        }
        // The 21st in the same second: the same id waits in the queue…
        let queued = run(&mut e, "a", light("bar", "blue"), 300).unwrap();
        assert_eq!((queued["coalesced"].clone(), queued["queued"].clone()), (json!(true), json!(true)));
        assert_eq!(key_at(&e, "esc", 300), Some([255, 0, 0]), "not up yet");
        // …a new one is refused.
        let err = run(&mut e, "a", light("other", "red"), 300).unwrap_err();
        assert_eq!((err.code, err.exit_code()), (ErrorCode::RateLimited, 75));
        assert_eq!(err.retry_after_ms, Some(700));
        assert_eq!(run(&mut e, "a", set(&[("night.on", json!(true))]), 300).unwrap_err().code, ErrorCode::RateLimited);
        // Clearing is never held back; other sources are not either.
        assert!(run(&mut e, "a", Command::Clear(ClearArgs { id: Some("nothing".into()) }), 300).is_ok());
        assert!(run(&mut e, "b", light("other", "red"), 300).is_ok());
        // The worker puts it up once the source is within the rate again.
        assert_eq!(e.apply_queued(500), 0);
        assert_eq!(e.apply_queued(1_000), 1);
        assert_eq!(key_at(&e, "esc", 1_000), Some([0, 0, 255]));
        assert_eq!(e.apply_queued(1_000), 0, "once");
        // A moment later the source may go on.
        assert!(run(&mut e, "a", light("other", "red"), 1_010).is_ok());
        // A source that keeps going is logged once a second, not each time.
        let before = e.log_entries().len();
        for i in 0..30 {
            let _ = run(&mut e, "a", light(&format!("x{i}"), "red"), 1_011);
        }
        let refused = e.log_entries().len() - before;
        assert!(refused <= 1, "{refused} entries");
    }

    #[test]
    fn a_flood_of_replacements_goes_up_twenty_a_second_newest_first() {
        let mut e = on();
        // 400 updates of one light in two seconds, each a new colour.
        let colour = |i: u64| format!("#00{:02x}{:02x}", i / 256, i % 256);
        let mut put_up = 0;
        for i in 0..400u64 {
            let answer = run(&mut e, "a", light("bar", &colour(i)), i * 5).unwrap();
            put_up += usize::from(answer["queued"] == false);
            // The worker's turn every 50 ms.
            if i % 10 == 9 {
                put_up += e.apply_queued(i * 5);
            }
        }
        assert!(put_up <= 2 * RATE_PER_SECOND + 1, "{put_up} put up in two seconds");
        // Only the newest waits; it goes up next.
        put_up = e.apply_queued(3_000);
        assert_eq!(put_up, 1);
        assert_eq!(key_at(&e, "esc", 3_000), Some([0, 1, 143]));
        // Clearing drops what waits too.
        for i in 0..30u64 {
            run(&mut e, "c", light("x", "red"), 4_000 + i).unwrap();
        }
        run(&mut e, "c", Command::Clear(ClearArgs::default()), 4_100).unwrap();
        assert_eq!(e.apply_queued(6_000), 0);
        assert!(!ids(&e, 6_000).contains(&"x".to_string()));
    }

    #[test]
    fn busy_lights_nothing_and_takes_the_sources_lights_away() {
        let mut e = on();
        run(&mut e, "a", signal("error"), 0).unwrap();
        run(&mut e, "a", Command::Progress(ProgressArgs { percent: Some(40.0), ..Default::default() }), 0).unwrap();
        run(&mut e, "b", signal("done"), 0).unwrap();
        let busy = run(&mut e, "a", signal("busy"), 10).unwrap();
        assert_eq!((busy["lit"].clone(), busy["cleared"].clone()), (json!(false), json!(2)));
        assert_eq!(ids(&e, 10), ["b:done"]);
        // Without the cable too: a question must not come back when the
        // keyboard is plugged in again.
        run(&mut e, "a", signal("attention"), 20).unwrap();
        let unplugged = e.command(&src("a"), signal("busy"), Err(usb()), at(30)).0.unwrap();
        assert_eq!(unplugged["cleared"], 1);
        assert_eq!(ids(&e, 30), ["b:done"]);
        // A look the user chose for it lights; without the cable it still
        // clears first, then says so.
        let mut s = *e.settings();
        s.signals.busy.effect = AgentEffect::Pulse;
        e.set_settings(s);
        let lit = run(&mut e, "a", signal("busy"), 40).unwrap();
        assert_eq!(lit["lit"], true);
        let err = e.command(&src("b"), signal("busy"), Err(usb()), at(50)).0.unwrap_err();
        assert_eq!(err.code, ErrorCode::NeedsUsb);
        assert_eq!(ids(&e, 50), ["a:busy"], "b's done went");
    }

    #[test]
    fn lights_need_the_cable_and_known_keys() {
        let mut e = on();
        for command in [
            signal("done"),
            light("x", "red"),
            Command::Progress(ProgressArgs { percent: Some(10.0), ..Default::default() }),
        ] {
            let name = command.name();
            let err = e.command(&src("a"), command, Err(usb()), at(0)).0.unwrap_err();
            assert_eq!((err.code, err.exit_code()), (ErrorCode::NeedsUsb, 69), "{name}");
        }
        let unknown = Command::Light(LightArgs { color: "red".into(), keys: Some("f13".into()), ..Default::default() });
        assert_eq!(run(&mut e, "a", unknown, 0).unwrap_err().code, ErrorCode::UnknownKey);
        let bad = Command::Light(LightArgs { color: "teal".into(), ..Default::default() });
        assert_eq!(run(&mut e, "a", bad, 0).unwrap_err().code, ErrorCode::BadColor);
        assert!(!e.busy());
    }

    #[test]
    fn a_light_with_cycles_ends_when_they_ran() {
        let mut e = on();
        let wave = LightArgs {
            color: "green".into(),
            keys: Some("f-row".into()),
            effect: Some("wave".into()),
            times: Some(2),
            ..Default::default()
        };
        let answer = run(&mut e, "a", Command::Light(wave), 0).unwrap();
        assert_eq!(answer["expires_in_ms"], 2 * 1800);
        let plain = run(&mut e, "a", light("p", "red"), 0).unwrap();
        assert_eq!(plain["expires_in_ms"], LIGHT_DEFAULT_MS);
        // An agent's own effect replaces the user's look.
        let flash =
            SignalArgs { name: "done".into(), effect: Some("blink".into()), times: Some(3), ..Default::default() };
        let answer = run(&mut e, "b", Command::Signal(flash), 0).unwrap();
        assert_eq!(answer["expires_in_ms"], 3 * 900);
    }

    #[test]
    fn a_progress_bar_fills_the_users_row() {
        let mut e = on();
        let p = |pct: f64| {
            Command::Progress(ProgressArgs { percent: Some(pct), label: Some("Tests".into()), ..Default::default() })
        };
        let answer = run(&mut e, "a", p(50.0), 0).unwrap();
        assert_eq!(answer["id"], "a:progress");
        let lit = |e: &Engine| {
            (1..=12).filter(|n| key_at(e, &format!("f{n}"), 10) == Some(e.settings().progress.color)).count()
        };
        assert_eq!(lit(&e), 6);
        assert_eq!(key_at(&e, "esc", 10), None, "only its row");
        run(&mut e, "a", p(100.0), 5).unwrap();
        assert_eq!(lit(&e), 12);
        let info = e.infos(10, true).remove(0);
        assert_eq!((info.percent, info.label.as_deref(), info.kind.as_str()), (Some(100.0), Some("Tests"), "progress"));
        // The number row when the user chose it.
        let mut s = *e.settings();
        s.progress.keys = AgentProgressKeys::Numbers;
        e.set_settings(s);
        run(&mut e, "b", p(10.0), 5).unwrap();
        assert_eq!(key_at(&e, "1", 10), Some(e.settings().progress.color));
    }

    #[test]
    fn leds_are_worked_out_once_and_again_for_another_keyboard() {
        let mut e = on();
        run(&mut e, "a", Command::Light(LightArgs { color: "red".into(), ..Default::default() }), 0).unwrap();
        assert_eq!(e.frame(10, 1.0).len(), index().keys().len());
        // The same model: nothing changes.
        e.refresh_keys(&index());
        assert_eq!(e.frame(10, 1.0).len(), index().keys().len());
        // A keyboard with one key: the light covers that one.
        let key = Key { id: "esc".into(), label: "Esc".into(), led: 9, x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
        e.refresh_keys(&KeyIndex::new("tiny", "Tiny", vec![key]));
        assert_eq!(e.frame(10, 1.0), [(9, [255, 0, 0])]);
    }

    #[test]
    fn night_quiet_dims_everything_but_the_pages_try() {
        let mut e = on();
        run(&mut e, "a", Command::Light(LightArgs { color: "#c8c8c8".into(), ..Default::default() }), 0).unwrap();
        let look = AgentLook::new([200, 100, 0], AgentEffect::Solid, 3, 60);
        e.try_look(&look, KeySpec::parse("esc").unwrap(), &index(), 0).unwrap();
        assert_eq!(key_at(&e, "esc", 10), Some([200, 100, 0]));
        let dim = e.frame(10, 0.5);
        assert_eq!(dim.iter().find(|k| k.0 == led("esc")).unwrap().1, [200, 100, 0], "the try is not dimmed");
        assert_eq!(dim.iter().find(|k| k.0 == led("f1")).unwrap().1, [100, 100, 100]);
        assert_eq!(e.frame(10, 0.0), [(led("esc"), [200, 100, 0])], "night off: only the try");
    }

    #[test]
    fn settings_set_changes_looks_but_never_access_permissions_or_limits() {
        let mut e = on();
        let answer =
            run(&mut e, "a", set(&[("signals.done.color", json!("blue")), ("night.from", json!("22:30"))]), 0).unwrap();
        assert_eq!(answer["changed"].as_array().unwrap().len(), 2);
        assert_eq!(e.settings().signals.done.color, [0, 0, 255]);
        assert_eq!(e.settings().night.from_min, 22 * 60 + 30);
        for key in config::USER_ONLY {
            let err = run(&mut e, "a", set(&[(key, json!(false))]), 100).unwrap_err();
            assert_eq!((err.code, err.exit_code()), (ErrorCode::PermissionOff, 77), "{key}");
        }
        // All or nothing: one protected key keeps the others from changing.
        let mixed = set(&[("signals.error.color", json!("white")), ("enabled", json!(true))]);
        assert_eq!(run(&mut e, "a", mixed, 200).unwrap_err().code, ErrorCode::PermissionOff);
        assert_eq!(e.settings().signals.error.color, [255, 0, 0]);
        assert_eq!(run(&mut e, "a", set(&[("night.glow", json!(1))]), 200).unwrap_err().code, ErrorCode::BadValue);
        assert_eq!(
            run(&mut e, "a", set(&[("night.from", json!("25:00"))]), 200).unwrap_err().code,
            ErrorCode::BadValue
        );
        // The user can say no to all of it.
        let mut s = *e.settings();
        s.agents_may_edit = false;
        e.set_settings(s);
        let err = run(&mut e, "a", set(&[("night.on", json!(true))]), 300).unwrap_err();
        assert_eq!(err.code, ErrorCode::PermissionOff);
    }

    #[test]
    fn undo_puts_back_what_an_agent_changed() {
        let mut e = on();
        let change = set(&[("signals.done.color", json!("blue")), ("signals.done.duration", json!("5m"))]);
        let (result, effects) = e.command(&src("a"), change, Ok(&index()), at(0));
        assert!(result.is_ok() && effects.settings);
        let entry = e.log_entries().remove(0);
        assert_eq!(entry.command, "settings_set");
        assert_eq!(
            entry.changes,
            vec![
                AgentSettingChange {
                    key: "signals.done.color".into(),
                    before: "#00ff00".into(),
                    after: "#0000ff".into()
                },
                AgentSettingChange { key: "signals.done.duration".into(), before: "2m".into(), after: "5m".into() },
            ]
        );
        assert_eq!(e.undo(entry.seq).unwrap(), Vec::<String>::new());
        assert_eq!(e.settings().signals.done, AgentSettings::default().signals.done);
        assert!(e.log_entries()[0].undone);
        assert!(e.undo(entry.seq).is_err(), "once");
        assert!(e.undo(9999).is_err());
        // An entry that changed nothing has nothing to undo.
        run(&mut e, "a", signal("done"), 10).unwrap();
        assert!(e.undo(e.log_entries()[0].seq).is_err());
    }

    #[test]
    fn undo_leaves_what_changed_again_since() {
        let mut e = on();
        run(&mut e, "a", set(&[("signals.done.color", json!("blue")), ("night.on", json!(true))]), 0).unwrap();
        let seq = e.log_entries()[0].seq;
        // The user then picks purple on the page.
        let mut s = *e.settings();
        s.signals.done.color = [128, 0, 255];
        e.set_settings(s);
        assert_eq!(e.undo(seq).unwrap(), ["signals.done.color"]);
        assert_eq!(e.settings().signals.done.color, [128, 0, 255], "purple stays");
        assert!(!e.settings().night.on, "night quiet went back");
        let entry = e.log_entries().into_iter().find(|l| l.seq == seq).unwrap();
        assert!(entry.undone);
        assert_eq!(entry.kept, ["signals.done.color"]);
        // When everything changed since, nothing is undone.
        run(&mut e, "a", set(&[("helper.on", json!(true))]), 100).unwrap();
        let seq = e.log_entries()[0].seq;
        let mut s = *e.settings();
        s.helper.on = false;
        e.set_settings(s);
        assert!(e.undo(seq).is_err());
        assert!(!e.log_entries()[0].undone);
    }

    #[test]
    fn the_log_keeps_the_last_two_hundred_and_no_text() {
        let mut e = on();
        for i in 0..(LOG_SIZE + 20) {
            run(&mut e, "a", light(&format!("l{}", i % 4), "red"), i as u64 * 100).unwrap();
        }
        let log = e.log_entries();
        assert_eq!(log.len(), LOG_SIZE);
        assert!(log.windows(2).all(|w| w[0].seq > w[1].seq), "newest first");
        let view = run(&mut e, "a", Command::Log(LogArgs { limit: Some(3) }), 0).unwrap();
        let entries = view["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0]["command"], "light");
        assert_eq!(entries[0]["result"], "ok");
        let mut keys: Vec<&str> = entries[0].as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["at_ms", "command", "id", "result", "seq", "source"]);
        // Updates of one light, one after the other, keep one entry;
        // replacements queued past the rate add none.
        let of = |e: &Engine, source: &str| {
            e.log_entries().into_iter().filter(|l| l.source == source).map(|l| l.at_ms).collect::<Vec<_>>()
        };
        for i in 0..30u64 {
            let p = ProgressArgs { percent: Some(i as f64), id: Some("bar".into()), ..Default::default() };
            run(&mut e, "p", Command::Progress(p), 100_000 + i * 100).unwrap();
        }
        assert_eq!(of(&e, "p"), [at(100_000 + 29 * 100).wall]);
        for i in 0..40u64 {
            run(&mut e, "m", light("m", "red"), 200_000 + i).unwrap();
        }
        assert_eq!(of(&e, "m"), [at(200_000 + RATE_PER_SECOND as u64 - 1).wall]);
        // Clearing nothing is not news.
        run(&mut e, "q", Command::Clear(ClearArgs::default()), 300_000).unwrap();
        assert!(of(&e, "q").is_empty());
        // A progress label stays with its light, not in the log.
        let secret = ProgressArgs { percent: Some(1.0), label: Some("secret plan".into()), ..Default::default() };
        run(&mut e, "a", Command::Progress(secret), 0).unwrap();
        let text = serde_json::to_string(&e.log_entries()).unwrap();
        assert!(!text.contains("secret"));
    }

    #[test]
    fn entries_that_can_be_undone_stay_longest() {
        let mut e = on();
        run(&mut e, "a", set(&[("signals.done.color", json!("blue"))]), 0).unwrap();
        let undoable = e.log_entries()[0].seq;
        for i in 0..(2 * LOG_SIZE) {
            run(&mut e, "b", light(&format!("l{}", i % 4), "red"), 1_000 + i as u64 * 100).unwrap();
        }
        let log = e.log_entries();
        assert_eq!(log.len(), LOG_SIZE);
        assert!(log.iter().any(|l| l.seq == undoable), "the settings change is still there to undo");
        e.undo(undoable).unwrap();
        // Undone, it may go like any other.
        for i in 0..LOG_SIZE {
            run(&mut e, "c", light(&format!("k{}", i % 4), "red"), 100_000 + i as u64 * 100).unwrap();
        }
        assert!(!e.log_entries().iter().any(|l| l.seq == undoable));
    }

    #[test]
    fn coming_back_flashes_what_came_meanwhile() {
        let mut e = on();
        let away_ms = u64::from(e.settings().away.after_s) * 1000;
        e.observe_idle(Some(away_ms), 0);
        assert!(e.away());
        // While away: a question that still waits and a done whose time runs out.
        run(&mut e, "a", signal("attention"), 1_000).unwrap();
        run(&mut e, "b", signal("done"), 1_000).unwrap();
        e.prune(1_000 + 120_000);
        assert_eq!(ids(&e, 121_000), ["a:attention"]);
        assert!(e.lit() && e.waiting());
        // Back at the computer: both flash, at the safe rate.
        let back = 200_000;
        e.observe_idle(Some(10), back);
        assert!(!e.away() && !e.waiting());
        let green = e.settings().signals.done.color;
        assert_eq!(key_at(&e, "esc", back), Some(e.settings().signals.attention.color), "the question ranks first");
        run(&mut e, "a", Command::Clear(ClearArgs::default()), back).unwrap();
        assert_eq!(key_at(&e, "esc", back), Some(green));
        assert_eq!(key_at(&e, "esc", back + flash_period_ms() / 2 + 10), Some([0, 0, 0]), "the flash's dark half");
        e.prune(back + BURST_MS);
        assert!(e.frame(back + BURST_MS, 1.0).is_empty(), "once");
        // A light that came before the user left does not flash on return.
        run(&mut e, "c", signal("error"), back + 2_000).unwrap();
        e.observe_idle(Some(away_ms), back + 3_000);
        e.observe_idle(Some(0), back + 400_000);
        let red = e.settings().signals.error.color;
        assert_eq!(key_at(&e, "esc", back + 400_000 + flash_period_ms() / 2 + 10), Some(red));
    }

    #[test]
    fn what_came_while_away_waits_half_a_day_at_most() {
        let mut e = on();
        e.observe_idle(Some(60 * 60_000), 0);
        run(&mut e, "a", signal("done"), 0).unwrap();
        e.prune(200_000);
        assert!(!e.lit() && e.waiting(), "only waiting: the worker need not draw");
        e.prune(120_000 + MISSED_KEEP_MS);
        assert!(e.waiting());
        e.prune(120_000 + MISSED_KEEP_MS + 1);
        assert!(!e.waiting());
        e.observe_idle(Some(0), 120_000 + MISSED_KEEP_MS + 2);
        assert!(!e.busy(), "back the next day: nothing flashes");
    }

    #[test]
    fn coming_back_does_nothing_when_the_user_turned_it_off_or_the_agent_cleared() {
        let mut e = on();
        let mut s = *e.settings();
        s.away.on = false;
        e.set_settings(s);
        e.observe_idle(Some(10 * 60_000), 0);
        run(&mut e, "a", signal("done"), 0).unwrap();
        e.prune(200_000);
        e.observe_idle(Some(0), 300_000);
        assert!(!e.busy());

        let mut e = on();
        e.observe_idle(Some(10 * 60_000), 0);
        run(&mut e, "a", signal("done"), 0).unwrap();
        e.prune(200_000);
        run(&mut e, "a", Command::Clear(ClearArgs::default()), 250_000).unwrap();
        e.observe_idle(Some(0), 300_000);
        assert!(e.frame(300_000, 1.0).is_empty());
    }

    #[test]
    fn an_unanswered_question_flashes_again_every_five_minutes_for_half_an_hour() {
        let mut e = on();
        run(&mut e, "a", signal("attention"), 0).unwrap();
        let orange = e.settings().signals.attention.color;
        let dark_half = flash_period_ms() / 2 + 10;
        // Between flashes the slow pulse: never fully dark.
        let pulse_low = 60_000 + 1_200 + 2_400 / 2;
        assert_ne!(key_at(&e, "esc", pulse_low), Some([0, 0, 0]));
        for k in 1..=5u64 {
            let t = k * 5 * 60_000;
            assert_eq!(key_at(&e, "esc", t), Some(orange), "{k}");
            assert_eq!(key_at(&e, "esc", t + dark_half), Some([0, 0, 0]), "{k}");
        }
        // The question ends at 30 minutes, the sixth flash with it; a longer
        // one does not flash after 30 minutes.
        assert_eq!(key_at(&e, "esc", 30 * 60_000), None);
        let mut s = *e.settings();
        s.signals.attention.duration_s = 60 * 60;
        e.set_settings(s);
        run(&mut e, "b", signal("attention"), 0).unwrap();
        let t = 35 * 60_000 + dark_half;
        let b = key_at(&e, "esc", t).unwrap();
        assert_ne!(b, [0, 0, 0], "no flash at 35 minutes");
        // Turned off: no flashes.
        let mut s = *e.settings();
        s.unanswered.on = false;
        e.set_settings(s);
        assert_ne!(key_at(&e, "esc", 5 * 60_000 + dark_half), Some([0, 0, 0]));
    }

    #[test]
    fn a_try_shows_over_everything_for_a_few_seconds_even_with_access_off() {
        let mut e = Engine::new(AgentSettings::default());
        let look = AgentLook::new([1, 2, 3], AgentEffect::Solid, 3, 60);
        e.try_look(&look, KeySpec::all(), &index(), 0).unwrap();
        assert_eq!(key_at(&e, "esc", 10), Some([1, 2, 3]));
        assert!(e.trying(10));
        e.prune(TRY_MS);
        assert!(!e.trying(TRY_MS) && !e.busy());
        assert_eq!(
            e.try_look(&look, KeySpec::parse("f13").unwrap(), &index(), 0).unwrap_err().code,
            ErrorCode::UnknownKey
        );
        // Clearing every agent's light leaves the try.
        let mut e = on();
        run(&mut e, "a", signal("attention"), 0).unwrap();
        e.try_look(&look, KeySpec::parse("esc").unwrap(), &index(), 0).unwrap();
        assert_eq!(key_at(&e, "esc", 10), Some([1, 2, 3]));
        assert_eq!(e.clear_agents(), 1);
        assert!(e.trying(10));
    }

    #[test]
    fn turning_access_off_takes_the_lights_away() {
        let mut e = on();
        run(&mut e, "a", signal("attention"), 0).unwrap();
        let mut s = *e.settings();
        s.enabled = false;
        e.set_settings(s);
        assert!(!e.busy());
    }

    fn hook(event: &str, calls: &[&str]) -> Command {
        Command::Hook(HookArgs {
            event: event.into(),
            agent: Some("claude-code".into()),
            label: Some("klavye".into()),
            calls: calls.iter().map(|c| c.to_string()).collect(),
        })
    }

    const A: &str = "claude-code:aaaa0001";
    const B: &str = "claude-code:bbbb0002";

    fn asks(e: &Engine, source: &str, ms: u64) -> bool {
        e.infos(ms, true).iter().any(|i| i.source == source && i.kind == "attention")
    }

    #[test]
    fn a_question_stays_until_its_calls_are_answered() {
        let mut e = on();
        assert_eq!(run(&mut e, A, hook("busy", &[]), 0).unwrap()["session"], 1);
        let up = run(&mut e, A, hook("attention", &["c1"]), 100).unwrap();
        assert_eq!((up["lit"].clone(), up["id"].clone()), (json!(true), json!(format!("{A}:attention"))));
        // A second question while it is up: kept as it was, no new flashes.
        let kept = run(&mut e, A, hook("attention", &["c2"]), 5_000).unwrap();
        assert_eq!(kept["kept"], true);
        let since = e.lamps().find(|l| l.kind == Kind::Signal(SignalName::Attention)).unwrap().since;
        assert_eq!(since, 100);
        // Another tool call finishing (parallel calls) answers nothing.
        assert_eq!(run(&mut e, A, hook("answered", &["other"]), 6_000).unwrap()["cleared"], 0);
        assert!(asks(&e, A, 6_000));
        assert_eq!(run(&mut e, A, hook("answered", &["c1"]), 7_000).unwrap()["waiting"], 1);
        assert!(asks(&e, A, 7_000), "c2 still waits");
        let gone = run(&mut e, A, hook("answered", &["c2"]), 8_000).unwrap();
        assert_eq!((gone["cleared"].clone(), gone["waiting"].clone()), (json!(1), json!(0)));
        assert!(!asks(&e, A, 8_000));
        // A question with no call of its own goes only with an answer naming
        // no call: another call finishing (a helper agent's, say) is no answer.
        run(&mut e, A, hook("attention", &[]), 9_000).unwrap();
        assert_eq!(run(&mut e, A, hook("answered", &["whatever"]), 9_400).unwrap()["cleared"], 0);
        assert!(asks(&e, A, 9_400));
        assert_eq!(run(&mut e, A, hook("answered", &[]), 9_500).unwrap()["cleared"], 1);
        // A call is named by any of its keys (Codex: the call or its turn).
        run(&mut e, A, hook("attention", &["call-x", "turn-1"]), 9_600).unwrap();
        run(&mut e, A, hook("attention", &["call-y", "turn-1"]), 9_700).unwrap();
        assert_eq!(run(&mut e, A, hook("answered", &["call-x"]), 9_800).unwrap()["waiting"], 1);
        assert_eq!(run(&mut e, A, hook("answered", &["turn-1"]), 9_900).unwrap()["cleared"], 1);
        // An answer naming no call answers them all.
        run(&mut e, A, hook("attention", &["c3"]), 10_000).unwrap();
        run(&mut e, A, hook("attention", &["c4"]), 10_100).unwrap();
        assert_eq!(run(&mut e, A, hook("answered", &[]), 10_200).unwrap()["cleared"], 1);
        // At most 30 minutes, whatever happens.
        run(&mut e, A, hook("attention", &["c5"]), 20_000).unwrap();
        e.prune(20_000 + 30 * 60_000);
        assert!(!asks(&e, A, 20_000 + 30 * 60_000));
    }

    #[test]
    fn the_notification_backing_a_question_up_is_no_second_question() {
        let mut e = on();
        run(&mut e, A, hook("attention", &["c1"]), 0).unwrap();
        assert_eq!(run(&mut e, A, hook("attention", &[]), 3_000).unwrap()["kept"], true);
        assert_eq!(run(&mut e, A, hook("answered", &["c1"]), 4_000).unwrap()["cleared"], 1);
        assert!(!asks(&e, A, 4_000));
    }

    /// When the question's first flashes are over (the owner: "about a
    /// second").
    fn shown_after(e: &Engine, since: u64) -> u64 {
        let intro = e.lamps().find(|l| l.kind == Kind::Signal(SignalName::Attention)).unwrap().look.intro_ms();
        since + ACK_GRACE_MS.max(intro)
    }

    #[test]
    fn touching_the_keyboard_once_the_question_showed_puts_it_out() {
        let mut e = on();
        e.set_visible(true, 0);
        run(&mut e, A, hook("attention", &["c1"]), 1_000).unwrap();
        let shown = shown_after(&e, 1_000);
        // Input before the question, or while it opens: it stays.
        e.observe_idle(Some(shown - 500), shown);
        assert!(asks(&e, A, shown));
        e.observe_idle(Some(0), shown);
        assert!(asks(&e, A, shown), "typing on while it opens is no answer");
        // Input after: out, and no flash again five minutes later (D).
        e.observe_idle(Some(0), shown + 1);
        assert!(!asks(&e, A, shown + 1));
        assert_eq!(key_at(&e, "esc", 5 * 60_000), None);
        // The session still waits for its answer; asked again (the
        // notification behind the question), it stays out.
        assert!(e.sessions(shown + 1)[0].waiting);
        assert_eq!(run(&mut e, A, hook("attention", &[]), shown + 2_000).unwrap()["lit"], false);
        assert_eq!(run(&mut e, A, hook("attention", &["c1"]), shown + 2_100).unwrap()["lit"], false);
        assert!(!asks(&e, A, shown + 2_100));
        run(&mut e, A, hook("answered", &["c1"]), shown + 3_000).unwrap();
        assert!(!e.sessions(shown + 3_000)[0].waiting);
        // A new question lights again.
        run(&mut e, A, hook("attention", &["c2"]), shown + 4_000).unwrap();
        assert!(asks(&e, A, shown + 4_000));

        // Lights nobody can see (a game hides them, the night put them off,
        // no cable): input is no answer; once they show, it is.
        let mut e = on();
        run(&mut e, A, hook("attention", &["c1"]), 0).unwrap();
        e.observe_idle(Some(0), 10_000);
        assert!(asks(&e, A, 10_000));
        e.set_visible(true, 20_000);
        e.observe_idle(Some(0), 20_500);
        assert!(asks(&e, A, 20_500));
        let shown = shown_after(&e, 20_000);
        e.observe_idle(Some(0), shown + 1);
        assert!(!asks(&e, A, shown + 1));
        e.set_visible(false, shown + 2);
        run(&mut e, A, hook("attention", &["c2"]), shown + 3).unwrap();
        e.observe_idle(Some(0), shown + 60_000);
        assert!(asks(&e, A, shown + 60_000));

        // Coming back to the computer is no answer: the flash of the return
        // runs first.
        let mut e = on();
        e.set_visible(true, 0);
        let away = u64::from(e.settings().away.after_s) * 1000;
        e.observe_idle(Some(away), away);
        run(&mut e, A, hook("attention", &["c1"]), away + 1_000).unwrap();
        let back = away + 60_000;
        e.observe_idle(Some(0), back);
        assert!(asks(&e, A, back), "the input that brought the user back");
        e.observe_idle(Some(0), back + BURST_MS);
        assert!(asks(&e, A, back + BURST_MS));
        let after = shown_after(&e, back + BURST_MS) + 1;
        e.observe_idle(Some(0), after);
        assert!(!asks(&e, A, after));
    }

    #[test]
    fn with_two_sessions_a_question_put_out_keeps_its_number_lit() {
        let mut e = on();
        e.set_visible(true, 0);
        let white = e.settings().sessions.color;
        run(&mut e, B, hook("busy", &[]), 0).unwrap();
        assert_eq!(run(&mut e, A, hook("attention", &["c1"]), 100).unwrap()["session"], 2);
        let t = shown_after(&e, 100) + 1;
        e.observe_idle(Some(0), t);
        assert!(!asks(&e, A, t));
        assert_eq!(key_at(&e, "2", t), Some(white), "its number, steady");
        assert_eq!(key_at(&e, "2", t + 700), Some(white));
        assert_eq!(key_at(&e, "esc", t), None, "the rest of the keyboard is dark");
        run(&mut e, A, hook("answered", &["c1"]), t + 1_000).unwrap();
        assert_eq!(key_at(&e, "2", t + 1_000), None);
    }

    #[test]
    fn a_session_quiet_for_half_an_hour_does_not_count_for_the_numbers() {
        let mut e = on();
        let white = e.settings().sessions.color;
        run(&mut e, B, hook("busy", &[]), 0).unwrap();
        run(&mut e, A, hook("attention", &["c1"]), SESSION_LIVE_MS).unwrap();
        let t = SESSION_LIVE_MS + 2_000;
        assert!(["1", "2"].iter().all(|k| key_at(&e, k, t) != Some(white)), "one live session: no numbers");
    }

    #[test]
    fn an_interrupted_turn_takes_its_question_and_working_light_away() {
        let mut e = on();
        let mut s = *e.settings();
        s.signals.busy.effect = AgentEffect::Pulse;
        e.set_settings(s);
        assert_eq!(run(&mut e, A, hook("busy", &[]), 0).unwrap()["lit"], true);
        run(&mut e, A, hook("attention", &["c1"]), 100).unwrap();
        run(&mut e, B, hook("done", &[]), 100).unwrap();
        let stop = run(&mut e, A, hook("interrupted", &[]), 200).unwrap();
        assert_eq!((stop["lit"].clone(), stop["cleared"].clone()), (json!(false), json!(2)));
        assert!(e.infos(200, true).iter().all(|i| i.source != A));
        assert!(!e.sessions(200).iter().any(|s| s.waiting));
        assert_eq!(ids(&e, 200), [format!("{B}:done")], "another session's light stays");
        // What the turn ended with before stays.
        run(&mut e, A, hook("error", &[]), 300).unwrap();
        assert_eq!(run(&mut e, A, hook("interrupted", &[]), 400).unwrap()["cleared"], 0);
        assert!(e.infos(400, true).iter().any(|i| i.source == A && i.kind == "error"));
    }

    #[test]
    fn until_answered_off_keeps_the_question_its_full_time() {
        let mut e = on();
        let mut s = *e.settings();
        s.signals.attention.until_answered = false;
        s.signals.attention.duration_s = 60;
        e.set_settings(s);
        run(&mut e, A, hook("attention", &["c1"]), 0).unwrap();
        assert_eq!(run(&mut e, A, hook("answered", &["c1"]), 1_000).unwrap()["cleared"], 0);
        assert!(asks(&e, A, 59_000));
        e.prune(60_000);
        assert!(!asks(&e, A, 60_000));
        // The end of the turn takes it away all the same.
        run(&mut e, A, hook("attention", &["c1"]), 70_000).unwrap();
        let done = run(&mut e, A, hook("done", &[]), 71_000).unwrap();
        assert_eq!((done["cleared"].clone(), done["id"].clone()), (json!(1), json!(format!("{A}:done"))));
        assert!(!asks(&e, A, 71_000));
    }

    #[test]
    fn a_turn_begins_with_busy_and_ends_with_done_error_or_a_usage_limit() {
        let mut e = on();
        run(&mut e, A, hook("attention", &["c1"]), 0).unwrap();
        let busy = run(&mut e, A, hook("busy", &[]), 1_000).unwrap();
        assert_eq!((busy["lit"].clone(), busy["cleared"].clone()), (json!(false), json!(1)));
        assert!(e.infos(1_000, true).is_empty());
        run(&mut e, A, hook("error", &[]), 2_000).unwrap();
        assert_eq!(e.infos(2_000, true)[0].kind, "error");
        // A: its own look, purple and slow, over every key.
        let limit = run(&mut e, A, hook("usage_limit", &[]), 3_000).unwrap();
        assert_eq!(limit["id"], format!("{A}:usage_limit"));
        let info = e.infos(3_000, true).into_iter().find(|i| i.kind == "usage_limit").unwrap();
        assert_eq!((info.color, info.keys.as_str()), (e.settings().usage_limit.look.color, "all"));
        assert_eq!(info.expires_in_ms, 30.0 * 60_000.0);
        // A turned off: an error like any other.
        let mut s = *e.settings();
        s.usage_limit.on = false;
        e.set_settings(s);
        let plain = run(&mut e, B, hook("usage_limit", &[]), 4_000).unwrap();
        assert_eq!(plain["id"], format!("{B}:error"));
        // The same turn failing again and again (Codex retrying): kept as
        // it is, no new blinks.
        run(&mut e, B, hook("error", &["fail-7"]), 4_050).unwrap();
        let again = run(&mut e, B, hook("error", &["fail-7"]), 4_080).unwrap();
        assert_eq!(again["kept"], true);
        let since = e.lamps().find(|l| l.source == B && l.kind == Kind::Signal(SignalName::Error)).unwrap().since;
        assert_eq!(since, 4_050);
        // A failure of a turn the agent goes on with (a retried request) goes.
        run(&mut e, B, hook("error", &["turn-7"]), 4_100).unwrap();
        assert_eq!(run(&mut e, B, hook("answered", &["turn-6"]), 4_200).unwrap()["cleared"], 0);
        assert_eq!(run(&mut e, B, hook("answered", &["turn-7"]), 4_300).unwrap()["cleared"], 1);
        assert!(e.infos(4_300, true).iter().all(|i| i.source != B));
        // The session ends: its question and its number go; what it ended
        // with stays its time (a headless run ends right after its turn).
        run(&mut e, A, hook("attention", &["c9"]), 4_500).unwrap();
        let end = run(&mut e, A, hook("session_end", &[]), 5_000).unwrap();
        assert_eq!(end["cleared"], 1);
        let left: Vec<String> = e.infos(5_000, true).into_iter().filter(|i| i.source == A).map(|i| i.kind).collect();
        assert_eq!(left, ["usage_limit", "error"]);
        assert!(e.sessions(5_000).iter().all(|s| s.source != A));
    }

    #[test]
    fn a_turn_says_how_long_it_took_when_it_is_done() {
        let mut e = on();
        run(&mut e, A, hook("session_start", &[]), 0).unwrap();
        assert_eq!(run(&mut e, A, hook("done", &[]), 100).unwrap()["turn_ms"], Value::Null, "no prompt seen");
        run(&mut e, A, hook("busy", &[]), 1_000).unwrap();
        run(&mut e, A, hook("attention", &["c1"]), 2_000).unwrap();
        assert_eq!(run(&mut e, A, hook("done", &[]), 61_000).unwrap()["turn_ms"], 60_000);
        assert_eq!(run(&mut e, A, hook("done", &[]), 62_000).unwrap()["turn_ms"], Value::Null, "once");
        // A turn that failed or was stopped is no task done.
        run(&mut e, A, hook("busy", &[]), 70_000).unwrap();
        run(&mut e, A, hook("interrupted", &[]), 80_000).unwrap();
        assert_eq!(run(&mut e, A, hook("done", &[]), 90_000).unwrap()["turn_ms"], Value::Null);
        // Without the cable the done cannot light, and still says how long.
        run(&mut e, A, hook("busy", &[]), 100_000).unwrap();
        let (unlit, _) = e.command(&src(A), hook("done", &[]), Err(usb()), at(700_000));
        let unlit = unlit.unwrap();
        assert_eq!((unlit["lit"].clone(), unlit["turn_ms"].clone()), (json!(false), json!(600_000)));
    }

    #[test]
    fn a_picture_of_its_own_shows_each_key_in_its_colour() {
        let mut e = on();
        let index = index();
        let places = index.resolve(&KeySpec::parse("esc,f1").unwrap()).unwrap();
        let keys: Vec<(u8, Place, Rgb)> =
            places.iter().zip([[255, 0, 0], [0, 0, 255]]).map(|(&(led, place), c)| (led, place, c)).collect();
        let answer = e.paint(&src("tests"), "tests:profile", keys, &index.model_id, 5_000, at(0)).unwrap();
        assert_eq!(answer["shown"], true);
        assert_eq!(key_at(&e, "esc", 10), Some([255, 0, 0]));
        assert_eq!(key_at(&e, "f1", 10), Some([0, 0, 255]));
        assert_eq!(key_at(&e, "f2", 10), None);
        assert_eq!(key_at(&e, "esc", 5_000), None, "its time is up");
        // On another model's keyboard it shows nothing.
        let places = index.resolve(&KeySpec::parse("esc").unwrap()).unwrap();
        let keys = vec![(places[0].0, places[0].1, [9, 9, 9])];
        e.paint(&src("tests"), "tests:profile", keys, "another-model", 5_000, at(6_000)).unwrap();
        e.refresh_keys(&index);
        assert_eq!(key_at(&e, "esc", 6_010), None);
    }

    #[test]
    fn requests_the_app_carries_out_need_access_and_go_in_the_log() {
        let mut off = Engine::new(AgentSettings::default());
        assert_eq!(off.gate(&src("tests"), at(0)).unwrap_err().code, ErrorCode::AgentAccessOff);
        let mut e = on();
        for i in 0..RATE_PER_SECOND as u64 {
            e.gate(&src("tests"), at(i)).unwrap();
        }
        assert_eq!(e.gate(&src("tests"), at(30)).unwrap_err().code, ErrorCode::RateLimited);
        e.note(&src("tests"), "focus", None, None, at(40));
        e.note(&src("tests"), "card", Some(ErrorCode::PermissionOff), None, at(50));
        e.note(&src("tests"), "card", Some(ErrorCode::PermissionOff), None, at(60));
        let log = e.log_entries();
        assert_eq!(log.len(), 2, "a refusal at most once a second");
        assert_eq!((log[0].command.as_str(), log[0].error.as_deref()), ("card", Some("permission_off")));
    }

    #[test]
    fn sessions_are_numbered_in_the_order_they_came() {
        let mut e = on();
        assert_eq!(run(&mut e, A, hook("session_start", &[]), 0).unwrap()["session"], 1);
        assert_eq!(run(&mut e, B, hook("busy", &[]), 10).unwrap()["session"], 2);
        assert_eq!(run(&mut e, A, hook("busy", &[]), 20).unwrap()["session"], 1, "a session keeps its number");
        let list = e.sessions(20);
        assert_eq!(
            list.iter().map(|s| (s.number, s.label.as_deref(), s.agent.as_deref())).collect::<Vec<_>>(),
            [(Some(1), Some("klavye"), Some("claude-code")), (Some(2), Some("klavye"), Some("claude-code"))]
        );
        // The first ends; the next new session takes its number.
        run(&mut e, A, hook("session_end", &[]), 30).unwrap();
        assert_eq!(run(&mut e, "codex:cccc0003", hook("session_start", &[]), 40).unwrap()["session"], 1);
        // Nine numbers; the tenth session has none.
        for i in 0..8u64 {
            run(&mut e, &format!("codex:d{i}"), hook("session_start", &[]), 50 + i).unwrap();
        }
        let tenth = run(&mut e, "codex:last", hook("session_start", &[]), 100).unwrap();
        assert_eq!(tenth["session"], Value::Null);
        // Half an hour on, nobody else heard of: the lowest number is free
        // again, and the one who held it takes the next it finds.
        let later = 100 + SESSION_LIVE_MS;
        assert_eq!(run(&mut e, "codex:last", hook("busy", &[]), later).unwrap()["session"], 1);
        assert_eq!(run(&mut e, "codex:cccc0003", hook("busy", &[]), later + 1).unwrap()["session"], 2);
        // A quiet session whose question is still up (the user gave
        // questions an hour) keeps its number.
        let mut s = *e.settings();
        s.signals.attention.duration_s = 60 * 60;
        e.set_settings(s);
        run(&mut e, "codex:d0", hook("attention", &["q"]), later + 2).unwrap();
        let much_later = later + 2 + 45 * 60_000;
        assert_eq!(run(&mut e, "codex:e1", hook("busy", &[]), much_later).unwrap()["session"], 1);
        assert_eq!(run(&mut e, "codex:e2", hook("busy", &[]), much_later).unwrap()["session"], 2);
        assert_eq!(run(&mut e, "codex:e3", hook("busy", &[]), much_later).unwrap()["session"], 4, "d0 waits on 3");
        // Sessions not heard of for twelve hours are forgotten.
        e.prune(much_later + SESSION_KEEP_MS);
        assert!(e.sessions(much_later + SESSION_KEEP_MS).is_empty());
        // Session starts and answers with nothing to answer stay out of the log.
        assert!(e.log_entries().iter().all(|l| l.command == "hook" && l.id.is_some()), "{:?}", e.log_entries());
    }

    #[test]
    fn with_two_sessions_the_waiting_ones_number_lights() {
        let mut e = on();
        run(&mut e, A, hook("attention", &["c1"]), 0).unwrap();
        let white = e.settings().sessions.color;
        // One session: the question alone, on every key.
        assert_ne!(key_at(&e, "1", 2_000), Some(white));
        // Two sessions: the waiting one's number key in white, over the question.
        run(&mut e, B, hook("busy", &[]), 100).unwrap();
        assert_eq!(key_at(&e, "1", 2_000), Some(white));
        assert_ne!(key_at(&e, "2", 2_000), Some(white));
        assert!(e.sessions(2_000)[0].waiting && !e.sessions(2_000)[1].waiting);
        run(&mut e, B, hook("attention", &[]), 200).unwrap();
        assert_eq!((key_at(&e, "1", 2_000), key_at(&e, "2", 2_000)), (Some(white), Some(white)));
        // Answered: the number goes with the question.
        run(&mut e, A, hook("answered", &["c1"]), 2_100).unwrap();
        assert_ne!(key_at(&e, "1", 2_200), Some(white));
        assert_eq!(key_at(&e, "2", 2_200), Some(white));
        // Turned off: no numbers.
        let mut s = *e.settings();
        s.sessions.on = false;
        e.set_settings(s);
        assert_ne!(key_at(&e, "2", 2_200), Some(white));
        // Night quiet dims them like the rest; "off" takes them away.
        let mut s = *e.settings();
        s.sessions.on = true;
        e.set_settings(s);
        let faint = e.frame(2_200, 0.5).into_iter().find(|k| k.0 == led("2")).map(|k| k.1);
        assert_eq!(faint, Some(scale(white, 0.5)));
        assert!(e.frame(2_200, 0.0).is_empty());
    }

    #[test]
    fn a_helper_agent_finishing_waves_once_when_the_user_wants_it() {
        let mut e = on();
        let off = run(&mut e, A, hook("helper", &[]), 0).unwrap();
        assert_eq!(off["lit"], false, "H is off by default");
        assert!(e.infos(0, true).is_empty());
        let mut s = *e.settings();
        s.helper.on = true;
        e.set_settings(s);
        let wave = run(&mut e, A, hook("helper", &[]), 1_000).unwrap();
        assert_eq!(wave["id"], format!("{A}:helper"));
        let info = e.infos(1_000, true).into_iter().next().unwrap();
        assert_eq!((info.kind.as_str(), info.keys.as_str()), ("helper", "f-row"));
        assert!(info.expires_in_ms <= 2_000.0, "{}", info.expires_in_ms);
        // A question keeps its F row under the wave.
        run(&mut e, A, hook("attention", &[]), 1_100).unwrap();
        assert_eq!(e.infos(1_200, true)[0].kind, "attention");
    }

    #[test]
    fn hook_events_need_access_and_are_checked() {
        let mut e = Engine::new(AgentSettings::default());
        assert_eq!(run(&mut e, A, hook("attention", &[]), 0).unwrap_err().code, ErrorCode::AgentAccessOff);
        let mut e = on();
        let bad = Command::Hook(HookArgs { event: "teleport".into(), ..Default::default() });
        assert_eq!(run(&mut e, A, bad, 0).unwrap_err().code, ErrorCode::BadValue);
        // Without the cable, a question still clears what came before.
        run(&mut e, A, hook("done", &[]), 0).unwrap();
        let (r, _) = e.command(&src(A), hook("busy", &[]), Err(usb()), at(100));
        assert_eq!(r.unwrap()["cleared"], 1);
        let (r, _) = e.command(&src(A), hook("attention", &["c1"]), Err(usb()), at(200));
        assert_eq!(r.unwrap_err().code, ErrorCode::NeedsUsb);
    }

    #[test]
    fn nights_may_run_over_midnight() {
        assert!(in_night(23 * 60, 8 * 60, 23 * 60));
        assert!(in_night(23 * 60, 8 * 60, 2 * 60));
        assert!(!in_night(23 * 60, 8 * 60, 8 * 60));
        assert!(!in_night(23 * 60, 8 * 60, 12 * 60));
        assert!(in_night(13 * 60, 14 * 60, 13 * 60 + 30));
        assert!(!in_night(13 * 60, 13 * 60, 13 * 60));
    }
}
