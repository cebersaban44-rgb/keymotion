//! League of Legends lights (keymotion-notes: oyun-oturumu-ve-otomatik-profil.md
//! §5.3, §7 D), from Riot's Live Client Data API: an HTTPS service the game
//! itself runs on this computer (`https://127.0.0.1:2999/liveclientdata/`)
//! while a match is on. Riot documents it for third-party tools; nothing
//! is read from the game's memory and nothing is sent to it.
//!
//! * Only the player's own state, as the game shows it, and public events
//!   become light: health (on the whole keyboard, receding the way the
//!   focus timer's light does, or on the number row) and mana on the F row,
//!   being dead and the time to respawn, and a short animation of the
//!   user's choice for level ups, kills, an ace, dragons, Baron, Herald and
//!   the end.
//!   No enemy timers, no objective spawn timers, nothing that tells the
//!   player what to do (Riot's game integrity rules).
//! * The service uses Riot's own certificate, which Windows does not
//!   trust; Riot's documentation says to accept it. Only this address on
//!   this computer is asked, and only while a League of Legends session
//!   runs.
//! * From the player list: the player's own entry, and which players are on
//!   the player's team (their names, as the scoreboard shows them), to tell
//!   whose team took a dragon, Herald or Baron.
//! * Teamfight Tactics runs in the same program; its matches get no lights.

use crate::dto::{GameRest, GamesSettings, LolAnim, LolEvent, LolLayout, LolResult, LolScene, LolSettings};
use crate::games::KeyIds;
use crate::pomodoro::{drain_order, lit};
use kb_protocol::sonix::Rgb;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BASE: &str = "https://127.0.0.1:2999/liveclientdata";
/// Health and mana this often; the rest every few passes.
const POLL: Duration = Duration::from_millis(300);
const SLOW_EVERY: u32 = 3;
/// Not answering (loading screen, between matches): try again after this.
const RETRY: Duration = Duration::from_secs(2);

const NUMBER_ROW: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
const F_ROW: [&str; 12] = ["f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"];

const HP_GREEN: Rgb = [52, 199, 89];
const HP_AMBER: Rgb = [255, 176, 32];
const HP_RED: Rgb = [255, 59, 48];
const ENERGY: Rgb = [255, 214, 10];
const FURY: Rgb = [255, 69, 58];
const DEAD: Rgb = [70, 0, 0];
const RESPAWN: Rgb = [240, 240, 240];
/// The faint light of a key the bar doesn't reach, so its length shows.
const TRACK: f32 = 0.06;
/// Effects are kept this long at most (the longest animation, a 5-kill
/// streak).
const EFFECT_KEEP_MS: f64 = 8_000.0;
/// The keys of one frame: LED and colour.
pub type Keys = Vec<(u8, Rgb)>;
/// Preview frames (`preview`), 25 a second.
pub const PREVIEW_STEP_MS: f64 = 40.0;
/// A preview of the bars, or of being dead.
const STILL_PREVIEW_MS: f64 = 2_000.0;
const DEATH_PREVIEW_MS: f64 = 4_000.0;

/// A short light effect and when it began.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    LevelUp,
    /// A kill by the player: one flash per kill of the streak.
    Kill(u8),
    Ace,
    /// A dragon, in its kind's colour; taken by the player's team.
    Dragon(Rgb, bool),
    /// Taken by the player's team.
    Baron(bool),
    Herald(bool),
    Win,
    Lose,
}

impl Effect {
    /// A dragon, Herald or Baron the other team took.
    fn theirs(self) -> bool {
        matches!(self, Self::Dragon(_, false) | Self::Baron(false) | Self::Herald(false))
    }

    pub fn event(self) -> LolEvent {
        match self {
            Self::LevelUp => LolEvent::LevelUp,
            Self::Kill(_) => LolEvent::Kill,
            Self::Ace => LolEvent::Ace,
            Self::Dragon(..) => LolEvent::Dragon,
            Self::Baron(_) => LolEvent::Baron,
            Self::Herald(_) => LolEvent::Herald,
            Self::Win => LolEvent::Win,
            Self::Lose => LolEvent::Lose,
        }
    }

    /// How many times its animation plays: once per kill of a streak.
    fn times(self) -> u8 {
        match self {
            Self::Kill(n) => n.clamp(1, 5),
            _ => 1,
        }
    }

    fn length_ms(self, anim: LolAnim) -> f64 {
        let one = anim_ms(anim);
        match self.times() {
            1 => one,
            // A streak plays it quicker, each time.
            n => one * 0.6 * f64::from(n),
        }
    }
}

/// One play of an animation.
fn anim_ms(anim: LolAnim) -> f64 {
    match anim {
        LolAnim::Wave => 1_200.0,
        LolAnim::Flash => 900.0,
        LolAnim::Pulse => 1_600.0,
        LolAnim::Sparkle | LolAnim::Drain => 2_500.0,
        LolAnim::Ring => 1_300.0,
    }
}

/// Dragon kinds as the API names them, and their colours.
fn dragon_color(kind: &str) -> Rgb {
    match kind.to_ascii_lowercase().as_str() {
        "fire" | "infernal" => [255, 90, 30],
        "water" | "ocean" => [47, 183, 201],
        "earth" | "mountain" => [181, 122, 60],
        "air" | "cloud" => [210, 230, 255],
        "hextech" => [80, 220, 255],
        "chemtech" => [120, 230, 60],
        "elder" => [255, 240, 200],
        _ => [255, 140, 40],
    }
}

/// What the match shows now, as read from the API.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub connected: bool,
    pub mode: String,
    me: String,
    team: String,
    /// The names of the players on the player's team (name#tag and name).
    allies: Vec<String>,
    pub champion: String,
    pub health: Option<(f32, f32)>,
    /// Kind ("MANA", "ENERGY", …), value, max.
    pub resource: Option<(String, f32, f32)>,
    pub level: u32,
    pub dead: bool,
    /// When the player respawns, and how long the wait was.
    respawn_at: f64,
    respawn_total: f64,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub cs: u32,
    pub win: Option<bool>,
    /// The last event seen (`EventID`), and whether events were read yet.
    last_event: i64,
    events_read: bool,
    pub effects: Vec<(Effect, f64)>,
}

impl Live {
    /// Teamfight Tactics gets no lights.
    pub fn lights(&self) -> bool {
        self.connected && !self.mode.eq_ignore_ascii_case("TFT")
    }

    pub fn result(&self) -> Option<LolResult> {
        (!self.me.is_empty()).then(|| LolResult {
            win: self.win,
            kills: self.kills,
            deaths: self.deaths,
            assists: self.assists,
            cs: self.cs,
            champion: self.champion.clone(),
            mode: self.mode.clone(),
        })
    }

    fn push(&mut self, e: Effect, now: f64) {
        self.effects.retain(|(_, at)| now - at < EFFECT_KEEP_MS);
        self.effects.push((e, now));
    }

    /// `activeplayer`: health, resource, level.
    pub fn read_active(&mut self, v: &Value, now: f64) {
        let stats = &v["championStats"];
        let f = |x: &Value| x.as_f64().map(|n| n as f32);
        if let (Some(h), Some(m)) = (f(&stats["currentHealth"]), f(&stats["maxHealth"])) {
            self.health = (m > 0.0).then_some((h.max(0.0), m));
        }
        let kind = stats["resourceType"].as_str().unwrap_or_default().to_string();
        self.resource = match (f(&stats["resourceValue"]), f(&stats["resourceMax"])) {
            (Some(r), Some(m)) if m > 0.0 && kind != "NONE" => Some((kind, r.max(0.0), m)),
            _ => None,
        };
        if let Some(level) = v["level"].as_u64() {
            let level = level as u32;
            if self.level > 0 && level > self.level {
                self.push(Effect::LevelUp, now);
            }
            self.level = level;
        }
        if let Some(id) = v["riotId"].as_str().or_else(|| v["summonerName"].as_str()) {
            self.me = id.to_string();
        }
    }

    /// The player's team took it. When the team or the killer isn't known,
    /// it counts as the team's, so nothing is hidden by mistake.
    fn ours(&self, killer: &str) -> bool {
        killer.is_empty() || self.allies.is_empty() || self.is_me(killer) || self.allies.iter().any(|a| a == killer)
    }

    fn is_me(&self, name: &str) -> bool {
        // Riot ids are "name#tag"; some events give the name alone.
        !self.me.is_empty() && (name == self.me || self.me.split('#').next() == Some(name))
    }

    /// `playerlist`: the player's own entry (dead, respawn, scores, team).
    pub fn read_players(&mut self, v: &Value, now: f64) {
        let Some(list) = v.as_array() else { return };
        let Some(me) = list.iter().find(|p| {
            let id = p["riotId"].as_str().or_else(|| p["summonerName"].as_str()).unwrap_or_default();
            self.is_me(id)
        }) else {
            return;
        };
        self.champion = me["championName"].as_str().unwrap_or_default().to_string();
        self.team = me["team"].as_str().unwrap_or_default().to_string();
        // The team's names only, to tell whose team took an objective.
        let team = self.team.clone();
        self.allies = list
            .iter()
            .filter(|p| !team.is_empty() && p["team"].as_str() == Some(team.as_str()))
            .flat_map(|p| [p["riotId"].as_str(), p["summonerName"].as_str()])
            .flatten()
            .flat_map(|id| [id.to_string(), id.split('#').next().unwrap_or(id).to_string()])
            .collect();
        let dead = me["isDead"].as_bool().unwrap_or(false);
        let respawn = me["respawnTimer"].as_f64().unwrap_or(0.0).max(0.0);
        if dead && !self.dead {
            self.respawn_total = respawn.max(1.0);
        }
        if dead {
            self.respawn_at = now + respawn * 1_000.0;
        }
        self.dead = dead;
        let s = &me["scores"];
        let n = |x: &Value| x.as_u64().unwrap_or(0) as u32;
        self.kills = n(&s["kills"]);
        self.deaths = n(&s["deaths"]);
        self.assists = n(&s["assists"]);
        self.cs = n(&s["creepScore"]);
    }

    /// `eventdata`: the events not seen yet.
    pub fn read_events(&mut self, v: &Value, now: f64) {
        let Some(events) = v["Events"].as_array() else { return };
        let first = !std::mem::replace(&mut self.events_read, true);
        for e in events {
            let id = e["EventID"].as_i64().unwrap_or(-1);
            if id <= self.last_event {
                continue;
            }
            self.last_event = id;
            // Events from before the lights started are history.
            if first {
                if e["EventName"] == "GameEnd" {
                    self.win = Some(e["Result"].as_str() == Some("Win"));
                }
                continue;
            }
            let killer = e["KillerName"].as_str().unwrap_or_default();
            match e["EventName"].as_str().unwrap_or_default() {
                "ChampionKill" if self.is_me(killer) => self.push(Effect::Kill(1), now),
                // The game sends it right after the kill that made the
                // streak: one animation for the streak, not one on another.
                "Multikill" if self.is_me(killer) => {
                    let n = e["KillStreak"].as_u64().unwrap_or(2).min(5) as u8;
                    self.effects.retain(|(e, _)| !matches!(e, Effect::Kill(_)));
                    self.push(Effect::Kill(n), now);
                }
                "Ace" => {
                    let mine = self.is_me(e["Acer"].as_str().unwrap_or_default())
                        || (!self.team.is_empty() && e["AcingTeam"].as_str() == Some(self.team.as_str()));
                    if mine {
                        self.push(Effect::Ace, now);
                    }
                }
                "DragonKill" => {
                    let color = dragon_color(e["DragonType"].as_str().unwrap_or_default());
                    self.push(Effect::Dragon(color, self.ours(killer)), now)
                }
                "BaronKill" => self.push(Effect::Baron(self.ours(killer)), now),
                "HeraldKill" => self.push(Effect::Herald(self.ours(killer)), now),
                "GameEnd" => {
                    let win = e["Result"].as_str() == Some("Win");
                    self.win = Some(win);
                    self.push(if win { Effect::Win } else { Effect::Lose }, now);
                }
                _ => {}
            }
        }
    }

    /// Forget the match (a new one starts).
    fn reset(&mut self) {
        *self = Self { last_event: -1, ..Self::default() };
    }
}

// ----------------------------------------------------------------- frame

fn scale(c: Rgb, k: f32) -> Rgb {
    c.map(|v| (f32::from(v) * k.clamp(0.0, 1.0)).round() as u8)
}

fn row(ids: &KeyIds, keys: &[&str]) -> Vec<u8> {
    keys.iter().filter_map(|k| ids.by_id.get(*k).copied()).collect()
}

/// A bar over `leds`: whole keys lit, the edge key partly, the rest faint.
fn bar(frame: &mut [(u8, Rgb)], leds: &[u8], share: f32, color: Rgb) {
    let fill = share.clamp(0.0, 1.0) * leds.len() as f32;
    for (i, led) in leds.iter().enumerate() {
        let lit = (fill - i as f32).clamp(0.0, 1.0);
        if let Some(k) = frame.iter_mut().find(|k| k.0 == *led) {
            k.1 = scale(color, lit.max(0.06));
        }
    }
}

/// Health's colour: the user's, or green, amber, red by how much is left.
fn health_color(l: &LolSettings, share: f32) -> Rgb {
    l.health_color.unwrap_or(if share > 0.6 {
        HP_GREEN
    } else if share > 0.3 {
        HP_AMBER
    } else {
        HP_RED
    })
}

fn at(ids: &KeyIds, led: u8) -> (f32, f32) {
    (ids.x.get(&led).copied().unwrap_or(0.5), ids.y.get(&led).copied().unwrap_or(0.5))
}

/// The keys while a League of Legends match runs.
pub fn frame(live: &Live, ids: &KeyIds, s: &GamesSettings, now: f64) -> Vec<(u8, Rgb)> {
    let l: LolSettings = s.lol;
    // Base: the game keys the user chose (QWER… usually), the rest dim or dark.
    let mut out: Vec<(u8, Rgb)> = if s.lights == crate::dto::GameLights::Keys {
        crate::games::keys_frame(ids, s)
    } else {
        let rest = match s.rest {
            GameRest::Dark => [0, 0, 0],
            GameRest::Dim => scale(s.color, 0.12),
        };
        ids.all.iter().map(|&led| (led, rest)).collect()
    };
    let whole = l.layout == LolLayout::Keyboard;
    // Once the match is over, its health and death mean nothing: the end
    // has the keys (and the game's own lights after it, until it closes).
    let over = live.win.is_some();
    if !over && l.death && live.dead {
        let left = ((live.respawn_at - now) / live.respawn_total.max(1.0) / 1_000.0).clamp(0.0, 1.0) as f32;
        if whole {
            // Dark red; the light comes back the way health would fill.
            for k in &mut out {
                let (x, y) = at(ids, k.0);
                k.1 = mix(DEAD, RESPAWN, lit(l.motion, drain_order(l.motion, x, y), 1.0 - left));
            }
        } else {
            for k in &mut out {
                k.1 = DEAD;
            }
            bar(&mut out, &row(ids, &NUMBER_ROW), left, RESPAWN);
        }
    } else if !over && l.bars {
        if let Some((h, max)) = live.health {
            let share = (h / max).clamp(0.0, 1.0);
            let color = health_color(&l, share);
            if whole {
                for k in &mut out {
                    let (x, y) = at(ids, k.0);
                    k.1 = mix(scale(color, TRACK), color, lit(l.motion, drain_order(l.motion, x, y), share));
                }
            } else {
                bar(&mut out, &row(ids, &NUMBER_ROW), share, color);
            }
        }
        if let (true, Some((kind, v, max))) = (l.mana, &live.resource) {
            let color = match kind.as_str() {
                "ENERGY" => ENERGY,
                "MANA" => l.mana_color,
                _ => FURY,
            };
            bar(&mut out, &row(ids, &F_ROW), v / max, color);
        }
    }
    if l.events {
        for (e, at) in &live.effects {
            let light = l.lights.get(e.event());
            if !light.on || (l.team_only && e.theirs()) {
                continue;
            }
            let t = ((now - at) / e.length_ms(light.anim)) as f32;
            if !(0.0..1.0).contains(&t) {
                continue;
            }
            let color = match e {
                Effect::Dragon(kind, _) if l.dragon_kind_color => *kind,
                _ => light.color,
            };
            // The match's end plays on the whole keyboard, not over health
            // and mana like the events during it.
            if matches!(e, Effect::Win | Effect::Lose) {
                for k in &mut out {
                    k.1 = [0, 0, 0];
                }
            }
            // A streak plays the animation once per kill.
            let n = f32::from(e.times());
            animate(&mut out, ids, light.anim, color, if n > 1.0 { (t * n).fract() } else { t });
        }
    }
    out
}

/// One animation at `t` (0 → 1) over the frame.
fn animate(out: &mut [(u8, Rgb)], ids: &KeyIds, anim: LolAnim, color: Rgb, t: f32) {
    for k in out.iter_mut() {
        let (x, y) = at(ids, k.0);
        let a = match anim {
            // A band moving left to right.
            LolAnim::Wave => 1.0 - ((x - (t * 1.3 - 0.15)).abs() / 0.15),
            // Two flashes, fading.
            LolAnim::Flash => f32::from(u8::from((t * 2.0).fract() < 0.5)) * (1.0 - t),
            // Two breaths.
            LolAnim::Pulse => (t * std::f32::consts::PI * 2.0).sin().powi(2) * (1.0 - t * 0.3),
            // A flash, then keys twinkling, each at its own moment.
            LolAnim::Sparkle => {
                let seed = ((f32::from(k.0) * 12.989).sin() * 43_758.547).fract().abs();
                let phase = (t * 4.0 + seed).fract();
                let twinkle = if phase < 0.25 { 1.0 - phase * 4.0 } else { 0.0 } * (1.0 - t * 0.5);
                twinkle.max(1.0 - t / 0.12)
            }
            // A ring from the middle outwards.
            LolAnim::Ring => {
                let d = (x - 0.5).hypot((y - 0.5) * 0.6);
                1.0 - (d - t * 0.75).abs() / 0.1
            }
            // Lit, then dark from the top row down.
            LolAnim::Drain => {
                if t < 0.2 {
                    t / 0.2
                } else {
                    let edge = (t - 0.2) / 0.8 * 1.2;
                    ((y - edge) / 0.08 + 0.5).clamp(0.0, 1.0) * (1.0 - t * 0.2)
                }
            }
        };
        k.1 = mix(k.1, color, a.clamp(0.0, 1.0));
    }
}

/// How long a preview scene runs.
pub fn scene_ms(s: &GamesSettings, scene: LolScene) -> f64 {
    match scene {
        LolScene::Bars { .. } => STILL_PREVIEW_MS,
        LolScene::Death => DEATH_PREVIEW_MS,
        LolScene::Event { event } => scene_effect(event).length_ms(s.lol.lights.get(event).anim),
    }
}

/// The effect a preview of `event` plays (a dragon of fire, a single kill).
fn scene_effect(event: LolEvent) -> Effect {
    match event {
        LolEvent::Win => Effect::Win,
        LolEvent::Lose => Effect::Lose,
        LolEvent::LevelUp => Effect::LevelUp,
        LolEvent::Kill => Effect::Kill(1),
        LolEvent::Ace => Effect::Ace,
        LolEvent::Dragon => Effect::Dragon(dragon_color("fire"), true),
        LolEvent::Baron => Effect::Baron(true),
        LolEvent::Herald => Effect::Herald(true),
    }
}

/// The keys `ms` into a preview scene (the page's drawing, "Try on the
/// keyboard"); `None` once it is over.
pub fn scene_frame(ids: &KeyIds, s: &GamesSettings, scene: LolScene, ms: f64) -> Option<Vec<(u8, Rgb)>> {
    if ms >= scene_ms(s, scene) {
        return None;
    }
    let (health, mana) = match scene {
        LolScene::Bars { health, mana } => (health.clamp(0.0, 1.0), mana.clamp(0.0, 1.0)),
        _ => (1.0, 0.6),
    };
    let mut live = Live {
        connected: true,
        health: Some((health * 100.0, 100.0)),
        resource: Some(("MANA".into(), mana * 100.0, 100.0)),
        ..Live::default()
    };
    // The scene shows what it is about, whatever else is turned off.
    let mut s = s.clone();
    match scene {
        LolScene::Bars { .. } => s.lol.bars = true,
        LolScene::Death => {
            s.lol.death = true;
            live.dead = true;
            live.respawn_total = DEATH_PREVIEW_MS / 1_000.0;
            live.respawn_at = DEATH_PREVIEW_MS;
        }
        LolScene::Event { event } => {
            s.lol.events = true;
            let light = s.lol.lights.get(event);
            let on = crate::dto::LolEventLight { on: true, ..light };
            set_light(&mut s.lol, event, on);
            live.effects.push((scene_effect(event), 0.0));
        }
    }
    Some(frame(&live, ids, &s, ms))
}

fn set_light(l: &mut LolSettings, e: LolEvent, light: crate::dto::LolEventLight) {
    let slot = match e {
        LolEvent::Win => &mut l.lights.win,
        LolEvent::Lose => &mut l.lights.lose,
        LolEvent::LevelUp => &mut l.lights.level_up,
        LolEvent::Kill => &mut l.lights.kill,
        LolEvent::Ace => &mut l.lights.ace,
        LolEvent::Dragon => &mut l.lights.dragon,
        LolEvent::Baron => &mut l.lights.baron,
        LolEvent::Herald => &mut l.lights.herald,
    };
    *slot = light;
}

/// Every frame of a preview scene, `PREVIEW_STEP_MS` apart.
pub fn preview(ids: &KeyIds, s: &GamesSettings, scene: LolScene) -> Vec<Keys> {
    let mut frames = Vec::new();
    let mut ms = 0.0;
    while let Some(f) = scene_frame(ids, s, scene, ms) {
        frames.push(f);
        // The bars don't move: one frame is enough.
        if matches!(scene, LolScene::Bars { .. }) {
            break;
        }
        ms += PREVIEW_STEP_MS;
    }
    frames
}

fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (f32::from(a[i]) * (1.0 - k) + f32::from(b[i]) * k).round() as u8)
}

// ---------------------------------------------------------------- poller

/// Reads the API while wanted, on a thread of its own.
pub struct LolHandle {
    pub live: Arc<Mutex<Live>>,
    wanted: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl LolHandle {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            live: Arc::new(Mutex::new(Live { last_event: -1, ..Live::default() })),
            wanted: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// A League of Legends session runs (`true`) or not.
    pub fn want(&self, on: bool) {
        let was = self.wanted.swap(on, Ordering::SeqCst);
        if on && !was {
            if let Ok(mut l) = self.live.lock() {
                l.reset();
            }
            if !self.running.swap(true, Ordering::SeqCst) {
                let (live, wanted, running) = (self.live.clone(), self.wanted.clone(), self.running.clone());
                let _ = std::thread::Builder::new().name("lol".into()).spawn(move || {
                    poll(&live, &wanted);
                    running.store(false, Ordering::SeqCst);
                });
            }
        }
    }

    pub fn snapshot(&self) -> Option<Live> {
        self.live.lock().ok().map(|l| l.clone())
    }
}

fn poll(live: &Mutex<Live>, wanted: &AtomicBool) {
    tracing::info!("reading League of Legends' match data");
    let mut pass = 0u32;
    while wanted.load(Ordering::SeqCst) {
        let now = crate::live::now_ms();
        let Ok(active) = net::get_json(&format!("{BASE}/activeplayer")) else {
            if let Ok(mut l) = live.lock() {
                l.connected = false;
            }
            std::thread::sleep(RETRY);
            continue;
        };
        let slow = pass % SLOW_EVERY == 0;
        let players = slow.then(|| net::get_json(&format!("{BASE}/playerlist")).ok()).flatten();
        let events = slow.then(|| net::get_json(&format!("{BASE}/eventdata")).ok()).flatten();
        let stats = (pass % (SLOW_EVERY * 10) == 0).then(|| net::get_json(&format!("{BASE}/gamestats")).ok()).flatten();
        if let Ok(mut l) = live.lock() {
            if !l.connected {
                tracing::info!("League of Legends' match data answers");
            }
            l.connected = true;
            l.read_active(&active, now);
            if let Some(p) = &players {
                l.read_players(p, now);
            }
            if let Some(e) = &events {
                l.read_events(e, now);
            }
            if let Some(g) = &stats {
                l.mode = g["gameMode"].as_str().unwrap_or_default().to_string();
            }
        }
        pass = pass.wrapping_add(1);
        std::thread::sleep(POLL);
    }
    tracing::info!("stopped reading League of Legends' match data");
}

#[cfg(windows)]
mod net {
    use serde_json::Value;
    use std::time::{Duration, Instant};
    use windows::core::HSTRING;
    use windows::Foundation::Uri;
    use windows::Security::Cryptography::Certificates::ChainValidationResult;
    use windows::Web::Http::Filters::{HttpBaseProtocolFilter, HttpCacheReadBehavior};
    use windows::Web::Http::HttpClient;

    const TIMEOUT: Duration = Duration::from_secs(2);
    /// Answers are a few kB; anything larger is not the game's.
    const MAX_BYTES: usize = 512 * 1024;

    thread_local! {
        static CLIENT: Option<HttpClient> = client().ok();
    }

    /// Windows' HTTP client accepting the game's own certificate: Riot's
    /// root is not one Windows trusts, and its name is not 127.0.0.1.
    fn client() -> windows::core::Result<HttpClient> {
        let filter = HttpBaseProtocolFilter::new()?;
        filter.SetAllowUI(false)?;
        // Never leave 127.0.0.1:2999, not even when told to go elsewhere.
        filter.SetAllowAutoRedirect(false)?;
        filter.CacheControl()?.SetReadBehavior(HttpCacheReadBehavior::NoCache)?;
        let ignore = filter.IgnorableServerCertificateErrors()?;
        ignore.Append(ChainValidationResult::Untrusted)?;
        ignore.Append(ChainValidationResult::InvalidName)?;
        HttpClient::Create(&filter)
    }

    pub fn get_json(url: &str) -> Result<Value, String> {
        // Only the game's own address: nothing else is ever asked this way.
        if !url.starts_with(super::BASE) {
            return Err("not the game's address".into());
        }
        let client = CLIENT.with(|c| c.clone()).ok_or("no HTTP client")?;
        let uri = Uri::CreateUri(&HSTRING::from(url)).map_err(|e| e.message())?;
        let op = client.GetStringAsync(&uri).map_err(|e| e.message())?;
        // AsyncStatus: 0 started, 1 completed, 2 canceled, 3 error.
        let started = Instant::now();
        loop {
            match op.Status().map_err(|e| e.message())?.0 {
                0 if started.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(10)),
                0 => {
                    let _ = op.Cancel();
                    return Err("timed out".into());
                }
                1 => break,
                _ => return Err("no answer".into()),
            }
        }
        let text = op.GetResults().map_err(|e| e.message())?.to_string_lossy();
        if text.len() > MAX_BYTES {
            return Err("answer too large".into());
        }
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }
}

#[cfg(not(windows))]
mod net {
    pub fn get_json(_url: &str) -> Result<serde_json::Value, String> {
        Err("no HTTP client on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn ids() -> KeyIds {
        let mut by_id = HashMap::new();
        let mut all = Vec::new();
        let mut x = HashMap::new();
        let mut y = HashMap::new();
        for (i, k) in NUMBER_ROW.iter().chain(F_ROW.iter()).chain(["q", "w"].iter()).enumerate() {
            by_id.insert((*k).to_string(), i as u8);
            all.push(i as u8);
            x.insert(i as u8, i as f32 / 23.0);
            y.insert(
                i as u8,
                if i < 10 {
                    0.4
                } else if i < 22 {
                    0.0
                } else {
                    0.8
                },
            );
        }
        KeyIds { all, by_id, x, y }
    }

    /// Health on the number row, as before the whole keyboard.
    fn on_the_row() -> GamesSettings {
        let mut s = GamesSettings::default();
        s.lol.layout = LolLayout::Row;
        s
    }

    fn active(hp: f64, max: f64, level: u64) -> Value {
        json!({
            "riotId": "Oyuncu#TR1",
            "level": level,
            "championStats": { "currentHealth": hp, "maxHealth": max, "resourceType": "MANA", "resourceValue": 50.0, "resourceMax": 100.0 }
        })
    }

    #[test]
    fn health_and_mana_become_bars() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(450.0, 1000.0, 3), 0.0);
        let s = on_the_row();
        let mana = s.lol.mana_color;
        let f = frame(&live, &ids(), &s, 0.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        // 45 % health: four keys and half of the fifth, amber.
        assert_eq!(at(0), HP_AMBER);
        assert_eq!(at(3), HP_AMBER);
        assert_eq!(at(4), scale(HP_AMBER, 0.5));
        assert_eq!(at(9), scale(HP_AMBER, 0.06));
        // Mana at half of the F row (leds 10–21).
        assert_eq!(at(10), mana);
        assert_eq!(at(21), scale(mana, 0.06));
    }

    #[test]
    fn health_on_the_whole_keyboard_recedes_the_chosen_way() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(500.0, 1000.0, 3), 0.0);
        let mut s = GamesSettings::default();
        assert_eq!(s.lol.layout, LolLayout::Keyboard, "the whole keyboard is the default");
        s.lol.mana = false;
        s.lol.health_color = Some([10, 200, 10]);
        let f = frame(&live, &ids(), &s, 0.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        // Right to left: the left half lit, the right half faint (q and w
        // are on the far right of this little keyboard).
        assert_eq!(at(0), [10, 200, 10]);
        assert_eq!(at(23), scale([10, 200, 10], TRACK));
        // Left to right: the other way round.
        s.lol.motion = crate::dto::PomodoroMotion::LeftToRight;
        let f = frame(&live, &ids(), &s, 0.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        assert_eq!(at(0), scale([10, 200, 10], TRACK));
        assert_eq!(at(23), [10, 200, 10]);
    }

    #[test]
    fn an_event_plays_the_users_animation_in_the_users_colour() {
        let mut s = GamesSettings::default();
        s.lol.lights.win = crate::dto::LolEventLight { on: true, anim: LolAnim::Flash, color: [1, 2, 3] };
        // Flash: the whole keyboard at the start.
        let f = scene_frame(&ids(), &s, LolScene::Event { event: LolEvent::Win }, 0.0).unwrap();
        assert!(f.iter().all(|k| k.1 == [1, 2, 3]));
        // Over after one play.
        assert_eq!(scene_ms(&s, LolScene::Event { event: LolEvent::Win }), 900.0);
        assert!(scene_frame(&ids(), &s, LolScene::Event { event: LolEvent::Win }, 900.0).is_none());
        // Turned off in a match: nothing; the preview still shows it.
        s.lol.lights.win.on = false;
        let mut live = Live { connected: true, ..Live::default() };
        live.effects.push((Effect::Win, 0.0));
        assert!(frame(&live, &ids(), &s, 10.0).iter().all(|k| k.1 != [1, 2, 3]));
        assert_eq!(preview(&ids(), &s, LolScene::Event { event: LolEvent::Win }).len(), 23);
        // A dragon in its kind's colour, unless the user picked one.
        s.lol.lights.dragon = crate::dto::LolEventLight { on: true, anim: LolAnim::Flash, color: [9, 9, 9] };
        let f = scene_frame(&ids(), &s, LolScene::Event { event: LolEvent::Dragon }, 0.0).unwrap();
        assert_eq!(f[0].1, dragon_color("fire"));
        s.lol.dragon_kind_color = false;
        let f = scene_frame(&ids(), &s, LolScene::Event { event: LolEvent::Dragon }, 0.0).unwrap();
        assert_eq!(f[0].1, [9, 9, 9]);
        // A streak plays it once per kill, quicker.
        assert_eq!(Effect::Kill(3).length_ms(LolAnim::Flash), 900.0 * 0.6 * 3.0);
    }

    #[test]
    fn the_end_of_the_match_has_the_whole_keyboard() {
        let mut s = GamesSettings::default();
        s.lol.lights.win = crate::dto::LolEventLight { on: true, anim: LolAnim::Flash, color: [1, 2, 3] };
        s.lol.lights.lose = crate::dto::LolEventLight { on: true, anim: LolAnim::Flash, color: [4, 5, 6] };
        let ids = ids();
        let plain = frame(&Live { connected: true, ..Live::default() }, &ids, &s, 0.0);
        for (result, color) in [("Win", [1, 2, 3]), ("Lose", [4, 5, 6])] {
            // Half health, the player dead, when the match ends.
            let mut live = Live { connected: true, last_event: -1, ..Live::default() };
            live.read_active(&active(500.0, 1000.0, 3), 0.0);
            live.read_players(
                &json!([{ "riotId": "Oyuncu#TR1", "isDead": true, "respawnTimer": 10.0, "scores": {} }]),
                0.0,
            );
            live.read_events(&json!({ "Events": [{ "EventID": 0, "EventName": "GameStart" }] }), 0.0);
            live.read_events(
                &json!({ "Events": [{ "EventID": 1, "EventName": "GameEnd", "Result": result }] }),
                1_000.0,
            );
            // A flash: every key, the lit half of health too.
            assert!(frame(&live, &ids, &s, 1_000.0).iter().all(|k| k.1 == color), "{result}");
            // Between the flashes: dark, not health, mana or death.
            assert!(frame(&live, &ids, &s, 1_300.0).iter().all(|k| k.1 == [0, 0, 0]), "{result}");
            // After it: the game's own lights, without the match's health.
            assert_eq!(frame(&live, &ids, &s, 2_000.0), plain, "{result}");
        }
    }

    #[test]
    fn dead_counts_down_and_level_up_waves() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(0.0, 1000.0, 3), 0.0);
        live.read_players(&json!([{ "riotId": "Oyuncu#TR1", "isDead": true, "respawnTimer": 10.0, "championName": "Ahri", "team": "ORDER", "scores": { "kills": 2, "deaths": 1, "assists": 3, "creepScore": 50 } }]), 0.0);
        assert!(live.dead);
        let f = frame(&live, &ids(), &on_the_row(), 5_000.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        // Half the wait left: five keys of the number row.
        assert_eq!(at(0), RESPAWN);
        assert_eq!(at(4), RESPAWN);
        assert_eq!(at(6), scale(RESPAWN, 0.06));
        assert_eq!(at(22), DEAD);
        assert_eq!(live.result().unwrap().champion, "Ahri");
        // On the whole keyboard: dark red, the light coming back from the
        // left (right to left recedes, so it fills from the left).
        let f = frame(&live, &ids(), &GamesSettings::default(), 5_000.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        assert_eq!(at(0), RESPAWN);
        assert_eq!(at(23), DEAD);
        // A level up adds an effect.
        live.read_active(&active(500.0, 1000.0, 4), 6_000.0);
        assert!(matches!(live.effects.last(), Some((Effect::LevelUp, _))));
    }

    #[test]
    fn only_new_events_light_up_and_the_end_is_kept() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(500.0, 1000.0, 1), 0.0);
        live.read_players(&json!([{ "riotId": "Oyuncu#TR1", "team": "ORDER", "isDead": false, "scores": {} }]), 0.0);
        // Events from before the lights started: nothing flashes.
        live.read_events(&json!({ "Events": [{ "EventID": 0, "EventName": "GameStart" }, { "EventID": 1, "EventName": "ChampionKill", "KillerName": "Oyuncu" }] }), 0.0);
        assert!(live.effects.is_empty());
        live.read_events(
            &json!({ "Events": [
                { "EventID": 1, "EventName": "ChampionKill", "KillerName": "Oyuncu" },
                { "EventID": 2, "EventName": "Multikill", "KillerName": "Oyuncu", "KillStreak": 3 },
                { "EventID": 3, "EventName": "ChampionKill", "KillerName": "Başkası" },
                { "EventID": 4, "EventName": "DragonKill", "DragonType": "Water" },
                { "EventID": 5, "EventName": "Ace", "Acer": "Başkası", "AcingTeam": "ORDER" },
                { "EventID": 6, "EventName": "GameEnd", "Result": "Win" }
            ]}),
            100.0,
        );
        let kinds: Vec<Effect> = live.effects.iter().map(|e| e.0).collect();
        assert_eq!(kinds, vec![Effect::Kill(3), Effect::Dragon(dragon_color("water"), true), Effect::Ace, Effect::Win]);
        assert_eq!(live.result().unwrap().win, Some(true));
    }

    #[test]
    fn a_double_kill_plays_one_animation() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(500.0, 1000.0, 1), 0.0);
        live.read_events(&json!({ "Events": [{ "EventID": 0, "EventName": "GameStart" }] }), 0.0);
        // As the game sends them: each kill, then the streak with the second.
        live.read_events(
            &json!({ "Events": [
                { "EventID": 1, "EventName": "ChampionKill", "KillerName": "Oyuncu", "VictimName": "A" }
            ]}),
            1_000.0,
        );
        live.read_events(
            &json!({ "Events": [
                { "EventID": 1, "EventName": "ChampionKill", "KillerName": "Oyuncu", "VictimName": "A" },
                { "EventID": 2, "EventName": "ChampionKill", "KillerName": "Oyuncu", "VictimName": "B" },
                { "EventID": 3, "EventName": "Multikill", "KillerName": "Oyuncu", "KillStreak": 2 }
            ]}),
            1_500.0,
        );
        assert_eq!(live.effects, vec![(Effect::Kill(2), 1_500.0)]);
        // Between the streak's flashes the keys are calm: no single kill's
        // flash mixed in.
        let mut s = GamesSettings::default();
        s.lol.lights.kill = crate::dto::LolEventLight { on: true, anim: LolAnim::Flash, color: [200, 100, 0] };
        let calm = Live { effects: Vec::new(), ..live.clone() };
        assert_eq!(frame(&live, &ids(), &s, 1_950.0), frame(&calm, &ids(), &s, 1_950.0));
        assert_ne!(frame(&live, &ids(), &s, 1_500.0), frame(&calm, &ids(), &s, 1_500.0), "it plays");
        // A kill the streak doesn't follow plays on its own.
        live.read_events(
            &json!({ "Events": [{ "EventID": 4, "EventName": "ChampionKill", "KillerName": "Oyuncu" }] }),
            20_000.0,
        );
        assert_eq!(live.effects.last(), Some(&(Effect::Kill(1), 20_000.0)));
    }

    #[test]
    fn objectives_the_other_team_takes_stay_dark_by_default() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(500.0, 1000.0, 1), 0.0);
        live.read_players(
            &json!([
                { "riotId": "Oyuncu#TR1", "team": "ORDER", "isDead": false, "scores": {} },
                { "riotId": "Dost#TR1", "team": "ORDER", "isDead": false, "scores": {} },
                { "riotId": "Rakip#EUW", "team": "CHAOS", "isDead": false, "scores": {} }
            ]),
            0.0,
        );
        live.read_events(&json!({ "Events": [{ "EventID": 0, "EventName": "GameStart" }] }), 0.0);
        live.read_events(
            &json!({ "Events": [
                { "EventID": 1, "EventName": "BaronKill", "KillerName": "Dost" },
                { "EventID": 2, "EventName": "DragonKill", "DragonType": "Fire", "KillerName": "Rakip" },
                { "EventID": 3, "EventName": "HeraldKill", "KillerName": "Oyuncu" }
            ]}),
            100.0,
        );
        let kinds: Vec<Effect> = live.effects.iter().map(|e| e.0).collect();
        assert_eq!(kinds, vec![Effect::Baron(true), Effect::Dragon(dragon_color("fire"), false), Effect::Herald(true)]);
        // The other team's dragon plays nothing unless the user wants it.
        let mut s = GamesSettings::default();
        assert!(s.lol.team_only);
        s.lol.lights.dragon.anim = LolAnim::Flash;
        live.effects = vec![(Effect::Dragon(dragon_color("fire"), false), 0.0)];
        let dark = frame(&live, &ids(), &s, 0.0);
        assert!(dark.iter().all(|k| k.1 != dragon_color("fire")));
        s.lol.team_only = false;
        let lit = frame(&live, &ids(), &s, 0.0);
        assert!(lit.iter().any(|k| k.1 == dragon_color("fire")));
    }

    #[test]
    fn tft_gets_no_lights() {
        let live = Live { connected: true, mode: "TFT".into(), ..Live::default() };
        assert!(!live.lights());
        let live = Live { connected: true, mode: "CLASSIC".into(), ..Live::default() };
        assert!(live.lights());
    }
}
