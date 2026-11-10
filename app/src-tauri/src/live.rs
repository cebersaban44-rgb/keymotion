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
use crate::dto::{LiveItem, LivePlan, LiveShown, LiveState, TrackInfo};
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
        }
    }
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
    /// Library item to put back on the GIF page (live mode turned off).
    restore: Option<String>,
    /// The Pomodoro holds the screen: "Çalan şarkı" waits.
    held: bool,
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

    /// `restore` (with `enabled: false`): put back what the GIF page showed
    /// before the live mode, if a live card is there now.
    pub fn configure(
        &self,
        enabled: Option<bool>,
        plan: Option<LivePlan>,
        now_playing: Option<bool>,
        restore: Option<bool>,
    ) -> Result<LiveState, String> {
        let mut put_back = None;
        {
            let mut lib = self.library.lock().map_err(|_| "screen library unavailable")?;
            if enabled == Some(false) && restore == Some(true) {
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
            lib.save().map_err(|e| e.to_string())?;
        }
        if enabled == Some(false) {
            if let Ok(mut rt) = self.rt.lock() {
                rt.pending = None;
                if put_back.is_some() {
                    tracing::info!("live mode off; putting the previous picture back");
                    rt.restore = put_back;
                    rt.retry_at = None;
                }
            }
        }
        self.wake();
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

    /// While held, sources other than the holder (the Pomodoro) wait.
    pub fn set_held(&self, held: bool) {
        let changed = self.rt.lock().map(|mut rt| std::mem::replace(&mut rt.held, held) != held).unwrap_or(false);
        if changed {
            tracing::debug!(held, "live screen hold");
            self.emit();
        }
    }

    pub fn held(&self) -> bool {
        self.rt.lock().map(|rt| rt.held).unwrap_or(false)
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

    fn next_wait(&self) -> Duration {
        let Ok(mut lib) = self.library.lock() else { return IDLE };
        let Ok(rt) = self.rt.lock() else { return IDLE };
        let now = now_ms();
        if rt.restore.is_some() {
            return Duration::from_millis(rt.retry_at.map_or(0.0, |t| (t - now).max(0.0)) as u64);
        }
        if rt.pending.is_none() || !lib.live().enabled {
            return IDLE;
        }
        let mut wait = IDLE.as_millis() as f64;
        if let Some(t) = rt.retry_at {
            wait = wait.min((t - now).max(0.0));
        }
        if let Some(ms) = lib.live_mut().budget.ms_until_token(now) {
            wait = wait.min(ms + 50.0);
        }
        Duration::from_millis(wait.max(0.0) as u64)
    }

    /// Put a library item back on the GIF page. Asked for by the user, so it
    /// is counted like a manual upload and does not wait for the budget.
    fn restore(&self, device: &DeviceHandle) -> bool {
        let now = now_ms();
        let (id, anim) = {
            let Ok(lib) = self.library.lock() else { return false };
            let Ok(mut rt) = self.rt.lock() else { return false };
            if rt.uploading.is_some() || rt.retry_at.is_some_and(|t| now < t) {
                return rt.restore.is_some();
            }
            let Some(id) = rt.restore.take() else { return false };
            let name = lib.state("").items.into_iter().find(|i| i.id == id).map(|i| i.name);
            match (lib.animation(&id), name) {
                (Ok(anim), Some(name)) => {
                    rt.uploading = Some(LiveItem { source: "restore".into(), name });
                    (id, anim)
                }
                _ => {
                    tracing::warn!(%id, "previous picture is gone from the library; nothing to put back");
                    return false;
                }
            }
        };
        self.emit();
        let result = device.upload_screen(id.clone(), anim);
        {
            let Ok(mut lib) = self.library.lock() else { return true };
            let Ok(mut rt) = self.rt.lock() else { return true };
            rt.uploading = None;
            match &result.error {
                None => {
                    let model = result.model_id.clone().unwrap_or_default();
                    if let Err(e) = lib.uploaded(&model, &id) {
                        tracing::warn!("restore not recorded: {e}");
                    }
                    lib.live_mut().before = None;
                    let _ = lib.save();
                    rt.last_error = None;
                    tracing::info!(%id, "previous picture put back");
                }
                Some(e) => {
                    tracing::warn!(flash = result.flash_written, "putting the previous picture back failed: {e}");
                    if result.flash_written {
                        lib.count_upload(true);
                        let _ = lib.save();
                    }
                    rt.last_error = Some(e.clone());
                    rt.restore = Some(id);
                    rt.retry_at = Some(now_ms() + RETRY_EVERY.as_millis() as f64);
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
            if !lib.live().enabled || rt.uploading.is_some() || rt.pending.is_none() {
                return;
            }
            if rt.retry_at.is_some_and(|t| now < t) {
                return;
            }
            if !lib.live_mut().budget.try_take(now) {
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
        held: rt.held,
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
    fn stored_settings_tolerate_missing_fields() {
        let s: LiveStored = serde_json::from_str("{}").unwrap();
        assert!(!s.enabled && s.now_playing);
        assert_eq!(s.budget.plan, LivePlan::Balanced);
        let s: LiveStored = serde_json::from_str(r#"{"enabled":true,"budget":{"plan":"heavy"}}"#).unwrap();
        assert!(s.enabled);
        assert_eq!(s.budget.plan, LivePlan::Heavy);
    }
}
