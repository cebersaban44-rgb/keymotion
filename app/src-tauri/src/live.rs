//! Live screen: cards that update the GIF page by themselves (phase C).
//!
//! Every upload erases flash sectors, so automatic uploads run on a budget
//! (token bucket: `plan.per_day()` a day, at most `plan.burst()` back to
//! back). Sources hand in finished cards; only the newest waiting card is
//! kept, the same content is not uploaded twice, and a card that cannot go
//! now (budget, no USB cable, another upload) waits and goes later.
//! Uploads the user starts by hand use the budget too but are never
//! blocked by it (docs/research/ekran-ve-veri-kaynaklari.md §3.4).

use crate::device::DeviceHandle;
use crate::dto::{LiveBack, LiveItem, LivePlan, LiveShown, LiveState, SongStop, TrackInfo};
use crate::screen_lib::ScreenLibrary;
use kb_protocol::sonix::screen::ScreenAnimation;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const EVENT_LIVE: &str = "live-screen";
/// Retry a waiting card this often while the keyboard is not reachable.
const RETRY_EVERY: Duration = Duration::from_secs(10);
/// Longest idle wait of the worker.
const IDLE: Duration = Duration::from_secs(60);
const HOUR_MS: f64 = 3_600_000.0;

pub fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0)
}

/// Token bucket for screen uploads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Budget {
    pub plan: LivePlan,
    tokens: f64,
    /// When `tokens` was last brought up to date (0: never).
    updated_ms: f64,
}

impl Default for Budget {
    fn default() -> Self {
        let plan = LivePlan::default();
        Self { plan, tokens: plan.burst() as f64, updated_ms: 0.0 }
    }
}

impl Budget {
    fn per_hour(&self) -> f64 {
        self.plan.per_day() as f64 / 24.0
    }

    fn cap(&self) -> f64 {
        self.plan.burst() as f64
    }

    fn refill(&mut self, now: f64) {
        if self.updated_ms > 0.0 && now > self.updated_ms {
            let hours = (now - self.updated_ms) / HOUR_MS;
            self.tokens = (self.tokens + hours * self.per_hour()).min(self.cap());
        }
        self.updated_ms = now;
    }

    pub fn tokens(&mut self, now: f64) -> f64 {
        self.refill(now);
        self.tokens
    }

    /// Take one upload for the live mode, if one is available.
    pub fn try_take(&mut self, now: f64) -> bool {
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// A manual upload: it uses the budget but is never refused. The debt
    /// is capped so the live mode is not locked out for long.
    pub fn charge(&mut self, now: f64) {
        self.refill(now);
        self.tokens = (self.tokens - 1.0).max(-self.cap());
    }

    /// Give back a token for an upload that never reached the flash.
    pub fn refund(&mut self, now: f64) {
        self.refill(now);
        self.tokens = (self.tokens + 1.0).min(self.cap());
    }

    /// Time until one upload is available, `None` if one is available now.
    pub fn ms_until_token(&mut self, now: f64) -> Option<f64> {
        self.refill(now);
        (self.tokens < 1.0).then(|| (1.0 - self.tokens) / self.per_hour() * HOUR_MS)
    }

    pub fn set_plan(&mut self, plan: LivePlan, now: f64) {
        self.refill(now);
        self.plan = plan;
        self.tokens = self.tokens.min(self.cap());
    }
}

/// Live settings and history, stored with the screen library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LiveStored {
    pub enabled: bool,
    pub budget: Budget,
    pub now_playing: bool,
    /// Content id of the last live card on the GIF page.
    pub last_content: Option<String>,
    pub last_source: Option<String>,
    pub last_name: Option<String>,
    pub last_ms: Option<f64>,
    /// Library item the GIF page showed before the first live card.
    pub before: Option<String>,
    /// What "Çalan şarkı" does when the music stops.
    pub song_stop: SongStop,
    /// The user accepted, once, that live cards erase the factory animation
    /// for good when there is no backup of it.
    pub factory_ack: bool,
}

impl Default for LiveStored {
    fn default() -> Self {
        Self {
            enabled: false,
            budget: Budget::default(),
            now_playing: true,
            last_content: None,
            last_source: None,
            last_name: None,
            last_ms: None,
            before: None,
            song_stop: SongStop::default(),
            factory_ack: false,
        }
    }
}

/// Turning the live mode on must ask first (see `LiveState::factory_ack_needed`).
fn factory_ack_needed(lib: &ScreenLibrary) -> bool {
    !lib.live().factory_ack && lib.factory_at_risk()
}

struct Pending {
    anim: ScreenAnimation,
    content: String,
    item: LiveItem,
}

#[derive(Default)]
struct Runtime {
    pending: Option<Pending>,
    uploading: Option<LiveItem>,
    last_error: Option<String>,
    /// The keyboard was not reachable: wait before trying again.
    retry_at: Option<f64>,
    track: Option<TrackInfo>,
    /// Library item to put back on the GIF page.
    restore: Option<String>,
    /// The put-back above is the live mode going back by itself, for this
    /// source's card; `None`: the user asked for it.
    restore_auto: Option<String>,
    /// The user asked to send the waiting card now, without waiting for
    /// the budget (it is still counted).
    send_now: bool,
    /// A source has nothing to show any more: go back to the user's own
    /// picture at this time, unless another card comes first.
    back: Option<LiveBack>,
    /// Sources holding the screen while they have something to show: the
    /// calendar (around a meeting) over a reminder's card, both over the
    /// focus timer, all over "Çalan şarkı", which waits while anyone holds.
    holders: Vec<String>,
}

/// What to do about a scheduled return to the user's own picture.
#[derive(Debug, Clone, Copy, PartialEq)]
enum BackStep {
    /// Not yet: check again in this many ms.
    Wait(f64),
    /// Nothing to do any more (another card is coming, or the source's card
    /// is gone from the screen).
    Drop,
    /// Put the user's picture back now.
    Go,
}

/// `showing`: the source's card is on the GIF page; `busy`: an upload runs;
/// `ready`: another card waits and can go now (it replaces the screen
/// anyway). A card that waits for the budget does not stop the return: the
/// old card would stay on the screen until then (owner's test, 2026-11-10).
fn back_step(back: &LiveBack, now: f64, enabled: bool, showing: bool, busy: bool, ready: bool) -> BackStep {
    if !enabled || ready {
        return BackStep::Drop;
    }
    if now < back.at_ms {
        return BackStep::Wait(back.at_ms - now);
    }
    if busy {
        // The upload may be this source's card: decide when it is done.
        return BackStep::Wait(RETRY_EVERY.as_millis() as f64 / 10.0);
    }
    if showing {
        BackStep::Go
    } else {
        BackStep::Drop
    }
}

enum Msg {
    Wake,
}

/// Handle to the live worker (managed Tauri state).
pub struct LiveHandle {
    tx: Sender<Msg>,
    rt: Arc<Mutex<Runtime>>,
    library: Arc<Mutex<ScreenLibrary>>,
    app: AppHandle,
}

impl LiveHandle {
    pub fn spawn(app: AppHandle, library: Arc<Mutex<ScreenLibrary>>, device: Arc<DeviceHandle>) -> Arc<Self> {
        let (tx, rx) = mpsc::channel();
        let rt = Arc::new(Mutex::new(Runtime::default()));
        let handle = Arc::new(Self { tx, rt: rt.clone(), library: library.clone(), app: app.clone() });
        let worker = handle.clone();
        std::thread::Builder::new()
            .name("live-screen".into())
            .spawn(move || loop {
                let wait = worker.next_wait();
                match rx.recv_timeout(wait) {
                    Ok(Msg::Wake) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                worker.try_upload(&device);
            })
            .expect("spawn live-screen thread");
        handle
    }

    fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    /// Current state for the UI.
    pub fn state(&self) -> Result<LiveState, String> {
        let mut lib = self.library.lock().map_err(|_| "screen library unavailable")?;
        let rt = self.rt.lock().map_err(|_| "live state unavailable")?;
        Ok(snapshot(&mut lib, &rt))
    }

    fn emit(&self) {
        if let Ok(s) = self.state() {
            let _ = self.app.emit(EVENT_LIVE, s);
        }
    }

    /// `restore`: put back what the GIF page showed before the live mode,
    /// if a live card is there now (with `enabled: false`, or on its own
    /// while the live mode stays on).
    pub fn configure(
        &self,
        enabled: Option<bool>,
        plan: Option<LivePlan>,
        now_playing: Option<bool>,
        restore: Option<bool>,
        song_stop: Option<SongStop>,
    ) -> Result<LiveState, String> {
        let mut put_back = None;
        {
            let mut lib = self.library.lock().map_err(|_| "screen library unavailable")?;
            if enabled == Some(true) && !lib.live().enabled && factory_ack_needed(&lib) {
                return Err("live mode would erase the factory animation; not acknowledged".into());
            }
            if restore == Some(true) && enabled != Some(true) {
                put_back = lib.live_restore_target().map(|r| r.id);
            }
            let now = now_ms();
            let live = lib.live_mut();
            if let Some(e) = enabled {
                live.enabled = e;
            }
            if let Some(p) = plan {
                live.budget.set_plan(p, now);
            }
            if let Some(n) = now_playing {
                live.now_playing = n;
            }
            if let Some(k) = song_stop {
                live.song_stop = k;
            }
            lib.save().map_err(|e| e.to_string())?;
        }
        if let Ok(mut rt) = self.rt.lock() {
            if enabled == Some(false) {
                rt.pending = None;
                rt.back = None;
            }
            let song_back = rt.back.as_ref().is_some_and(|b| b.source == crate::now_playing::SOURCE);
            if song_stop == Some(SongStop::Keep) && song_back {
                rt.back = None;
            }
            if put_back.is_some() {
                tracing::info!(live_off = enabled == Some(false), "putting the previous picture back");
                rt.restore = put_back;
                rt.restore_auto = None;
                rt.back = None;
                rt.retry_at = None;
            }
        }
        self.wake();
        self.emit();
        self.state()
    }

    /// The user accepted that live cards erase the factory animation (no
    /// backup of it): turning the live mode on no longer asks.
    pub fn ack_factory(&self) -> Result<LiveState, String> {
        {
            let mut lib = self.library.lock().map_err(|_| "screen library unavailable")?;
            lib.live_mut().factory_ack = true;
            lib.save().map_err(|e| e.to_string())?;
        }
        tracing::info!("factory animation loss acknowledged for the live mode");
        self.emit();
        self.state()
    }

    /// A finished card from a source. Ignored while the live mode is off
    /// or when the GIF page already shows exactly this content.
    pub fn submit(&self, anim: ScreenAnimation, source: &str, name: &str) -> Result<LiveState, String> {
        let content = crate::screen_lib::content_id(&kb_protocol::sonix::screen::blob_unpadded(&anim));
        {
            let lib = self.library.lock().map_err(|_| "screen library unavailable")?;
            if !lib.live().enabled {
                return Err("live mode is off".into());
            }
            if lib.live_is_showing(&content) {
                tracing::debug!(%source, "live card unchanged; not uploaded again");
                drop(lib);
                return self.state();
            }
        }
        {
            let mut rt = self.rt.lock().map_err(|_| "live state unavailable")?;
            if rt.pending.as_ref().is_some_and(|p| p.content == content) {
                drop(rt);
                return self.state();
            }
            // A return that is due stays: if this card has to wait for the
            // budget, the old card should not stay on the screen meanwhile.
            // The newest card wins.
            rt.pending = Some(Pending {
                anim,
                content,
                item: LiveItem { source: source.to_string(), name: crate::screen_lib::clean_name(name) },
            });
        }
        tracing::info!(%source, "live card waiting");
        self.wake();
        self.emit();
        self.state()
    }

    /// A manual upload used the budget.
    pub fn changed(&self) {
        self.wake();
        self.emit();
    }

    pub fn set_track(&self, track: Option<TrackInfo>) {
        if let Ok(mut rt) = self.rt.lock() {
            rt.track = track;
        }
        self.emit();
    }

    pub fn now_playing_wanted(&self) -> bool {
        self.library.lock().map(|l| l.live().enabled && l.live().now_playing).unwrap_or(false)
    }

    pub fn enabled(&self) -> bool {
        self.library.lock().map(|l| l.live().enabled).unwrap_or(false)
    }

    /// A card could go now without waiting for the budget. A reminder's card
    /// that would have to wait is not sent at all: late, it means nothing.
    pub fn budget_ready(&self) -> bool {
        self.library.lock().map(|mut l| l.live_mut().budget.tokens(now_ms()) >= 1.0).unwrap_or(false)
    }

    /// `source` holds the screen (`on`) or lets go. Lower sources wait
    /// while a higher one holds: the calendar over the focus timer, both
    /// over "Çalan şarkı".
    pub fn set_hold(&self, source: &str, on: bool) {
        let changed = self
            .rt
            .lock()
            .map(|mut rt| {
                let has = rt.holders.iter().any(|h| h == source);
                if on && !has {
                    rt.holders.push(source.to_string());
                } else if !on && has {
                    rt.holders.retain(|h| h != source);
                }
                on != has
            })
            .unwrap_or(false);
        if changed {
            tracing::debug!(%source, on, "live screen hold");
            self.emit();
        }
    }

    /// Anyone holds the screen ("Çalan şarkı" waits).
    pub fn held(&self) -> bool {
        self.rt.lock().map(|rt| !rt.holders.is_empty()).unwrap_or(false)
    }

    pub fn held_by(&self, source: &str) -> bool {
        self.rt.lock().map(|rt| rt.holders.iter().any(|h| h == source)).unwrap_or(false)
    }

    /// Forget a card from `source` that has not gone yet.
    pub fn drop_pending(&self, source: &str) {
        let dropped = self
            .rt
            .lock()
            .map(|mut rt| {
                let mine = rt.pending.as_ref().is_some_and(|p| p.item.source == source);
                if mine {
                    rt.pending = None;
                }
                mine
            })
            .unwrap_or(false);
        if dropped {
            tracing::debug!(%source, "waiting live card dropped");
            self.emit();
        }
    }

    /// A card from `source` is on the GIF page, going there, or waiting.
    pub fn has_card_from(&self, source: &str) -> bool {
        let showing = self.library.lock().map(|l| l.live_source_showing(source)).unwrap_or(false);
        let queued = self
            .rt
            .lock()
            .map(|rt| {
                rt.pending.as_ref().is_some_and(|p| p.item.source == source)
                    || rt.uploading.as_ref().is_some_and(|u| u.source == source)
            })
            .unwrap_or(false);
        showing || queued
    }

    /// A song plays now (as the "Çalan şarkı" source last saw it).
    pub fn track_playing(&self) -> bool {
        self.rt.lock().map(|rt| rt.track.as_ref().is_some_and(|t| t.playing)).unwrap_or(false)
    }

    pub fn song_stop(&self) -> SongStop {
        self.library.lock().map(|l| l.live().song_stop).unwrap_or_default()
    }

    /// `source` has nothing to show any more: go back to the user's own
    /// picture after `delay`, unless another card comes first. Only when
    /// the source's card is on the screen (or going there) now.
    pub fn return_after(&self, source: &str, delay: Duration) {
        if !self.enabled() || !self.has_card_from(source) {
            return;
        }
        if let Ok(mut rt) = self.rt.lock() {
            rt.back = Some(LiveBack { source: source.to_string(), at_ms: now_ms() + delay.as_millis() as f64 });
        }
        tracing::info!(%source, secs = delay.as_secs(), "going back to the own picture soon");
        self.wake();
        self.emit();
    }

    /// Send the waiting card now instead of waiting for the budget. Asked
    /// for by the user, so it is counted like a manual upload.
    pub fn send_now(&self) -> Result<LiveState, String> {
        {
            let mut rt = self.rt.lock().map_err(|_| "live state unavailable")?;
            if rt.pending.is_none() {
                return Err("no card is waiting".into());
            }
            rt.send_now = true;
            rt.retry_at = None;
        }
        tracing::info!("sending the waiting live card now");
        self.wake();
        self.state()
    }

    /// `source` has something to show again: forget its return.
    pub fn cancel_return(&self, source: &str) {
        let cancelled = self
            .rt
            .lock()
            .map(|mut rt| {
                let mine = rt.back.as_ref().is_some_and(|b| b.source == source);
                if mine {
                    rt.back = None;
                }
                mine
            })
            .unwrap_or(false);
        if cancelled {
            tracing::debug!(%source, "return to the own picture cancelled");
            self.emit();
        }
    }

    fn next_wait(&self) -> Duration {
        let Ok(mut lib) = self.library.lock() else { return IDLE };
        let Ok(rt) = self.rt.lock() else { return IDLE };
        let now = now_ms();
        if rt.restore.is_some() {
            return Duration::from_millis(rt.retry_at.map_or(0.0, |t| (t - now).max(0.0)) as u64);
        }
        if rt.pending.is_none() && lib.live().enabled {
            if let Some(b) = &rt.back {
                return Duration::from_millis((b.at_ms - now).clamp(0.0, IDLE.as_millis() as f64) as u64);
            }
        }
        if rt.pending.is_none() || !lib.live().enabled {
            return IDLE;
        }
        let mut wait = IDLE.as_millis() as f64;
        if let Some(t) = rt.retry_at {
            wait = wait.min((t - now).max(0.0));
        }
        if let Some(b) = &rt.back {
            wait = wait.min((b.at_ms - now).max(0.0));
        }
        if let Some(ms) = lib.live_mut().budget.ms_until_token(now) {
            wait = wait.min(ms + 50.0);
        }
        Duration::from_millis(wait.max(0.0) as u64)
    }

    /// Put a library item back on the GIF page: asked for by the user, or
    /// the screen going back to the own picture. Counted like a manual
    /// upload; it does not wait for the budget.
    fn restore(&self, device: &DeviceHandle) -> bool {
        let now = now_ms();
        let (id, anim) = {
            let Ok(mut lib) = self.library.lock() else { return false };
            let Ok(mut rt) = self.rt.lock() else { return false };
            if rt.uploading.is_some() || rt.retry_at.is_some_and(|t| now < t) {
                return rt.restore.is_some();
            }
            let Some(id) = rt.restore.take() else { return false };
            if rt.restore_auto.is_some() && card_ready(&mut lib, &rt, now) {
                // A new card that can go came meanwhile: it replaces the
                // screen, so going back would be an upload for nothing.
                rt.restore_auto = None;
                tracing::debug!("going back skipped; a new card goes instead");
                return false;
            }
            let name = lib.state("").items.into_iter().find(|i| i.id == id).map(|i| i.name);
            match (lib.animation(&id), name) {
                (Ok(anim), Some(name)) => {
                    rt.uploading = Some(LiveItem { source: "restore".into(), name });
                    (id, anim)
                }
                _ => {
                    tracing::warn!(%id, "previous picture is gone from the library; nothing to put back");
                    rt.restore_auto = None;
                    return false;
                }
            }
        };
        let auto = self.rt.lock().map(|rt| rt.restore_auto.clone()).unwrap_or(None);
        self.emit();
        let result = device.upload_screen(id.clone(), anim);
        {
            let Ok(mut lib) = self.library.lock() else { return true };
            let Ok(mut rt) = self.rt.lock() else { return true };
            rt.uploading = None;
            rt.restore_auto = None;
            match &result.error {
                None => {
                    let model = result.model_id.clone().unwrap_or_default();
                    if let Err(e) = lib.uploaded(&model, &id) {
                        tracing::warn!("restore not recorded: {e}");
                    }
                    lib.live_mut().before = None;
                    let _ = lib.save();
                    rt.last_error = None;
                    tracing::info!(%id, auto = auto.is_some(), "previous picture put back");
                }
                Some(e) => {
                    tracing::warn!(flash = result.flash_written, "putting the previous picture back failed: {e}");
                    let now = now_ms();
                    if result.flash_written {
                        lib.count_upload(true);
                        let _ = lib.save();
                    }
                    rt.last_error = Some(e.clone());
                    rt.retry_at = Some(now + RETRY_EVERY.as_millis() as f64);
                    match auto {
                        // Try again through the budget, if still wanted then.
                        Some(source) => {
                            rt.back = Some(LiveBack { source, at_ms: now + RETRY_EVERY.as_millis() as f64 });
                        }
                        None => rt.restore = Some(id),
                    }
                }
            }
        }
        self.emit();
        true
    }

    fn try_upload(&self, device: &DeviceHandle) {
        if self.restore(device) {
            self.wake();
            return;
        }
        let now = now_ms();
        let job = {
            let Ok(mut lib) = self.library.lock() else { return };
            let Ok(mut rt) = self.rt.lock() else { return };
            if go_back(&mut lib, &mut rt, now) {
                drop(rt);
                drop(lib);
                self.emit();
                self.wake();
                return;
            }
            if !lib.live().enabled || rt.uploading.is_some() || rt.pending.is_none() {
                return;
            }
            if rt.retry_at.is_some_and(|t| now < t) {
                return;
            }
            if std::mem::take(&mut rt.send_now) {
                // The user's choice: counted, never refused.
                lib.live_mut().budget.charge(now);
            } else if !lib.live_mut().budget.try_take(now) {
                drop(rt);
                drop(lib);
                self.emit();
                return;
            }
            let _ = lib.save();
            let job = rt.pending.take().expect("checked above");
            rt.uploading = Some(job.item.clone());
            rt.retry_at = None;
            job
        };
        self.emit();
        tracing::info!(source = %job.item.source, "live upload starting");
        let result = device.upload_screen(format!("live:{}", job.item.source), job.anim.clone());
        {
            let Ok(mut lib) = self.library.lock() else { return };
            let Ok(mut rt) = self.rt.lock() else { return };
            rt.uploading = None;
            let now = now_ms();
            match &result.error {
                None => {
                    let model = result.model_id.clone().unwrap_or_default();
                    if let Err(e) = lib.live_uploaded(&model, &job.anim, &job.content, &job.item) {
                        tracing::warn!("live upload not recorded: {e}");
                    }
                    rt.last_error = None;
                    tracing::info!(source = %job.item.source, "live upload done");
                }
                Some(e) => {
                    tracing::warn!(source = %job.item.source, flash = result.flash_written, "live upload failed: {e}");
                    rt.last_error = Some(e.clone());
                    if result.flash_written {
                        // The flash was touched: it counts, and the page may
                        // be half written, so the card should go again.
                        lib.count_upload(false);
                    } else {
                        lib.live_mut().budget.refund(now);
                    }
                    let _ = lib.save();
                    if rt.pending.is_none() {
                        rt.pending = Some(job);
                    }
                    rt.retry_at = Some(now + RETRY_EVERY.as_millis() as f64);
                }
            }
        }
        self.emit();
        // A newer card may be waiting already.
        self.wake();
    }
}

/// A card waits and may go now (budget, or the user said "send now").
fn card_ready(lib: &mut ScreenLibrary, rt: &Runtime, now: f64) -> bool {
    rt.pending.is_some() && (rt.send_now || lib.live_mut().budget.ms_until_token(now).is_none())
}

/// A scheduled return is due: turn it into a put-back. True when something
/// changed.
fn go_back(lib: &mut ScreenLibrary, rt: &mut Runtime, now: f64) -> bool {
    let Some(back) = rt.back.clone() else { return false };
    let showing = lib.live_source_showing(&back.source);
    let ready = card_ready(lib, rt, now);
    match back_step(&back, now, lib.live().enabled, showing, rt.uploading.is_some(), ready) {
        BackStep::Wait(_) => false,
        BackStep::Drop => {
            tracing::debug!(source = %back.source, "return to the own picture no longer needed");
            rt.back = None;
            true
        }
        BackStep::Go => {
            let Some(target) = lib.live_restore_target() else {
                rt.back = None;
                return true;
            };
            if rt.retry_at.is_some_and(|t| now < t) {
                return false;
            }
            // It does not wait for the budget: a card left on the screen
            // after its source ended is untrue ("Mola" after a stop), and
            // there is at most one return per series or music stop. It is
            // counted like a manual upload (owner's test, 2026-11-10: with
            // the budget empty the break card stayed for an hour).
            tracing::info!(source = %back.source, id = %target.id, "going back to the own picture");
            rt.back = None;
            rt.restore = Some(target.id);
            rt.restore_auto = Some(back.source);
            true
        }
    }
}

fn snapshot(lib: &mut ScreenLibrary, rt: &Runtime) -> LiveState {
    let now = now_ms();
    let live = lib.live_mut();
    let plan = live.budget.plan;
    let tokens = live.budget.tokens(now);
    let next_token_ms = live.budget.ms_until_token(now);
    let live = lib.live();
    LiveState {
        enabled: live.enabled,
        plan,
        per_day: plan.per_day(),
        burst: plan.burst(),
        tokens,
        next_token_ms,
        pending: rt.pending.as_ref().map(|p| p.item.clone()),
        uploading: rt.uploading.clone(),
        last: match (&live.last_source, &live.last_name, live.last_ms) {
            (Some(source), Some(name), Some(at_ms)) => {
                Some(LiveShown { source: source.clone(), name: name.clone(), at_ms })
            }
            _ => None,
        },
        last_error: rt.last_error.clone(),
        now_playing: live.now_playing,
        track: rt.track.clone(),
        // Not offered while it is being put back already.
        restore: if rt.restore.is_some() || rt.uploading.as_ref().is_some_and(|u| u.source == "restore") {
            None
        } else {
            lib.live_restore_target()
        },
        held: !rt.holders.is_empty(),
        holders: rt.holders.clone(),
        song_stop: live.song_stop,
        own: lib.live_own(),
        going_back: rt.back.clone(),
        factory_ack_needed: factory_ack_needed(lib),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = HOUR_MS;

    #[test]
    fn bucket_starts_full_and_refills_hourly() {
        let mut b = Budget::default();
        let t0 = 1_000_000.0;
        assert_eq!(b.tokens(t0), 6.0);
        for _ in 0..6 {
            assert!(b.try_take(t0));
        }
        assert!(!b.try_take(t0), "burst of 6");
        // 24 a day = 1 an hour.
        let wait = b.ms_until_token(t0).unwrap();
        assert!((wait - H).abs() < 1.0, "{wait}");
        assert!(!b.try_take(t0 + 0.5 * H));
        assert!(b.try_take(t0 + H + 1.0));
        // Never above the burst, however long it waits.
        assert_eq!(b.tokens(t0 + 100.0 * H), 6.0);
    }

    #[test]
    fn manual_uploads_charge_but_are_capped_and_refunds_return() {
        let mut b = Budget::default();
        let t = 5.0 * H;
        for _ in 0..20 {
            b.charge(t);
        }
        assert_eq!(b.tokens(t), -6.0, "debt capped at one burst");
        b.refund(t);
        assert_eq!(b.tokens(t), -5.0);
        assert!(b.ms_until_token(t).unwrap() > 5.0 * H);
    }

    #[test]
    fn plans_change_rate_and_cap() {
        let mut b = Budget::default();
        let t = 2.0 * H;
        b.tokens(t);
        b.set_plan(LivePlan::Saver, t);
        assert_eq!(b.tokens(t), 3.0, "capped to the saver burst");
        for _ in 0..3 {
            assert!(b.try_take(t));
        }
        // 8 a day = one every 3 hours.
        assert!((b.ms_until_token(t).unwrap() - 3.0 * H).abs() < 1.0);
        b.set_plan(LivePlan::Heavy, t);
        assert!((b.ms_until_token(t).unwrap() - 0.5 * H).abs() < 1.0);
    }

    #[test]
    fn going_back_waits_its_time_and_gives_way_to_new_cards() {
        let b = LiveBack { source: "pomodoro".into(), at_ms: 1_000.0 };
        assert_eq!(back_step(&b, 400.0, true, true, false, false), BackStep::Wait(600.0));
        assert_eq!(back_step(&b, 1_000.0, true, true, false, false), BackStep::Go);
        // Another card is waiting and can go: it replaces the screen anyway.
        assert_eq!(back_step(&b, 400.0, true, true, false, true), BackStep::Drop);
        assert_eq!(back_step(&b, 2_000.0, true, true, false, true), BackStep::Drop);
        // The source's card is no longer on the page (the user uploaded
        // something, or another source took over).
        assert_eq!(back_step(&b, 2_000.0, true, false, false, false), BackStep::Drop);
        // An upload runs: decide after it.
        assert!(matches!(back_step(&b, 2_000.0, true, false, true, false), BackStep::Wait(_)));
        // Live mode off: nothing goes by itself.
        assert_eq!(back_step(&b, 2_000.0, false, true, false, false), BackStep::Drop);
    }

    /// Owner's test, 2026-11-10: the budget was empty, the timer was
    /// stopped, and its break card stayed for an hour. Going back to the own
    /// picture must not wait for the budget.
    #[test]
    fn going_back_does_not_wait_for_an_empty_budget() {
        use crate::dto::{LiveItem, ScreenItemKind};
        use kb_protocol::sonix::screen::{rgb565, Frame, ScreenAnimation};
        let dir = std::env::temp_dir().join(format!("klavye-live-back-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = ScreenLibrary::open(Some(&dir));
        let mine = lib.add("cat", ScreenItemKind::Image, &ScreenAnimation::still(Frame::filled(0))).unwrap();
        lib.uploaded("m", &mine.id).unwrap();
        lib.live_mut().enabled = true;
        let card = ScreenAnimation::still(Frame::filled(rgb565(0, 200, 80)));
        let item = LiveItem { source: "pomodoro".into(), name: "Kısa mola".into() };
        lib.live_uploaded("m", &card, "c1", &item).unwrap();
        let now = now_ms();
        while lib.live_mut().budget.try_take(now) {}
        assert!(lib.live_mut().budget.ms_until_token(now).is_some(), "budget empty");

        let mut rt =
            Runtime { back: Some(LiveBack { source: "pomodoro".into(), at_ms: now - 1.0 }), ..Runtime::default() };
        assert!(go_back(&mut lib, &mut rt, now));
        assert_eq!(rt.restore.as_deref(), Some(mine.id.as_str()), "the own picture goes up now");
        assert_eq!(rt.restore_auto.as_deref(), Some("pomodoro"));
        assert!(rt.back.is_none());

        // A song card waits for the budget too: the return still goes, the
        // song follows when the budget allows (the owner's case at 20:08).
        let song = LiveItem { source: "now-playing".into(), name: "Şarkı".into() };
        let waiting = || Pending { anim: card.clone(), content: "c2".into(), item: song.clone() };
        let mut rt =
            Runtime { back: Some(LiveBack { source: "pomodoro".into(), at_ms: now - 1.0 }), ..Runtime::default() };
        rt.pending = Some(waiting());
        assert!(go_back(&mut lib, &mut rt, now));
        assert_eq!(rt.restore.as_deref(), Some(mine.id.as_str()));

        // The user sends it now: it replaces the screen, no return.
        let mut rt =
            Runtime { back: Some(LiveBack { source: "pomodoro".into(), at_ms: now - 1.0 }), ..Runtime::default() };
        rt.pending = Some(waiting());
        rt.send_now = true;
        assert!(go_back(&mut lib, &mut rt, now));
        assert!(rt.restore.is_none() && rt.back.is_none());

        // With budget left the waiting card goes first: no return either.
        lib.live_mut().budget.set_plan(LivePlan::Heavy, now);
        lib.live_mut().budget.refund(now);
        lib.live_mut().budget.refund(now);
        let mut rt =
            Runtime { back: Some(LiveBack { source: "pomodoro".into(), at_ms: now - 1.0 }), ..Runtime::default() };
        rt.pending = Some(waiting());
        assert!(go_back(&mut lib, &mut rt, now));
        assert!(rt.restore.is_none() && rt.back.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stored_settings_tolerate_missing_fields() {
        let s: LiveStored = serde_json::from_str("{}").unwrap();
        assert!(!s.enabled && s.now_playing);
        assert!(!s.factory_ack, "older files: turning on asks about the factory animation");
        assert_eq!(s.song_stop, SongStop::Back, "older files: the own picture comes back");
        assert_eq!(s.budget.plan, LivePlan::Balanced);
        let s: LiveStored = serde_json::from_str(r#"{"enabled":true,"budget":{"plan":"heavy"}}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.budget.plan, LivePlan::Heavy);
    }
}
