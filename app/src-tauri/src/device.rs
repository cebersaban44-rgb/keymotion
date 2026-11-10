//! Device actor: the only code that talks to keyboards.
//!
//! One thread owns the HID subsystem and every open device. Requests from
//! the UI arrive on a channel and run one at a time, so protocol sessions
//! (START … COMMIT) can never interleave. Between requests the actor
//!
//! * re-enumerates devices and emits `keyboards-changed`,
//! * streams the live (RAM-only) LED frame while one is set,
//! * mirrors the colours currently shown (`04 F5`) while the UI asks for it.

use crate::dto::{Battery, KeyboardInfo, LedColor};
use kb_core::color::LedCalibration;
use kb_core::detect::BlePeer;
use kb_core::sonix::SonixFf13;
use kb_devices::ConnectionKind;
use kb_protocol::sonix::{EffectSettings, Rgb};
use kb_transport::hid::{Hid, HidChannel};
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub const EVENT_KEYBOARDS_CHANGED: &str = "keyboards-changed";
pub const EVENT_MIRROR: &str = "lighting-mirror";

const ENUMERATE_EVERY: Duration = Duration::from_secs(2);
const BLE_RESCAN: Duration = Duration::from_secs(10);
const BATTERY_REFRESH: Duration = Duration::from_secs(60);
/// Live frames are re-sent continuously; the keyboard reverts to its saved
/// effect when the stream stops (verified 2026-11-10).
const LIVE_EVERY: Duration = Duration::from_millis(50);
/// `04 F5` polling rate for the on-screen mirror.
const MIRROR_EVERY: Duration = Duration::from_millis(250);

pub type Reply<T> = Sender<Result<T, String>>;

pub enum Request {
    Refresh,
    ApplyEffect(EffectSettings, Reply<()>),
    ApplyPerKey { colors: Vec<(u8, Rgb)>, brightness: u8, reply: Reply<()> },
    /// `Some` starts/updates the live frame, `None` stops streaming.
    SetLive(Option<Vec<(u8, Rgb)>>),
    SetMirror(bool),
}

#[derive(Default)]
pub struct Snapshot {
    pub keyboards: Vec<KeyboardInfo>,
}

pub struct DeviceHandle {
    pub snapshot: Arc<Mutex<Snapshot>>,
    tx: Sender<Request>,
}

impl DeviceHandle {
    pub fn send(&self, r: Request) -> Result<(), String> {
        self.tx.send(r).map_err(|_| "device actor stopped".to_string())
    }

    /// Send a request carrying a reply channel and wait for the answer.
    pub fn call<T>(&self, make: impl FnOnce(Reply<T>) -> Request) -> Result<T, String> {
        let (tx, rx) = mpsc::channel();
        self.send(make(tx))?;
        rx.recv_timeout(Duration::from_secs(20))
            .map_err(|_| "keyboard did not answer in time".to_string())?
    }
}

pub fn spawn(app: AppHandle) -> DeviceHandle {
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));
    let (tx, rx) = mpsc::channel();
    let shared = snapshot.clone();
    std::thread::Builder::new()
        .name("device-actor".into())
        .spawn(move || Actor::new(app, shared).run(rx))
        .expect("spawn device actor");
    DeviceHandle { snapshot, tx }
}

struct Command {
    path: String,
    driver: SonixFf13<HidChannel>,
}

struct Actor {
    app: AppHandle,
    shared: Arc<Mutex<Snapshot>>,
    hid: Option<Hid>,
    peers: Vec<BlePeer>,
    last_ble_scan: Option<Instant>,
    batteries: HashMap<u64, (Battery, Instant)>,
    /// Path of the wired command interface of the active keyboard, if any.
    command_path: Option<String>,
    command: Option<Command>,
    live: Option<Vec<(u8, Rgb)>>,
    mirror: bool,
    /// UI colours are monitor sRGB; LEDs are linear. Applied to every
    /// host-provided colour (live frames, per-key tables). Verified by eye
    /// on 2026-11-10: gamma 2.2 matches the UI palette, raw values wash out.
    calibration: LedCalibration,
    next_enumerate: Instant,
    next_live: Instant,
    next_mirror: Instant,
}

impl Actor {
    fn new(app: AppHandle, shared: Arc<Mutex<Snapshot>>) -> Self {
        let hid = Hid::new().map_err(|e| tracing::error!("HID init failed: {e}")).ok();
        let now = Instant::now();
        Self {
            app,
            shared,
            hid,
            peers: Vec::new(),
            last_ble_scan: None,
            batteries: HashMap::new(),
            command_path: None,
            command: None,
            live: None,
            mirror: false,
            calibration: LedCalibration::default(),
            next_enumerate: now,
            next_live: now,
            next_mirror: now,
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Request>) {
        loop {
            let now = Instant::now();
            if now >= self.next_enumerate {
                self.enumerate();
                self.next_enumerate = Instant::now() + ENUMERATE_EVERY;
            }
            if self.live.is_some() && now >= self.next_live {
                self.send_live();
                self.next_live = Instant::now() + LIVE_EVERY;
            } else if self.mirror && self.live.is_none() && now >= self.next_mirror {
                self.send_mirror();
                self.next_mirror = Instant::now() + MIRROR_EVERY;
            }

            let mut wake = self.next_enumerate;
            if self.live.is_some() {
                wake = wake.min(self.next_live);
            } else if self.mirror {
                wake = wake.min(self.next_mirror);
            }
            let timeout = wake.saturating_duration_since(Instant::now());
            match rx.recv_timeout(timeout) {
                Ok(req) => self.handle(req),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }

    fn handle(&mut self, req: Request) {
        match req {
            Request::Refresh => {
                self.last_ble_scan = None;
                self.batteries.clear();
                self.next_enumerate = Instant::now();
            }
            Request::ApplyEffect(settings, reply) => {
                let r = self.with_command(|d| d.apply_effect(&settings));
                let _ = reply.send(r);
            }
            Request::ApplyPerKey { colors, brightness, reply } => {
                let cal = self.calibration;
                let colors: Vec<(u8, Rgb)> = colors.into_iter().map(|(l, c)| (l, cal.apply(c))).collect();
                let r = self.with_command(|d| d.apply_per_key(colors, brightness));
                let _ = reply.send(r);
            }
            Request::SetLive(frame) => {
                if let Some(f) = &frame {
                    let sample: Vec<String> =
                        f.iter().take(6).map(|(l, [r, g, b])| format!("{l}=#{r:02X}{g:02X}{b:02X}")).collect();
                    tracing::debug!(leds = f.len(), ?sample, "live frame");
                }
                match (&frame, &self.live) {
                    (None, Some(_)) => tracing::info!("live stream stopped; keyboard reverts to its saved effect"),
                    (Some(f), None) => tracing::info!(leds = f.len(), "live stream started"),
                    _ => {}
                }
                let cal = self.calibration;
                self.live = frame.map(|f| f.into_iter().map(|(l, c)| (l, cal.apply(c))).collect());
                self.next_live = Instant::now();
            }
            Request::SetMirror(on) => {
                self.mirror = on;
                self.next_mirror = Instant::now();
            }
        }
    }

    /// Run `f` against the wired command channel, opening it on demand.
    /// Any transport error drops the handle so the next call reopens it.
    fn with_command<T>(
        &mut self,
        f: impl FnOnce(&mut SonixFf13<HidChannel>) -> kb_core::sonix::Result<T>,
    ) -> Result<T, String> {
        let path = self
            .command_path
            .clone()
            .ok_or_else(|| "Klavye USB modunda değil. Kabloyu takıp anahtarı ortaya (USB) alın.".to_string())?;
        if self.command.as_ref().is_none_or(|c| c.path != path) {
            let hid = self.hid.as_ref().ok_or("HID unavailable")?;
            let iface = hid
                .interfaces()
                .into_iter()
                .find(|i| i.path == path)
                .ok_or("command interface disappeared")?;
            let channel = hid.open(&iface).map_err(|e| e.to_string())?;
            self.command = Some(Command { path, driver: SonixFf13::new(channel) });
        }
        let cmd = self.command.as_mut().expect("opened above");
        match f(&mut cmd.driver) {
            Ok(v) => Ok(v),
            Err(e) => {
                tracing::warn!("command failed: {e}");
                if matches!(e, kb_core::sonix::DriverError::Transport(_)) {
                    self.command = None;
                }
                Err(e.to_string())
            }
        }
    }

    fn send_live(&mut self) {
        let Some(frame) = self.live.clone() else { return };
        if self.command_path.is_none() {
            return;
        }
        if let Err(e) = self.with_command(|d| d.realtime_frame(frame)) {
            tracing::warn!("live frame skipped: {e}");
        }
    }

    fn send_mirror(&mut self) {
        if self.command_path.is_none() {
            return;
        }
        match self.with_command(|d| d.read_colors()) {
            Ok(colors) => {
                let payload: Vec<LedColor> = colors
                    .into_iter()
                    .map(|(led, [r, g, b])| LedColor { led, r, g, b })
                    .collect();
                let _ = self.app.emit(EVENT_MIRROR, payload);
            }
            Err(e) => tracing::debug!("mirror read skipped: {e}"),
        }
    }

    fn enumerate(&mut self) {
        // BLE enumeration is slow; skip it while streaming so frames stay smooth.
        let busy = self.live.is_some() || self.mirror;
        if !busy && self.last_ble_scan.is_none_or(|t| t.elapsed() >= BLE_RESCAN) {
            self.peers = scan_ble();
            self.last_ble_scan = Some(Instant::now());
        }

        let interfaces = match self.hid.as_mut() {
            Some(h) => {
                if let Err(e) = h.refresh() {
                    tracing::warn!("HID refresh failed: {e}");
                }
                h.interfaces()
            }
            None => Vec::new(),
        };
        let detected = kb_core::detect(&interfaces, &self.peers);

        self.command_path = detected
            .iter()
            .find(|k| k.connection == ConnectionKind::Usb)
            .and_then(|k| k.command.as_ref())
            .map(|i| i.path.clone());
        if self.command_path.is_none() {
            self.command = None;
        }

        let mut infos = Vec::with_capacity(detected.len());
        for k in &detected {
            let battery = match (&k.ble, k.connection) {
                (Some(peer), ConnectionKind::Bluetooth) if peer.connected && !busy => {
                    let stale = self
                        .batteries
                        .get(&peer.address)
                        .is_none_or(|(_, at)| at.elapsed() >= BATTERY_REFRESH);
                    if stale {
                        if let Some(b) = read_battery(peer.address) {
                            self.batteries.insert(peer.address, (b, Instant::now()));
                        }
                    }
                    self.batteries.get(&peer.address).map(|(b, _)| b.clone())
                }
                (Some(peer), _) => self.batteries.get(&peer.address).map(|(b, _)| b.clone()),
                _ => None,
            };
            infos.push(KeyboardInfo::from_detected(k, battery));
        }

        let changed = {
            let mut snap = self.shared.lock().expect("snapshot lock");
            if snap.keyboards != infos {
                snap.keyboards = infos.clone();
                true
            } else {
                false
            }
        };
        if changed {
            tracing::info!(count = infos.len(), "keyboards changed");
            if let Err(e) = self.app.emit(EVENT_KEYBOARDS_CHANGED, &infos) {
                tracing::warn!("emit failed: {e}");
            }
        }
    }
}

#[cfg(windows)]
fn scan_ble() -> Vec<BlePeer> {
    match kb_transport::ble::paired_devices() {
        Ok(list) => list
            .into_iter()
            .map(|d| BlePeer {
                address_text: d.address_string(),
                name: d.name,
                address: d.address,
                connected: d.connected,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("BLE scan failed: {e}");
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
fn scan_ble() -> Vec<BlePeer> {
    Vec::new()
}

#[cfg(windows)]
fn read_battery(address: u64) -> Option<Battery> {
    match kb_transport::ble::read_battery(address) {
        Ok(percent) => Some(Battery {
            percent,
            read_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or_default(),
        }),
        Err(e) => {
            tracing::warn!("battery read failed: {e}");
            None
        }
    }
}

#[cfg(not(windows))]
fn read_battery(_address: u64) -> Option<Battery> {
    let _ = (SystemTime::now(), UNIX_EPOCH);
    None
}
