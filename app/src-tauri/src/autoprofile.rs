//! Per-app profiles ("Uygulamaya göre profil"; keymotion-notes:
//! oyun-oturumu-ve-otomatik-profil.md §2.4).
//!
//! * A rule links a program (its path; a folder covers the programs in it)
//!   or every game (`games`, the game session's list) to a profile. The
//!   program in front comes from `front.rs`; the shell (desktop, Alt+Tab,
//!   Start) changes nothing.
//! * Lighting, by default: a profile's per-key design goes on the keys as a
//!   RAM layer (`LightLayer::Profile`) a second after its program comes in
//!   front, and goes when another program does. Nothing is written, so
//!   switching back and forth wears nothing. An effect (rolling rainbow…)
//!   cannot be drawn from RAM: it changes only with written switches.
//! * Written switches, when a rule asks for them: after the program has
//!   been in front for 10 s, the profile's lighting, key mapping and macros
//!   go to the keyboard (flash; never the screen), at most `WRITES_MAX`
//!   times a day (the last one is kept for going back), only when nothing
//!   is unsaved and no write runs. While a game's session runs only that
//!   game writes: Alt+Tab to other work changes the lighting, never the key
//!   mapping or macros mid-match. A game in quiet mode gets neither light
//!   nor write. After every write the pages read the keyboard again
//!   (`EVENT_AUTO_PROFILE_WRITTEN`).
//! * Going back (`AutoBack`): before the first written switch the keyboard's
//!   lighting, key mapping and macros are kept (`Back`). When the game's
//!   session ends, or a program without a written rule comes in front, they
//!   go back on the keyboard (or the base profile does). A keyboard changed
//!   by hand since the last written switch is the user's: it stays. Another
//!   keyboard on the cable writes nothing; the copy waits for its own.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{AutoBack, AutoProfileSettings, AutoProfileState, AutoProgram, AutoRule, AUTO_RULE_GAMES};
use crate::front::FrontHub;
use crate::games::covered;
use crate::live::now_ms;
use crate::profiles::{LitKey, Profile};
use crate::settings;
use kb_protocol::sonix::level_factor;
use kb_winsys::foreground::Front;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const EVENT_AUTO_PROFILE: &str = "auto-profile";
/// A switch was written (or tried): the keyboard's records changed, so the
/// Lighting, Key mapping and Macros pages read them again. No payload.
pub const EVENT_AUTO_PROFILE_WRITTEN: &str = "auto-profile-written";
const FILE: &str = "auto-profiles.json";
const VERSION: u32 = 1;
const WATCHER: &str = "autoprofile";
/// A program has to stay in front this long before its lighting shows
/// (Alt+Tab passes through others).
const RAM_AFTER_MS: f64 = 1_000.0;
/// … and this long before anything is written.
const WRITE_AFTER_MS: f64 = 10_000.0;
/// Written switches a day.
pub const WRITES_MAX: u32 = 20;
/// `last_error` when the day's written switches are used up (the UI knows it).
const LIMIT_REACHED: &str = "daily limit reached";
const RECENT_MAX: usize = 12;
const TICK: Duration = Duration::from_millis(500);
const IDLE: Duration = Duration::from_secs(30);

/// What the app around this module does for it (lib.rs).
pub trait Host: Send + Sync {
    /// The per-key design of profile `id`, if it is one and fits the
    /// keyboard on the cable.
    fn per_key(&self, id: &str) -> Option<(Vec<LitKey>, u8)>;
    /// Every LED of the keyboard on the cable.
    fn leds(&self) -> Option<Vec<u8>>;
    /// Writing is fine now: a keyboard on the cable, nothing unsaved, no
    /// write going on.
    fn may_write(&self) -> bool;
    /// Profile `id`.
    fn profile(&self, id: &str) -> Option<Profile>;
    /// The lighting, key mapping and macros of the keyboard on the cable, as
    /// this computer recorded them.
    fn keyboard(&self) -> Option<Profile>;
    /// Write `profile`'s lighting, key mapping and macros (not the screen);
    /// the number of parts written (0: the keyboard had them).
    fn write(&self, profile: &Profile) -> Result<u32, String>;
    /// The model of the keyboard on the cable; `None` while there is none
    /// (a profile refused on it is tried again after it comes back).
    fn on_cable(&self) -> Option<String>;
    /// The game this program is, as the game session sees it.
    fn game(&self, front: &Front) -> Option<String>;
    /// The game whose session runs now.
    fn session(&self) -> Option<String>;
    /// A game runs with "leave the keyboard alone while playing" on.
    fn quiet(&self) -> bool;
    /// Game `game` gets quiet mode when it plays (for every game, or for
    /// this one).
    fn quiet_game(&self, game: &str) -> bool;
}

/// The rule for the program in front: its own (the closest folder), else
/// the games' rule when it is a game.
pub fn rule_for<'a>(rules: &'a [AutoRule], path: Option<&str>, is_game: bool) -> Option<&'a AutoRule> {
    let own = path.and_then(|p| {
        rules.iter().filter(|r| r.path != AUTO_RULE_GAMES && covered(p, &r.path)).max_by_key(|r| r.path.len())
    });
    own.or_else(|| is_game.then(|| rules.iter().find(|r| r.path == AUTO_RULE_GAMES)).flatten())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Stored {
    settings: AutoProfileSettings,
    /// Local date (YYYY-MM-DD) the count is for.
    writes_day: String,
    writes: u32,
    /// A written switch is on the keyboard: what to go back to.
    back: Option<Back>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Back {
    /// The keyboard before the first written switch.
    before: Profile,
    /// The keyboard right after the last one; a keyboard that differs was
    /// changed by hand. `None`: it couldn't be read then, and the keyboard
    /// is taken as the switch left it.
    #[serde(default)]
    after: Option<Profile>,
}

/// A written switch.
#[derive(Debug, Clone, PartialEq)]
enum Write {
    Profile(String),
    /// What the user chose to go back to (`AutoBack`).
    Back,
}

/// What a target that held `WRITE_AFTER_MS` writes. `back`: a written switch
/// is on the keyboard; `session`: the game whose session runs.
fn to_write(t: &Target, settings: &AutoProfileSettings, back: bool, session: Option<&str>) -> Option<Write> {
    if !t.may_write {
        return None;
    }
    // While a game's session runs only the game writes (Alt+Tab changes the
    // lighting, not the key mapping or macros).
    if session.is_some() && t.game.as_deref() != session {
        return None;
    }
    match &t.rule {
        Some(r) if r.writes => Some(Write::Profile(r.profile.clone())),
        _ if back && settings.back != AutoBack::Stay => Some(Write::Back),
        _ => None,
    }
}

/// The day's written switches leave room for this one and, after a
/// profile, for going back: else the day's last write could leave a game's
/// key mapping on the keyboard overnight.
fn room(writes: u32, what: &Write, mode: AutoBack) -> bool {
    let needed = match what {
        Write::Profile(_) if mode != AutoBack::Stay => 2,
        _ => 1,
    };
    writes + needed <= WRITES_MAX
}

/// The stored copy, checked against the keyboard on the cable.
#[derive(Debug, PartialEq)]
enum Kept {
    Nothing,
    /// The keyboard is as the last switch left it.
    Valid(Box<Back>),
    /// Changed by hand since: that keyboard is the user's choice now (and
    /// what a later switch goes back to).
    ChangedByHand,
    /// Another keyboard model is on the cable: nothing is written, and the
    /// copy waits for its own keyboard.
    OtherKeyboard,
}

fn check_copy(stored: Option<Back>, keyboard: &Profile) -> Kept {
    match stored {
        None => Kept::Nothing,
        Some(b) if b.before.model_id != keyboard.model_id => Kept::OtherKeyboard,
        Some(b) if b.after.as_ref().is_none_or(|a| same_keyboard(keyboard, a)) => Kept::Valid(Box::new(b)),
        Some(_) => Kept::ChangedByHand,
    }
}

/// The copy to keep after a written switch, and whether it counts towards
/// the day's limit. `copy`: the valid stored one; `before`, `after`: the
/// keyboard read before and after the write; `result`: `None` when nothing
/// was written; `stay`: "Leave as it is", read after the write (it may have
/// been chosen meanwhile).
fn settle(
    what: &Write,
    stay: bool,
    copy: Option<Back>,
    before: &Profile,
    result: Option<&Result<u32, String>>,
    after: Option<Profile>,
) -> (Option<Back>, bool) {
    let counted = match result {
        None => false,
        Some(Ok(parts)) => *parts > 0,
        // Some parts may have gone before the failure: it counts when the
        // keyboard changed, or when that can't be told.
        Some(Err(_)) => after.as_ref().is_none_or(|a| !same_keyboard(a, before)),
    };
    let back = match (what, result) {
        _ if stay => None,
        // Nothing written: the copy stays as it was.
        (_, None) => copy,
        // The first switch keeps the keyboard as it was before it.
        (Write::Profile(_), Some(_)) => Some(Back { before: copy.map_or_else(|| before.clone(), |b| b.before), after }),
        // A failed write back tries again with the next program.
        (Write::Back, Some(Err(_))) => copy.map(|b| Back { after: after.or(b.after), before: b.before }),
        (Write::Back, Some(Ok(_))) => None,
    };
    (back, counted)
}

/// The target for a program a rule could be for. A game in quiet mode
/// gets none: no light and no write, from the moment it comes in front
/// (its session starts later).
fn program_target(
    rules: &[AutoRule],
    front: &Front,
    game: Option<String>,
    quiet: bool,
    since: f64,
    may_write: bool,
) -> Target {
    let rule = if quiet { None } else { rule_for(rules, front.path.as_deref(), game.is_some()).cloned() };
    Target { rule, because: Some(stem(&front.name)), game, since, may_write: may_write && !quiet }
}

/// The target after a look at the program in front (`looked`): `want` is the
/// one for it when a rule could be for it (not the desktop, not Keymotion).
/// `ended`: the game's session just ended; `fresh`: what is in front counts
/// again at once.
fn next_target(
    current: Option<Target>,
    looked: bool,
    want: Option<Target>,
    ended: bool,
    fresh: bool,
    since: f64,
) -> Option<Target> {
    match want {
        Some(want) => {
            let same = current.as_ref().is_some_and(|t| (&t.rule, &t.game) == (&want.rule, &want.game));
            if fresh || !same {
                Some(want)
            } else {
                current
            }
        }
        // The desktop or Keymotion in front when the game ended: the game's
        // lighting goes, and the keyboard goes back.
        None if ended => Some(Target { rule: None, because: None, game: None, since, may_write: true }),
        // The desktop or Keymotion: a switch that hasn't happened yet waits
        // for the program to come back (its 10 s start over).
        None if looked => None,
        None => current,
    }
}

/// Two records of a keyboard hold the same lighting, key mapping and macros.
fn same_keyboard(a: &Profile, b: &Profile) -> bool {
    a.model_id == b.model_id
        && a.keymap == b.keymap
        && a.macros == b.macros
        && match (&a.lighting, &b.lighting) {
            (Some(x), Some(y)) => x.same(y),
            (None, None) => true,
            _ => false,
        }
}

fn today() -> String {
    crate::zones::now().strftime("%Y-%m-%d").to_string()
}

/// What should be in force: a rule (or none), the program, the game.
#[derive(Debug, Clone, PartialEq)]
struct Target {
    rule: Option<AutoRule>,
    because: Option<String>,
    game: Option<String>,
    since: f64,
    /// It may end in a written switch (not when only the rules changed).
    may_write: bool,
}

struct Inner {
    stored: Stored,
    /// The latest program in front, not looked at yet.
    front: Option<Front>,
    /// The last program in front that a rule could be for (not the
    /// desktop, not Keymotion).
    last_program: Option<Front>,
    /// `front` is `last_program` again because the rules changed.
    recheck: bool,
    /// A game ran in quiet mode: nothing went to the keyboard.
    quiet: bool,
    target: Option<Target>,
    /// The rule whose lighting shows (RAM) or was written.
    active: Option<Target>,
    sent: Option<LiveFrame>,
    /// The game whose session ran at the last pass.
    session: Option<String>,
    last_error: Option<String>,
    recent: Vec<AutoProgram>,
    dirty: bool,
    /// Profiles refused on the keyboard on the cable.
    held: Held,
}

/// A failed write that no retry on this keyboard changes (the profile's
/// macros would replace macros made in another program): held. Any other
/// failure (another program open, the firmware question, a busy keyboard,
/// two keyboards on the cable) is tried again on the next switch.
fn holds(error: &str) -> bool {
    error == crate::profiles::ERR_MACROS_KEPT
}

/// Profiles whose write was refused for good while one keyboard is on the
/// cable ([`holds`]): not tried again on every switch, only after the
/// keyboard was unplugged (or another one came).
#[derive(Debug, Default)]
struct Held {
    /// The keyboard the list is about ([`Host::on_cable`]).
    on: Option<String>,
    ids: Vec<String>,
}

impl Held {
    /// The keyboard on the cable now: another one (or none) clears the list.
    fn see(&mut self, on_cable: Option<String>) {
        if self.on != on_cable {
            self.on = on_cable;
            self.ids.clear();
        }
    }

    fn blocks(&self, id: &str) -> bool {
        self.ids.iter().any(|h| h == id)
    }

    fn refused(&mut self, id: &str) {
        if !self.blocks(id) {
            self.ids.push(id.to_string());
        }
    }
}

enum Msg {
    Wake,
    /// The program in front changed.
    Front(Box<Front>),
}

/// Handle to the per-app profiles worker (managed Tauri state).
pub struct AutoProfiles {
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    config_dir: Option<PathBuf>,
    hub: Arc<FrontHub>,
    /// The last state's number (`AutoProfileState::rev`).
    rev: AtomicU32,
}

impl AutoProfiles {
    pub fn spawn(
        app: AppHandle,
        device: Arc<DeviceHandle>,
        hub: Arc<FrontHub>,
        host: Arc<dyn Host>,
        config_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let mut stored =
            config_dir.as_deref().and_then(|d| settings::load::<Stored>(d, FILE, VERSION)).unwrap_or_default();
        stored.settings = stored.settings.sanitized();
        let inner = Arc::new(Mutex::new(Inner {
            stored,
            front: None,
            last_program: None,
            recheck: false,
            quiet: false,
            target: None,
            active: None,
            sent: None,
            session: None,
            last_error: None,
            recent: Vec::new(),
            dirty: true,
            held: Held::default(),
        }));
        let (tx, rx) = mpsc::channel();
        let handle =
            Arc::new(Self { tx: tx.clone(), inner, app, config_dir, hub: hub.clone(), rev: AtomicU32::new(0) });
        // Handed to the worker: a written switch holds the worker's lock for
        // a while, and the hub must not wait for it.
        let to_worker = tx.clone();
        hub.listen(move |front| {
            let _ = to_worker.send(Msg::Front(Box::new(front.clone())));
        });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("autoprofile".into())
            .spawn(move || loop {
                let wait = worker.tick(&device, host.as_ref());
                match rx.recv_timeout(wait) {
                    Ok(Msg::Front(front)) => {
                        if let Ok(mut i) = worker.inner.lock() {
                            i.front = Some(*front);
                        }
                    }
                    Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            })
            .expect("spawn autoprofile thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    pub fn state(&self) -> Result<AutoProfileState, String> {
        let inner = self.inner.lock().map_err(|_| "per-app profiles unavailable")?;
        Ok(self.state_of(&inner))
    }

    fn state_of(&self, inner: &Inner) -> AutoProfileState {
        let writes = if inner.stored.writes_day == today() { inner.stored.writes } else { 0 };
        AutoProfileState {
            // Made under the lock, so the numbers follow the changes.
            rev: self.rev.fetch_add(1, Ordering::Relaxed).wrapping_add(1),
            settings: inner.stored.settings.clone(),
            active: inner.active.as_ref().and_then(|a| a.rule.clone()),
            because: inner.active.as_ref().and_then(|a| a.because.clone()),
            writes_today: writes,
            writes_max: WRITES_MAX,
            // Yesterday's limit is over.
            last_error: inner
                .last_error
                .clone()
                .filter(|e| !(e == LIMIT_REACHED && inner.stored.writes_day != today())),
            recent: inner.recent.clone(),
        }
    }

    fn save(&self, stored: &Stored) -> Result<(), String> {
        match &self.config_dir {
            Some(dir) => settings::save(dir, FILE, VERSION, stored).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    pub fn configure(&self, settings: AutoProfileSettings) -> Result<AutoProfileState, String> {
        let stored = {
            let mut inner = self.inner.lock().map_err(|_| "per-app profiles unavailable")?;
            let settings = settings.sanitized();
            if inner.stored.settings.enabled != settings.enabled || inner.stored.settings.paused != settings.paused {
                tracing::info!(on = settings.enabled, paused = settings.paused, "per-app profiles");
            }
            if settings.back == AutoBack::Stay {
                // Written parts stay from now on.
                inner.stored.back = None;
            }
            inner.stored.settings = settings;
            // The last program again with the new rules: its lighting now, a
            // written switch only once it comes to the front again.
            inner.active = None;
            inner.target = None;
            inner.front = inner.last_program.clone();
            inner.recheck = true;
            inner.dirty = true;
            inner.stored.clone()
        };
        self.save(&stored)?;
        self.wake();
        self.state()
    }

    /// Paused, or running again, from the tray menu. Running turns the
    /// feature on too.
    pub fn set_running(&self, on: bool) {
        let settings = self.inner.lock().ok().map(|i| {
            let s = i.stored.settings.clone();
            AutoProfileSettings { paused: !on, enabled: s.enabled || on, ..s }
        });
        if let Some(s) = settings {
            if let Err(e) = self.configure(s) {
                tracing::warn!("per-app profiles not changed: {e}");
            }
        }
    }

    /// Turned on and not paused (the tray menu's tick).
    pub fn running(&self) -> Option<bool> {
        let i = self.inner.lock().ok()?;
        Some(i.stored.settings.enabled && !i.stored.settings.paused)
    }

    /// One pass of the worker; returns how long to sleep.
    fn tick(&self, device: &DeviceHandle, host: &dyn Host) -> Duration {
        let now = now_ms();
        let on = match self.inner.lock() {
            Ok(i) => i.stored.settings.enabled && !i.stored.settings.paused,
            Err(_) => return IDLE,
        };
        // Other modules' locks first, never under this one's.
        self.hub.want(WATCHER, on);
        let in_front = self.hub.last();
        let session = host.session();
        let quiet = on && host.quiet();
        let on_cable = host.on_cable();

        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        // Another keyboard, or the same one plugged in again: refused
        // profiles get another try.
        inner.held.see(on_cable);
        let s = inner.stored.settings.clone();
        // The game's session ended (or another game's began).
        let ended = inner.session.is_some() && inner.session != session;
        inner.session = session.clone();
        if !on || quiet {
            // Off, or a game wants the keyboard left alone: no light, no write.
            if let Some(front) = inner.front.take() {
                if !front.is_shell() && front.pid != std::process::id() {
                    inner.last_program = Some(front);
                }
            }
            if !on {
                inner.front = None;
                inner.last_program = None;
            }
            inner.quiet = quiet;
            inner.target = None;
            if inner.active.take().is_some() {
                inner.dirty = true;
            }
            self.send(inner, device, None);
            return self.finish(guard, if on { TICK } else { IDLE });
        }

        // Quiet mode or the game's session ended: what is in front counts
        // again, at once (the session's end waited long enough).
        let fresh = std::mem::take(&mut inner.quiet) || ended;
        let mut look = inner.front.take();
        if fresh {
            look = look.or_else(|| in_front.clone());
        }
        let since = if fresh { now - WRITE_AFTER_MS } else { now };
        let recheck = std::mem::take(&mut inner.recheck);
        let want = match &look {
            Some(front) if !front.is_shell() && front.pid != std::process::id() => {
                if let Some(path) = &front.path {
                    inner.recent.retain(|p| !crate::games::same_path(&p.path, path));
                    inner.recent.insert(0, AutoProgram { path: path.clone(), name: stem(&front.name) });
                    inner.recent.truncate(RECENT_MAX);
                }
                let game = host.game(front);
                let quiet_game = game.as_deref().is_some_and(|g| host.quiet_game(g));
                inner.last_program = Some(front.clone());
                Some(program_target(&s.rules, front, game, quiet_game, since, !recheck))
            }
            _ => None,
        };
        inner.target = next_target(inner.target.take(), look.is_some(), want, ended, fresh, since);

        // Lighting (RAM) once the target has held a moment.
        let mut pending = None;
        if let Some(t) = inner.target.clone() {
            if now - t.since >= RAM_AFTER_MS {
                let frame = t.rule.as_ref().and_then(|r| ram_frame(host, &r.profile));
                if inner.active.as_ref().map(|a| (&a.rule, &a.game)) != Some((&t.rule, &t.game)) {
                    tracing::debug!(profile = ?t.rule.as_ref().map(|r| &r.profile), "per-app profile in force");
                    inner.active = Some(t.clone());
                    inner.dirty = true;
                }
                self.send(inner, device, frame);
                // Written switches wait longer. The keyboard may hold another
                // profile by now (loaded by hand), so no "already written"
                // shortcut: `load_part` skips the parts the keyboard has.
                if now - t.since >= WRITE_AFTER_MS {
                    inner.target = None;
                    pending = to_write(&t, &s, inner.stored.back.is_some(), session.as_deref()).map(|w| (w, t));
                }
            }
        }
        let wait = self.finish(guard, TICK);
        // The write takes a few seconds: not under the lock (the page and
        // the tray menu read the state meanwhile).
        if let Some((what, t)) = pending {
            self.write(host, what, t);
        }
        wait
    }

    /// A written switch, within the day's limit. When it can't go now
    /// (unsaved changes, another write), `target` waits for the next pass.
    fn write(&self, host: &dyn Host, what: Write, target: Target) {
        let (stored_back, mode, base) = {
            let Ok(mut inner) = self.inner.lock() else { return };
            if let Write::Profile(id) = &what {
                if inner.held.blocks(id) {
                    tracing::debug!(profile = %id, "per-app profile was refused on this keyboard; not tried again");
                    return;
                }
            }
            if inner.stored.writes_day != today() {
                inner.stored.writes_day = today();
                inner.stored.writes = 0;
            }
            let mode = inner.stored.settings.back;
            if !room(inner.stored.writes, &what, mode) {
                if inner.last_error.as_deref() != Some(LIMIT_REACHED) {
                    tracing::info!("per-app profiles: daily write limit reached");
                    inner.last_error = Some(LIMIT_REACHED.into());
                    inner.dirty = true;
                }
                self.finish(inner, TICK);
                return;
            }
            (inner.stored.back.clone(), mode, inner.stored.settings.base.clone())
        };
        if !host.may_write() {
            if let Ok(mut inner) = self.inner.lock() {
                if inner.target.is_none() {
                    inner.target = Some(target);
                }
            }
            return;
        }
        // Unknown: there is nothing to compare with or go back to.
        let Some(keyboard) = host.keyboard() else { return };
        let copy = match check_copy(stored_back, &keyboard) {
            Kept::OtherKeyboard => {
                tracing::info!("per-app profiles: another keyboard on the cable; nothing written");
                return;
            }
            Kept::ChangedByHand => {
                tracing::info!("per-app profiles: the keyboard was changed by hand");
                None
            }
            Kept::Valid(b) => Some(*b),
            Kept::Nothing => None,
        };
        let profile = match &what {
            Write::Profile(id) => host.profile(id),
            Write::Back => match (&copy, mode) {
                (Some(b), AutoBack::Previous) => Some(b.before.clone()),
                (Some(_), AutoBack::Profile) => base.as_deref().and_then(|id| host.profile(id)),
                _ => None,
            },
        };
        // A profile made for another model: nothing written, nothing counted.
        if profile.as_ref().is_some_and(|p| !p.fits(&keyboard.model_id)) {
            tracing::info!("per-app profiles: the profile is for another keyboard model");
        }
        let profile = profile.filter(|p| p.fits(&keyboard.model_id));
        let result = profile.as_ref().map(|p| host.write(p));
        let after = if result.is_some() { host.keyboard() } else { None };
        if result.is_some() {
            // The pages show the keyboard's lighting, key mapping and macros
            // again: their records changed (some, even when it failed).
            let _ = self.app.emit(EVENT_AUTO_PROFILE_WRITTEN, ());
        }

        let Ok(mut inner) = self.inner.lock() else { return };
        match &result {
            Some(Ok(parts)) => {
                tracing::info!(parts, back = (what == Write::Back), "per-app profile written");
                inner.last_error = None;
            }
            Some(Err(e)) => {
                tracing::warn!("per-app profile not written: {e}");
                inner.last_error = Some(e.clone());
                if let Write::Profile(id) = &what {
                    if holds(e) {
                        inner.held.refused(id);
                    }
                }
            }
            None => {}
        }
        let stay = inner.stored.settings.back == AutoBack::Stay;
        let (back, counted) = settle(&what, stay, copy, &keyboard, result.as_ref(), after);
        inner.stored.back = back;
        if counted {
            inner.stored.writes += 1;
        }
        if let Err(e) = self.save(&inner.stored) {
            tracing::warn!("per-app profiles not saved: {e}");
        }
        inner.dirty = true;
        self.finish(inner, TICK);
    }

    fn send(&self, inner: &mut Inner, device: &DeviceHandle, frame: Option<LiveFrame>) {
        if frame != inner.sent {
            let _ = device.send(Request::SetLayer(LightLayer::Profile, frame.clone()));
            inner.sent = frame;
        }
    }

    fn finish(&self, mut guard: std::sync::MutexGuard<'_, Inner>, wait: Duration) -> Duration {
        if std::mem::take(&mut guard.dirty) {
            let state = self.state_of(&guard);
            drop(guard);
            let _ = self.app.emit(EVENT_AUTO_PROFILE, state);
        }
        wait
    }
}

/// A program's file name without its extension.
fn stem(name: &str) -> String {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string()
}

/// A per-key design as a RAM frame: its keys lit, the rest off.
fn ram_frame(host: &dyn Host, profile: &str) -> Option<LiveFrame> {
    let (keys, brightness) = host.per_key(profile)?;
    let leds = host.leds()?;
    let colors =
        leds.into_iter().map(|led| (led, keys.iter().find(|k| k.0 == led).map_or([0, 0, 0], |k| k.1))).collect();
    Some(LiveFrame { colors, level: level_factor(brightness) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(path: &str, profile: &str) -> AutoRule {
        AutoRule { path: path.into(), name: path.into(), profile: profile.into(), writes: false }
    }

    #[test]
    fn only_a_refusal_for_good_holds_a_profile() {
        assert!(holds(crate::profiles::ERR_MACROS_KEPT));
        // Conditions the user clears: tried again on the next switch
        // ("04 xx" keyboards too).
        for passing in [
            "another program is using the keyboard (ajazz-driver)",
            "untested keyboard firmware 1.07",
            "more than one keyboard on the USB cable",
            "keyboard did not answer in time",
            "profile part failed (keymap): no valid echo",
        ] {
            assert!(!holds(passing), "{passing}");
        }
    }

    #[test]
    fn a_refused_profile_is_tried_once_per_connection() {
        let mut held = Held::default();
        held.see(Some("ajazz-ak980-pro".into()));
        assert!(!held.blocks("work"));
        held.refused("work");
        held.refused("work");
        assert!(held.blocks("work") && !held.blocks("games"));
        // The same keyboard, still on the cable: not again.
        held.see(Some("ajazz-ak980-pro".into()));
        assert!(held.blocks("work"));
        // Unplugged (and plugged in again): another try.
        held.see(None);
        held.see(Some("ajazz-ak980-pro".into()));
        assert!(!held.blocks("work"));
    }

    #[test]
    fn a_programs_own_rule_then_the_games_rule() {
        let rules = vec![
            rule(r"C:\Program Files\Code\Code.exe", "work"),
            rule(AUTO_RULE_GAMES, "games"),
            rule(r"D:\Games\Racer", "racing"),
        ];
        let pick = |p: &str, game: bool| rule_for(&rules, Some(p), game).map(|r| r.profile.as_str());
        assert_eq!(pick(r"c:\program files\code\CODE.EXE", false), Some("work"));
        // A folder covers its programs; a game's own rule beats the games' rule.
        assert_eq!(pick(r"D:\Games\Racer\bin\racer.exe", true), Some("racing"));
        assert_eq!(pick(r"C:\Riot Games\League of Legends\Game\League of Legends.exe", true), Some("games"));
        assert_eq!(pick(r"C:\Windows\notepad.exe", false), None);
        // A folder name that only starts the same.
        assert_eq!(pick(r"D:\Games\Racer2\racer.exe", false), None);
        assert_eq!(rule_for(&rules, None, false), None);
    }

    fn target(rule: Option<AutoRule>, game: Option<&str>) -> Target {
        Target { rule, because: None, game: game.map(Into::into), since: 0.0, may_write: true }
    }

    #[test]
    fn what_a_program_writes() {
        let s = AutoProfileSettings::default();
        assert_eq!(s.back, AutoBack::Previous, "going back is the default");
        let racing = AutoRule { writes: true, ..rule(r"D:\Games\Racer", "racing") };
        let work = rule(r"C:\Code\Code.exe", "work");
        let racer = Some("racer");
        // A written rule writes; without one the keyboard goes back, once a
        // switch was written.
        assert_eq!(
            to_write(&target(Some(racing.clone()), racer), &s, false, None),
            Some(Write::Profile("racing".into()))
        );
        assert_eq!(to_write(&target(None, None), &s, true, None), Some(Write::Back));
        assert_eq!(to_write(&target(Some(work.clone()), None), &s, true, None), Some(Write::Back));
        assert_eq!(to_write(&target(None, None), &s, false, None), None);
        // During a game's session only the game writes: Alt+Tab to other
        // work changes the lighting, not the key mapping or macros.
        assert_eq!(to_write(&target(None, None), &s, true, racer), None);
        let work_written = AutoRule { writes: true, ..work };
        assert_eq!(to_write(&target(Some(work_written), None), &s, true, racer), None);
        assert_eq!(
            to_write(&target(Some(racing.clone()), racer), &s, true, racer),
            Some(Write::Profile("racing".into()))
        );
        // "Leave as it is"; a change of the rules only.
        let stay = AutoProfileSettings { back: AutoBack::Stay, ..s.clone() };
        assert_eq!(to_write(&target(None, None), &stay, true, None), None);
        let rules_changed = Target { may_write: false, ..target(Some(racing), racer) };
        assert_eq!(to_write(&rules_changed, &s, true, None), None);
    }

    /// A keyboard's record, told apart by its number of macros.
    fn kb(macros: usize) -> Profile {
        Profile {
            id: String::new(),
            name: String::new(),
            created_ms: 0.0,
            updated_ms: 0.0,
            model_id: "ak980pro".into(),
            lighting: Some(crate::profiles::ProfileLighting::factory()),
            keymap: Some(Default::default()),
            macros: Some(vec![None; macros]),
            screen: None,
        }
    }

    fn back(before: usize, after: Option<usize>) -> Back {
        Back { before: kb(before), after: after.map(kb) }
    }

    #[test]
    fn a_keyboard_changed_by_hand_is_another_one() {
        assert!(same_keyboard(&kb(0), &kb(0)));
        assert!(!same_keyboard(&kb(0), &kb(1)));
        assert!(!same_keyboard(&kb(0), &Profile { lighting: None, ..kb(0) }));
        assert!(!same_keyboard(&kb(0), &Profile { model_id: "other".into(), ..kb(0) }));
    }

    #[test]
    fn the_copy_is_checked_against_the_keyboard_on_the_cable() {
        assert_eq!(check_copy(None, &kb(1)), Kept::Nothing);
        assert_eq!(check_copy(Some(back(0, Some(1))), &kb(1)), Kept::Valid(Box::new(back(0, Some(1)))));
        assert_eq!(check_copy(Some(back(0, Some(1))), &kb(2)), Kept::ChangedByHand);
        // Not read after the last switch: taken as the switch left it.
        assert_eq!(check_copy(Some(back(0, None)), &kb(2)), Kept::Valid(Box::new(back(0, None))));
        // Another model on the cable is not a change by hand: the copy waits.
        let other = Profile { model_id: "other".into(), ..kb(1) };
        assert_eq!(check_copy(Some(back(0, Some(1))), &other), Kept::OtherKeyboard);
        // Copies saved by the earlier version read the same; one without
        // `after` reads too.
        let saved = serde_json::to_value(back(0, Some(1))).unwrap();
        assert_eq!(serde_json::from_value::<Back>(saved.clone()).unwrap(), back(0, Some(1)));
        let without = serde_json::json!({ "before": saved["before"] });
        assert_eq!(serde_json::from_value::<Back>(without).unwrap(), back(0, None));
    }

    #[test]
    fn what_a_written_switch_keeps_and_counts() {
        let racing = Write::Profile("racing".into());
        let ok = |parts: u32| -> Option<Result<u32, String>> { Some(Ok(parts)) };
        let failed: Option<Result<u32, String>> = Some(Err("unplugged".into()));
        let settle_racing = |copy, before: usize, result: &Option<Result<u32, String>>, after: Option<usize>| {
            settle(&racing, false, copy, &kb(before), result.as_ref(), after.map(kb))
        };
        // The first switch keeps the keyboard as it was, and counts.
        assert_eq!(settle_racing(None, 0, &ok(3), Some(1)), (Some(back(0, Some(1))), true));
        // The keyboard had it already: not counted.
        assert_eq!(settle_racing(None, 1, &ok(0), Some(1)), (Some(back(1, Some(1))), false));
        // A later switch keeps the first "before".
        assert_eq!(settle_racing(Some(back(0, Some(1))), 1, &ok(2), Some(2)), (Some(back(0, Some(2))), true));
        // The keyboard couldn't be read after the write: the copy stays.
        assert_eq!(settle_racing(None, 0, &ok(1), None), (Some(back(0, None)), true));
        // A failure counts only when it changed the keyboard, or when that
        // can't be told.
        assert_eq!(settle_racing(None, 0, &failed, Some(0)), (Some(back(0, Some(0))), false));
        assert!(settle_racing(None, 0, &failed, Some(1)).1);
        assert!(settle_racing(None, 0, &failed, None).1);
        // The profile is gone, or made for another model: nothing written.
        assert_eq!(settle_racing(Some(back(0, Some(1))), 1, &None, None), (Some(back(0, Some(1))), false));
        // "Leave as it is", even when chosen during the write: no copy.
        let stay = settle(&racing, true, Some(back(0, Some(1))), &kb(1), ok(2).as_ref(), Some(kb(2)));
        assert_eq!(stay, (None, true));
    }

    #[test]
    fn going_back_drops_the_copy_or_tries_again() {
        let ok: Option<Result<u32, String>> = Some(Ok(3));
        let failed: Option<Result<u32, String>> = Some(Err("unplugged".into()));
        let copy = Some(back(0, Some(1)));
        let going_back = |copy, result: &Option<Result<u32, String>>, after: Option<usize>| {
            settle(&Write::Back, false, copy, &kb(1), result.as_ref(), after.map(kb))
        };
        assert_eq!(going_back(copy.clone(), &ok, Some(0)), (None, true));
        // A failed write back tries again with the next program, from where
        // the keyboard is now.
        assert_eq!(going_back(copy.clone(), &failed, Some(5)), (Some(back(0, Some(5))), true));
        // Nothing went: not counted.
        assert_eq!(going_back(copy.clone(), &failed, Some(1)), (copy.clone(), false));
        assert_eq!(going_back(copy.clone(), &failed, None), (copy.clone(), true));
        // The base profile is gone: the copy stays. Nothing to go back to.
        assert_eq!(going_back(copy.clone(), &None, None), (copy, false));
        assert_eq!(going_back(None, &None, None), (None, false));
    }

    #[test]
    fn the_last_write_of_the_day_is_kept_for_going_back() {
        let racing = Write::Profile("racing".into());
        assert!(room(WRITES_MAX - 2, &racing, AutoBack::Previous));
        assert!(!room(WRITES_MAX - 1, &racing, AutoBack::Previous));
        assert!(!room(WRITES_MAX - 1, &racing, AutoBack::Profile));
        assert!(room(WRITES_MAX - 1, &Write::Back, AutoBack::Previous));
        assert!(!room(WRITES_MAX, &Write::Back, AutoBack::Previous));
        // Nothing goes back after "Leave as it is".
        assert!(room(WRITES_MAX - 1, &racing, AutoBack::Stay));
        assert!(!room(WRITES_MAX, &racing, AutoBack::Stay));
    }

    #[test]
    fn the_desktop_in_front_as_the_session_ends_goes_back() {
        let s = AutoProfileSettings::default();
        let racing = AutoRule { writes: true, ..rule(r"D:\Games\Racer", "racing") };
        let playing = Some(target(Some(racing), Some("racer")));
        let t = next_target(playing.clone(), true, None, true, true, 0.0).expect("a target");
        assert_eq!((t.rule.as_ref(), t.game.as_deref()), (None, None));
        assert_eq!(to_write(&t, &s, true, None), Some(Write::Back));
        // Also when nothing new came in front.
        assert!(next_target(playing.clone(), false, None, true, true, 0.0).is_some_and(|t| t.rule.is_none()));
        // Otherwise the desktop changes nothing: a switch not made yet waits
        // for its program to come back.
        assert_eq!(next_target(playing.clone(), true, None, false, false, 0.0), None);
        assert_eq!(next_target(playing.clone(), false, None, false, false, 0.0), playing);
    }

    #[test]
    fn a_game_in_quiet_mode_gets_no_light_and_no_write() {
        let rules = vec![AutoRule { writes: true, ..rule(AUTO_RULE_GAMES, "games") }];
        let s = AutoProfileSettings { rules: rules.clone(), ..AutoProfileSettings::default() };
        let racer = Front {
            pid: 7,
            name: "racer.exe".into(),
            path: Some(r"D:\Games\Racer\racer.exe".into()),
            class: "GameWindow".into(),
            covers_monitor: true,
            minimized: false,
        };
        let loud = program_target(&rules, &racer, Some("racer".into()), false, 0.0, true);
        assert_eq!(to_write(&loud, &s, false, None), Some(Write::Profile("games".into())));
        // Before its session starts (15 s) as much as during it.
        let quiet = program_target(&rules, &racer, Some("racer".into()), true, 0.0, true);
        assert_eq!(quiet.rule, None, "no light");
        assert_eq!(to_write(&quiet, &s, true, None), None, "no write, not even going back");
    }

    #[test]
    fn a_rule_still_holds_after_the_program_updates() {
        let rules = vec![
            rule(r"C:\Users\a\AppData\Local\Discord\app-1.0.9165\Discord.exe", "chat"),
            rule(
                r"C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_11.2410.21.0_x64__8wekyb3d8bbwe\Notepad\Notepad.exe",
                "notes",
            ),
            rule(r"D:\Oyunlar\Şehir", "city"),
        ];
        let pick = |p: &str| rule_for(&rules, Some(p), false).map(|r| r.profile.as_str());
        assert_eq!(pick(r"C:\Users\a\AppData\Local\Discord\app-1.0.9172\Discord.exe"), Some("chat"));
        assert_eq!(
            pick(
                r"C:\Program Files\WindowsApps\Microsoft.WindowsNotepad_11.2507.26.0_x64__8wekyb3d8bbwe\Notepad\Notepad.exe"
            ),
            Some("notes")
        );
        // Turkish letters in any case.
        assert_eq!(pick(r"d:\oyunlar\şehir\bin\city.exe"), Some("city"));
        assert_eq!(pick(r"D:\OYUNLAR\ŞEHIR\BIN\CITY.EXE"), Some("city"));
    }
}
