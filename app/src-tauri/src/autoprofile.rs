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
//!   times a day, only when nothing is unsaved and no write runs. A game's
//!   profile stays for its whole session (Alt+Tab does not switch back).
//!   When a program without a rule comes in front, the base profile is
//!   written back, if the user chose one.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{AutoProfileSettings, AutoProfileState, AutoProgram, AutoRule, AUTO_RULE_GAMES};
use crate::front::FrontHub;
use crate::games::covered;
use crate::live::now_ms;
use crate::profiles::LitKey;
use crate::settings;
use kb_protocol::sonix::level_factor;
use kb_winsys::foreground::Front;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const EVENT_AUTO_PROFILE: &str = "auto-profile";
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
    /// Write profile `id`'s lighting, key mapping and macros (not the
    /// screen); the number of parts written (0: the keyboard had them).
    fn write(&self, id: &str) -> Result<u32, String>;
    /// The game this program is, as the game session sees it.
    fn game(&self, front: &Front) -> Option<String>;
    /// The game whose session runs now.
    fn session(&self) -> Option<String>;
    /// A game runs with "leave the keyboard alone while playing" on.
    fn quiet(&self) -> bool;
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
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
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
    /// The profile this module last wrote to the keyboard.
    written: Option<String>,
    sent: Option<LiveFrame>,
    /// The active rule's game had its session running (so its end counts).
    session_seen: bool,
    last_error: Option<String>,
    recent: Vec<AutoProgram>,
    dirty: bool,
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
            written: None,
            sent: None,
            session_seen: false,
            last_error: None,
            recent: Vec::new(),
            dirty: true,
        }));
        let (tx, rx) = mpsc::channel();
        let handle = Arc::new(Self { tx: tx.clone(), inner, app, config_dir, hub: hub.clone() });
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
        Ok(Self::state_of(&inner))
    }

    fn state_of(inner: &Inner) -> AutoProfileState {
        let writes = if inner.stored.writes_day == today() { inner.stored.writes } else { 0 };
        AutoProfileState {
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

        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        let s = inner.stored.settings.clone();
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

        let active_game = inner.active.as_ref().and_then(|a| a.game.clone());
        if active_game.is_some() && active_game == session {
            inner.session_seen = true;
        }
        let mut look = inner.front.take();
        // The game's session ended, or quiet mode did: what is in front now
        // counts again.
        if std::mem::take(&mut inner.quiet) {
            look = look.or_else(|| in_front.clone());
        }
        if inner.session_seen && active_game.is_some() && active_game != session {
            inner.session_seen = false;
            look = look.or_else(|| in_front.clone());
            inner.active = None;
        }
        let recheck = std::mem::take(&mut inner.recheck);
        if let Some(front) = look {
            if front.is_shell() || front.pid == std::process::id() {
                // The desktop or Keymotion: a switch that hasn't happened yet
                // waits for the program to come back (its 10 s start over).
                inner.target = None;
            } else {
                if let Some(path) = &front.path {
                    inner.recent.retain(|p| !crate::games::same_path(&p.path, path));
                    inner.recent.insert(0, AutoProgram { path: path.clone(), name: stem(&front.name) });
                    inner.recent.truncate(RECENT_MAX);
                }
                let game = host.game(&front);
                let rule = rule_for(&s.rules, front.path.as_deref(), game.is_some()).cloned();
                // A game's profile stays for its session (Alt+Tab).
                let keep = inner.session_seen && active_game.is_some() && active_game == session && game.is_none();
                let want = Target { rule, because: Some(stem(&front.name)), game, since: now, may_write: !recheck };
                let same = inner.target.as_ref().map(|t| (&t.rule, &t.game)) == Some((&want.rule, &want.game));
                if !keep && !same {
                    inner.target = Some(want);
                }
                inner.last_program = Some(front);
            }
        }

        // Lighting (RAM) once the target has held a moment.
        let mut pending = None;
        if let Some(t) = inner.target.clone() {
            if now - t.since >= RAM_AFTER_MS {
                let frame = t.rule.as_ref().and_then(|r| ram_frame(host, &r.profile));
                if inner.active.as_ref().map(|a| (&a.rule, &a.game)) != Some((&t.rule, &t.game)) {
                    tracing::debug!(profile = ?t.rule.as_ref().map(|r| &r.profile), "per-app profile in force");
                    inner.active = Some(t.clone());
                    inner.session_seen = false;
                    inner.dirty = true;
                }
                self.send(inner, device, frame);
                // Written switches wait longer. The keyboard may hold another
                // profile by now (loaded by hand), so no "already written"
                // shortcut: `load_part` skips the parts the keyboard has.
                let write = match &t.rule {
                    Some(r) if r.writes => Some(r.profile.clone()),
                    Some(_) => None,
                    None => s.base.clone().filter(|_| inner.written.is_some()),
                };
                if now - t.since >= WRITE_AFTER_MS {
                    inner.target = None;
                    pending = write.filter(|_| t.may_write).map(|id| (id, t));
                }
            }
        }
        let wait = self.finish(guard, TICK);
        // The write takes a few seconds: not under the lock (the page and
        // the tray menu read the state meanwhile).
        if let Some((id, t)) = pending {
            self.write(host, &id, t);
        }
        wait
    }

    /// A written switch to profile `id`, within the day's limit. When it
    /// can't go now (unsaved changes, another write), `target` waits for
    /// the next pass.
    fn write(&self, host: &dyn Host, id: &str, target: Target) {
        {
            let Ok(mut inner) = self.inner.lock() else { return };
            if inner.stored.writes_day != today() {
                inner.stored.writes_day = today();
                inner.stored.writes = 0;
            }
            if inner.stored.writes >= WRITES_MAX {
                if inner.last_error.as_deref() != Some(LIMIT_REACHED) {
                    tracing::info!("per-app profiles: daily write limit reached");
                    inner.last_error = Some(LIMIT_REACHED.into());
                    inner.dirty = true;
                }
                self.finish(inner, TICK);
                return;
            }
        }
        if !host.may_write() {
            if let Ok(mut inner) = self.inner.lock() {
                if inner.target.is_none() {
                    inner.target = Some(target);
                }
            }
            return;
        }
        let result = host.write(id);
        let Ok(mut inner) = self.inner.lock() else { return };
        let counted = match result {
            Ok(parts) => {
                tracing::info!(parts, "per-app profile written");
                inner.written = Some(id.to_string());
                inner.last_error = None;
                parts > 0
            }
            Err(e) => {
                // Some parts may have gone before the failure: it counts.
                tracing::warn!("per-app profile not written: {e}");
                inner.last_error = Some(e);
                true
            }
        };
        if counted {
            inner.stored.writes += 1;
            if let Err(e) = self.save(&inner.stored) {
                tracing::warn!("per-app profiles not saved: {e}");
            }
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
            let state = Self::state_of(&guard);
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
