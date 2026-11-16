//! League of Legends lights (keymotion-notes: oyun-oturumu-ve-otomatik-profil.md
//! §5.3, §7 D), from Riot's Live Client Data API: an HTTPS service the game
//! itself runs on this computer (`https://127.0.0.1:2999/liveclientdata/`)
//! while a match is on. Riot documents it for third-party tools; nothing
//! is read from the game's memory and nothing is sent to it.
//!
//! * Only the player's own state, as the game shows it, and public events
//!   become light: health and mana bars, being dead and the time to
//!   respawn, level ups, kills, an ace, dragons, Baron, Herald, the end.
//!   No enemy timers, no objective spawn timers, nothing that tells the
//!   player what to do (Riot's game integrity rules).
//! * The service uses Riot's own certificate, which Windows does not
//!   trust; Riot's documentation says to accept it. Only this address on
//!   this computer is asked, and only while a League of Legends session
//!   runs.
//! * Teamfight Tactics runs in the same program; its matches get no lights.

use crate::dto::{GameRest, GamesSettings, LolResult, LolSettings};
use crate::games::KeyIds;
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
const MANA: Rgb = [61, 139, 255];
const ENERGY: Rgb = [255, 214, 10];
const FURY: Rgb = [255, 69, 58];
const DEAD: Rgb = [70, 0, 0];
const RESPAWN: Rgb = [240, 240, 240];
const GOLD: Rgb = [240, 199, 94];
const BARON: Rgb = [155, 93, 229];
const HERALD: Rgb = [196, 120, 255];
const LOSE: Rgb = [255, 40, 40];

/// A short light effect and when it began.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Effect {
    LevelUp,
    /// A kill by the player: one flash per kill of the streak.
    Kill(u8),
    Ace,
    /// A dragon, in its kind's colour.
    Dragon(Rgb),
    Baron,
    Herald,
    Win,
    Lose,
}

impl Effect {
    fn length_ms(self) -> f64 {
        match self {
            Self::LevelUp => 900.0,
            Self::Kill(n) => 260.0 * f64::from(n.clamp(1, 5)),
            Self::Ace => 1_400.0,
            Self::Dragon(_) | Self::Baron | Self::Herald => 900.0,
            Self::Win | Self::Lose => 2_500.0,
        }
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
        self.effects.retain(|(x, at)| now - at < x.length_ms());
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
                "Multikill" if self.is_me(killer) => {
                    let n = e["KillStreak"].as_u64().unwrap_or(2).min(5) as u8;
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
                    self.push(Effect::Dragon(dragon_color(e["DragonType"].as_str().unwrap_or_default())), now)
                }
                "BaronKill" => self.push(Effect::Baron, now),
                "HeraldKill" => self.push(Effect::Herald, now),
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
    if l.death && live.dead {
        for k in &mut out {
            k.1 = DEAD;
        }
        let left = ((live.respawn_at - now) / live.respawn_total.max(1.0) / 1_000.0).clamp(0.0, 1.0) as f32;
        bar(&mut out, &row(ids, &NUMBER_ROW), left, RESPAWN);
    } else if l.bars {
        if let Some((h, max)) = live.health {
            let share = h / max;
            let color = if share > 0.6 {
                HP_GREEN
            } else if share > 0.3 {
                HP_AMBER
            } else {
                HP_RED
            };
            bar(&mut out, &row(ids, &NUMBER_ROW), share, color);
        }
        if let Some((kind, v, max)) = &live.resource {
            let color = match kind.as_str() {
                "ENERGY" => ENERGY,
                "MANA" => MANA,
                _ => FURY,
            };
            bar(&mut out, &row(ids, &F_ROW), v / max, color);
        }
    }
    if l.events {
        for (e, at) in &live.effects {
            let t = ((now - at) / e.length_ms()) as f32;
            if !(0.0..1.0).contains(&t) {
                continue;
            }
            effect(&mut out, ids, *e, t);
        }
    }
    out
}

/// One effect at `t` (0 → 1) over the frame.
fn effect(out: &mut [(u8, Rgb)], ids: &KeyIds, e: Effect, t: f32) {
    let flash = |out: &mut [(u8, Rgb)], color: Rgb, k: f32| {
        for led in out.iter_mut() {
            led.1 = mix(led.1, color, k);
        }
    };
    let wave = |out: &mut [(u8, Rgb)], color: Rgb| {
        // A band moving left to right.
        let pos = t * 1.3 - 0.15;
        for led in out.iter_mut() {
            let x = ids.x.get(&led.0).copied().unwrap_or(0.5);
            let k = (1.0 - ((x - pos).abs() / 0.15)).clamp(0.0, 1.0);
            led.1 = mix(led.1, color, k);
        }
    };
    match e {
        Effect::LevelUp => wave(out, GOLD),
        Effect::Kill(n) => {
            // n quick flashes.
            let phase = (t * f32::from(n.max(1))).fract();
            flash(out, GOLD, if phase < 0.5 { 1.0 - phase * 2.0 } else { 0.0 });
        }
        Effect::Ace => flash(out, GOLD, (1.0 - t).powf(0.7)),
        Effect::Dragon(c) => flash(out, c, ((t * 2.0).fract() < 0.5) as u8 as f32 * (1.0 - t)),
        Effect::Baron => flash(out, BARON, ((t * 2.0).fract() < 0.5) as u8 as f32 * (1.0 - t)),
        Effect::Herald => flash(out, HERALD, (1.0 - t) * 0.8),
        Effect::Win => wave(out, GOLD),
        Effect::Lose => flash(out, LOSE, (1.0 - t) * 0.9),
    }
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
        for (i, k) in NUMBER_ROW.iter().chain(F_ROW.iter()).chain(["q", "w"].iter()).enumerate() {
            by_id.insert((*k).to_string(), i as u8);
            all.push(i as u8);
            x.insert(i as u8, i as f32 / 23.0);
        }
        KeyIds { all, by_id, x }
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
        let s = GamesSettings::default();
        let f = frame(&live, &ids(), &s, 0.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        // 45 % health: four keys and half of the fifth, amber.
        assert_eq!(at(0), HP_AMBER);
        assert_eq!(at(3), HP_AMBER);
        assert_eq!(at(4), scale(HP_AMBER, 0.5));
        assert_eq!(at(9), scale(HP_AMBER, 0.06));
        // Mana at half of the F row (leds 10–21).
        assert_eq!(at(10), MANA);
        assert_eq!(at(21), scale(MANA, 0.06));
    }

    #[test]
    fn dead_counts_down_and_level_up_waves() {
        let mut live = Live { connected: true, last_event: -1, ..Live::default() };
        live.read_active(&active(0.0, 1000.0, 3), 0.0);
        live.read_players(&json!([{ "riotId": "Oyuncu#TR1", "isDead": true, "respawnTimer": 10.0, "championName": "Ahri", "team": "ORDER", "scores": { "kills": 2, "deaths": 1, "assists": 3, "creepScore": 50 } }]), 0.0);
        assert!(live.dead);
        let f = frame(&live, &ids(), &GamesSettings::default(), 5_000.0);
        let at = |led: u8| f.iter().find(|k| k.0 == led).unwrap().1;
        // Half the wait left: five keys of the number row.
        assert_eq!(at(0), RESPAWN);
        assert_eq!(at(4), RESPAWN);
        assert_eq!(at(6), scale(RESPAWN, 0.06));
        assert_eq!(at(22), DEAD);
        assert_eq!(live.result().unwrap().champion, "Ahri");
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
        assert_eq!(kinds, vec![Effect::Kill(3), Effect::Dragon(dragon_color("water")), Effect::Ace, Effect::Win]);
        assert_eq!(live.result().unwrap().win, Some(true));
    }

    #[test]
    fn tft_gets_no_lights() {
        let live = Live { connected: true, mode: "TFT".into(), ..Live::default() };
        assert!(!live.lights());
        let live = Live { connected: true, mode: "CLASSIC".into(), ..Live::default() };
        assert!(live.lights());
    }
}
