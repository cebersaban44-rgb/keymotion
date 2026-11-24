//! The keyboard's command family, behind one type.
//!
//! The device actor (device.rs) talks to a keyboard only through [`Driver`]
//! and asks [`family`] what the app knows about a family without talking
//! to the keyboard (its limits, whether it reports its battery on the
//! cable). Two families:
//!
//! * SONiX's "04 xx" ([`Protocol::SonixFf13`], `kb_core::sonix`): every
//!   operation is its driver's own, and the hooks around them
//!   ([`Driver::live_starting`], [`Driver::live_stopped`],
//!   [`Driver::device_info`]) do nothing, so its keyboards get exactly the
//!   packets they got before families existed.
//! * AJAZZ's "AA" ([`Protocol::AjazzAa`], AK980 PRO firmware 769,
//!   `kb_core::aa`): its arm is [`crate::aa::AaDriver`], which reads the
//!   keyboard when the channel opens, keeps it read-only when it does not
//!   look as expected, switches it to per-key mode for live lights and back,
//!   and keeps its backups.
//!
//! [`Ports`] is where channels come from: the HID subsystem, or in debug
//! builds a simulated "AA" keyboard (`KEYMOTION_SIM`, sim.rs), in which
//! case no HID device is enumerated or opened at all.

use crate::aa::{AaDriver, AaPort, AaTarget};
use crate::device::{Expected, MirrorExpect};
use crate::dto::DeviceReport;
use kb_core::aa::{AaEffect, AaError, EffectColor, RestoreReport, UsbIdentity};
use kb_core::color::LedCalibration;
use kb_core::keymap::LayerMap;
use kb_core::readback;
use kb_core::sonix::{DisplayPort, FeaturePort, SonixFf13};
use kb_devices::{InterfaceRole, Model, Protocol};
use kb_protocol::sonix::keymap::Layer;
use kb_protocol::sonix::macros::MacroStep;
use kb_protocol::sonix::screen::ScreenAnimation;
use kb_protocol::sonix::{scale_to_level, ClockTime, EffectSettings, Rgb, MAX_LEVEL};
use kb_transport::hid::{Hid, HidChannel, HidInterface};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Why a channel was not opened: its report layout is not the family's.
/// lib/errors.ts maps it to "this firmware is not supported".
pub const UNEXPECTED_LAYOUT: &str = "unsupported keyboard firmware: unexpected report layout";

/// What a family's driver reports when an operation fails. Its text is the
/// family driver's own, unchanged: lib/errors.ts maps the "04 xx" texts
/// ("keyboard refused 0x13 …", "no valid echo …") and the "AA" ones to
/// messages.
#[derive(Debug)]
pub enum Error {
    Ff13(kb_core::sonix::DriverError),
    /// The "AA" driver's error.
    Aa(AaError),
    /// The "AA" screen channel failed. It is dropped with the upload; the
    /// command channel, and the quiet time after a cut upload it keeps,
    /// stay.
    AaScreen(AaError),
    /// Refused by the app before anything went to the keyboard (a
    /// read-only "AA" keyboard, a backup that could not be saved).
    Refused(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ff13(e) => e.fmt(f),
            Self::Aa(e) | Self::AaScreen(e) => e.fmt(f),
            Self::Refused(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Error {}

impl From<kb_core::sonix::DriverError> for Error {
    fn from(e: kb_core::sonix::DriverError) -> Self {
        Self::Ff13(e)
    }
}

impl Error {
    /// The command channel itself failed (unplugged, a write that timed
    /// out): the handle is dropped and the next operation opens it again
    /// ("AA": [`AaError::needs_reopen`], and reads everything again).
    pub fn is_transport(&self) -> bool {
        match self {
            Self::Ff13(e) => matches!(e, kb_core::sonix::DriverError::Transport(_)),
            Self::Aa(e) => e.needs_reopen(),
            Self::AaScreen(_) | Self::Refused(_) => false,
        }
    }

    /// A refusal that happens now and then and needs no warning: the "04 xx"
    /// keyboard refuses about one real-time frame in 100 (`FF` to `04 20`),
    /// and the next one goes 50 ms later; an "AA" keyboard takes no command
    /// for a few seconds after a cut screen upload, and the frames, mirror
    /// reads and device reads of that time are skipped.
    pub fn is_noise(&self) -> bool {
        match self {
            Self::Ff13(e) => matches!(e, kb_core::sonix::DriverError::Refused { op: 0x20, .. }),
            Self::Aa(e) => matches!(e, AaError::UploadInterrupted { .. }),
            Self::AaScreen(_) | Self::Refused(_) => false,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A screen upload that finished (for the log).
#[derive(Debug, Clone)]
pub struct ScreenUploaded {
    /// Pages (or parts) sent.
    pub pages: usize,
    /// The first bytes of the keyboard's answer to the end of the upload
    /// ("04 xx"; the "AA" family has none).
    pub commit_echo: Vec<u8>,
}

/// An open command channel of one keyboard, in its family. `C` is the
/// "04 xx" channel (a HID handle; a fake in tests).
pub enum Driver<C: FeaturePort = HidChannel> {
    /// SONiX "04 xx": 64-byte feature reports on FF13.
    Ff13(SonixFf13<C>),
    /// AJAZZ "AA": 64-byte output and input reports on FF68.
    Aa(Box<AaDriver>),
}

/// An open screen channel, in its family.
pub enum Display<D: DisplayPort = HidChannel> {
    /// "04 xx": 4 KiB pages on FF68.
    Ff13(D),
    /// "AA": 4104-byte packets on FF67.
    Aa(AaPort),
}

impl<C: FeaturePort> Driver<C> {
    /// The command family this driver speaks.
    pub fn protocol(&self) -> Protocol {
        match self {
            Self::Ff13(_) => Protocol::SonixFf13,
            Self::Aa(_) => Protocol::AjazzAa,
        }
    }

    /// The "AA" arm, for what only that family has.
    pub fn aa(&self) -> Option<&AaDriver> {
        match self {
            Self::Aa(d) => Some(d),
            Self::Ff13(_) => None,
        }
    }

    pub fn aa_mut(&mut self) -> Option<&mut AaDriver> {
        match self {
            Self::Aa(d) => Some(d),
            Self::Ff13(_) => None,
        }
    }

    /// Store a firmware effect. **Writes the keyboard's memory.**
    pub fn apply_effect(&mut self, settings: &EffectSettings) -> Result<()> {
        match self {
            Self::Ff13(d) => Ok(d.apply_effect(settings)?),
            Self::Aa(d) => d.apply_effect(settings),
        }
    }

    /// Store per-key colours (LED colours, calibrated) and switch to the
    /// per-key mode. Keys missing from `colors` turn off. **Writes memory.**
    pub fn apply_per_key(&mut self, colors: Vec<(u8, Rgb)>, brightness: u8) -> Result<()> {
        match self {
            Self::Ff13(d) => Ok(d.apply_per_key(colors, brightness)?),
            Self::Aa(d) => d.apply_per_key(colors, brightness),
        }
    }

    /// Make one keymap layer what `map` says, keys by the app's key ids
    /// (the "04 xx" LED numbers); keys not in `map` go back to the default.
    /// "04 xx" writes the whole layer; "AA" changes only the keys that
    /// differ from a fresh read. **Writes memory.**
    pub fn apply_keymap(&mut self, layer: Layer, map: &LayerMap) -> Result<()> {
        match self {
            Self::Ff13(d) => Ok(d.apply_keymap(layer, map.iter().map(|(&l, &a)| (l, a)))?),
            Self::Aa(d) => d.apply_keymap(layer, map),
        }
    }

    /// Store every macro (slot `i` is macro `i`; empty: unused). Replaces
    /// all macros. **Writes memory.**
    pub fn write_macros(&mut self, slots: &[Vec<MacroStep>]) -> Result<()> {
        match self {
            Self::Ff13(d) => Ok(d.write_macros(slots)?),
            Self::Aa(d) => d.write_macros(slots),
        }
    }

    /// Set the keyboard clock.
    pub fn set_clock(&mut self, time: &ClockTime) -> Result<()> {
        match self {
            Self::Ff13(d) => Ok(d.set_clock(time)?),
            Self::Aa(d) => d.set_clock(time),
        }
    }

    /// Before the first frame of a live stream on this handle (the actor
    /// calls it once, right before that frame; again only after
    /// [`Self::live_stopped`]). Both families show real-time frames over
    /// any effect: nothing is switched.
    pub fn live_starting(&mut self) -> Result<()> {
        match self {
            Self::Ff13(_) => Ok(()),
            Self::Aa(d) => {
                d.live_starting();
                Ok(())
            }
        }
    }

    /// One live frame (LED colours, calibrated and dimmed): RAM only, gone
    /// once the stream stops. "AA": held back while the keyboard ignores
    /// writes (just switched on or reset; the UI says so).
    pub fn live_frame(&mut self, frame: Vec<(u8, Rgb)>) -> Result<()> {
        match self {
            Self::Ff13(d) => Ok(d.realtime_frame(frame)?),
            Self::Aa(d) => d.live_frame(frame),
        }
    }

    /// The live stream that [`Self::live_starting`] began has ended: no
    /// frame is due any more (the preview and every light layer are gone,
    /// or a game asked for quiet). Called once per stream, on the same
    /// handle; not called when the handle was dropped meanwhile (the cable
    /// went, a transport error). Both families go back to their saved
    /// effect by themselves ("AA": about 5 s after the last frame).
    pub fn live_stopped(&mut self) -> Result<()> {
        match self {
            Self::Ff13(_) => Ok(()),
            Self::Aa(d) => {
                d.live_stopped();
                Ok(())
            }
        }
    }

    /// When the driver has something to do on its own ("AA": read the
    /// keyboard again); "04 xx": never.
    pub fn next_due(&self) -> Option<Instant> {
        match self {
            Self::Ff13(_) => None,
            Self::Aa(d) => d.next_due(),
        }
    }

    /// Do what [`Self::next_due`] said; `app_effect` (the app's lighting
    /// record) fills in a backup a read takes.
    pub fn run_due(&mut self, app_effect: &dyn Fn() -> Option<kb_core::aa::AaEffect>) -> Result<()> {
        match self {
            Self::Ff13(_) => Ok(()),
            Self::Aa(d) => d.run_due(app_effect),
        }
    }

    /// The colours the keys show now, as LED values (calibrated; the
    /// caller turns them back into UI colours). "04 xx": `expect` is what
    /// the app last saved, `indicators` the firmware's status lights (LED,
    /// colour when lit): the read-back (`04 F5`) reports red ambiguously
    /// and they resolve it ([`ff13_mirror`]). "AA" reports the colours
    /// themselves (`33`): nothing to resolve.
    pub fn mirror_colors(
        &mut self,
        expect: &MirrorExpect,
        calibration: LedCalibration,
        indicators: &[(u8, Rgb)],
    ) -> Result<Vec<(u8, Rgb)>> {
        match self {
            Self::Ff13(d) => {
                let raw = d.read_colors()?;
                Ok(ff13_mirror(&raw, expect, calibration, indicators))
            }
            Self::Aa(d) => d.mirror_colors(),
        }
    }

    /// What the keyboard reports about itself (battery, charging, screen
    /// limits). "04 xx" reports nothing and nothing is sent: `None`. Asked
    /// only for a family that [`family::reports_device`].
    pub fn device_info(&mut self) -> Result<Option<DeviceReport>> {
        match self {
            Self::Ff13(_) => Ok(None),
            Self::Aa(d) => d.device_info().map(Some),
        }
    }

    /// Write an "AA" backup back ([`crate::aa::AaDriver::restore`]);
    /// `source`, the backup's file, is never pruned.
    pub fn restore(
        &mut self,
        backup: &kb_core::aa::AaSnapshot,
        color: Option<EffectColor>,
        app_effect: Option<kb_core::aa::AaEffect>,
        source: Option<&str>,
    ) -> Result<RestoreReport> {
        match self {
            Self::Ff13(_) => Err(Error::Refused("this keyboard has no backup to restore".into())),
            Self::Aa(d) => d.restore(backup, color, app_effect, source),
        }
    }

    /// Upload an animation to the screen over `display` (the family's
    /// screen channel, [`Ports::open_display`]). **Writes the keyboard's
    /// memory**; `progress(done, total)` after each page or part. `cancel`
    /// stops an "AA" upload before its next part ("04 xx" has no way to
    /// stop and goes on).
    ///
    /// Nothing else reaches the keyboard meanwhile: the driver is borrowed
    /// for the whole upload, and `progress` gets only numbers. (The actor
    /// runs one request at a time, so live frames, the mirror, the clock
    /// and device reads wait until it returns.)
    pub fn upload_screen<D: DisplayPort>(
        &mut self,
        display: &mut Display<D>,
        anim: &ScreenAnimation,
        progress: impl FnMut(usize, usize),
        cancel: &AtomicBool,
    ) -> Result<ScreenUploaded> {
        match (self, display) {
            (Self::Ff13(d), Display::Ff13(display)) => {
                let r = d.upload_screen(display, anim, progress)?;
                Ok(ScreenUploaded { pages: r.pages, commit_echo: r.commit_echo })
            }
            (Self::Aa(d), Display::Aa(display)) => {
                let pages = d.upload_screen(display, anim, progress, cancel)?;
                Ok(ScreenUploaded { pages, commit_echo: Vec::new() })
            }
            _ => Err(Error::Refused("the screen channel is another family's".into())),
        }
    }
}

/// The "04 xx" mirror: resolve the red channel of a `04 F5` read-back with
/// what the app expects each LED to show, at the level the keyboard runs
/// at (`kb_core::readback`).
pub fn ff13_mirror(
    raw: &[(u8, Rgb)],
    expect: &MirrorExpect,
    cal: LedCalibration,
    indicators: &[(u8, Rgb)],
) -> Vec<(u8, Rgb)> {
    let level = expect.level.min(MAX_LEVEL);
    // The LED value a UI colour is driven with at this level.
    let driven = |c: Rgb| cal.apply(c).map(|v| scale_to_level(v, level));
    let expected = |led: u8| {
        if let Some((_, c)) = indicators.iter().find(|(l, _)| *l == led) {
            return Some(*c);
        }
        match &expect.colors {
            Expected::Unknown => None,
            Expected::Uniform(c) => Some(driven(*c)),
            Expected::PerKey(keys) => Some(keys.get(&led).map_or([0, 0, 0], |c| driven(*c))),
        }
    };
    let peak = scale_to_level(255, if level == 0 { MAX_LEVEL } else { level });
    readback::decode_frame(raw, expected, peak)
}

/// What opening a command channel needs to know about the keyboard.
pub struct OpenTarget<'a> {
    pub protocol: Protocol,
    pub model: &'a Model,
    /// Its record id (`DetectedKeyboard::record_id`).
    pub record_id: String,
    /// The app's data folder ("AA": backups and the live note).
    pub data_dir: Option<PathBuf>,
    /// The effect the app last stored, in the "AA" keyboard's values (its
    /// colour goes into the backup when the keyboard does not report one).
    pub app_effect: Option<AaEffect>,
}

/// Where keyboards and their channels come from.
pub enum Ports {
    /// The HID subsystem (`None`: it could not be started).
    Hid(Option<Hid>),
    /// A simulated "AA" keyboard and nothing else: no HID device is listed
    /// or opened, no radio is scanned (debug builds, `KEYMOTION_SIM`).
    #[cfg(any(test, feature = "sim"))]
    Sim(crate::sim::SimBoard),
}

impl Ports {
    /// The real HID subsystem.
    pub fn hid() -> Self {
        Self::Hid(Hid::new().map_err(|e| tracing::error!("HID init failed: {e}")).ok())
    }

    /// No real device is reached: Bluetooth is not scanned either.
    pub fn simulated(&self) -> bool {
        match self {
            Self::Hid(_) => false,
            #[cfg(any(test, feature = "sim"))]
            Self::Sim(_) => true,
        }
    }

    /// Every HID collection there is now (listed again).
    pub fn interfaces(&mut self) -> Vec<HidInterface> {
        match self {
            Self::Hid(Some(h)) => {
                if let Err(e) = h.refresh() {
                    tracing::warn!("HID refresh failed: {e}");
                }
                h.interfaces()
            }
            Self::Hid(None) => Vec::new(),
            #[cfg(any(test, feature = "sim"))]
            Self::Sim(s) => s.interfaces(),
        }
    }

    /// A collection's report descriptor, as the OS keeps it (nothing is
    /// sent to the keyboard).
    pub fn report_descriptor_at(&self, path: &str) -> std::result::Result<Vec<u8>, String> {
        match self {
            Self::Hid(Some(h)) => h.report_descriptor_at(path).map_err(|e| e.to_string()),
            Self::Hid(None) => Err("HID unavailable".into()),
            #[cfg(any(test, feature = "sim"))]
            Self::Sim(s) => s.report_descriptor_at(path),
        }
    }

    /// Open the command channel at `path` in the keyboard's family. An "AA"
    /// keyboard is read whole before this returns (its driver's connect).
    pub fn open_command(&self, path: &str, target: &OpenTarget<'_>) -> std::result::Result<Driver, String> {
        match (self, target.protocol) {
            (Self::Hid(Some(hid)), Protocol::SonixFf13) => {
                Ok(Driver::Ff13(SonixFf13::new(open_checked(hid, path, InterfaceRole::Command, Protocol::SonixFf13)?)))
            }
            (Self::Hid(Some(hid)), Protocol::AjazzAa) => {
                let (channel, usb) = open_aa(hid, path, InterfaceRole::Command)?;
                connect_aa(AaPort::new(channel), usb, target)
            }
            (Self::Hid(None), _) => Err("HID unavailable".into()),
            #[cfg(any(test, feature = "sim"))]
            (Self::Sim(s), Protocol::AjazzAa) => {
                let port = s.command_port(path)?;
                connect_aa(port, s.usb(), target)
            }
            #[cfg(any(test, feature = "sim"))]
            (Self::Sim(_), Protocol::SonixFf13) => Err(UNEXPECTED_LAYOUT.into()),
        }
    }

    /// Open the screen channel at `path` for a keyboard of `protocol`.
    pub fn open_display(&self, path: &str, protocol: Protocol) -> std::result::Result<Display, String> {
        match (self, protocol) {
            (Self::Hid(Some(hid)), Protocol::SonixFf13) => {
                open_checked(hid, path, InterfaceRole::Display, protocol).map(Display::Ff13)
            }
            (Self::Hid(Some(hid)), Protocol::AjazzAa) => {
                open_aa(hid, path, InterfaceRole::Display).map(|(channel, _)| Display::Aa(AaPort::new(channel)))
            }
            (Self::Hid(None), _) => Err("HID unavailable".into()),
            #[cfg(any(test, feature = "sim"))]
            (Self::Sim(s), Protocol::AjazzAa) => s.display_port(path).map(Display::Aa),
            #[cfg(any(test, feature = "sim"))]
            (Self::Sim(_), Protocol::SonixFf13) => Err(UNEXPECTED_LAYOUT.into()),
        }
    }
}

/// The "AA" driver on `port`, after its first full read.
fn connect_aa(port: AaPort, usb: UsbIdentity, target: &OpenTarget<'_>) -> std::result::Result<Driver, String> {
    let aa = AaTarget { record_id: target.record_id.clone(), usb, data_dir: target.data_dir.clone() };
    let mut d = AaDriver::new(port, target.model, aa)?;
    d.connect(target.app_effect);
    // A channel the read left unusable is not kept: the next operation
    // opens it again.
    if let Some(e) = d.take_reopen() {
        return Err(e.to_string());
    }
    Ok(Driver::Aa(Box::new(d)))
}

/// Open an "AA" collection through `kb_core::aa::open_channel`, which checks
/// its name, layout and report sizes before anything is sent; with the USB
/// ids it reports.
fn open_aa(hid: &Hid, path: &str, role: InterfaceRole) -> std::result::Result<(HidChannel, UsbIdentity), String> {
    let iface = hid.interfaces().into_iter().find(|i| i.path == path).ok_or("keyboard disconnected")?;
    let usb = UsbIdentity { pid: iface.pid, release: iface.release };
    match kb_core::aa::open_channel(hid, &iface, role) {
        Ok(channel) => Ok((channel, usb)),
        Err(AaError::Channel(why)) => {
            tracing::warn!(usage_page = format_args!("{:04X}", iface.usage_page), "{why}; channel not used");
            Err(UNEXPECTED_LAYOUT.into())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Open a vendor channel only if its report layout is `protocol`'s for
/// `role` (`kb_core::shape::fits`): the same IDs also belong to boards that
/// speak another family, and nothing may be sent to those.
fn open_checked(
    hid: &Hid,
    path: &str,
    role: InterfaceRole,
    protocol: Protocol,
) -> std::result::Result<HidChannel, String> {
    let iface = hid.interfaces().into_iter().find(|i| i.path == path).ok_or("keyboard disconnected")?;
    let channel = hid.open(&iface).map_err(|e| e.to_string())?;
    let desc = channel.report_descriptor().map_err(|e| e.to_string())?;
    if !kb_core::shape::fits(protocol, role, &desc) {
        tracing::warn!(
            usage_page = format_args!("{:04X}", iface.usage_page),
            shape = ?kb_core::shape::report_shape(&desc),
            "unexpected report layout; channel not used"
        );
        return Err(UNEXPECTED_LAYOUT.into());
    }
    Ok(channel)
}

/// What the app knows about each command family without asking the
/// keyboard. The "04 xx" values are the constants the app used before
/// families existed; the "AA" ones come from the protocol research (notes:
/// research/protokol-aa.md) and wait for the beta's measurements.
pub mod family {
    use crate::dto::{DeviceReport, KeyActionSupport, KeyboardLimits, LevelRange, MacroBudgetRule, ScreenFrameLimits};
    use kb_devices::Protocol;
    use kb_protocol::sonix::{screen, MAX_LEVEL};

    /// "04 xx": measured time per 4 KiB screen page (E1/E4, 2026-11-10).
    pub const FF13_PAGE_MS: u32 = 145;

    /// The keyboard's limits as the app's controls use them. Brightness and
    /// speed are the app's own 0–5 scale in every family (a driver maps it
    /// to its keyboard's); the rest is the family's. `device` refines what
    /// the keyboard reports about itself.
    pub fn limits(protocol: Protocol, device: Option<&DeviceReport>) -> KeyboardLimits {
        let levels = LevelRange { min: 0, max: MAX_LEVEL };
        KeyboardLimits {
            brightness: levels,
            speed: levels,
            macro_budget: match protocol {
                Protocol::SonixFf13 => MacroBudgetRule::Ff13,
                Protocol::AjazzAa => MacroBudgetRule::Aa512,
            },
            screen: screen_limits(protocol, device),
            key_actions: match protocol {
                Protocol::SonixFf13 => KeyActionSupport { disabled: true, hold_macro: true, double_click: true },
                // "Disabled" is only inferred (`02 00 00 00`), "while held"
                // is not among the AA macro modes, and a double click is
                // not documented: hidden until the beta shows otherwise.
                Protocol::AjazzAa => KeyActionSupport { disabled: false, hold_macro: false, double_click: false },
            },
        }
    }

    /// Frames an upload may have, frames proven on the keyboard, and the
    /// time one page takes.
    pub fn screen_limits(protocol: Protocol, device: Option<&DeviceReport>) -> ScreenFrameLimits {
        let max = screen::MAX_FRAMES as u32;
        match protocol {
            Protocol::SonixFf13 => ScreenFrameLimits {
                max_frames: max,
                proven_frames: screen::PROVEN_FRAMES as u32,
                page_ms: FF13_PAGE_MS,
            },
            // The keyboard's own limit (`tftMaxFrames`), never above the
            // app's; nothing proven on an "AA" board yet; the time per part
            // is not measured, the "04 xx" one stands in.
            Protocol::AjazzAa => ScreenFrameLimits {
                max_frames: device.and_then(|d| d.screen_max_frames).map_or(max, |n| n.min(max)),
                proven_frames: 0,
                page_ms: FF13_PAGE_MS,
            },
        }
    }

    /// The keyboard reports its battery on the cable too ("AA": device
    /// information `0x10`); "04 xx" only over Bluetooth. On the cable, such
    /// a keyboard's battery is its own report only, never an older
    /// Bluetooth reading.
    pub fn battery_over_usb(protocol: Protocol) -> bool {
        protocol == Protocol::AjazzAa
    }

    /// Worth asking [`super::Driver::device_info`] while it is on the cable.
    /// Never for "04 xx": it has nothing to say, and nothing is sent.
    pub fn reports_device(protocol: Protocol) -> bool {
        protocol == Protocol::AjazzAa
    }

    /// The first upload to the screen may replace the keyboard's clock page
    /// for good (only a full reset brings it back; plan K-13). "04 xx"
    /// keeps its clock page: its uploads replace the picture page.
    pub fn upload_replaces_clock(protocol: Protocol) -> bool {
        protocol == Protocol::AjazzAa
    }

    /// The record id of a model's keyboards in `protocol`: the model id in
    /// the "04 xx" family (so records from before families existed still
    /// match), `"<model>.aa"` in the "AA" family (kb_core
    /// `DetectedKeyboard::record_id`).
    pub fn record_id(model_id: &str, protocol: Protocol) -> String {
        match protocol {
            Protocol::SonixFf13 => model_id.to_string(),
            Protocol::AjazzAa => format!("{model_id}.aa"),
        }
    }

    /// The family whose records `id` names: a known model's record id in a
    /// family the model has (its own or one of its modes').
    pub fn protocol_of_record(id: &str) -> Option<Protocol> {
        let models = kb_devices::catalogue().ok()?;
        models.iter().find_map(|m| {
            std::iter::once(m.protocol)
                .chain(m.modes.iter().map(|mode| m.protocol_of(mode)))
                .find(|&p| record_id(&m.id, p) == id)
        })
    }

    /// Whether `id` is the record id of a known model in a family it has.
    pub fn known_record(id: &str) -> bool {
        protocol_of_record(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kb_core::aa::sim::Sim;
    use kb_core::sonix::DriverError;
    use kb_protocol::sonix::keymap::KeyAction;
    use kb_protocol::sonix::macros::StepKind;
    use kb_protocol::sonix::{screen, REPORT_LEN};
    use std::collections::HashMap;
    use std::time::Duration;

    /// A "04 xx" keyboard, as in kb-core's tests: the feature report holds
    /// the latest echo (headers echoed with status 01, data packets as they
    /// are, `04 02` with the byte sum of the data); every write is kept.
    #[derive(Default)]
    struct Fake {
        writes: Vec<[u8; REPORT_LEN]>,
        last: Option<Vec<u8>>,
        pending: usize,
        sum: u16,
        /// Refuse real-time frames (`FF` to `04 20`).
        refuse_live: bool,
        /// The colour table `04 F5` reads back: 9 data reports after its
        /// echo (status 01).
        colors: Option<Vec<Vec<u8>>>,
        /// Reports to read before the latest echo again.
        queued: std::collections::VecDeque<Vec<u8>>,
        /// Every operation fails as if the cable went.
        unplugged: bool,
    }

    fn sum(bytes: &[u8]) -> u16 {
        bytes.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16))
    }

    impl FeaturePort for Fake {
        fn set_feature(&mut self, r: &[u8; REPORT_LEN]) -> kb_core::sonix::Result<()> {
            if self.unplugged {
                return Err(DriverError::Transport(kb_transport::TransportError::Timeout(1000)));
            }
            self.writes.push(*r);
            if self.pending > 0 {
                self.pending -= 1;
                self.sum = self.sum.wrapping_add(sum(&r[1..]));
                self.last = Some(r.to_vec());
                return Ok(());
            }
            if r[1] == 0x04 {
                let mut echo = r.to_vec();
                echo[4] = if r[2] == 0x20 && self.refuse_live { 0xFF } else { 0x01 };
                if r[2] == 0x02 {
                    echo[5..7].copy_from_slice(&self.sum.to_le_bytes());
                } else {
                    self.sum = 0;
                }
                // `04 F5` announces the reports the host reads: its echo,
                // then the table. The others announce what the host sends.
                if r[2] == 0xF5 {
                    if let Some(table) = self.colors.take() {
                        self.queued.push_back(echo.clone());
                        self.queued.extend(table.into_iter().map(|t| [vec![0], t].concat()));
                    }
                } else {
                    self.pending = r[9] as usize | (r[10] as usize) << 8;
                }
                self.last = Some(echo);
            }
            Ok(())
        }
        fn get_feature(&mut self) -> kb_core::sonix::Result<Vec<u8>> {
            if self.unplugged {
                return Err(DriverError::Transport(kb_transport::TransportError::Timeout(1000)));
            }
            if let Some(r) = self.queued.pop_front() {
                return Ok(r);
            }
            Ok(self.last.clone().unwrap_or_else(|| vec![0; REPORT_LEN]))
        }
    }

    /// A screen channel that acknowledges every page.
    #[derive(Default)]
    struct FakeDisplay {
        pages: usize,
    }

    impl DisplayPort for FakeDisplay {
        fn drain(&mut self) -> usize {
            0
        }
        fn write_page(&mut self, _: &[u8; screen::PAGE]) -> kb_core::sonix::Result<()> {
            self.pages += 1;
            Ok(())
        }
        fn read_ack(&mut self, _: Duration) -> kb_core::sonix::Result<Vec<u8>> {
            Ok(vec![0x01, 0x5A, 0x02, 0x00])
        }
    }

    /// The packets `op` sends through `Driver` and through `SonixFf13`
    /// directly, on fresh fakes.
    fn both<T: fmt::Debug>(
        via_driver: impl FnOnce(&mut Driver<Fake>) -> Result<T>,
        direct: impl FnOnce(&mut SonixFf13<Fake>) -> kb_core::sonix::Result<T>,
    ) -> (Vec<[u8; REPORT_LEN]>, Vec<[u8; REPORT_LEN]>) {
        let mut d = Driver::Ff13(SonixFf13::new(Fake::default()));
        via_driver(&mut d).unwrap();
        let Driver::Ff13(d) = d else { unreachable!("an \"04 xx\" driver") };
        let mut s = SonixFf13::new(Fake::default());
        direct(&mut s).unwrap();
        (d.into_inner().writes, s.into_inner().writes)
    }

    #[test]
    fn the_ff13_arm_sends_exactly_what_the_ff13_driver_sends() {
        let effect = EffectSettings::factory();
        let (a, b) = both(|d| d.apply_effect(&effect), |s| s.apply_effect(&effect));
        assert_eq!(a, b, "effect");
        assert!(!a.is_empty());

        let colors = vec![(1, [0, 255, 0]), (33, [9, 8, 7])];
        let (a, b) = both(|d| d.apply_per_key(colors.clone(), 4), |s| s.apply_per_key(colors.clone(), 4));
        assert_eq!(a, b, "per-key colours");

        let map = LayerMap::from([(33, KeyAction::key(0x04)), (34, KeyAction::Consumer(0xE9))]);
        let (a, b) = both(|d| d.apply_keymap(Layer::Fn, &map), |s| s.apply_keymap(Layer::Fn, map.clone()));
        assert_eq!(a, b, "keymap");

        let slots = vec![vec![MacroStep::new(StepKind::KeyDown, 4, 20), MacroStep::new(StepKind::KeyUp, 4, 20)]];
        let (a, b) = both(|d| d.write_macros(&slots), |s| s.write_macros(&slots));
        assert_eq!(a, b, "macros");

        let t = ClockTime { year: 2026, month: 11, day: 23, hour: 21, minute: 5, second: 0, weekday: 1 };
        let (a, b) = both(|d| d.set_clock(&t), |s| s.set_clock(&t));
        assert_eq!(a, b, "clock");

        let frame = vec![(1, [255, 0, 0]), (2, [0, 0, 255])];
        let (a, b) = both(|d| d.live_frame(frame.clone()), |s| s.realtime_frame(frame.clone()));
        assert_eq!(a, b, "live frame");
    }

    #[test]
    fn the_ff13_hooks_send_nothing() {
        let mut d = Driver::Ff13(SonixFf13::new(Fake::default()));
        assert_eq!(d.protocol(), Protocol::SonixFf13);
        d.live_starting().unwrap();
        d.live_stopped().unwrap();
        assert!(d.device_info().unwrap().is_none(), "no device report");
        let Driver::Ff13(d) = d else { unreachable!("an \"04 xx\" driver") };
        assert!(d.into_inner().writes.is_empty(), "no packet for the hooks");
    }

    #[test]
    fn the_screen_upload_goes_through_unchanged() {
        let anim = ScreenAnimation::still(screen::test_pattern());
        let mut d = Driver::Ff13(SonixFf13::new(Fake::default()));
        let mut display = Display::Ff13(FakeDisplay::default());
        let mut seen = Vec::new();
        let cancel = AtomicBool::new(false);
        let done = d.upload_screen(&mut display, &anim, |n, total| seen.push((n, total)), &cancel).unwrap();
        let Display::Ff13(display) = display else { unreachable!("an \"04 xx\" screen channel") };
        assert_eq!((done.pages, display.pages), (16, 16));
        assert_eq!(seen.last(), Some(&(16, 16)));
        let Driver::Ff13(d) = d else { unreachable!("an \"04 xx\" driver") };
        let w = d.into_inner().writes;
        // START, 04 72, COMMIT on the command channel and nothing else:
        // no live frame, read or clock packet in between.
        let ops: Vec<u8> = w.iter().map(|r| r[2]).collect();
        assert_eq!(ops, vec![0x18, 0x72, 0x02]);
    }

    #[test]
    fn errors_keep_the_ff13_texts_and_kinds() {
        // lib/errors.ts matches these texts.
        let refused = Error::from(DriverError::Refused { op: 0x13, status: 0xFF });
        assert_eq!(refused.to_string(), "keyboard refused 0x13 (status 0xFF)");
        assert!(!refused.is_noise() && !refused.is_transport());
        assert_eq!(Error::from(DriverError::NoEcho(0x18)).to_string(), "no valid echo for 0x18");
        assert_eq!(
            Error::from(DriverError::Checksum { expected: 0x10, got: 0x11 }).to_string(),
            "keyboard received different data (checksum 0x0011, expected 0x0010)"
        );
        assert_eq!(
            Error::from(DriverError::Busy).to_string(),
            "keyboard stayed busy; no configuration session could be opened"
        );
        assert_eq!(
            Error::from(DriverError::ScreenPage { page: 3, total: 16, reason: "x".into() }).to_string(),
            "screen upload stopped at page 3 of 16: x"
        );
        let timeout = DriverError::Transport(kb_transport::TransportError::Timeout(1000));
        let text = timeout.to_string();
        let timeout = Error::from(timeout);
        assert_eq!(timeout.to_string(), text, "transport errors pass through as they are");
        assert!(timeout.is_transport());

        // A refused real-time frame is noise; it comes back as before.
        let mut d = Driver::Ff13(SonixFf13::new(Fake { refuse_live: true, ..Fake::default() }));
        let e = d.live_frame(vec![(1, [1, 2, 3])]).unwrap_err();
        assert!(e.is_noise(), "{e}");
        assert_eq!(e.to_string(), "keyboard refused 0x20 (status 0xFF)");

        let mut d = Driver::Ff13(SonixFf13::new(Fake { unplugged: true, ..Fake::default() }));
        assert!(d.apply_effect(&EffectSettings::factory()).unwrap_err().is_transport());
    }

    /// The colour table as `04 F5` sends it: 9 reports of 64 bytes,
    /// `(led, r, g, b)` from byte 4 on (`kb_protocol::sonix::parse_color_table`).
    fn table(leds: &[(u8, Rgb)]) -> Vec<Vec<u8>> {
        let mut bytes = vec![0u8; 9 * 64];
        for &(led, [r, g, b]) in leds {
            let i = led as usize * 4;
            bytes[i..i + 4].copy_from_slice(&[led, r, g, b]);
        }
        bytes.chunks(64).map(<[u8]>::to_vec).collect()
    }

    #[test]
    fn the_mirror_decodes_as_before() {
        // Red reads back as 2·r mod 256: 0xFE is 0x7F or 0xFF.
        let raw = vec![(5, [0xFE, 0, 0]), (6, [0xFE, 0xFF, 0xFF]), (96, [0xFE, 0, 0])];
        let cal = LedCalibration::RAW;
        let indicators = [(96u8, [255u8, 0u8, 0u8])];
        let unknown = MirrorExpect { level: MAX_LEVEL, colors: Expected::Unknown };
        let per_key = MirrorExpect { level: MAX_LEVEL, colors: Expected::PerKey(HashMap::from([(5, [127, 0, 0])])) };
        for expect in [&unknown, &per_key] {
            // The same call kb_core's read-back got in device.rs before.
            let level = expect.level.min(MAX_LEVEL);
            let driven = |c: Rgb| cal.apply(c).map(|v| scale_to_level(v, level));
            let expected = |led: u8| {
                if let Some((_, c)) = indicators.iter().find(|(l, _)| *l == led) {
                    return Some(*c);
                }
                match &expect.colors {
                    Expected::Unknown => None,
                    Expected::Uniform(c) => Some(driven(*c)),
                    Expected::PerKey(keys) => Some(keys.get(&led).map_or([0, 0, 0], |c| driven(*c))),
                }
            };
            let before = readback::decode_frame(&raw, expected, scale_to_level(255, MAX_LEVEL));
            assert_eq!(ff13_mirror(&raw, expect, cal, &indicators), before);
        }
        // The indicator is taken at its full colour, whatever the level.
        let dim = MirrorExpect { level: 1, colors: Expected::Unknown };
        assert_eq!(ff13_mirror(&raw, &dim, cal, &indicators)[2], (96, [0xFF, 0, 0]));
    }

    #[test]
    fn the_mirror_reads_through_the_driver() {
        let leds = [(1u8, [0x20u8, 0x40u8, 0x60u8]), (33, [0, 0x10, 0])];
        let fake = Fake { colors: Some(table(&leds)), ..Fake::default() };
        let mut d = Driver::Ff13(SonixFf13::new(fake));
        let expect = MirrorExpect { level: MAX_LEVEL, colors: Expected::Unknown };
        let shown = d.mirror_colors(&expect, LedCalibration::RAW, &[]).unwrap();
        // Green and blue are exact; red is one of the two values its byte
        // stands for.
        for (led, [r, g, b]) in leds {
            let [sr, sg, sb] = shown.iter().find(|(l, _)| *l == led).map(|(_, c)| *c).expect("shown");
            assert_eq!((sg, sb), (g, b), "LED {led}");
            assert!(kb_protocol::sonix::readback_red_candidates(r).contains(&sr), "LED {led}: {sr}");
        }
        let Driver::Ff13(d) = d else { unreachable!("an \"04 xx\" driver") };
        let ops: Vec<u8> = d.into_inner().writes.iter().map(|r| r[2]).collect();
        assert_eq!(ops, vec![0xF5, 0x02], "read, then close the session");
    }

    #[test]
    fn ff13_limits_are_todays_constants() {
        let l = family::limits(Protocol::SonixFf13, None);
        assert_eq!((l.brightness.min, l.brightness.max), (0, 5));
        assert_eq!((l.speed.min, l.speed.max), (0, 5));
        assert_eq!(l.macro_budget, crate::dto::MacroBudgetRule::Ff13);
        assert_eq!((l.screen.max_frames, l.screen.proven_frames, l.screen.page_ms), (140, 135, 145));
        assert!(l.key_actions.disabled && l.key_actions.hold_macro && l.key_actions.double_click);
        // The same values the screen library hands the UI.
        let s = crate::dto::ScreenLimits::new();
        assert_eq!((s.max_frames, s.proven_frames, s.page_ms), (140, 135, 145));
        assert!(!family::battery_over_usb(Protocol::SonixFf13));
        assert!(!family::reports_device(Protocol::SonixFf13));
        assert!(!family::upload_replaces_clock(Protocol::SonixFf13));
    }

    #[test]
    fn aa_limits_follow_the_keyboards_report() {
        let report = |frames| DeviceReport { screen_max_frames: frames, ..DeviceReport::default() };
        let max = |d: Option<&DeviceReport>| family::screen_limits(Protocol::AjazzAa, d).max_frames;
        assert_eq!(max(None), 140);
        assert_eq!(max(Some(&report(Some(120)))), 120);
        assert_eq!(max(Some(&report(Some(142)))), 140, "never above the app's limit");
        assert_eq!(family::limits(Protocol::AjazzAa, None).macro_budget, crate::dto::MacroBudgetRule::Aa512);
    }

    #[test]
    fn record_ids_match_the_detected_keyboards() {
        assert_eq!(family::record_id("ajazz-ak980-pro", Protocol::SonixFf13), "ajazz-ak980-pro");
        assert_eq!(family::record_id("ajazz-ak980-pro", Protocol::AjazzAa), "ajazz-ak980-pro.aa");
        assert!(family::known_record("ajazz-ak980-pro"));
        assert_eq!(family::protocol_of_record("ajazz-ak980-pro"), Some(Protocol::SonixFf13));
        assert!(!family::known_record("unknown-model"));
        // The model file's "AA" modes give it an "AA" record.
        assert!(family::known_record("ajazz-ak980-pro.aa"));
        assert_eq!(family::protocol_of_record("ajazz-ak980-pro.aa"), Some(Protocol::AjazzAa));
        assert!(!family::known_record("unknown-model.aa"));
        // The same rule kb_core detects keyboards with.
        let iface = kb_transport::hid::HidInterface {
            vid: 0x0C45,
            pid: 0x8009,
            release: 0x0106,
            usage_page: 0xFF13,
            usage: 1,
            interface_number: -1,
            bus: kb_transport::hid::Bus::Usb,
            manufacturer: None,
            product: Some("AK980 PRO".into()),
            serial: None,
            path: "p".into(),
        };
        let found = kb_core::detect(&[iface], &[]);
        let k = found.first().expect("a keyboard");
        assert_eq!(family::record_id(&k.model.id, k.protocol()), k.record_id());
    }

    #[test]
    fn an_aa_channel_the_first_read_left_dead_is_not_kept() {
        let sim = Sim::ak980();
        let target = OpenTarget {
            protocol: Protocol::AjazzAa,
            model: Sim::model(),
            record_id: "ajazz-ak980-pro.aa".into(),
            data_dir: None,
            app_effect: None,
        };
        let port = crate::aa::tests::Dying { port: sim.command(), left: 3 };
        let e = connect_aa(AaPort::new(port), Sim::usb(), &target).err().expect("not kept");
        assert!(e.contains("the handle died"), "{e}");
        // A channel that reads: kept.
        let sim = Sim::ak980();
        assert!(connect_aa(AaPort::new(sim.command()), Sim::usb(), &target).is_ok());
    }
}
