//! Game session (keymotion-notes: oyun-oturumu-ve-otomatik-profil.md).
//!
//! * A game is a program the user marked as one, or one the launchers list
//!   (`kb_winsys::games`: Steam, Epic, GOG, Xbox, Riot, Game Bar). The
//!   program in front comes from `front.rs`; no game is ever opened, read or
//!   written to (anti-cheat software, §5.2).
//! * A session starts when a game has been in front for 15 s, and lasts
//!   while its program runs: Alt+Tab or minimising does not end it. It ends
//!   when the program has been gone for 10 s (some games restart
//!   themselves).
//! * While it runs, the processor and graphics load and the graphics card's
//!   temperature are read every 2 s. A finished session goes into the
//!   history (`game-history.json`, on this computer only).
//! * The keyboard's screen (live screen on): a start card when it starts,
//!   a summary card when a session of 5 min or more ends; nothing new while
//!   it runs. The session holds the screen, so "Çalan şarkı" waits; the
//!   calendar, a reminder and the focus timer rank above it.
//! * The keys (`LightLayer::Game`, RAM only): the chosen keys lit, the rest
//!   dark or dim. The gauges wait while it shows; the alerts stay on top.
//!   They follow the window: Alt+Tab to other work and they wait (the
//!   session goes on) until the game is in front again.
//! * "Quiet": while a game runs nothing goes to the keyboard, but what the
//!   user asks for ("Try on the keyboard") and, when the user lets them,
//!   the meetings' and reminders' lights and cards.
//! * A program that covers the screen for 2 min with the graphics card
//!   busy, and is neither a game nor a known non-game, is offered as
//!   "Is this a game?". It never becomes one by itself.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Quiet, Request};
use crate::dto::{
    Connection, GameCard, GameCardKind, GameCardStatus, GameEntry, GameLights, GameMark, GameRecord, GameRest,
    GameSessionInfo, GameSource, GameSuggestion, GamesSettings, GamesState, LolScene,
};
use crate::front::FrontHub;
use crate::live::{now_ms, LiveHandle};
use crate::settings;
use kb_devices::Model;
use kb_protocol::sonix::{level_factor, Rgb};
use kb_winsys::foreground::{process_names, Front};
use kb_winsys::games::{self as library, KnownGame, Source};
use kb_winsys::{CpuMeter, GpuMeter};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Live screen source id.
pub const SOURCE: &str = "game";
pub const EVENT_GAMES: &str = "games";
pub const EVENT_GAME_CARD: &str = "game-card";
const FILE: &str = "games.json";
const VERSION: u32 = 1;
const HISTORY_FILE: &str = "game-history.json";
const HISTORY_VERSION: u32 = 1;
const HISTORY_MAX: usize = 1_000;
/// The front-hub user name.
const WATCHER: &str = "games";

const START_AFTER_MS: f64 = 15_000.0;
const GONE_AFTER_MS: f64 = 10_000.0;
pub const SUMMARY_MIN_MS: f64 = 5.0 * 60_000.0;
const SAMPLE_MS: f64 = 2_000.0;
const ALIVE_MS: f64 = 5_000.0;
const RESCAN_MS: f64 = 10.0 * 60_000.0;
const SUGGEST_AFTER_MS: f64 = 2.0 * 60_000.0;
const SUGGEST_GPU: f32 = 40.0;
/// The page's numbers (minutes played) refresh this often during a session.
const EMIT_MS: f64 = 15_000.0;
/// "Try on the keyboard" shows the game lights this long.
const TRY_MS: f64 = 5_000.0;
/// The start card left after a short session goes away after this.
const STALE_CARD_BACK: Duration = Duration::from_secs(60);
const RECENT: usize = 10;
const DAY_MS: f64 = 86_400_000.0;
const TICK: Duration = Duration::from_secs(1);
/// While League of Legends' lights show.
const LOL_TICK: Duration = Duration::from_millis(50);
const IDLE: Duration = Duration::from_secs(30);
/// A pass this much later than due: the computer slept, and a session
/// ends where it stopped.
const SLEPT_MS: f64 = 60_000.0;
/// Programs kept for "Add a game".
const PROGRAMS_MAX: usize = 12;
/// The dim keys of `GameRest::Dim`: a faint share of the colour.
const DIM: f32 = 0.12;

/// Programs that cover the screen without being games: browsers and video
/// players (full-screen video), calls, streaming, remote desktops.
const NOT_GAMES: [&str; 25] = [
    "explorer.exe",
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "opera.exe",
    "brave.exe",
    "vivaldi.exe",
    "vlc.exe",
    "mpc-hc64.exe",
    "mpc-be64.exe",
    "potplayermini64.exe",
    "wmplayer.exe",
    "microsoft.media.player.exe",
    "video.ui.exe",
    "obs64.exe",
    "discord.exe",
    "spotify.exe",
    "powerpnt.exe",
    "ms-teams.exe",
    "zoom.exe",
    "mstsc.exe",
    "vmware.exe",
    "virtualboxvm.exe",
    "keymotion.exe",
    "netflix.exe",
];

/// League of Legends' match program (Riot's list names it alone).
pub fn is_lol(key: &str) -> bool {
    key.ends_with("league of legends.exe")
}

fn not_a_game(exe: &str) -> bool {
    let lower = exe.to_ascii_lowercase();
    NOT_GAMES.contains(&lower.as_str()) || library::is_helper(exe)
}

fn source_of(s: Source) -> GameSource {
    match s {
        Source::Steam => GameSource::Steam,
        Source::Epic => GameSource::Epic,
        Source::Gog => GameSource::Gog,
        Source::Xbox => GameSource::Xbox,
        Source::Riot => GameSource::Riot,
        Source::Windows => GameSource::Windows,
    }
}

/// A path as compared: any case (Ş and ş too), either slash, and without
/// the version an update changes (Store apps, `app-1.0.9165`, Roblox), so a
/// rule or a mark still holds after the program updates.
fn norm(path: &str) -> String {
    kb_winsys::games::stable_key(path)
}

/// The same program or folder.
pub(crate) fn same_path(a: &str, b: &str) -> bool {
    norm(a) == norm(b)
}

/// `path` is `mark` (a program) or lies in it (a game's folder).
pub(crate) fn covered(path: &str, mark: &str) -> bool {
    let (path, mark) = (norm(path), norm(mark));
    path == mark || (path.starts_with(&mark) && path.as_bytes().get(mark.len()) == Some(&b'\\'))
}

/// The user's word on `path`, if any (the closest mark wins).
fn mark_for<'a>(marks: &'a [GameMark], path: &str) -> Option<&'a GameMark> {
    marks.iter().filter(|m| covered(path, &m.path)).max_by_key(|m| m.path.len())
}

/// Whether the session's game is still a game under `s`: the user may have
/// just said it isn't, forgotten their own game, or turned the launchers'
/// lists off.
fn still_a_game(game: &Detected, s: &GamesSettings) -> bool {
    match mark_for(&s.marks, &game.key) {
        Some(m) => m.game,
        None => game.source != GameSource::Mine && s.launchers,
    }
}

/// Whether a game known only from the history stays in the list of games:
/// not once the user said it isn't one, nor their own game once forgotten.
fn still_listed(marks: &[GameMark], r: &GameRecord) -> bool {
    match mark_for(marks, &r.key) {
        Some(m) => m.game,
        None => r.source != GameSource::Mine,
    }
}

/// The id a launcher's game is kept under: its folder, else its program.
fn key_of(g: &KnownGame) -> String {
    g.dir.as_ref().or(g.exes.first()).map(|p| p.to_lowercase()).unwrap_or_default()
}

// ------------------------------------------------------------- detection

/// A game in front.
#[derive(Debug, Clone, PartialEq)]
pub struct Detected {
    pub key: String,
    pub name: String,
    pub source: GameSource,
    pub pid: u32,
    /// Executable file name.
    pub exe: String,
    /// The program's full path.
    pub path: String,
    pub steam_app: Option<u32>,
}

/// The game the program in front is, if any: the user's word first, then
/// the launchers' lists.
pub fn classify(front: &Front, marks: &[GameMark], known: &[KnownGame], launchers: bool) -> Option<Detected> {
    if front.is_shell() {
        return None;
    }
    let path = front.path.as_deref()?;
    if let Some(m) = mark_for(marks, path) {
        return m.game.then(|| Detected {
            key: path.to_lowercase(),
            name: m.name.clone(),
            source: GameSource::Mine,
            pid: front.pid,
            exe: front.name.clone(),
            path: path.to_string(),
            steam_app: None,
        });
    }
    if !launchers {
        return None;
    }
    let g = library::find(known, path)?;
    Some(Detected {
        key: key_of(g),
        name: g.name.clone(),
        source: source_of(g.source),
        pid: front.pid,
        exe: front.name.clone(),
        path: path.to_string(),
        steam_app: g.steam_app,
    })
}

// ------------------------------------------------------------- session

/// Readings over a session.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stats {
    gpu_sum: f64,
    gpu_n: u32,
    pub gpu_peak: Option<f32>,
    cpu_sum: f64,
    cpu_n: u32,
    pub cpu_peak: Option<f32>,
    pub temp_peak: Option<f32>,
}

fn peak(old: Option<f32>, new: f32) -> Option<f32> {
    Some(old.map_or(new, |o| o.max(new)))
}

impl Stats {
    pub fn add(&mut self, gpu: Option<f32>, cpu: Option<f32>, temp: Option<f32>) {
        if let Some(g) = gpu {
            self.gpu_sum += f64::from(g);
            self.gpu_n += 1;
            self.gpu_peak = peak(self.gpu_peak, g);
        }
        if let Some(c) = cpu {
            self.cpu_sum += f64::from(c);
            self.cpu_n += 1;
            self.cpu_peak = peak(self.cpu_peak, c);
        }
        if let Some(t) = temp {
            self.temp_peak = peak(self.temp_peak, t);
        }
    }

    pub fn gpu_avg(&self) -> Option<f32> {
        (self.gpu_n > 0).then(|| (self.gpu_sum / f64::from(self.gpu_n)) as f32)
    }

    pub fn cpu_avg(&self) -> Option<f32> {
        (self.cpu_n > 0).then(|| (self.cpu_sum / f64::from(self.cpu_n)) as f32)
    }
}

/// A session that runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Running {
    pub game: Detected,
    pub started: f64,
    missing_since: Option<f64>,
    pub stats: Stats,
}

impl Running {
    fn record(&self, end: f64) -> GameRecord {
        GameRecord {
            key: self.game.key.clone(),
            name: self.game.name.clone(),
            source: self.game.source,
            start_ms: self.started,
            end_ms: end,
            gpu_avg: self.stats.gpu_avg(),
            gpu_peak: self.stats.gpu_peak,
            cpu_avg: self.stats.cpu_avg(),
            cpu_peak: self.stats.cpu_peak,
            temp_peak: self.stats.temp_peak,
            lol: None,
        }
    }
}

/// What a step of the tracker did.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Started,
    Ended(GameRecord),
}

/// Starts and ends sessions from the game in front and the running
/// programs (no Windows calls: testable).
#[derive(Debug, Default)]
pub struct Tracker {
    candidate: Option<(Detected, f64)>,
    pub session: Option<Running>,
}

impl Tracker {
    /// The program in front changed: `game` is what it is.
    pub fn front(&mut self, now: f64, game: Option<Detected>) {
        match game {
            Some(g) if self.session.as_ref().is_some_and(|s| s.game.key == g.key) => {
                // Back to the game of the session (it may have restarted).
                if let Some(s) = &mut self.session {
                    s.game.pid = g.pid;
                    s.missing_since = None;
                }
                self.candidate = None;
            }
            Some(g) => {
                if self.candidate.as_ref().is_none_or(|(c, _)| c.key != g.key) {
                    self.candidate = Some((g, now));
                }
            }
            None => self.candidate = None,
        }
    }

    /// A step: start the game that has been in front long enough, end the
    /// session whose program is gone. `procs`: the running programs, when
    /// they were looked at in this step.
    pub fn tick(&mut self, now: f64, procs: Option<&[(u32, String)]>) -> Option<Change> {
        if let Some((g, since)) = &self.candidate {
            if now - since >= START_AFTER_MS {
                if let Some(s) = self.session.take() {
                    // Another game took over: this one ended when that one
                    // came to the front (no time counted twice).
                    return Some(Change::Ended(s.record(since.max(s.started))));
                }
                self.session =
                    Some(Running { game: g.clone(), started: *since, missing_since: None, stats: Stats::default() });
                self.candidate = None;
                return Some(Change::Started);
            }
        }
        let s = self.session.as_mut()?;
        if let Some(procs) = procs {
            let same = procs.iter().any(|(p, n)| *p == s.game.pid && n.eq_ignore_ascii_case(&s.game.exe));
            let again = || procs.iter().find(|(_, n)| n.eq_ignore_ascii_case(&s.game.exe)).map(|(p, _)| *p);
            if same {
                s.missing_since = None;
            } else if let Some(pid) = again() {
                // Restarted itself under a new process id.
                s.game.pid = pid;
                s.missing_since = None;
            } else {
                let since = *s.missing_since.get_or_insert(now);
                if now - since >= GONE_AFTER_MS {
                    let record = s.record(since);
                    self.session = None;
                    return Some(Change::Ended(record));
                }
            }
        }
        None
    }

    /// Finish the session now (the feature was turned off).
    pub fn stop(&mut self, now: f64) -> Option<GameRecord> {
        self.candidate = None;
        self.session.take().map(|s| s.record(now))
    }
}

// ------------------------------------------------------------- history

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct History {
    records: Vec<GameRecord>,
}

/// Start of today, local time, in ms since the epoch (a day ago when a
/// clock change skips midnight).
fn day_start(now: f64) -> f64 {
    use jiff::tz::AmbiguousOffset;
    let tz = crate::zones::pc_zone();
    let Ok(t) = jiff::Timestamp::from_millisecond(now as i64) else { return now - DAY_MS };
    let midnight = tz.to_ambiguous_timestamp(tz.to_datetime(t).date().to_datetime(jiff::civil::Time::midnight()));
    match midnight.offset() {
        AmbiguousOffset::Gap { .. } => now - DAY_MS,
        _ => midnight.earlier().map_or(now - DAY_MS, |m| m.as_millisecond() as f64),
    }
}

/// Time played within [from, to], the running session (started at
/// `running`) included.
pub fn played_between(records: &[GameRecord], running: Option<f64>, from: f64, to: f64) -> f64 {
    let overlap = |a: f64, b: f64| (b.min(to) - a.max(from)).max(0.0);
    records.iter().map(|r| overlap(r.start_ms, r.end_ms)).sum::<f64>() + running.map_or(0.0, |s| overlap(s, to))
}

// ------------------------------------------------------------- lights

/// Keys of the model on the cable: id → LED, and every LED.
#[derive(Debug, Clone, Default)]
pub(crate) struct KeyIds {
    pub(crate) all: Vec<u8>,
    pub(crate) by_id: HashMap<String, u8>,
    /// Where each LED sits from left (0) to right (1), for waves.
    pub(crate) x: HashMap<u8, f32>,
    /// … and from the top (0) to the bottom (1).
    pub(crate) y: HashMap<u8, f32>,
}

impl KeyIds {
    pub(crate) fn of(model: &Model) -> Self {
        let Some(g) = &model.geometry else { return Self::default() };
        let width = g.width.max(1.0);
        let height = g.height.max(1.0);
        Self {
            all: g.keys.iter().map(|k| k.led).collect(),
            by_id: g.keys.iter().map(|k| (k.id.clone(), k.led)).collect(),
            x: g.keys.iter().map(|k| (k.led, ((k.x + k.w / 2.0) / width).clamp(0.0, 1.0))).collect(),
            y: g.keys.iter().map(|k| (k.led, ((k.y + k.h / 2.0) / height).clamp(0.0, 1.0))).collect(),
        }
    }
}

fn scale(c: Rgb, k: f32) -> Rgb {
    c.map(|v| (f32::from(v) * k.clamp(0.0, 1.0)).round() as u8)
}

/// The chosen keys in their colour, the others dark or dim.
pub(crate) fn keys_frame(ids: &KeyIds, s: &GamesSettings) -> Vec<(u8, Rgb)> {
    let rest = match s.rest {
        GameRest::Dark => [0, 0, 0],
        GameRest::Dim => scale(s.color, DIM),
    };
    let lit: Vec<u8> = s.keys.iter().filter_map(|k| ids.by_id.get(k).copied()).collect();
    ids.all.iter().map(|&led| (led, if lit.contains(&led) { s.color } else { rest })).collect()
}

/// Quiet mode for the game with this key: for every game, or for this one.
fn quiet_for(s: &GamesSettings, key: &str) -> bool {
    s.quiet || s.quiet_games.iter().any(|k| same_path(k, key))
}

/// A profile's per-key design on every key of the model: its keys lit,
/// the rest off.
fn profile_frame(ids: &KeyIds, design: &[(u8, Rgb)]) -> Vec<(u8, Rgb)> {
    ids.all.iter().map(|&led| (led, design.iter().find(|k| k.0 == led).map_or([0, 0, 0], |k| k.1))).collect()
}

/// The program in front is the session's game (its process, or a program
/// of its folder: a launcher game's other windows).
fn plays(front: &Front, game: &Detected) -> bool {
    front.pid == game.pid || front.path.as_deref().is_some_and(|p| covered(p, &game.key))
}

// ------------------------------------------------------------- worker

type Frame = Option<(Vec<(u8, Rgb)>, u8)>;
/// A profile's per-key design: its model, the lit keys, their brightness.
type Design = (String, Vec<(u8, Rgb)>, u8);

struct Inner {
    settings: GamesSettings,
    history: Vec<GameRecord>,
    known: Arc<Vec<KnownGame>>,
    next_scan: f64,
    tracker: Tracker,
    /// The latest program in front, not looked at yet.
    front: Option<Front>,
    /// The program in front that covers the screen and might be a game:
    /// (path, name, since).
    suggest: Option<(String, String, f64)>,
    suggestions: Vec<GameSuggestion>,
    next_sample: f64,
    next_alive: f64,
    next_emit: f64,
    /// The last game layer handed to the device.
    sent: Frame,
    /// The device was told the game layer is the user's own try.
    tried_sent: bool,
    trying_until: Option<f64>,
    /// "Try on the keyboard" in the League of Legends section: the scene
    /// and when it began.
    trying_lol: Option<(LolScene, f64)>,
    model: Option<(String, Arc<KeyIds>)>,
    /// A temperature was read on this computer.
    temperature: bool,
    last_gpu: Option<f32>,
    /// League of Legends' data was read during this session (else its
    /// result is an older match's).
    lol_seen: bool,
    /// Programs lately in front that aren't games (for adding one by hand).
    programs: Vec<GameSuggestion>,
    /// When the last pass ran: a long gap means the computer slept.
    last_tick: f64,
    /// What the device and the live screen were told about leaving the
    /// keyboard alone.
    quiet_sent: Quiet,
    /// The session's game is not in front (Alt+Tab to other work).
    away: bool,
    dirty: bool,
}

enum Msg {
    Wake,
}

/// Handle to the game session worker (managed Tauri state).
pub struct GamesHandle {
    tx: Sender<Msg>,
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    config_dir: Option<PathBuf>,
    live: Arc<LiveHandle>,
    hub: Arc<FrontHub>,
    lol: Arc<crate::lol::LolHandle>,
    /// The last state's number (`GamesState::rev`).
    rev: AtomicU32,
}

/// Meters live on the worker thread (their handles stay on one thread).
struct Meters {
    cpu: CpuMeter,
    gpu: Option<GpuMeter>,
    gpu_tried: bool,
}

impl GamesHandle {
    pub fn spawn(
        app: AppHandle,
        live: Arc<LiveHandle>,
        device: Arc<DeviceHandle>,
        hub: Arc<FrontHub>,
        config_dir: Option<PathBuf>,
    ) -> Arc<Self> {
        let settings = config_dir
            .as_deref()
            .and_then(|d| settings::load::<GamesSettings>(d, FILE, VERSION))
            .unwrap_or_default()
            .sanitized();
        let history = config_dir
            .as_deref()
            .and_then(|d| settings::load::<History>(d, HISTORY_FILE, HISTORY_VERSION))
            .unwrap_or_default()
            .records;
        if settings.enabled {
            tracing::info!("game session on");
        }
        let inner = Arc::new(Mutex::new(Inner {
            settings,
            history,
            known: Arc::new(Vec::new()),
            next_scan: 0.0,
            tracker: Tracker::default(),
            front: None,
            suggest: None,
            suggestions: Vec::new(),
            next_sample: 0.0,
            next_alive: 0.0,
            next_emit: 0.0,
            sent: None,
            tried_sent: false,
            trying_until: None,
            trying_lol: None,
            model: None,
            temperature: false,
            last_gpu: None,
            lol_seen: false,
            programs: Vec::new(),
            last_tick: 0.0,
            quiet_sent: Quiet::Off,
            away: false,
            dirty: true,
        }));
        let (tx, rx) = mpsc::channel();
        let handle = Arc::new(Self {
            tx: tx.clone(),
            inner: inner.clone(),
            app,
            config_dir,
            live,
            hub: hub.clone(),
            lol: crate::lol::LolHandle::new(),
            rev: AtomicU32::new(0),
        });
        hub.listen(move |front| {
            if let Ok(mut i) = inner.lock() {
                i.front = Some(front.clone());
            }
            let _ = tx.send(Msg::Wake);
        });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("games".into())
            .spawn(move || {
                let mut meters = Meters { cpu: CpuMeter::new(), gpu: None, gpu_tried: false };
                loop {
                    let wait = worker.tick(&device, &mut meters);
                    match rx.recv_timeout(wait) {
                        Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("spawn games thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    /// A session runs now ("update by itself" waits for it to end).
    pub fn in_session(&self) -> bool {
        self.inner.lock().is_ok_and(|i| i.tracker.session.is_some())
    }

    /// The game a program is, as the session would see it (per-app
    /// profiles' "every game" rule): its key.
    pub fn game_of(&self, front: &Front) -> Option<String> {
        let i = self.inner.lock().ok()?;
        classify(front, &i.settings.marks, &i.known, i.settings.launchers).map(|d| d.key)
    }

    /// The key of the game whose session runs.
    pub fn session_key(&self) -> Option<String> {
        let i = self.inner.lock().ok()?;
        i.tracker.session.as_ref().map(|s| s.game.key.clone())
    }

    /// A session runs and its game is in front, as for the game's own
    /// lights: the desktop and Alt+Tab do not count as leaving it (the
    /// music lights' "pause while playing a game").
    pub fn playing_in_front(&self) -> bool {
        self.inner.lock().is_ok_and(|i| i.tracker.session.is_some() && !i.away)
    }

    /// A session runs with "leave the keyboard alone while playing" on (for
    /// every game, or for its game).
    pub fn quiet_now(&self) -> bool {
        self.inner.lock().is_ok_and(|i| {
            i.settings.enabled && i.tracker.session.as_ref().is_some_and(|r| quiet_for(&i.settings, &r.game.key))
        })
    }

    /// Game `key` gets quiet mode when it plays (the per-app profiles leave
    /// it alone from the moment it comes in front).
    pub fn quiet_game(&self, key: &str) -> bool {
        self.inner.lock().is_ok_and(|i| i.settings.enabled && quiet_for(&i.settings, key))
    }

    /// The game of the running session and when it started (tray menu).
    pub fn session(&self) -> Option<(String, f64)> {
        let i = self.inner.lock().ok()?;
        i.tracker.session.as_ref().map(|s| (s.game.name.clone(), s.started))
    }

    pub fn state(&self) -> Result<GamesState, String> {
        let inner = self.inner.lock().map_err(|_| "games unavailable")?;
        Ok(self.state_of(&inner, now_ms()))
    }

    fn state_of(&self, inner: &Inner, now: f64) -> GamesState {
        let running = inner.tracker.session.as_ref();
        let started = running.map(|s| s.started);
        let mut totals: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for r in &inner.history {
            let t = totals.entry(r.key.as_str()).or_default();
            t.0 += (r.end_ms - r.start_ms).max(0.0);
            t.1 = t.1.max(r.end_ms);
        }
        let mut games: Vec<GameEntry> = Vec::new();
        let mut push = |key: String, name: String, source: GameSource, exe: Option<String>| {
            if games.iter().any(|g| g.key == key) {
                return;
            }
            // The running game's own file is the surest.
            let exe = running.filter(|s| s.game.key == key).map(|s| s.game.path.clone()).or(exe);
            let (total, last) = totals.get(key.as_str()).copied().unwrap_or_default();
            let live = running.filter(|s| s.game.key == key).map_or(0.0, |s| (now - s.started).max(0.0));
            games.push(GameEntry {
                total_ms: total + live,
                last_ms: (last > 0.0).then_some(last),
                played: total + live > 0.0,
                key,
                name,
                source,
                exe,
            });
        };
        for m in inner.settings.marks.iter().filter(|m| m.game) {
            push(m.path.to_lowercase(), m.name.clone(), GameSource::Mine, Some(m.path.clone()));
        }
        if inner.settings.launchers {
            for g in inner.known.iter() {
                let key = key_of(g);
                // The user said it is not a game.
                if mark_for(&inner.settings.marks, &key).is_some_and(|m| !m.game) {
                    continue;
                }
                push(key, g.name.clone(), source_of(g.source), g.exes.first().cloned());
            }
        }
        // Games played before that no list has any more.
        for r in inner.history.iter().filter(|r| still_listed(&inner.settings.marks, r)) {
            // A key that names a program (not a folder) is its file.
            let exe = r.key.to_lowercase().ends_with(".exe").then(|| r.key.clone());
            push(r.key.clone(), r.name.clone(), r.source, exe);
        }
        games.sort_by(|a, b| {
            b.last_ms
                .unwrap_or(0.0)
                .total_cmp(&a.last_ms.unwrap_or(0.0))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let mut recent: Vec<GameRecord> = inner.history.iter().rev().take(RECENT).cloned().collect();
        recent.sort_by(|a, b| b.end_ms.total_cmp(&a.end_ms));
        GamesState {
            // Made under the lock, so the numbers follow the changes.
            rev: self.rev.fetch_add(1, Ordering::Relaxed).wrapping_add(1),
            settings: inner.settings.clone(),
            session: running.map(|s| {
                let quiet = quiet_for(&inner.settings, &s.game.key);
                GameSessionInfo {
                    key: s.game.key.clone(),
                    name: s.game.name.clone(),
                    source: s.game.source,
                    started_ms: s.started,
                    gpu_avg: s.stats.gpu_avg(),
                    gpu_peak: s.stats.gpu_peak,
                    cpu_avg: s.stats.cpu_avg(),
                    cpu_peak: s.stats.cpu_peak,
                    temp_peak: s.stats.temp_peak,
                    quiet,
                    card: self.card_status(&inner.settings, quiet),
                }
            }),
            games,
            suggestions: inner.suggestions.clone(),
            today_ms: played_between(&inner.history, started, day_start(now), now),
            week_ms: played_between(&inner.history, started, now - 7.0 * DAY_MS, now),
            recent,
            temperature: inner.temperature,
            programs: inner
                .programs
                .iter()
                .filter(|p| mark_for(&inner.settings.marks, &p.path).is_none())
                .cloned()
                .collect(),
        }
    }

    fn emit(&self) {
        if let Ok(s) = self.state() {
            let _ = self.app.emit(EVENT_GAMES, s);
        }
    }

    fn save_settings(&self, settings: &GamesSettings) -> Result<(), String> {
        match &self.config_dir {
            Some(dir) => settings::save(dir, FILE, VERSION, settings).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }

    fn save_history(&self, history: &[GameRecord]) {
        if let Some(dir) = &self.config_dir {
            let h = History { records: history.to_vec() };
            if let Err(e) = settings::save(dir, HISTORY_FILE, HISTORY_VERSION, h) {
                tracing::warn!("game history not saved: {e}");
            }
        }
    }

    pub fn configure(&self, settings: GamesSettings) -> Result<GamesState, String> {
        let settings = settings.sanitized();
        {
            let mut inner = self.inner.lock().map_err(|_| "games unavailable")?;
            if inner.settings.enabled != settings.enabled {
                tracing::info!(on = settings.enabled, "game session");
            }
            if inner.settings.launchers != settings.launchers {
                inner.next_scan = 0.0;
            }
            inner.settings = settings.clone();
            inner.dirty = true;
        }
        self.save_settings(&settings)?;
        self.wake();
        self.state()
    }

    /// The user's word on a program: a game (`Some(true)`), not a game
    /// (`Some(false)`), or forget it (`None`).
    pub fn mark(&self, path: String, name: String, game: Option<bool>) -> Result<GamesState, String> {
        let settings = {
            let mut inner = self.inner.lock().map_err(|_| "games unavailable")?;
            inner.settings.marks.retain(|m| !same_path(&m.path, &path));
            if let Some(game) = game {
                let name =
                    if name.trim().is_empty() { kb_winsys::foreground::file_name(&path).to_string() } else { name };
                inner.settings.marks.push(GameMark { path: path.clone(), name, game });
            }
            inner.suggestions.retain(|s| !same_path(&s.path, &path));
            inner.settings = inner.settings.clone().sanitized();
            inner.dirty = true;
            // The program in front may be the one just marked.
            if inner.front.is_none() {
                inner.front = self.hub.last();
            }
            inner.settings.clone()
        };
        tracing::info!(?game, "program marked");
        self.save_settings(&settings)?;
        self.wake();
        self.state()
    }

    pub fn clear_history(&self) -> Result<GamesState, String> {
        {
            let mut inner = self.inner.lock().map_err(|_| "games unavailable")?;
            inner.history.clear();
            inner.dirty = true;
        }
        self.save_history(&[]);
        tracing::info!("game history cleared");
        self.emit();
        self.state()
    }

    /// Show the game lights on the keyboard for a moment.
    pub fn try_lights(&self) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().map_err(|_| "games unavailable")?;
            inner.trying_until = Some(now_ms() + TRY_MS);
        }
        self.wake();
        Ok(())
    }

    /// Play a League of Legends scene on the keyboard (RAM only).
    pub fn try_lol(&self, scene: LolScene) -> Result<(), String> {
        {
            let mut inner = self.inner.lock().map_err(|_| "games unavailable")?;
            inner.trying_lol = Some((scene, now_ms()));
            inner.trying_until = None;
        }
        self.wake();
        Ok(())
    }

    /// Every frame of a League of Legends scene for the page's drawing, on
    /// keyboard model `model_id`.
    pub fn lol_preview(
        model_id: &str,
        settings: &GamesSettings,
        scene: LolScene,
    ) -> Result<Vec<crate::lol::Keys>, String> {
        let model = kb_devices::catalogue()
            .map_err(|e| e.to_string())?
            .iter()
            .find(|m| m.id == model_id)
            .ok_or("unknown keyboard model")?;
        Ok(crate::lol::preview(&KeyIds::of(model), settings, scene))
    }

    /// The League of Legends scene being tried, `now`; `None` when none
    /// is (or it is over).
    fn trying_lol(inner: &mut Inner, ids: Option<&KeyIds>, now: f64) -> Option<Vec<(u8, Rgb)>> {
        let frame = match (inner.trying_lol, ids) {
            (Some((scene, start)), Some(ids)) => crate::lol::scene_frame(ids, &inner.settings, scene, now - start),
            _ => None,
        };
        if frame.is_none() {
            inner.trying_lol = None;
        }
        frame
    }

    /// Read the launchers' lists again now.
    pub fn rescan(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.next_scan = 0.0;
        }
        self.wake();
    }

    /// Steam's picture of a game, for the start card.
    pub fn cover(app: u32) -> Option<Vec<u8>> {
        let path = library::steam_cover(app)?;
        let meta = std::fs::metadata(&path).ok()?;
        // A cover is a few hundred kB at most.
        (meta.len() <= 4 * 1024 * 1024).then(|| std::fs::read(path).ok()).flatten()
    }

    /// Sources ranked above the game hold the screen.
    /// Where the running session's start card is.
    fn card_status(&self, s: &GamesSettings, quiet: bool) -> GameCardStatus {
        if quiet {
            GameCardStatus::Quiet
        } else if !s.start_card || !self.live.enabled() {
            GameCardStatus::Off
        } else if self.live.card_shown(SOURCE) {
            GameCardStatus::Shown
        } else if self.live.card_waiting(SOURCE) {
            GameCardStatus::Waiting
        } else {
            GameCardStatus::Other
        }
    }

    /// The design of profile `id` for `GameLights::Profile`: its model, the
    /// lit keys and their brightness. Read outside the worker's lock.
    fn profile_design(&self, id: &str) -> Option<Design> {
        let store = self.app.try_state::<Arc<Mutex<crate::profiles::ProfileStore>>>()?;
        let profile = store.lock().ok()?.get(id)?;
        let (keys, brightness) = profile.lighting.as_ref()?.per_key()?;
        Some((profile.model_id, keys, brightness))
    }

    fn higher_holds(&self) -> bool {
        self.live.held_by(crate::calendar::SOURCE)
            || self.live.held_by(crate::reminders::SOURCE)
            || self.live.held_by(crate::pomodoro::SOURCE)
    }

    fn start_card(r: &Running, today: f64) -> GameCard {
        GameCard {
            kind: GameCardKind::Start,
            name: r.game.name.clone(),
            steam_app: r.game.steam_app,
            exe: Some(r.game.path.clone()),
            started_ms: r.started,
            ended_ms: None,
            gpu_avg: None,
            gpu_peak: None,
            cpu_avg: None,
            cpu_peak: None,
            temp_peak: None,
            today_ms: today,
            lol: None,
        }
    }

    fn summary_card(r: &GameRecord, today: f64) -> GameCard {
        GameCard {
            kind: GameCardKind::Summary,
            name: r.name.clone(),
            steam_app: None,
            exe: None,
            started_ms: r.start_ms,
            ended_ms: Some(r.end_ms),
            gpu_avg: r.gpu_avg,
            gpu_peak: r.gpu_peak,
            cpu_avg: r.cpu_avg,
            cpu_peak: r.cpu_peak,
            temp_peak: r.temp_peak,
            today_ms: today,
            lol: r.lol.clone(),
        }
    }

    /// One pass of the worker; returns how long to sleep.
    fn tick(&self, device: &DeviceHandle, meters: &mut Meters) -> Duration {
        let now = now_ms();
        let (enabled, launchers, scan_due, lights_profile) = match self.inner.lock() {
            Ok(i) => (
                i.settings.enabled,
                i.settings.launchers,
                now >= i.next_scan,
                i.settings.lights_profile.clone().filter(|_| i.settings.lights == GameLights::Profile),
            ),
            Err(_) => return IDLE,
        };
        self.hub.want(WATCHER, enabled);
        if !enabled {
            return self.idle(device, now);
        }
        let in_front = self.hub.last();
        // The profile store's lock is not taken under this worker's.
        let design = lights_profile.and_then(|id| self.profile_design(&id));
        // The launchers' lists: read without holding the lock (it takes a moment).
        if scan_due {
            let known = if launchers { library::installed() } else { Vec::new() };
            tracing::debug!(games = known.len(), "launchers' games read");
            if let Ok(mut i) = self.inner.lock() {
                i.known = Arc::new(known);
                i.next_scan = now + RESCAN_MS;
                i.dirty = true;
                // A game just installed may be in front already.
                if i.front.is_none() {
                    i.front = in_front.clone();
                }
            }
        }
        let (in_session, suggesting, sample_due, alive_due, ids) = match self.inner.lock() {
            Ok(i) => (
                i.tracker.session.is_some(),
                i.suggest.is_some(),
                now >= i.next_sample,
                now >= i.next_alive,
                i.tracker.session.as_ref().map(|s| (s.game.pid, s.game.exe.clone(), s.game.key.clone())),
            ),
            Err(_) => return IDLE,
        };
        // Readings, outside the lock too.
        let mut reading = None;
        if (in_session || suggesting) && sample_due {
            if !meters.gpu_tried {
                meters.gpu_tried = true;
                meters.gpu = GpuMeter::new();
            }
            let gpu = meters.gpu.as_mut().and_then(GpuMeter::sample);
            let cpu = meters.cpu.sample();
            let temp = in_session.then(kb_winsys::thermal::gpu_temperature).flatten();
            reading = Some((gpu, cpu, temp));
        }
        let procs = (in_session && alive_due).then(|| {
            let mut procs = process_names();
            if let Some((pid, exe, key)) = &ids {
                // Another program with the same file name (python.exe,
                // Game.exe) is not this game started again.
                procs.retain(|(p, n)| {
                    *p == *pid
                        || !n.eq_ignore_ascii_case(exe)
                        || kb_winsys::foreground::image_path(*p).is_some_and(|path| covered(&path, key))
                });
            }
            procs
        });

        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        let s = inner.settings.clone();
        let own_pid = std::process::id();
        let live_on = self.live.enabled();

        // The computer slept: a session ends where it stopped, and what is in
        // front is looked at again.
        let before = std::mem::replace(&mut inner.last_tick, now);
        if before > 0.0 && now - before > SLEPT_MS {
            tracing::info!(minutes = ((now - before) / 60_000.0).round(), "the computer slept");
            if let Some(record) = inner.tracker.stop(before) {
                let start = record.start_ms;
                self.keep_record(inner, record);
                self.let_go(start);
            }
            inner.suggest = None;
            if inner.front.is_none() {
                inner.front = in_front.clone();
            }
        }

        if let Some(front) = inner.front.take() {
            let game = if front.pid == own_pid { None } else { classify(&front, &s.marks, &inner.known, s.launchers) };
            // Kept for "Add a game": programs with a path that aren't games.
            if let (None, Some(path)) = (&game, &front.path) {
                if !front.is_shell() && front.pid != own_pid && !not_a_game(&front.name) {
                    inner.programs.retain(|p| !same_path(&p.path, path));
                    let name = front.name.rsplit_once('.').map_or(front.name.as_str(), |(stem, _)| stem).to_string();
                    inner.programs.insert(0, GameSuggestion { path: path.clone(), name });
                    inner.programs.truncate(PROGRAMS_MAX);
                    inner.dirty = true;
                }
            }
            // A full-screen program that is not known: maybe a game.
            inner.suggest = match (&game, &front.path) {
                (None, Some(path))
                    if front.covers_monitor
                        && !front.is_shell()
                        && front.pid != own_pid
                        && !not_a_game(&front.name)
                        && mark_for(&s.marks, path).is_none()
                        && !inner.suggestions.iter().any(|x| same_path(&x.path, path)) =>
                {
                    match &inner.suggest {
                        Some((p, n, since)) if same_path(p, path) => Some((p.clone(), n.clone(), *since)),
                        _ => Some((path.clone(), front.name.clone(), now)),
                    }
                }
                _ => None,
            };
            inner.tracker.front(now, game);
        }
        if let Some((gpu, cpu, temp)) = reading {
            inner.next_sample = now + SAMPLE_MS;
            inner.last_gpu = gpu;
            if temp.is_some() && !inner.temperature {
                inner.temperature = true;
                inner.dirty = true;
            }
            if let Some(r) = &mut inner.tracker.session {
                r.stats.add(gpu, cpu, temp);
            }
        }
        if procs.is_some() {
            inner.next_alive = now + ALIVE_MS;
        }
        // "Is this a game?" after two busy minutes.
        if let Some((path, name, since)) = inner.suggest.clone() {
            if now - since >= SUGGEST_AFTER_MS && inner.last_gpu.is_some_and(|g| g >= SUGGEST_GPU) {
                tracing::info!("a full-screen program offered as a game");
                let name = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem).to_string();
                inner.suggestions.push(GameSuggestion { path, name });
                inner.suggest = None;
                inner.dirty = true;
            }
        }

        // The session's game isn't one any more: it ends without a record.
        if let Some(start) = inner.tracker.session.as_ref().filter(|r| !still_a_game(&r.game, &s)).map(|r| r.started) {
            inner.tracker.stop(now);
            tracing::info!("game session dropped: not a game any more");
            inner.dirty = true;
            self.let_go(start);
        }

        let change = inner.tracker.tick(now, procs.as_deref());
        match &change {
            Some(Change::Started) => {
                inner.dirty = true;
                inner.lol_seen = false;
                let r = inner.tracker.session.clone().expect("a session just started");
                tracing::info!(source = ?r.game.source, "game session started");
                let today = played_between(&inner.history, Some(r.started), day_start(now), now);
                if s.start_card && live_on && !quiet_for(&s, &r.game.key) && !self.higher_holds() {
                    let _ = self.app.emit(EVENT_GAME_CARD, Self::start_card(&r, today));
                }
            }
            Some(Change::Ended(record)) => {
                let record = &self.keep_record(inner, record.clone());
                let minutes = ((record.end_ms - record.start_ms) / 60_000.0).round();
                tracing::info!(minutes, "game session ended");
                let long = record.end_ms - record.start_ms >= SUMMARY_MIN_MS;
                self.live.set_hold(SOURCE, false);
                if s.summary_card && long && live_on && !quiet_for(&s, &record.key) && !self.higher_holds() {
                    let today = played_between(&inner.history, None, day_start(now), now);
                    let _ = self.app.emit(EVENT_GAME_CARD, Self::summary_card(record, today));
                    // The summary replaces the start card, but may wait for
                    // the budget: a meeting's card left meanwhile goes.
                    self.live.back_when_free(STALE_CARD_BACK, record.start_ms, SOURCE);
                } else {
                    // The start card is old news now.
                    self.let_go(record.start_ms);
                }
            }
            None => {}
        }
        let in_session = inner.tracker.session.is_some();
        // The lights follow the window: away from the game its keys and the
        // match's lights wait. The desktop and Alt+Tab change nothing.
        match (&inner.tracker.session, &in_front) {
            (Some(r), Some(f)) if !f.is_shell() => inner.away = !plays(f, &r.game),
            (None, _) => inner.away = false,
            _ => {}
        }
        let here = !inner.away;
        let quiet_game = inner.tracker.session.as_ref().is_some_and(|r| quiet_for(&s, &r.game.key));
        self.live.set_hold(SOURCE, in_session && live_on && !quiet_game && (s.start_card || s.summary_card));
        // "Leave the keyboard alone while playing": every light layer and
        // card waits, not only the game's.
        let quiet = match (quiet_game, s.quiet_reminders) {
            (false, _) => Quiet::Off,
            (true, true) => Quiet::ButReminders,
            (true, false) => Quiet::All,
        };
        self.set_quiet(inner, device, quiet);

        // Lights.
        let trying = inner.trying_until.is_some_and(|t| now < t);
        if !trying {
            inner.trying_until = None;
        }
        let keys = self.usb_keys(inner, device);
        // League of Legends: the match's own lights, while its data answers.
        let lol_on = in_session
            && !quiet_game
            && s.lol.enabled
            && inner.tracker.session.as_ref().is_some_and(|r| is_lol(&r.game.key));
        self.lol.want(lol_on);
        inner.lol_seen |= lol_on;
        let lol = lol_on.then(|| self.lol.snapshot()).flatten().filter(|l| l.lights());
        let playing = in_session && here && !quiet_game;
        let show = trying || (playing && s.lights == GameLights::Keys);
        // The profile's design, when it was made for this keyboard.
        let model = inner.model.as_ref().map(|(id, _)| id.as_str());
        let design = design.filter(|(m, _, _)| playing && (m.is_empty() || Some(m.as_str()) == model));
        let tried = Self::trying_lol(inner, keys.as_deref(), now);
        let trying_lol = tried.is_some();
        // The match's lights, when they can show.
        let lol_shows = !trying_lol && !trying && here && keys.is_some() && lol.is_some();
        let frame: Frame = match (&keys, &lol, show, &design) {
            _ if trying_lol => tried.map(|f| (f, s.brightness)),
            (Some(k), Some(l), _, _) if lol_shows => Some((crate::lol::frame(l, k, &s, now), s.brightness)),
            (Some(k), _, true, _) => Some((keys_frame(k, &s), s.brightness)),
            (Some(k), _, false, Some((_, d, b))) if !trying => Some((profile_frame(k, d), *b)),
            _ => None,
        };
        Self::send_layer(inner, device, frame, trying || trying_lol);

        if in_session && now >= inner.next_emit {
            inner.dirty = true;
        }
        if inner.dirty {
            inner.dirty = false;
            inner.next_emit = now + EMIT_MS;
            let state = self.state_of(inner, now);
            drop(guard);
            let _ = self.app.emit(EVENT_GAMES, state);
        }
        // The match's lights move (bars, flashes): 20 frames a second, but
        // not while nothing can show (no keyboard on the cable, Teamfight
        // Tactics, the loading screen).
        if lol_shows || trying_lol {
            LOL_TICK
        } else {
            TICK
        }
    }

    /// A session that ended goes into the history (with its League of
    /// Legends result when the match's data was read in it).
    fn keep_record(&self, inner: &mut Inner, mut record: GameRecord) -> GameRecord {
        if is_lol(&record.key) && std::mem::take(&mut inner.lol_seen) {
            record.lol = self.lol.snapshot().and_then(|l| l.result());
        }
        inner.history.push(record.clone());
        if inner.history.len() > HISTORY_MAX {
            let drop = inner.history.len() - HISTORY_MAX;
            inner.history.drain(..drop);
        }
        self.save_history(&inner.history);
        inner.dirty = true;
        record
    }

    /// The session that started at `start` lets go of the screen: its
    /// start card goes back to the user's own picture, and so does a card
    /// another source left there during the game (a meeting's, a
    /// reminder's). A card from before the game stays.
    fn let_go(&self, start: f64) {
        self.live.set_hold(SOURCE, false);
        self.live.drop_pending(SOURCE);
        self.live.return_after(SOURCE, STALE_CARD_BACK);
        self.live.back_when_free(STALE_CARD_BACK, start, SOURCE);
    }

    /// Hand the game layer to the device when it changed. `tried`: it is the
    /// user's own try, which shows even while a game wants quiet.
    fn send_layer(inner: &mut Inner, device: &DeviceHandle, frame: Frame, tried: bool) {
        if inner.tried_sent != tried {
            inner.tried_sent = tried;
            let _ = device.send(Request::SetGameTried(tried));
        }
        if frame != inner.sent {
            let _ = device.send(Request::SetLayer(
                LightLayer::Game,
                frame.clone().map(|(colors, b)| LiveFrame { colors, level: level_factor(b) }),
            ));
            inner.sent = frame;
        }
    }

    /// Tell the device and the live screen to leave the keyboard alone (or not).
    fn set_quiet(&self, inner: &mut Inner, device: &DeviceHandle, quiet: Quiet) {
        if inner.quiet_sent != quiet {
            inner.quiet_sent = quiet;
            let _ = device.send(Request::SetQuiet(quiet));
            self.live.set_quiet(quiet);
        }
    }

    /// Turned off: end the session, let go of the screen and the keys.
    /// Returns how long to sleep.
    fn idle(&self, device: &DeviceHandle, now: f64) -> Duration {
        self.lol.want(false);
        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        inner.last_tick = 0.0;
        self.set_quiet(inner, device, Quiet::Off);
        if let Some(record) = inner.tracker.stop(now) {
            tracing::info!("game session ended (turned off)");
            let start = record.start_ms;
            self.keep_record(inner, record);
            // Its start card is old news.
            self.let_go(start);
        }
        inner.suggest = None;
        inner.front = None;
        let trying = inner.trying_until.is_some_and(|t| now < t);
        let keys = self.usb_keys(inner, device);
        let tried = Self::trying_lol(inner, keys.as_deref(), now);
        let trying_lol = tried.is_some();
        let frame: Frame = if let Some(f) = tried {
            Some((f, inner.settings.brightness))
        } else if trying {
            keys.map(|k| (keys_frame(&k, &inner.settings), inner.settings.brightness))
        } else {
            inner.trying_until = None;
            None
        };
        Self::send_layer(inner, device, frame, trying || trying_lol);
        let dirty = std::mem::take(&mut inner.dirty);
        drop(guard);
        self.live.set_hold(SOURCE, false);
        if dirty {
            self.emit();
        }
        // "Try on the keyboard" ends on time, not 30 s later.
        if trying_lol {
            LOL_TICK
        } else if trying {
            TICK
        } else {
            IDLE
        }
    }

    /// Keys of the keyboard on the USB cable, cached per model.
    fn usb_keys(&self, inner: &mut Inner, device: &DeviceHandle) -> Option<Arc<KeyIds>> {
        let model_id = device.snapshot.lock().ok().and_then(|s| {
            s.keyboards
                .iter()
                .find(|k| k.connection == Connection::Usb && k.has_command_channel)
                .map(|k| k.model_id.clone())
        })?;
        if inner.model.as_ref().is_none_or(|(id, _)| *id != model_id) {
            let model = kb_devices::catalogue().ok()?.iter().find(|m| m.id == model_id)?;
            inner.model = Some((model_id, Arc::new(KeyIds::of(model))));
        }
        inner.model.as_ref().map(|(_, k)| k.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(pid: u32, path: &str) -> Front {
        Front {
            pid,
            name: kb_winsys::foreground::file_name(path).to_string(),
            path: Some(path.to_string()),
            class: "GameWindow".into(),
            covers_monitor: true,
            minimized: false,
        }
    }

    fn lol() -> KnownGame {
        KnownGame {
            name: "League of Legends".into(),
            source: Source::Riot,
            dir: None,
            exes: vec![r"C:\Riot Games\League of Legends\Game\League of Legends.exe".into()],
            steam_app: None,
        }
    }

    const LOL: &str = r"C:\Riot Games\League of Legends\Game\League of Legends.exe";

    #[test]
    fn the_users_word_comes_first() {
        let known = vec![lol()];
        let f = front(7, LOL);
        let g = classify(&f, &[], &known, true).unwrap();
        assert_eq!((g.name.as_str(), g.source), ("League of Legends", GameSource::Riot));
        // The user said it is not a game.
        let no = [GameMark { path: LOL.to_uppercase(), name: "x".into(), game: false }];
        assert_eq!(classify(&f, &no, &known, true), None);
        // Launchers' lists off: only the user's own games.
        assert_eq!(classify(&f, &[], &known, false), None);
        let mine = [GameMark { path: r"D:\Emu\emu.exe".into(), name: "Emulator".into(), game: true }];
        let g = classify(&front(8, r"d:\emu\EMU.exe"), &mine, &known, false).unwrap();
        assert_eq!((g.name.as_str(), g.source), ("Emulator", GameSource::Mine));
        // "Not a game" on a launcher's game folder covers its programs.
        let steam = KnownGame {
            name: "Dota 2".into(),
            source: Source::Steam,
            dir: Some(r"C:\Steam\steamapps\common\dota 2 beta".into()),
            exes: vec![],
            steam_app: Some(570),
        };
        let dota = front(10, r"C:\Steam\steamapps\common\dota 2 beta\game\bin\win64\dota2.exe");
        assert!(classify(&dota, &[], std::slice::from_ref(&steam), true).is_some());
        let no =
            [GameMark { path: r"c:\steam\steamapps\common\dota 2 beta".into(), name: "Dota 2".into(), game: false }];
        assert_eq!(classify(&dota, &no, &[steam], true), None);
        // The shell is never a game.
        let mut shell = front(9, r"C:\Windows\explorer.exe");
        shell.class = "Progman".into();
        assert_eq!(classify(&shell, &mine, &known, true), None);
    }

    #[test]
    fn a_session_ends_when_its_game_is_no_longer_one() {
        let s = GamesSettings::default();
        let dota = game(r"c:\steam\steamapps\common\dota 2 beta", 3);
        assert!(still_a_game(&dota, &s));
        // "Not a game" on it, from the list of games (its key).
        let mut no = s.clone();
        no.marks.push(GameMark {
            path: r"C:\Steam\steamapps\common\dota 2 beta".into(),
            name: "x".into(),
            game: false,
        });
        assert!(!still_a_game(&dota, &no));
        // The launchers' lists turned off.
        let mut off = s.clone();
        off.launchers = false;
        assert!(!still_a_game(&dota, &off));
        // The user's own game: a game while marked, not once forgotten.
        let mine = Detected { source: GameSource::Mine, ..game(r"d:\emu\emu.exe", 4) };
        let mut marked = off.clone();
        marked.marks.push(GameMark { path: r"D:\Emu\emu.exe".into(), name: "Emulator".into(), game: true });
        assert!(still_a_game(&mine, &marked));
        assert!(!still_a_game(&mine, &s));
    }

    #[test]
    fn the_history_keeps_no_game_the_user_took_back() {
        let record = |key: &str, source| GameRecord {
            key: key.into(),
            name: key.into(),
            source,
            start_ms: 0.0,
            end_ms: 60_000.0,
            gpu_avg: None,
            gpu_peak: None,
            cpu_avg: None,
            cpu_peak: None,
            temp_peak: None,
            lol: None,
        };
        // A launcher's game no list has any more still shows its time.
        let old = record(r"c:\steam\steamapps\common\portal", GameSource::Steam);
        assert!(still_listed(&[], &old));
        let no = [GameMark { path: r"C:\Steam\steamapps\common\Portal".into(), name: "Portal".into(), game: false }];
        assert!(!still_listed(&no, &old));
        // The user's own game: listed while marked, gone once forgotten.
        let mine = record(r"c:\windows\system32\charmap.exe", GameSource::Mine);
        let yes =
            [GameMark { path: r"C:\Windows\System32\charmap.exe".into(), name: "Character Map".into(), game: true }];
        assert!(still_listed(&yes, &mine));
        assert!(!still_listed(&[], &mine));
    }

    #[test]
    fn quiet_for_every_game_or_for_one() {
        let mut s = GamesSettings::default();
        assert!(!quiet_for(&s, r"D:\Games\Racer"));
        s.quiet_games = vec![r"d:\games\racer".into()];
        assert!(quiet_for(&s, r"D:\Games\Racer"));
        assert!(!quiet_for(&s, r"D:\Games\Other"));
        s.quiet = true;
        assert!(quiet_for(&s, r"D:\Games\Other"));
    }

    #[test]
    fn a_profiles_design_on_every_key() {
        let ids = KeyIds { all: vec![1, 2, 3], ..KeyIds::default() };
        assert_eq!(profile_frame(&ids, &[(2, [9, 9, 9])]), vec![(1, [0, 0, 0]), (2, [9, 9, 9]), (3, [0, 0, 0])]);
    }

    fn game(key: &str, pid: u32) -> Detected {
        Detected {
            key: key.into(),
            name: key.into(),
            source: GameSource::Steam,
            pid,
            exe: format!("{key}.exe"),
            path: format!("{key}.exe"),
            steam_app: None,
        }
    }

    #[test]
    fn the_game_in_front_by_its_process_or_folder() {
        let racer = game(r"D:\Games\Racer", 7);
        assert!(plays(&front(7, r"D:\Games\Racer\bin\racer.exe"), &racer));
        assert!(plays(&front(9, r"d:\games\racer\launcher.exe"), &racer), "another program of the game");
        assert!(!plays(&front(9, r"C:\Program Files\Discord\Discord.exe"), &racer));
        assert!(!plays(&front(9, r"D:\Games\Racer2\racer.exe"), &racer));
    }

    fn procs(list: &[(u32, &str)]) -> Vec<(u32, String)> {
        list.iter().map(|(p, n)| (*p, (*n).to_string())).collect()
    }

    #[test]
    fn a_session_starts_after_15_s_in_front() {
        let mut t = Tracker::default();
        t.front(0.0, Some(game("a", 1)));
        assert_eq!(t.tick(14_000.0, None), None);
        assert_eq!(t.tick(15_000.0, None), Some(Change::Started));
        assert_eq!(t.session.as_ref().unwrap().started, 0.0, "counted from when it came to the front");
        // Leaving before 15 s starts nothing.
        let mut t = Tracker::default();
        t.front(0.0, Some(game("a", 1)));
        t.front(10_000.0, None);
        assert_eq!(t.tick(20_000.0, None), None);
        assert!(t.session.is_none());
    }

    #[test]
    fn alt_tab_does_not_end_it_and_the_program_going_does() {
        let mut t = Tracker::default();
        t.front(0.0, Some(game("a", 1)));
        t.tick(15_000.0, None);
        // Another window in front: the session goes on while the game runs.
        t.front(20_000.0, None);
        assert_eq!(t.tick(25_000.0, Some(&procs(&[(1, "a.exe")]))), None);
        assert!(t.session.is_some());
        // Gone for less than 10 s: still on.
        assert_eq!(t.tick(30_000.0, Some(&procs(&[]))), None);
        assert_eq!(t.tick(35_000.0, Some(&procs(&[]))), None);
        match t.tick(40_000.0, Some(&procs(&[]))) {
            Some(Change::Ended(r)) => {
                assert_eq!((r.start_ms, r.end_ms), (0.0, 30_000.0), "ends when it was first missed");
            }
            other => panic!("{other:?}"),
        }
        assert!(t.session.is_none());
    }

    #[test]
    fn a_game_that_restarts_keeps_its_session() {
        let mut t = Tracker::default();
        t.front(0.0, Some(game("a", 1)));
        t.tick(15_000.0, None);
        assert_eq!(t.tick(20_000.0, Some(&procs(&[]))), None);
        // The same program again under a new id.
        assert_eq!(t.tick(25_000.0, Some(&procs(&[(5, "A.EXE")]))), None);
        assert_eq!(t.session.as_ref().unwrap().game.pid, 5);
        assert_eq!(t.tick(40_000.0, Some(&procs(&[(5, "a.exe")]))), None);
    }

    #[test]
    fn another_game_ends_the_first() {
        let mut t = Tracker::default();
        t.front(0.0, Some(game("a", 1)));
        t.tick(15_000.0, None);
        t.front(100_000.0, Some(game("b", 2)));
        // "a" ended when "b" came to the front: no 15 s counted twice.
        assert!(matches!(t.tick(115_000.0, None), Some(Change::Ended(r)) if r.key == "a" && r.end_ms == 100_000.0));
        assert_eq!(t.tick(116_000.0, None), Some(Change::Started));
        assert_eq!(t.session.as_ref().unwrap().game.key, "b");
    }

    #[test]
    fn readings_average_and_peak() {
        let mut s = Stats::default();
        s.add(Some(50.0), Some(10.0), None);
        s.add(Some(100.0), None, Some(70.5));
        s.add(None, Some(30.0), Some(65.0));
        assert_eq!(s.gpu_avg(), Some(75.0));
        assert_eq!(s.gpu_peak, Some(100.0));
        assert_eq!(s.cpu_avg(), Some(20.0));
        assert_eq!(s.temp_peak, Some(70.5));
        assert_eq!(Stats::default().gpu_avg(), None);
    }

    #[test]
    fn time_played_in_a_window() {
        let r = |a: f64, b: f64| GameRecord {
            key: "k".into(),
            name: "n".into(),
            source: GameSource::Mine,
            start_ms: a,
            end_ms: b,
            gpu_avg: None,
            gpu_peak: None,
            cpu_avg: None,
            cpu_peak: None,
            temp_peak: None,
            lol: None,
        };
        let records = [r(0.0, 100.0), r(150.0, 300.0)];
        assert_eq!(played_between(&records, None, 50.0, 400.0), 50.0 + 150.0);
        assert_eq!(played_between(&records, Some(350.0), 50.0, 400.0), 200.0 + 50.0);
    }

    #[test]
    fn chosen_keys_lit_the_rest_dim_or_dark() {
        let ids = KeyIds {
            all: vec![1, 2, 3],
            by_id: [("w".to_string(), 2u8)].into_iter().collect(),
            x: HashMap::new(),
            y: HashMap::new(),
        };
        let mut s = GamesSettings { keys: vec!["w".into(), "nope".into()], color: [200, 100, 0], ..Default::default() };
        s.rest = GameRest::Dark;
        assert_eq!(keys_frame(&ids, &s), vec![(1, [0, 0, 0]), (2, [200, 100, 0]), (3, [0, 0, 0])]);
        s.rest = GameRest::Dim;
        assert_eq!(keys_frame(&ids, &s)[0], (1, [24, 12, 0]));
    }

    #[test]
    fn browsers_and_launchers_are_never_offered() {
        assert!(not_a_game("chrome.exe"));
        assert!(not_a_game("Steam.exe"));
        assert!(!not_a_game("emulator.exe"));
    }
}
