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
//! * "Quiet": while a game runs nothing goes to the keyboard at all.
//! * A program that covers the screen for 2 min with the graphics card
//!   busy, and is neither a game nor a known non-game, is offered as
//!   "Is this a game?". It never becomes one by itself.

use crate::device::{DeviceHandle, LightLayer, LiveFrame, Request};
use crate::dto::{
    Connection, GameCard, GameCardKind, GameEntry, GameLights, GameMark, GameRecord, GameRest, GameSessionInfo,
    GameSource, GameSuggestion, GamesSettings, GamesState,
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
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

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
const IDLE: Duration = Duration::from_secs(30);
/// The dim keys of `GameRest::Dim`: a faint share of the colour.
const DIM: f32 = 0.12;

/// Programs that cover the screen without being games: browsers and video
/// players (full-screen video), calls, streaming, remote desktops.
const NOT_GAMES: [&str; 24] = [
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

/// `path` is `mark` (a program) or lies in it (a game's folder).
fn covered(path: &str, mark: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let (path, mark) = (norm(path), norm(mark));
    path == mark || (path.starts_with(&mark) && path.as_bytes().get(mark.len()) == Some(&b'\\'))
}

/// The user's word on `path`, if any (the closest mark wins).
fn mark_for<'a>(marks: &'a [GameMark], path: &str) -> Option<&'a GameMark> {
    marks.iter().filter(|m| covered(path, &m.path)).max_by_key(|m| m.path.len())
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
            temp_peak: self.stats.temp_peak,
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
                    // Another game took over: this one ends now.
                    return Some(Change::Ended(s.record(now)));
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

/// Start of today, local time, in ms since the epoch.
fn day_start(now: f64) -> f64 {
    use chrono::{Local, TimeZone};
    let Some(t) = Local.timestamp_millis_opt(now as i64).single() else { return now - DAY_MS };
    t.date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|d| Local.from_local_datetime(&d).earliest())
        .map_or(now - DAY_MS, |d| d.timestamp_millis() as f64)
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
}

impl KeyIds {
    pub(crate) fn of(model: &Model) -> Self {
        let Some(g) = &model.geometry else { return Self::default() };
        Self {
            all: g.keys.iter().map(|k| k.led).collect(),
            by_id: g.keys.iter().map(|k| (k.id.clone(), k.led)).collect(),
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

// ------------------------------------------------------------- worker

type Frame = Option<(Vec<(u8, Rgb)>, u8)>;

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
    trying_until: Option<f64>,
    model: Option<(String, Arc<KeyIds>)>,
    /// A temperature was read on this computer.
    temperature: bool,
    last_gpu: Option<f32>,
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
            trying_until: None,
            model: None,
            temperature: false,
            last_gpu: None,
            dirty: true,
        }));
        let (tx, rx) = mpsc::channel();
        let handle = Arc::new(Self { tx: tx.clone(), inner: inner.clone(), app, config_dir, live, hub: hub.clone() });
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

    /// The game of the running session and when it started (tray menu).
    pub fn session(&self) -> Option<(String, f64)> {
        let i = self.inner.lock().ok()?;
        i.tracker.session.as_ref().map(|s| (s.game.name.clone(), s.started))
    }

    pub fn state(&self) -> Result<GamesState, String> {
        let inner = self.inner.lock().map_err(|_| "games unavailable")?;
        Ok(Self::state_of(&inner, now_ms()))
    }

    fn state_of(inner: &Inner, now: f64) -> GamesState {
        let running = inner.tracker.session.as_ref();
        let started = running.map(|s| s.started);
        let mut totals: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
        for r in &inner.history {
            let t = totals.entry(r.key.as_str()).or_default();
            t.0 += (r.end_ms - r.start_ms).max(0.0);
            t.1 = t.1.max(r.end_ms);
        }
        let mut games: Vec<GameEntry> = Vec::new();
        let mut push = |key: String, name: String, source: GameSource| {
            if games.iter().any(|g| g.key == key) {
                return;
            }
            let (total, last) = totals.get(key.as_str()).copied().unwrap_or_default();
            let live = running.filter(|s| s.game.key == key).map_or(0.0, |s| (now - s.started).max(0.0));
            games.push(GameEntry {
                total_ms: total + live,
                last_ms: (last > 0.0).then_some(last),
                played: total + live > 0.0,
                key,
                name,
                source,
            });
        };
        for m in inner.settings.marks.iter().filter(|m| m.game) {
            push(m.path.to_lowercase(), m.name.clone(), GameSource::Mine);
        }
        if inner.settings.launchers {
            for g in inner.known.iter() {
                let key = key_of(g);
                // The user said it is not a game.
                if mark_for(&inner.settings.marks, &key).is_some_and(|m| !m.game) {
                    continue;
                }
                push(key, g.name.clone(), source_of(g.source));
            }
        }
        // Games played before that no list has any more.
        for r in &inner.history {
            push(r.key.clone(), r.name.clone(), r.source);
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
            settings: inner.settings.clone(),
            session: running.map(|s| GameSessionInfo {
                key: s.game.key.clone(),
                name: s.game.name.clone(),
                source: s.game.source,
                started_ms: s.started,
                gpu_avg: s.stats.gpu_avg(),
                temp_peak: s.stats.temp_peak,
            }),
            games,
            suggestions: inner.suggestions.clone(),
            today_ms: played_between(&inner.history, started, day_start(now), now),
            week_ms: played_between(&inner.history, started, now - 7.0 * DAY_MS, now),
            recent,
            temperature: inner.temperature,
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
            inner.settings.marks.retain(|m| !m.path.eq_ignore_ascii_case(&path));
            if let Some(game) = game {
                let name =
                    if name.trim().is_empty() { kb_winsys::foreground::file_name(&path).to_string() } else { name };
                inner.settings.marks.push(GameMark { path: path.clone(), name, game });
            }
            inner.suggestions.retain(|s| !s.path.eq_ignore_ascii_case(&path));
            inner.settings = inner.settings.clone().sanitized();
            inner.dirty = true;
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
    fn higher_holds(&self) -> bool {
        self.live.held_by(crate::calendar::SOURCE)
            || self.live.held_by(crate::reminders::SOURCE)
            || self.live.held_by(crate::pomodoro::SOURCE)
    }

    fn card(&self, kind: GameCardKind, r: &Running, end: Option<f64>, today: f64) -> GameCard {
        GameCard {
            kind,
            name: r.game.name.clone(),
            steam_app: r.game.steam_app,
            started_ms: r.started,
            ended_ms: end,
            gpu_avg: r.stats.gpu_avg(),
            gpu_peak: r.stats.gpu_peak,
            cpu_avg: r.stats.cpu_avg(),
            temp_peak: r.stats.temp_peak,
            today_ms: today,
        }
    }

    /// One pass of the worker; returns how long to sleep.
    fn tick(&self, device: &DeviceHandle, meters: &mut Meters) -> Duration {
        let now = now_ms();
        let (enabled, launchers, scan_due) = match self.inner.lock() {
            Ok(i) => (i.settings.enabled, i.settings.launchers, now >= i.next_scan),
            Err(_) => return IDLE,
        };
        self.hub.want(WATCHER, enabled);
        if !enabled {
            self.idle(device, now);
            return IDLE;
        }
        // The launchers' lists: read without holding the lock (it takes a moment).
        if scan_due {
            let known = if launchers { library::installed() } else { Vec::new() };
            tracing::debug!(games = known.len(), "launchers' games read");
            if let Ok(mut i) = self.inner.lock() {
                i.known = Arc::new(known);
                i.next_scan = now + RESCAN_MS;
                i.dirty = true;
            }
        }
        let (in_session, suggesting, sample_due, alive_due) = match self.inner.lock() {
            Ok(i) => (i.tracker.session.is_some(), i.suggest.is_some(), now >= i.next_sample, now >= i.next_alive),
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
        let procs = (in_session && alive_due).then(process_names);

        let Ok(mut guard) = self.inner.lock() else { return IDLE };
        let inner = &mut *guard;
        let s = inner.settings.clone();
        let own_pid = std::process::id();

        if let Some(front) = inner.front.take() {
            let game = if front.pid == own_pid { None } else { classify(&front, &s.marks, &inner.known, s.launchers) };
            // A full-screen program that is not known: maybe a game.
            inner.suggest = match (&game, &front.path) {
                (None, Some(path))
                    if front.covers_monitor
                        && !front.is_shell()
                        && front.pid != own_pid
                        && !not_a_game(&front.name)
                        && !s.marks.iter().any(|m| m.path.eq_ignore_ascii_case(path))
                        && !inner.suggestions.iter().any(|x| x.path.eq_ignore_ascii_case(path)) =>
                {
                    match &inner.suggest {
                        Some((p, n, since)) if p.eq_ignore_ascii_case(path) => Some((p.clone(), n.clone(), *since)),
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

        let change = inner.tracker.tick(now, procs.as_deref());
        let live_on = self.live.enabled();
        match &change {
            Some(Change::Started) => {
                inner.dirty = true;
                let r = inner.tracker.session.clone().expect("a session just started");
                tracing::info!(source = ?r.game.source, "game session started");
                let today = played_between(&inner.history, Some(r.started), day_start(now), now);
                if s.start_card && live_on && !s.quiet && !self.higher_holds() {
                    let _ = self.app.emit(EVENT_GAME_CARD, self.card(GameCardKind::Start, &r, None, today));
                }
            }
            Some(Change::Ended(record)) => {
                inner.dirty = true;
                let minutes = ((record.end_ms - record.start_ms) / 60_000.0).round();
                tracing::info!(minutes, "game session ended");
                inner.history.push(record.clone());
                if inner.history.len() > HISTORY_MAX {
                    let drop = inner.history.len() - HISTORY_MAX;
                    inner.history.drain(..drop);
                }
                self.save_history(&inner.history);
                let long = record.end_ms - record.start_ms >= SUMMARY_MIN_MS;
                self.live.set_hold(SOURCE, false);
                if s.summary_card && long && live_on && !s.quiet && !self.higher_holds() {
                    let today = played_between(&inner.history, None, day_start(now), now);
                    let r = Running {
                        game: Detected {
                            key: record.key.clone(),
                            name: record.name.clone(),
                            source: record.source,
                            pid: 0,
                            exe: String::new(),
                            steam_app: None,
                        },
                        started: record.start_ms,
                        missing_since: None,
                        stats: Stats {
                            gpu_sum: f64::from(record.gpu_avg.unwrap_or(0.0)),
                            gpu_n: u32::from(record.gpu_avg.is_some()),
                            gpu_peak: record.gpu_peak,
                            cpu_sum: f64::from(record.cpu_avg.unwrap_or(0.0)),
                            cpu_n: u32::from(record.cpu_avg.is_some()),
                            cpu_peak: None,
                            temp_peak: record.temp_peak,
                        },
                    };
                    let _ = self
                        .app
                        .emit(EVENT_GAME_CARD, self.card(GameCardKind::Summary, &r, Some(record.end_ms), today));
                } else {
                    // The start card is old news now.
                    self.live.drop_pending(SOURCE);
                    self.live.return_after(SOURCE, STALE_CARD_BACK);
                }
            }
            None => {}
        }
        let in_session = inner.tracker.session.is_some();
        self.live.set_hold(SOURCE, in_session && live_on && !s.quiet && (s.start_card || s.summary_card));

        // Lights.
        let trying = inner.trying_until.is_some_and(|t| now < t);
        if !trying {
            inner.trying_until = None;
        }
        let keys = self.usb_keys(inner, device);
        let show = trying || (in_session && !s.quiet && s.lights == GameLights::Keys);
        let frame: Frame = match (&keys, show) {
            (Some(k), true) => Some((keys_frame(k, &s), s.brightness)),
            _ => None,
        };
        if frame != inner.sent {
            let _ = device.send(Request::SetLayer(
                LightLayer::Game,
                frame.clone().map(|(colors, b)| LiveFrame { colors, level: level_factor(b) }),
            ));
            inner.sent = frame;
        }

        if in_session && now >= inner.next_emit {
            inner.dirty = true;
        }
        if inner.dirty {
            inner.dirty = false;
            inner.next_emit = now + EMIT_MS;
            let state = Self::state_of(inner, now);
            drop(guard);
            let _ = self.app.emit(EVENT_GAMES, state);
        }
        TICK
    }

    /// Turned off: end the session, let go of the screen and the keys.
    fn idle(&self, device: &DeviceHandle, now: f64) {
        let Ok(mut inner) = self.inner.lock() else { return };
        if let Some(record) = inner.tracker.stop(now) {
            tracing::info!("game session ended (turned off)");
            inner.history.push(record);
            self.save_history(&inner.history);
            inner.dirty = true;
        }
        inner.suggest = None;
        inner.front = None;
        let trying = inner.trying_until.is_some_and(|t| now < t);
        let frame: Frame = if trying {
            self.usb_keys(&mut inner, device).map(|k| (keys_frame(&k, &inner.settings), inner.settings.brightness))
        } else {
            inner.trying_until = None;
            None
        };
        if frame != inner.sent {
            let _ = device.send(Request::SetLayer(
                LightLayer::Game,
                frame.clone().map(|(colors, b)| LiveFrame { colors, level: level_factor(b) }),
            ));
            inner.sent = frame;
        }
        let dirty = std::mem::take(&mut inner.dirty);
        drop(inner);
        self.live.set_hold(SOURCE, false);
        if dirty {
            self.emit();
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

    fn game(key: &str, pid: u32) -> Detected {
        Detected {
            key: key.into(),
            name: key.into(),
            source: GameSource::Steam,
            pid,
            exe: format!("{key}.exe"),
            steam_app: None,
        }
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
        assert!(matches!(t.tick(115_000.0, None), Some(Change::Ended(r)) if r.key == "a"));
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
            temp_peak: None,
        };
        let records = [r(0.0, 100.0), r(150.0, 300.0)];
        assert_eq!(played_between(&records, None, 50.0, 400.0), 50.0 + 150.0);
        assert_eq!(played_between(&records, Some(350.0), 50.0, 400.0), 200.0 + 50.0);
    }

    #[test]
    fn chosen_keys_lit_the_rest_dim_or_dark() {
        let ids = KeyIds { all: vec![1, 2, 3], by_id: [("w".to_string(), 2u8)].into_iter().collect() };
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
