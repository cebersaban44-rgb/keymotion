//! Types shared with the UI. `cargo test` exports them to
//! `app/src/bindings/*.ts` through ts-rs, so the UI never re-declares them.

use crate::settings::{StoredKeymap, StoredLayer, StoredMacro, StoredMacros, StoredStep};
use kb_core::DetectedKeyboard;
use kb_devices::{Capability, ConnectionKind, Model};
use kb_protocol::sonix::keymap::{KeyAction, Layer, MacroPlay, MouseButton, UsageGroup};
use kb_protocol::sonix::macros::{MacroStep, StepKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Connection {
    Usb,
    Dongle,
    Bluetooth,
}

impl From<ConnectionKind> for Connection {
    fn from(k: ConnectionKind) -> Self {
        match k {
            ConnectionKind::Usb => Self::Usb,
            ConnectionKind::Dongle => Self::Dongle,
            ConnectionKind::Bluetooth => Self::Bluetooth,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum Feature {
    Lighting,
    PerKeyRgb,
    Keymap,
    Macros,
    Options,
    Screen,
    Clock,
    Battery,
}

impl From<Capability> for Feature {
    fn from(c: Capability) -> Self {
        match c {
            Capability::Lighting => Self::Lighting,
            Capability::PerKeyRgb => Self::PerKeyRgb,
            Capability::Keymap => Self::Keymap,
            Capability::Macros => Self::Macros,
            Capability::Options => Self::Options,
            Capability::Screen => Self::Screen,
            Capability::Clock => Self::Clock,
            Capability::Battery => Self::Battery,
        }
    }
}

/// What the app offers in one connection mode of a model, for the
/// connection map and "switch to USB to unlock" guidance in the UI.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModeInfo {
    pub connection: Connection,
    pub features: Vec<Feature>,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyGeo {
    pub id: String,
    pub label: String,
    pub led: u8,
    /// Default HID usage; 0xAF is the Fn key (never remapped).
    pub hid: u8,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RectGeo {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct KnobGeo {
    pub x: f32,
    pub y: f32,
    pub d: f32,
}

/// Physical layout in key units, straight from the model file.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GeometryInfo {
    pub width: f32,
    pub height: f32,
    pub source: String,
    pub screen: Option<RectGeo>,
    pub knob: Option<KnobGeo>,
    pub keys: Vec<KeyGeo>,
}

impl GeometryInfo {
    pub fn from_model(g: &kb_devices::Geometry) -> Self {
        Self {
            width: g.width,
            height: g.height,
            source: g.source.clone(),
            screen: g.screen.map(|r| RectGeo { x: r.x, y: r.y, w: r.w, h: r.h }),
            knob: g.knob.map(|k| KnobGeo { x: k.x, y: k.y, d: k.d }),
            keys: g
                .keys
                .iter()
                .map(|k| KeyGeo {
                    id: k.id.clone(),
                    label: k.label.clone(),
                    led: k.led,
                    hid: k.hid,
                    x: k.x,
                    y: k.y,
                    w: k.w,
                    h: k.h,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Battery {
    pub percent: u8,
    /// Unix epoch milliseconds of the reading.
    #[ts(type = "number")]
    pub read_at_ms: u64,
}

/// A keyboard that looks like a SONiX or AJAZZ one but isn't supported yet
/// (kb_core `unknown_keyboards`): the UI offers "Request support".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UnsupportedKeyboard {
    /// Ids and bus, "0C45:8009:usb".
    pub id: String,
    /// The name the keyboard reports, when it has one.
    pub name: Option<String>,
}

impl UnsupportedKeyboard {
    pub fn from_unknown(k: &kb_core::UnknownKeyboard) -> Self {
        Self { id: k.key(), name: k.name().map(str::to_string) }
    }
}

/// A crash note from an earlier run, offered once for reporting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CrashNote {
    /// File name (`crash-….txt`): marks it seen.
    pub name: String,
    /// Its text (version, time, message), already redacted.
    pub text: String,
}

/// Which GitHub issue form to open (`.github/ISSUE_TEMPLATE/<kind>.yml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum IssueKind {
    Bug,
    Keyboard,
    Idea,
}

/// The command family a keyboard speaks in the detected mode
/// (`kb_devices::Protocol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum KeyboardProtocol {
    /// SONiX "04 xx" (feature reports on FF13).
    #[serde(rename = "sonix-ff13")]
    SonixFf13,
    /// AJAZZ "AA" (AK980 PRO firmware 769 and others).
    #[serde(rename = "ajazz-aa")]
    AjazzAa,
}

impl From<kb_devices::Protocol> for KeyboardProtocol {
    fn from(p: kb_devices::Protocol) -> Self {
        match p {
            kb_devices::Protocol::SonixFf13 => Self::SonixFf13,
            kb_devices::Protocol::AjazzAa => Self::AjazzAa,
        }
    }
}

/// The steps a slider offers, `min` to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct LevelRange {
    pub min: u8,
    pub max: u8,
}

/// How macros count against the keyboard's macro memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub enum MacroBudgetRule {
    /// "04 xx": `(steps + slots) × 8` bytes within `MacrosState.budget`.
    #[serde(rename = "ff13")]
    Ff13,
    /// "AA": `(steps + slots) × 4` bytes within 512.
    #[serde(rename = "aa512")]
    Aa512,
}

/// What a screen upload may hold on this keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenFrameLimits {
    pub max_frames: u32,
    /// Frames proven on this kind of keyboard (more may not play).
    pub proven_frames: u32,
    /// Time one page of an upload takes.
    pub page_ms: u32,
}

/// Key actions this keyboard takes besides the ones every keyboard does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyActionSupport {
    /// A key that does nothing.
    pub disabled: bool,
    /// A macro that plays while its key is held.
    pub hold_macro: bool,
    /// The double-click mouse action.
    pub double_click: bool,
}

/// The keyboard's limits, for the controls that edit what it stores
/// (driver.rs `family::limits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyboardLimits {
    /// Brightness levels of effects and per-key designs (the app's 0–5
    /// scale in every family).
    pub brightness: LevelRange,
    /// Effect speed levels.
    pub speed: LevelRange,
    pub macro_budget: MacroBudgetRule,
    pub screen: ScreenFrameLimits,
    pub key_actions: KeyActionSupport,
}

/// Whether the battery charges, as the keyboard reports it. Only what is
/// known: the "AA" device information's charge status 1. Its other values
/// (2 on the cable with a full battery, 0 over the receiver) mean nothing
/// sure yet, so they report no state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ChargeState {
    Charging,
}

/// Why Keymotion writes nothing to the "AA" keyboard on the cable. Every
/// write refuses with the matching backend error (device.rs `WriteBlock`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[ts(export)]
pub enum ReadOnlyReason {
    /// Its answers did not look as expected, or the full read did not
    /// finish (`reasons`: English, for the diagnostics report;
    /// `readFailed`: the read did not finish, which the background tries
    /// again, rather than data that failed the check).
    NotSane {
        reasons: Vec<String>,
        #[serde(rename = "readFailed")]
        read_failed: bool,
    },
    /// A firmware update was left unfinished: AJAZZ's tool has to finish it.
    UpdatePending,
    /// The backup of its settings could not be saved (`error`: English).
    NoBackup { error: String },
}

/// Why live lights (gauges, music, games, agents, previews) do not show on
/// the "AA" keyboard: it shows them only in per-key mode, which the app
/// switches to and back (plan K-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[ts(export)]
pub enum LiveLightsOff {
    /// The colour of the keyboard's effect is not known, so it could not be
    /// put back afterwards: setting the lighting once in Keymotion fixes it.
    ColorUnknown,
    /// The keyboard is read-only ([`ReadOnlyReason`]).
    ReadOnly,
    /// A write was refused (`reason`: the backend error, such as "untested
    /// keyboard firmware 3.01" or "more than one keyboard on the USB cable").
    Blocked { reason: String },
    /// The switch failed (`error`: the backend error).
    Failed { error: String },
}

/// What the app knows about an "AA" keyboard on the cable beyond the model
/// file ([`KeyboardInfo::aa_status`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AaStatus {
    /// Set: nothing is written to it, and why.
    pub read_only: Option<ReadOnlyReason>,
    /// Set: live lights do not show on it, and why.
    pub live_lights_off: Option<LiveLightsOff>,
    /// Live lights hold it in per-key mode now.
    pub live_armed: bool,
    /// Live lights switched it to per-key mode: its own effect comes back
    /// about 30 s after they end. False while it was in a live mode before
    /// (per-key lighting saved from Keymotion): nothing goes back.
    pub live_restores: bool,
    /// Replies nobody asked for keep coming: another program (AJAZZ's web
    /// page?) seems to talk to the keyboard. Nothing is blocked.
    pub other_program_suspected: bool,
    /// The backup of its settings taken before the first write (file name
    /// in the data folder's `keyboard-backups`).
    pub backup: Option<String>,
    /// The colour of its effect is known (the keyboard reports white after
    /// any write, so only one Keymotion wrote is).
    pub effect_color_known: bool,
}

/// Why a keyboard backup was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum KeyboardBackupReason {
    /// Before Keymotion first wrote to the keyboard.
    FirstConnect,
    /// Before a restore.
    BeforeRestore,
}

/// A backup of an "AA" keyboard's settings (`keyboard_backups`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyboardBackupInfo {
    /// Its file name: what `keyboard_backup_restore` takes.
    pub file: String,
    #[ts(type = "number")]
    pub saved_at_ms: u64,
    pub reason: KeyboardBackupReason,
    pub record_id: String,
    /// Firmware version as people read it ("3.01").
    pub firmware: String,
    /// Taken from the keyboard on the cable now (same USB ids and
    /// firmware): only such a backup can be restored.
    pub matches: bool,
    /// It knows its effect's colour. If not, a restore that changes the
    /// effect needs one (`color` of `keyboard_backup_restore`).
    pub color_known: bool,
}

/// A key a restore left as the keyboard has it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeptKey {
    pub layer: KeymapLayer,
    /// The "AA" keymap slot.
    pub slot: u8,
    /// The key's LED (its id in the app), when the slot is a key.
    pub led: Option<u8>,
    /// Why (English).
    pub reason: String,
}

/// What a restore wrote.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyboardRestoreResult {
    /// The sections written, in English ("base layer", "Fn layer",
    /// "effect", "per-key table", "macros"); unchanged ones are not.
    pub written: Vec<String>,
    pub kept: Vec<KeptKey>,
    /// The effect the keyboard shows now, as the Lighting page's record
    /// holds one, when its colour is known; `null`: not known (or per-key
    /// mode), so the record is not either.
    pub effect: Option<EffectRequest>,
}

impl From<kb_core::aa::RestoreReport> for KeyboardRestoreResult {
    fn from(r: kb_core::aa::RestoreReport) -> Self {
        Self {
            written: r.written.iter().map(|s| s.to_string()).collect(),
            kept: r
                .kept
                .into_iter()
                .map(|k| KeptKey {
                    layer: match k.layer {
                        Layer::Base => KeymapLayer::Base,
                        Layer::Fn => KeymapLayer::Fn,
                    },
                    slot: k.slot,
                    led: k.led,
                    reason: k.reason,
                })
                .collect(),
            effect: None,
        }
    }
}

/// What a keyboard reports about itself on the cable (driver.rs
/// `Driver::device_info`; the "AA" family's device information). The
/// "04 xx" family reports nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeviceReport {
    /// Battery level (%).
    pub battery: Option<u8>,
    pub charge: Option<ChargeState>,
    /// Frames its screen takes (the "AA" `tftMaxFrames`).
    pub screen_max_frames: Option<u32>,
    /// A firmware update was left unfinished: nothing may be written until
    /// AJAZZ's tool completes it (the "AA" `firmwareStatus` 1).
    pub update_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyboardInfo {
    /// Stable identity: record id + connection.
    pub key: String,
    pub model_id: String,
    /// The command family of the detected mode.
    pub protocol: KeyboardProtocol,
    /// What the app keys this keyboard's records by (keymap, macros, what
    /// its screen shows): the model id in the "04 xx" family, `<model>.aa`
    /// in the "AA" family.
    pub record_id: String,
    pub limits: KeyboardLimits,
    /// The battery level is read on the cable too (else the last Bluetooth
    /// reading stands in there).
    pub battery_over_usb: bool,
    /// What the keyboard reports about itself on the cable; absent when it
    /// reports nothing ("04 xx").
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub device: Option<DeviceReport>,
    /// An "AA" keyboard on the cable, once the app read it: read-only or
    /// not, live lights, the backup, the conflict hint. Absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub aa_status: Option<AaStatus>,
    pub model_name: String,
    pub vendor: String,
    pub layout: String,
    pub key_count: u16,
    pub has_knob: bool,
    pub has_side_light: bool,
    pub screen: Option<ScreenInfo>,
    pub connection: Connection,
    pub product: Option<String>,
    /// Firmware version over the cable, as people read it ("1.06").
    pub firmware: Option<String>,
    /// Features usable right now in this connection mode.
    pub features: Vec<Feature>,
    /// What every connection mode of this model offers.
    pub modes: Vec<ModeInfo>,
    pub has_command_channel: bool,
    pub has_display_channel: bool,
    pub ble_name: Option<String>,
    pub ble_address: Option<String>,
    /// Bluetooth only: the paired keyboard says it is connected right now.
    pub ble_connected: bool,
    pub battery: Option<Battery>,
    pub geometry: Option<GeometryInfo>,
}

impl KeyboardInfo {
    /// `device`: what the keyboard reported about itself, if anything.
    pub fn from_detected(k: &DetectedKeyboard, battery: Option<Battery>, device: Option<DeviceReport>) -> Self {
        let m: &Model = k.model;
        let protocol = k.protocol();
        // One entry per connection mode: the detected mode for this
        // connection, the model file's first of each other kind.
        let mut kinds: Vec<ConnectionKind> = Vec::new();
        for mode in &m.modes {
            if !kinds.contains(&mode.kind) {
                kinds.push(mode.kind);
            }
        }
        let modes = kinds
            .into_iter()
            .filter_map(|kind| if kind == k.connection { Some(k.mode) } else { m.mode(kind) })
            .map(|mode| ModeInfo {
                connection: mode.kind.into(),
                features: kb_core::mode_capabilities(m, mode).into_iter().map(Feature::from).collect(),
                verified: mode.verified,
            })
            .collect();
        Self {
            key: k.key(),
            model_id: m.id.clone(),
            protocol: protocol.into(),
            record_id: k.record_id(),
            limits: crate::driver::family::limits(protocol, device.as_ref()),
            battery_over_usb: crate::driver::family::battery_over_usb(protocol),
            device,
            aa_status: None,
            model_name: m.name.clone(),
            vendor: m.vendor.clone(),
            layout: m.layout.clone(),
            key_count: m.key_count,
            has_knob: m.has_knob,
            has_side_light: m.has_side_light,
            screen: m.screen.as_ref().map(|s| ScreenInfo { width: s.width, height: s.height, verified: s.verified }),
            connection: k.connection.into(),
            product: k.product.clone(),
            firmware: k.firmware.map(kb_devices::firmware_text),
            features: k.capabilities().into_iter().map(Feature::from).collect(),
            modes,
            has_command_channel: k.command.is_some(),
            has_display_channel: k.display.is_some(),
            ble_name: k.ble.as_ref().map(|b| b.name.clone()),
            ble_address: k.ble.as_ref().map(|b| b.address_text.clone()),
            ble_connected: k.connection == ConnectionKind::Bluetooth && k.ble.as_ref().is_some_and(|b| b.connected),
            battery,
            geometry: m.geometry.as_ref().map(GeometryInfo::from_model),
        }
    }

    /// Which entry the app shows when it sees the keyboard more than one
    /// way, lowest first: the cable; Bluetooth while the keyboard says it
    /// is connected; the 2.4 GHz receiver, which shows up whether or not a
    /// keyboard is on behind it (nothing is sent to it yet, so nothing
    /// tells); Bluetooth not known to be connected. The tray follows it, and
    /// the UI's `useActiveKeyboard` (lib/store.ts) does the same.
    pub fn focus_rank(&self) -> u8 {
        match (self.connection, self.ble_connected) {
            (Connection::Usb, _) => 0,
            (Connection::Bluetooth, true) => 1,
            (Connection::Dongle, _) => 2,
            (Connection::Bluetooth, false) => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Connection, EffectInfo, GeometryInfo, KeyActionSupport, KeyboardInfo, KeyboardProtocol, KeymapCatalog,
        LevelRange, MacroBudgetRule, ScreenFrameLimits,
    };
    use kb_core::detect::BlePeer;
    use kb_transport::hid::{Bus, HidInterface};

    fn collection(pid: u16, usage_page: u16, bus: Bus) -> HidInterface {
        HidInterface {
            vid: 0x0C45,
            pid,
            release: 0x0138,
            usage_page,
            usage: 1,
            interface_number: -1,
            bus,
            manufacturer: None,
            product: None,
            serial: None,
            path: format!("path-{pid:x}-{usage_page:x}"),
        }
    }

    /// The entry the tray and the UI show, out of what the app sees.
    fn shown(interfaces: &[HidInterface], ble: &[BlePeer]) -> Connection {
        let infos: Vec<KeyboardInfo> =
            kb_core::detect(interfaces, ble).iter().map(|k| KeyboardInfo::from_detected(k, None, None)).collect();
        infos.iter().min_by_key(|k| k.focus_rank()).expect("a keyboard").connection
    }

    #[test]
    fn a_receiver_does_not_hide_a_connected_bluetooth_keyboard() {
        let receiver = collection(0xFEFE, 0xFF60, Bus::Usb);
        let bluetooth = collection(0xFEFE, 0x0001, Bus::Bluetooth);
        let peer = |connected| BlePeer {
            name: "AK980 PRO-1".into(),
            address: 1,
            address_text: "00:00:00:00:00:01".into(),
            connected,
        };
        // The keyboard is on Bluetooth and the receiver is still plugged in:
        // whether a keyboard answers behind the receiver is not known.
        assert_eq!(shown(&[receiver.clone(), bluetooth.clone()], &[peer(true)]), Connection::Bluetooth);
        // Bluetooth paired but not connected: the keyboard may well be on
        // the receiver.
        assert_eq!(shown(&[receiver.clone(), bluetooth.clone()], &[peer(false)]), Connection::Dongle);
        assert_eq!(shown(&[receiver.clone(), bluetooth], &[]), Connection::Dongle);
        // The cable comes first, whatever else is there.
        let cable = HidInterface { product: Some("AK980 PRO".into()), ..collection(0x8009, 0xFF13, Bus::Usb) };
        assert_eq!(shown(&[receiver, cable], &[peer(true)]), Connection::Usb);
    }

    #[test]
    fn an_ff13_keyboard_reports_todays_limits() {
        let cable = HidInterface { product: Some("AK980 PRO".into()), ..collection(0x8009, 0xFF13, Bus::Usb) };
        let found = kb_core::detect(&[cable], &[]);
        let info = KeyboardInfo::from_detected(&found[0], None, None);
        assert_eq!(info.protocol, KeyboardProtocol::SonixFf13);
        assert_eq!((info.record_id.as_str(), info.key.as_str()), ("ajazz-ak980-pro", "ajazz-ak980-pro:Usb"));
        assert!(!info.battery_over_usb);
        assert_eq!(info.device, None);
        let l = info.limits;
        assert_eq!((l.brightness, l.speed), (LevelRange { min: 0, max: 5 }, LevelRange { min: 0, max: 5 }));
        assert_eq!(l.macro_budget, MacroBudgetRule::Ff13);
        assert_eq!(l.screen, ScreenFrameLimits { max_frames: 140, proven_frames: 135, page_ms: 145 });
        assert_eq!(l.key_actions, KeyActionSupport { disabled: true, hold_macro: true, double_click: true });
        // One entry per connection, as before: what each mode offers.
        let modes: Vec<(Connection, usize)> = info.modes.iter().map(|m| (m.connection, m.features.len())).collect();
        assert_eq!(modes, vec![(Connection::Usb, 6), (Connection::Dongle, 0), (Connection::Bluetooth, 1)]);

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["protocol"], "sonix-ff13");
        assert_eq!(json["recordId"], "ajazz-ak980-pro");
        assert_eq!(json["batteryOverUsb"], false);
        assert_eq!(json["limits"]["macroBudget"], "ff13");
        assert_eq!(json["limits"]["screen"]["maxFrames"], 140);
        assert_eq!(json["limits"]["keyActions"]["holdMacro"], true);
        assert!(json.get("device").is_none(), "left out when the keyboard reports nothing");
    }

    /// Writes each model's geometry and the effect catalog for the UI's
    /// browser-only mock backend, so the mock never drifts from Rust.
    #[test]
    fn export_mock_geometry() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings");
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = serde_json::to_string_pretty(&EffectInfo::catalog()).unwrap();
        std::fs::write(dir.join("lighting-catalog.json"), catalog + "\n").unwrap();
        let keymap = serde_json::to_string_pretty(&KeymapCatalog::build()).unwrap();
        std::fs::write(dir.join("keymap-catalog.json"), keymap + "\n").unwrap();
        for m in kb_devices::catalogue().unwrap() {
            let Some(g) = &m.geometry else { continue };
            let json = serde_json::to_string_pretty(&GeometryInfo::from_model(g)).unwrap();
            std::fs::write(dir.join(format!("geometry-{}.json", m.id)), json + "\n").unwrap();
        }
    }
}

/// A settings file that could not be read at start-up and went back to its
/// defaults. The old file was kept beside it (`backup`); the UI names the
/// record once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SettingsReset {
    /// File name, e.g. `keymap.json`.
    pub file: String,
    /// Name of the kept copy; `None` when it could not be moved aside (it is
    /// then left in place and not overwritten).
    pub backup: Option<String>,
}

impl From<crate::settings::Reset> for SettingsReset {
    fn from(r: crate::settings::Reset) -> Self {
        Self { file: r.file, backup: r.backup }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiEnvironment {
    pub app_version: String,
    /// Windows 11 Mica backdrop is active behind the webview.
    pub mica: bool,
    pub os_build: u32,
}

/// Running in the background (tray.rs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BackgroundSettings {
    /// Closing the window hides it; the app goes on in the tray.
    pub close_to_tray: bool,
    /// Windows starts the app (hidden) when the user signs in.
    pub start_with_windows: bool,
    /// The startup entry is there but Task Manager or Settings turned it
    /// off (read only: setting `start_with_windows` turns it on again).
    #[serde(default)]
    pub start_turned_off: bool,
    /// Only Windows' startup settings can change it now (read only; the
    /// Store build, whose startup task the user turned off there, or a
    /// policy set).
    #[serde(default)]
    pub start_locked: bool,
}

/// Windows' own pages the app opens (the Store build).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum WindowsPage {
    /// Settings › Apps › Startup.
    StartupApps,
    /// Microsoft Store › Downloads and updates.
    StoreUpdates,
    /// Settings › Apps › Installed apps.
    InstalledApps,
}

/// The tray menu's words and the background notice, in the UI language.
/// Templates keep their `{name}` places and plural forms
/// (`{n, plural, one {…} other {…}}`); the backend fills them in (i18n.rs).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrayTexts {
    /// The UI language: "en", "tr", "ru" or "zh-CN". It picks the plural
    /// forms, how a date further than tomorrow is written ("Nov 14",
    /// "14.11", "11月14日") and the documentation site's folder.
    pub locale: String,
    pub open: String,
    pub no_keyboard: String,
    /// `{n}`: percent.
    pub battery: String,
    pub conn_usb: String,
    pub conn_dongle: String,
    pub conn_bluetooth: String,
    pub focus: String,
    pub focus_start: String,
    pub focus_pause: String,
    pub focus_resume: String,
    pub focus_skip: String,
    pub focus_stop: String,
    pub phase_focus: String,
    pub phase_break: String,
    /// Block `{n}` of `{total}`.
    pub round: String,
    /// `{min}`.
    pub left: String,
    pub paused: String,
    /// `{blocks}` blocks, `{min}` minutes in all.
    pub ready: String,
    pub reminders: String,
    pub reminders_on: String,
    pub reminders_off: String,
    /// `{time}`, `{title}`.
    pub reminders_next: String,
    pub reminders_none: String,
    pub tomorrow: String,
    pub gauges: String,
    /// Music mode's switch.
    pub music: String,
    pub profiles: String,
    pub default_profile: String,
    pub quit: String,
    pub hidden_title: String,
    pub hidden_body: String,
    /// The menu's first line while a new version waits. `{version}`.
    pub update_available: String,
    /// The same once it is downloaded, to install it now. `{version}`.
    pub update_ready: String,
    /// Added to the icon's tooltip while a new version waits.
    pub update_tooltip: String,
    /// The line while a game session runs. `{name}`, `{time}`.
    pub game: String,
    /// The play time in it: `{h}` and `{min}`, `{min}`, under a minute.
    pub game_hours: String,
    pub game_minutes: String,
    pub game_less: String,
    /// Profiles ▸ the per-app profiles' switch.
    pub auto_profile: String,
    /// The notification that a new version is ready. `{version}`.
    pub notice_ready_title: String,
    pub notice_ready_body: String,
    pub notice_install: String,
    pub notice_later: String,
    /// The notification after an update. `{version}`.
    pub notice_installed_title: String,
    /// Its line when the app installed the update by itself.
    pub notice_installed_auto: String,
    pub notice_whats_new: String,
    /// Keyboard models' names in the UI language, by model id, for the
    /// models sold under another name there (Chinese: 黑爵 AK980 V2（带屏版）).
    /// A model not listed keeps its model file's name.
    #[serde(default)]
    #[ts(type = "Record<string, string>")]
    pub model_names: std::collections::BTreeMap<String, String>,
}

/// Until the UI has sent its words (a start with Windows, before the page
/// has loaded).
impl Default for TrayTexts {
    fn default() -> Self {
        let s = |t: &str| t.to_string();
        Self {
            locale: s("en"),
            open: s("Open Keymotion"),
            no_keyboard: s("No keyboard connected"),
            battery: s("battery {n}%"),
            conn_usb: s("USB cable"),
            conn_dongle: s("2.4 GHz receiver"),
            conn_bluetooth: s("Bluetooth"),
            focus: s("Focus timer"),
            focus_start: s("Start"),
            focus_pause: s("Pause"),
            focus_resume: s("Resume"),
            focus_skip: s("Skip to the next phase"),
            focus_stop: s("Stop"),
            phase_focus: s("Focus"),
            phase_break: s("Break"),
            round: s("Block {n}/{total}"),
            left: s("{min} min left"),
            paused: s("Paused"),
            ready: s("Ready: {blocks, plural, one {# block} other {# blocks}}, {min} min"),
            reminders: s("Reminders"),
            reminders_on: s("Reminders on"),
            reminders_off: s("Reminders off"),
            reminders_next: s("Next: {time} · {title}"),
            reminders_none: s("No reminders coming up"),
            tomorrow: s("tomorrow"),
            gauges: s("System gauges on the keys"),
            music: s("Music mode"),
            profiles: s("Profiles"),
            default_profile: s("Default"),
            quit: s("Quit"),
            hidden_title: s("Still running"),
            hidden_body: s("To open it again, click the Keymotion icon next to the clock."),
            update_available: s("Update available: Keymotion {version}"),
            update_ready: s("Install now: Keymotion {version}"),
            update_tooltip: s("update available"),
            game: s("Playing {name} · {time}"),
            game_hours: s("{h} h {min} min"),
            game_minutes: s("{min} min"),
            game_less: s("under 1 min"),
            auto_profile: s("Switch by app"),
            notice_ready_title: s("Keymotion {version} is ready"),
            notice_ready_body: s("Installing it closes the app and opens it again in a few seconds."),
            notice_install: s("Update now"),
            notice_later: s("Later"),
            notice_installed_title: s("Keymotion {version} installed"),
            notice_installed_auto: s("It updated by itself while the computer wasn't in use."),
            notice_whats_new: s("What's new"),
            model_names: Default::default(),
        }
    }
}

/// One LED and its colour (live frames, mirror read-back, per-key tables).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[ts(export)]
pub struct LedColor {
    pub led: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A firmware effect and which controls it responds to.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EffectInfo {
    pub id: u8,
    /// English vendor name; the UI translates it.
    pub name: String,
    pub brightness: bool,
    pub speed: bool,
    pub color: bool,
    pub rainbow: bool,
    /// 0, 2 or 4 selectable directions.
    pub directions: u8,
    /// How direction 0 moves (1 is the other way), for directional effects.
    pub motion: Option<EffectMotion>,
    pub factory_default: bool,
}

/// How a directional effect moves with direction 0; 1 runs the other way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum EffectMotion {
    /// 0 down, 1 up.
    Vertical,
    /// 0 right, 1 left.
    Horizontal,
    /// 0 counter-clockwise, 1 clockwise.
    Rotation,
    /// A train of lights along the LED order: 0 forward, 1 backward.
    Path,
    /// 0 up-right, 1 down-left.
    Diagonal,
}

impl From<kb_protocol::sonix::Motion> for EffectMotion {
    fn from(m: kb_protocol::sonix::Motion) -> Self {
        use kb_protocol::sonix::Motion as M;
        match m {
            M::Vertical => Self::Vertical,
            M::Horizontal => Self::Horizontal,
            M::Rotation => Self::Rotation,
            M::Path => Self::Path,
            M::Diagonal => Self::Diagonal,
        }
    }
}

impl EffectInfo {
    pub fn catalog() -> Vec<Self> {
        use kb_protocol::sonix::{caps, EFFECTS, FACTORY_EFFECT};
        EFFECTS
            .iter()
            .map(|e| Self {
                id: e.id,
                name: e.name.to_string(),
                brightness: e.has(caps::BRIGHTNESS),
                speed: e.has(caps::SPEED),
                color: e.has(caps::COLOR),
                rainbow: e.has(caps::RAINBOW),
                directions: if e.has(caps::DIRECTION4) {
                    4
                } else if e.has(caps::DIRECTION2) {
                    2
                } else {
                    0
                },
                motion: e.motion.map(EffectMotion::from),
                factory_default: e.id == FACTORY_EFFECT,
            })
            .collect()
    }
}

/// Keyboard clock sync state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClockStatus {
    /// Set the clock when the keyboard connects over USB, at most once a day.
    pub auto_sync: bool,
    /// Last successful sync, Unix time in milliseconds.
    pub last_sync_ms: Option<f64>,
}

impl From<crate::settings::ClockSettings> for ClockStatus {
    fn from(c: crate::settings::ClockSettings) -> Self {
        Self { auto_sync: c.auto_sync, last_sync_ms: c.last_sync_ms.map(|ms| ms as f64) }
    }
}

/// What the UI believes the keyboard shows, so the mirror can resolve the
/// read-back's ambiguous red byte (`04 F5` reports red as `2·r mod 256`).
#[derive(Debug, Clone, serde::Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MirrorHint {
    /// Brightness level (0–5) of the saved effect or design.
    pub brightness: u8,
    /// Single-colour firmware effect: its colour, `#RRGGBB`.
    pub color: Option<String>,
    /// Per-key design: the saved colours (keys not listed are off).
    pub keys: Option<Vec<LedColor>>,
}

/// Effect settings as the UI edits them.
#[derive(Debug, Clone, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EffectRequest {
    pub mode: u8,
    /// `#RRGGBB`
    pub color: String,
    pub rainbow: bool,
    pub brightness: u8,
    pub speed: u8,
    pub direction: u8,
}

pub fn parse_hex(s: &str) -> Result<[u8; 3], String> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return Err(format!("invalid colour `{s}`"));
    }
    let v = u32::from_str_radix(h, 16).map_err(|_| format!("invalid colour `{s}`"))?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl EffectRequest {
    pub fn to_settings(&self) -> Result<kb_protocol::sonix::EffectSettings, String> {
        Ok(kb_protocol::sonix::EffectSettings {
            mode: self.mode,
            color: parse_hex(&self.color)?,
            rainbow: self.rainbow,
            brightness: self.brightness,
            speed: self.speed,
            direction: self.direction,
        })
    }
}

/// sRGB → LED colour calibration, per channel (R, G, B).
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, Serialize, TS)]
#[ts(export)]
pub struct Calibration {
    /// Exponent per channel; 1.0 sends raw values.
    pub gamma: [f32; 3],
    /// Gain per channel, 0.0–1.0 (white balance).
    pub gain: [f32; 3],
}

impl From<kb_core::color::LedCalibration> for Calibration {
    fn from(c: kb_core::color::LedCalibration) -> Self {
        Self { gamma: c.gamma, gain: c.gain }
    }
}

impl From<Calibration> for kb_core::color::LedCalibration {
    fn from(c: Calibration) -> Self {
        Self { gamma: c.gamma, gain: c.gain }.sanitized()
    }
}

/// The active calibration plus the values the UI needs to edit it.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalibrationInfo {
    pub current: Calibration,
    pub factory: Calibration,
    pub gamma_min: f32,
    pub gamma_max: f32,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ThemePreference {
    System,
    Dark,
    Light,
}

// ------------------------------------------------------------------ keymap

/// Which keymap layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum KeymapLayer {
    Base,
    Fn,
}

impl From<KeymapLayer> for Layer {
    fn from(l: KeymapLayer) -> Self {
        match l {
            KeymapLayer::Base => Layer::Base,
            KeymapLayer::Fn => Layer::Fn,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum MouseButtonName {
    Left,
    Right,
    Middle,
    /// Side button 4: browser back.
    Back,
    /// Side button 5: browser forward.
    Forward,
    DoubleClick,
}

impl From<MouseButton> for MouseButtonName {
    fn from(b: MouseButton) -> Self {
        match b {
            MouseButton::Left => Self::Left,
            MouseButton::Right => Self::Right,
            MouseButton::Middle => Self::Middle,
            MouseButton::Back => Self::Back,
            MouseButton::Forward => Self::Forward,
            MouseButton::DoubleClick => Self::DoubleClick,
        }
    }
}

impl From<MouseButtonName> for MouseButton {
    fn from(b: MouseButtonName) -> Self {
        match b {
            MouseButtonName::Left => Self::Left,
            MouseButtonName::Right => Self::Right,
            MouseButtonName::Middle => Self::Middle,
            MouseButtonName::Back => Self::Back,
            MouseButtonName::Forward => Self::Forward,
            MouseButtonName::DoubleClick => Self::DoubleClick,
        }
    }
}

/// How a key plays its macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum MacroPlayMode {
    /// Once per press.
    Once,
    /// `count` times per press.
    Times,
    /// Until the key is pressed again.
    Toggle,
    /// While the key is held.
    Hold,
}

/// What a key does, as the UI edits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[ts(export)]
pub enum KeyActionInfo {
    /// The firmware's own function.
    Default,
    /// The key does nothing.
    Disabled,
    /// A keyboard usage with modifier bits (`code` 0: modifiers only).
    Key {
        modifiers: u8,
        code: u8,
    },
    /// A consumer-control usage (media, volume, apps).
    Media {
        usage: u16,
    },
    Mouse {
        button: MouseButtonName,
    },
    /// One wheel notch.
    Wheel {
        up: bool,
    },
    /// Play stored macro `slot`; `count` is used with `times` (1–255).
    Macro {
        slot: u8,
        mode: MacroPlayMode,
        count: u8,
    },
}

impl From<KeyAction> for KeyActionInfo {
    fn from(a: KeyAction) -> Self {
        match a {
            KeyAction::Default => Self::Default,
            KeyAction::Disabled => Self::Disabled,
            KeyAction::Key { mods, code } => Self::Key { modifiers: mods, code },
            KeyAction::Consumer(usage) => Self::Media { usage },
            KeyAction::Mouse(b) => Self::Mouse { button: b.into() },
            KeyAction::Wheel { up } => Self::Wheel { up },
            KeyAction::Macro { slot, play } => {
                let (mode, count) = match play {
                    MacroPlay::Once => (MacroPlayMode::Once, 1),
                    MacroPlay::Times(n) => (MacroPlayMode::Times, n),
                    MacroPlay::Toggle => (MacroPlayMode::Toggle, 1),
                    MacroPlay::WhileHeld => (MacroPlayMode::Hold, 1),
                };
                Self::Macro { slot, mode, count }
            }
        }
    }
}

impl From<KeyActionInfo> for KeyAction {
    fn from(a: KeyActionInfo) -> Self {
        match a {
            KeyActionInfo::Default => Self::Default,
            KeyActionInfo::Disabled => Self::Disabled,
            KeyActionInfo::Key { modifiers, code } => Self::Key { mods: modifiers, code },
            KeyActionInfo::Media { usage } => Self::Consumer(usage),
            KeyActionInfo::Mouse { button } => Self::Mouse(button.into()),
            KeyActionInfo::Wheel { up } => Self::Wheel { up },
            KeyActionInfo::Macro { slot, mode, count } => Self::Macro {
                slot,
                play: match mode {
                    MacroPlayMode::Once => MacroPlay::Once,
                    MacroPlayMode::Times => MacroPlay::Times(count),
                    MacroPlayMode::Toggle => MacroPlay::Toggle,
                    MacroPlayMode::Hold => MacroPlay::WhileHeld,
                },
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[ts(export)]
pub struct KeyAssignment {
    pub led: u8,
    pub action: KeyActionInfo,
}

/// One layer as last written by this app.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeymapLayerState {
    /// Keys that differ from the firmware default.
    pub assignments: Vec<KeyAssignment>,
    /// Unix ms of the last write; `null` when the app never wrote this
    /// layer (taken to be the factory default).
    pub written_ms: Option<f64>,
    /// LEDs of keys set to something the app can neither show nor write
    /// (a firmware function, a combo, set in AJAZZ's tool...): shown as
    /// such, never changed by a save ("AA" family; empty otherwise).
    pub fixed: Vec<u8>,
    /// LEDs of keys at one of the keyboard's own functions in this layer
    /// (a factory reset, a mode switch): shown as such, never changed
    /// ("AA" family; empty otherwise).
    pub builtin: Vec<u8>,
    /// LEDs of keys in `assignments` whose default never comes back (their
    /// factory function is one of the keyboard's own): no reset offered.
    pub pinned: Vec<u8>,
}

impl From<&StoredLayer> for KeymapLayerState {
    fn from(l: &StoredLayer) -> Self {
        Self {
            assignments: l.actions().into_iter().map(|(led, a)| KeyAssignment { led, action: a.into() }).collect(),
            written_ms: l.written_ms.map(|ms| ms as f64),
            fixed: l.fixed.clone(),
            builtin: l.builtin.clone(),
            pinned: l.pinned.clone(),
        }
    }
}

/// A keyboard's keymap: what the app last wrote ("04 xx", which cannot
/// report it), or what the keyboard itself holds ("AA", `readMs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct KeymapState {
    pub base: KeymapLayerState,
    #[serde(rename = "fn")]
    pub fn_layer: KeymapLayerState,
    /// When the layers were read from the keyboard (Unix ms); `null`: they
    /// are the app's own record.
    #[serde(rename = "readMs")]
    pub read_ms: Option<f64>,
    /// The keyboard's tables the layers were read from: sent back with a
    /// save (`apply_keymap`'s `base`), which is refused when the keyboard
    /// changed since ("AA"; `null` otherwise).
    pub source: Option<String>,
}

impl From<&StoredKeymap> for KeymapState {
    fn from(k: &StoredKeymap) -> Self {
        Self {
            base: (&k.base).into(),
            fn_layer: (&k.fn_layer).into(),
            read_ms: k.read_ms.map(|ms| ms as f64),
            source: k.source.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum KeyUsageGroup {
    Letter,
    Digit,
    Function,
    Editing,
    Navigation,
    Symbol,
    Keypad,
    Modifier,
}

impl From<UsageGroup> for KeyUsageGroup {
    fn from(g: UsageGroup) -> Self {
        match g {
            UsageGroup::Letter => Self::Letter,
            UsageGroup::Digit => Self::Digit,
            UsageGroup::Function => Self::Function,
            UsageGroup::Editing => Self::Editing,
            UsageGroup::Navigation => Self::Navigation,
            UsageGroup::Symbol => Self::Symbol,
            UsageGroup::Keypad => Self::Keypad,
            UsageGroup::Modifier => Self::Modifier,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct KeyUsageInfo {
    pub code: u8,
    pub id: String,
    /// Keycap legend (US positions).
    pub label: String,
    pub group: KeyUsageGroup,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct MediaUsageInfo {
    pub usage: u16,
    /// Stable id; the UI translates it.
    pub id: String,
}

/// Everything a key can be assigned.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct KeymapCatalog {
    pub keys: Vec<KeyUsageInfo>,
    pub media: Vec<MediaUsageInfo>,
    pub mouse: Vec<MouseButtonName>,
}

impl KeymapCatalog {
    pub fn build() -> Self {
        use kb_protocol::sonix::keymap::{CONSUMER, USAGES};
        Self {
            keys: USAGES
                .iter()
                .map(|u| KeyUsageInfo { code: u.code, id: u.id.into(), label: u.label.into(), group: u.group.into() })
                .collect(),
            media: CONSUMER.iter().map(|c| MediaUsageInfo { usage: c.code, id: c.id.into() }).collect(),
            mouse: MouseButton::ALL.into_iter().map(MouseButtonName::from).collect(),
        }
    }
}

// ------------------------------------------------------------------ macros

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum MacroStepKind {
    KeyDown,
    KeyUp,
    MouseDown,
    MouseUp,
}

/// One macro step: press or release, then wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MacroStepInfo {
    pub kind: MacroStepKind,
    /// Keyboard usage (modifiers as 0xE0–0xE7) or mouse mask (1 left, 2 right, 4 middle).
    pub code: u8,
    /// Pause after the step, in ms.
    pub delay_ms: u16,
}

impl From<MacroStep> for MacroStepInfo {
    fn from(s: MacroStep) -> Self {
        let kind = match s.kind {
            StepKind::KeyDown => MacroStepKind::KeyDown,
            StepKind::KeyUp => MacroStepKind::KeyUp,
            StepKind::MouseDown => MacroStepKind::MouseDown,
            StepKind::MouseUp => MacroStepKind::MouseUp,
        };
        Self { kind, code: s.code, delay_ms: s.delay_ms }
    }
}

impl From<MacroStepInfo> for MacroStep {
    fn from(s: MacroStepInfo) -> Self {
        let kind = match s.kind {
            MacroStepKind::KeyDown => StepKind::KeyDown,
            MacroStepKind::KeyUp => StepKind::KeyUp,
            MacroStepKind::MouseDown => StepKind::MouseDown,
            MacroStepKind::MouseUp => StepKind::MouseUp,
        };
        MacroStep { kind, code: s.code, delay_ms: s.delay_ms }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Serialize, TS)]
#[ts(export)]
pub struct MacroInfo {
    /// The app's own name; the keyboard stores none.
    pub name: String,
    pub steps: Vec<MacroStepInfo>,
}

impl From<&StoredMacro> for MacroInfo {
    fn from(m: &StoredMacro) -> Self {
        Self { name: m.name.clone(), steps: m.steps.iter().map(|s| s.0.into()).collect() }
    }
}

impl From<MacroInfo> for StoredMacro {
    fn from(m: MacroInfo) -> Self {
        Self {
            name: m.name.trim().chars().take(MACRO_NAME_MAX).collect(),
            steps: m.steps.into_iter().map(|s| StoredStep(s.into())).collect(),
        }
    }
}

/// Longest macro name the app keeps.
pub const MACRO_NAME_MAX: usize = 40;

// ------------------------------------------------------------------ screen

/// What a library item holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum ScreenItemKind {
    /// One still image.
    Image,
    /// Several frames (GIF, video, slideshow).
    Animation,
    /// The keyboard's factory animation (backup; cannot be removed).
    Factory,
    /// Keymotion's own start-up animation, drawn by the app for keyboards
    /// without a factory backup (built in: one per library, cannot be
    /// removed or renamed).
    Keymotion,
}

impl ScreenItemKind {
    /// Comes with the app or the keyboard: kept, and named by the UI.
    pub fn built_in(self) -> bool {
        matches!(self, Self::Factory | Self::Keymotion)
    }
}

/// One prepared animation in the app's screen library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenItem {
    /// Content hash (16 hex digits).
    pub id: String,
    pub name: String,
    pub kind: ScreenItemKind,
    pub frames: u32,
    /// One loop, in ms.
    pub loop_ms: u32,
    /// Display-channel pages an upload takes.
    pub pages: u32,
    pub created_ms: f64,
    /// Last upload the keyboard confirmed; `null` when never uploaded.
    pub uploaded_ms: Option<f64>,
}

/// The panel and upload limits (kb-protocol `screen`). The frame limits
/// and the time per page here are the "04 xx" family's; the connected
/// keyboard's are in `KeyboardInfo.limits.screen`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenLimits {
    pub width: u32,
    pub height: u32,
    pub max_frames: u32,
    /// Frames proven on this keyboard (its own factory animation).
    pub proven_frames: u32,
    pub min_delay_ms: u32,
    pub max_delay_ms: u32,
    pub still_delay_ms: u32,
    /// Measured time per page (E1/E4, 2026-11-10).
    pub page_ms: u32,
    pub header_len: u32,
    pub frame_bytes: u32,
    pub max_pages: u32,
    pub name_max: u32,
}

impl ScreenLimits {
    pub fn new() -> Self {
        use kb_protocol::sonix::screen as s;
        Self {
            width: s::WIDTH as u32,
            height: s::HEIGHT as u32,
            max_frames: s::MAX_FRAMES as u32,
            proven_frames: s::PROVEN_FRAMES as u32,
            min_delay_ms: s::MIN_DELAY as u32 * s::DELAY_UNIT_MS,
            max_delay_ms: s::MAX_DELAY as u32 * s::DELAY_UNIT_MS,
            still_delay_ms: s::STILL_DELAY as u32 * s::DELAY_UNIT_MS,
            page_ms: crate::driver::family::FF13_PAGE_MS,
            header_len: s::HEADER_LEN as u32,
            frame_bytes: s::FRAME_BYTES as u32,
            max_pages: s::MAX_PAGES as u32,
            name_max: crate::screen_lib::NAME_MAX as u32,
        }
    }
}

impl Default for ScreenLimits {
    fn default() -> Self {
        Self::new()
    }
}

/// The screen library and what the keyboard's GIF page shows.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenState {
    /// Factory backup first, then newest first.
    pub items: Vec<ScreenItem>,
    /// Item the GIF page shows, as far as the app knows (`null`: unknown,
    /// for example the factory animation the keyboard shipped with).
    pub shown: Option<String>,
    pub uploads_today: u32,
    pub uploads_total: u32,
    pub limits: ScreenLimits,
    /// The last upload to the GIF page stopped partway (an error, a quit, a
    /// crash): the page holds part of it. `shown` is `null` meanwhile.
    pub incomplete: Option<ScreenIncomplete>,
    /// The next upload may replace this keyboard's clock page for good, and
    /// the user has not said yes yet (`screen_ack_clock_page`): the first
    /// upload to an "AA" keyboard. Always `false` in the "04 xx" family.
    pub clock_page_at_risk: bool,
}

/// An upload that was cut short (see [`ScreenState::incomplete`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenIncomplete {
    /// Library item to upload again: the one cut short, or for a live card
    /// the user's own picture. `null` when there is none.
    pub id: Option<String>,
    /// When the upload started (Unix ms).
    pub at_ms: f64,
}

/// A write to the keyboard's flash in progress (`flash-write` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FlashWrite {
    /// A screen upload (up to ~5 minutes).
    Screen,
    /// Lighting, keymap or macros (under a second or two).
    Settings,
}

/// `screen-upload-progress` event payload.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScreenProgress {
    pub id: String,
    pub done: u32,
    pub total: u32,
}

/// What the app last wrote to a model's macro memory, plus its limits.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MacrosState {
    /// Slot `i` is macro `i`; `null` marks an unused slot.
    pub slots: Vec<Option<MacroInfo>>,
    /// Unix ms of the last write; `null` when the app never wrote macros.
    pub written_ms: Option<f64>,
    /// Memory limit of the "04 xx" family: `(steps + slots) × 8` bytes may
    /// not exceed this (`KeyboardInfo.limits.macroBudget` names the rule a
    /// keyboard counts by).
    pub budget: u32,
    pub max_slots: u32,
    pub min_delay_ms: u16,
    pub max_name: u32,
    /// Macros the keyboard holds that the app's steps cannot express (made
    /// in another program): shown as such, never changed by a save ("AA"
    /// family; empty otherwise).
    pub fixed: Vec<MacroFixed>,
    /// When the macros were read from the keyboard (Unix ms); `null`: they
    /// are the app's own record.
    pub read_ms: Option<f64>,
    /// The keyboard's macro memory they were read from: sent back with a
    /// save (`apply_macros`' `base`; see `KeymapState::source`).
    pub source: Option<String>,
}

/// A macro slot kept as the keyboard has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MacroFixed {
    pub slot: u8,
    /// Its steps as the keyboard counts them (macro memory).
    pub steps: u16,
}

impl From<&StoredMacros> for MacrosState {
    fn from(m: &StoredMacros) -> Self {
        use kb_protocol::sonix::macros::{BUDGET, MAX_MACROS, MIN_DELAY_MS};
        Self {
            fixed: m.fixed.iter().map(|f| MacroFixed { slot: f.slot, steps: f.entries }).collect(),
            read_ms: m.read_ms.map(|ms| ms as f64),
            source: m.source.clone(),
            slots: m.slots.iter().map(|s| s.as_ref().map(MacroInfo::from)).collect(),
            written_ms: m.written_ms.map(|ms| ms as f64),
            budget: BUDGET as u32,
            max_slots: MAX_MACROS as u32,
            min_delay_ms: MIN_DELAY_MS,
            max_name: MACRO_NAME_MAX as u32,
        }
    }
}

// ------------------------------------------------------------- live screen

/// How many automatic screen uploads a day the live mode may use (flash
/// wear budget; docs site: developers/architecture).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LivePlan {
    /// 8 a day.
    Saver,
    /// 24 a day (default).
    #[default]
    Balanced,
    /// 48 a day.
    Heavy,
}

impl LivePlan {
    pub fn per_day(self) -> u32 {
        match self {
            Self::Saver => 8,
            Self::Balanced => 24,
            Self::Heavy => 48,
        }
    }

    /// Uploads that may run back to back before the hourly refill limits them.
    pub fn burst(self) -> u32 {
        match self {
            Self::Saver => 3,
            Self::Balanced => 6,
            Self::Heavy => 8,
        }
    }
}

/// A card waiting for, or going through, an automatic upload.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LiveItem {
    /// Source id, e.g. `now-playing`.
    pub source: String,
    pub name: String,
}

/// The last card the live mode put on the GIF page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LiveShown {
    pub source: String,
    pub name: String,
    pub at_ms: f64,
}

/// What Windows reports as playing (System Media Transport Controls).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrackInfo {
    /// Stable id of the track (app + title + artist).
    pub key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Friendly name of the playing app when known (Spotify, Chrome, …).
    pub app: String,
    pub playing: bool,
    /// Played long enough (and not paused) to be worth an upload.
    pub settled: bool,
    pub has_cover: bool,
}

/// What the GIF page showed before the live mode replaced it: offered to
/// put back when the live mode is turned off.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LiveRestore {
    /// Library item id.
    pub id: String,
    pub name: String,
    pub frames: u32,
    pub pages: u32,
}

/// Live mode, its budget and sources (`live-screen` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LiveState {
    pub enabled: bool,
    pub plan: LivePlan,
    pub per_day: u32,
    pub burst: u32,
    /// Uploads available now (may be fractional while refilling, negative
    /// after manual uploads beyond the budget).
    pub tokens: f64,
    /// Time until the next upload is allowed, when none is available now.
    pub next_token_ms: Option<f64>,
    pub pending: Option<LiveItem>,
    /// Time until the waiting card may go, when it waits for the budget.
    /// The focus timer's and the song's cards leave one upload for a
    /// reminder or a meeting, so theirs can be later than `next_token_ms`.
    pub pending_wait_ms: Option<f64>,
    pub uploading: Option<LiveItem>,
    pub last: Option<LiveShown>,
    pub last_error: Option<String>,
    /// Source switch: the song playing.
    pub now_playing: bool,
    pub track: Option<TrackInfo>,
    /// A live card is on the GIF page and this library item was there
    /// before it.
    pub restore: Option<LiveRestore>,
    /// The source whose card is on the GIF page now (`calendar`,
    /// `reminder`, `pomodoro`, `now-playing`), as far as the app knows;
    /// null when the user's own picture or the factory animation is there.
    /// Not the same as `restore`: the first card can replace the factory
    /// animation, which has nothing to put back.
    pub showing: Option<String>,
    /// Someone holds the screen (the calendar around a meeting, the focus
    /// timer while it runs): "Çalan şarkı" waits.
    pub held: bool,
    /// Who holds it (`calendar`, `pomodoro`).
    pub holders: Vec<String>,
    /// What "Çalan şarkı" does when the music stops.
    pub song_stop: SongStop,
    /// The user's own picture: what the screen goes back to when no source
    /// has anything to show (the last library item uploaded by hand).
    pub own: Option<LiveRestore>,
    /// The screen goes back to the user's own picture at this time.
    pub going_back: Option<LiveBack>,
    /// Turning the live mode on asks first: there is no factory backup and
    /// the GIF page probably still shows the factory animation, which the
    /// first card erases for good. Asked once.
    pub factory_ack_needed: bool,
    /// The keyboard on the cable would lose its clock page with the next
    /// upload (an "AA" keyboard whose screen the app never wrote, and the
    /// user has not said yes: `screen_ack_clock_page`). No card goes and
    /// the screen does not go back to the own picture until then.
    pub clock_ack_needed: bool,
}

/// What the "Çalan şarkı" source does when the music stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SongStop {
    /// After a while without music, the user's own picture comes back.
    #[default]
    Back,
    /// The song's card stays until the next card.
    Keep,
}

/// A source has nothing to show any more: the screen goes back to the
/// user's own picture unless something else comes first.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LiveBack {
    /// Source whose card is on the screen now.
    pub source: String,
    pub at_ms: f64,
}

/// What a block of the focus timer is: focus or a break. A break is a
/// break, whatever its length (K7: the owner found "short" and "long"
/// breaks that differ only in length pointless).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroPhase {
    #[default]
    Focus,
    Break,
}

/// One block of the sequence the user builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroBlock {
    pub phase: PomodoroPhase,
    /// 1–`BLOCK_MAX_MIN`.
    pub minutes: u16,
}

impl PomodoroBlock {
    pub const fn focus(minutes: u16) -> Self {
        Self { phase: PomodoroPhase::Focus, minutes }
    }

    pub const fn rest(minutes: u16) -> Self {
        Self { phase: PomodoroPhase::Break, minutes }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroStatus {
    #[default]
    Idle,
    Running,
    Paused,
}

/// How the light leaves the keyboard while a phase runs. The keyboard is
/// full at the start of a phase and empties towards its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroMotion {
    /// The right side goes dark first; the light ends on the left.
    #[default]
    RightToLeft,
    /// The left side goes dark first.
    LeftToRight,
    /// The top rows go dark first, like a level falling.
    TopToBottom,
    /// Both sides go dark first; the light ends in the middle.
    ToCenter,
    /// The whole keyboard dims evenly.
    Fade,
}

/// What the keys do when a phase starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroTransition {
    /// The whole keyboard blinks twice in the new colour.
    #[default]
    Blink,
    /// The new colour washes over the keyboard from the side that stays
    /// lit longest.
    Wave,
}

/// Pomodoro settings (persisted in `pomodoro.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroSettings {
    /// The sequence the user built, in order (1–`BLOCKS_MAX` blocks).
    /// Owner's design, 2026-11-12 (K7): blocks are added, removed and
    /// dragged into order; "25 / 5 klasik" is the default.
    pub blocks: Vec<PomodoroBlock>,
    /// How many times the sequence runs: 1–`REPEAT_MAX`, 0 until stopped.
    pub repeat: u8,
    /// Progress on the key lights (RAM only, no flash write).
    pub lights: bool,
    /// A card on the screen when a phase starts (through the live mode).
    pub screen: bool,
    pub motion: PomodoroMotion,
    /// Phase colours (UI colours, sRGB).
    pub focus_color: [u8; 3],
    pub break_color: [u8; 3],
    /// Light brightness in the keyboard's own steps, 1–5.
    pub brightness: u8,
    /// The light that is left breathes in the last minute of a phase.
    pub pulse: bool,
    pub transition: PomodoroTransition,
    /// The Windows notification sound when a phase ends by itself.
    pub sound: bool,
    /// What the screen shows when a series ends or is stopped.
    pub end_screen: PomodoroEndScreen,
    /// Settings files from before the sequence (focus, short and long
    /// break, rounds): read once, turned into `blocks` by `sanitized`,
    /// never written.
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub focus_min: Option<u16>,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub short_min: Option<u16>,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub long_min: Option<u16>,
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub rounds: Option<u8>,
    /// The old short break's colour becomes the break colour (the long
    /// break's is dropped).
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub short_color: Option<[u8; 3]>,
}

/// Longest block, in minutes.
pub const BLOCK_MAX_MIN: u16 = 180;
/// Most blocks in a sequence.
pub const BLOCKS_MAX: usize = 16;
/// Most runs of the sequence (0 means until stopped).
pub const REPEAT_MAX: u8 = 9;

/// "25 / 5 klasik": three focus blocks of 25 minutes, 5-minute breaks
/// between them, a 15-minute break at the end (the owner's choice from the
/// preview, 2026-11-12).
pub fn classic_blocks() -> Vec<PomodoroBlock> {
    use PomodoroBlock as B;
    vec![B::focus(25), B::rest(5), B::focus(25), B::rest(5), B::focus(25), B::rest(15)]
}

/// What the screen shows when a series ends or is stopped (with screen
/// cards on). A song that plays always comes back first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroEndScreen {
    /// The user's own picture comes back (owner's choice, 2026-11-10).
    #[default]
    Back,
    /// A summary card (rounds, minutes) stays until the next card.
    Summary,
}

impl Default for PomodoroSettings {
    fn default() -> Self {
        Self {
            blocks: classic_blocks(),
            repeat: 1,
            lights: true,
            screen: true,
            motion: PomodoroMotion::default(),
            focus_color: [255, 80, 20],
            break_color: [30, 220, 100],
            brightness: 4,
            pulse: true,
            transition: PomodoroTransition::default(),
            sound: false,
            end_screen: PomodoroEndScreen::default(),
            focus_min: None,
            short_min: None,
            long_min: None,
            rounds: None,
            short_color: None,
        }
    }
}

impl PomodoroSettings {
    /// In range, and an older file's focus, breaks and rounds turned into
    /// the sequence they made: each round a focus block, a short break
    /// between rounds and the long break at the end.
    pub fn sanitized(mut self) -> Self {
        if let Some(rounds) = self.rounds.take() {
            let focus = self.focus_min.take().unwrap_or(25);
            let short = self.short_min.take().unwrap_or(5);
            let long = self.long_min.take().unwrap_or(15);
            let rounds = rounds.clamp(1, 8);
            self.blocks = (1..=rounds)
                .flat_map(|r| {
                    [PomodoroBlock::focus(focus), PomodoroBlock::rest(if r == rounds { long } else { short })]
                })
                .collect();
            self.repeat = 1;
        }
        if let Some(c) = self.short_color.take() {
            self.break_color = c;
        }
        (self.focus_min, self.short_min, self.long_min) = (None, None, None);
        self.blocks.truncate(BLOCKS_MAX);
        for b in &mut self.blocks {
            b.minutes = b.minutes.clamp(1, BLOCK_MAX_MIN);
        }
        if self.blocks.is_empty() {
            self.blocks = classic_blocks();
        }
        self.repeat = self.repeat.min(REPEAT_MAX);
        self.brightness = self.brightness.clamp(1, 5);
        self
    }

    pub fn color(&self, phase: PomodoroPhase) -> [u8; 3] {
        match phase {
            PomodoroPhase::Focus => self.focus_color,
            PomodoroPhase::Break => self.break_color,
        }
    }

    /// Focus blocks in one run of the sequence (at least 1, for the cards).
    pub fn focus_blocks(&self) -> u8 {
        self.blocks.iter().filter(|b| b.phase == PomodoroPhase::Focus).count().clamp(1, 255) as u8
    }

    /// Minutes of one run of the sequence.
    pub fn total_minutes(&self) -> u32 {
        self.blocks.iter().map(|b| u32::from(b.minutes)).sum()
    }
}

/// How the last series ended.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroSummary {
    /// Focus phases finished (or skipped forward).
    pub focus_done: u8,
    /// Minutes actually spent in focus.
    pub focus_min: u32,
    /// All rounds done, the long break included.
    pub complete: bool,
    pub ended_ms: f64,
}

/// Pomodoro state (`pomodoro` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroState {
    pub settings: PomodoroSettings,
    pub status: PomodoroStatus,
    pub phase: PomodoroPhase,
    /// The current block in the sequence (1-based).
    pub block: u8,
    /// The focus block the current one is or follows (1-based, of
    /// `settings.focus_blocks()`).
    pub round: u8,
    /// The run of the sequence (1-based).
    pub cycle: u8,
    /// Length of the current phase.
    pub phase_ms: f64,
    /// Time left in the phase as of `at_ms` (counts down from there while
    /// running).
    pub remaining_ms: f64,
    pub at_ms: f64,
    pub focus_done: u8,
    /// The last series, once the timer is idle again.
    pub last: Option<PomodoroSummary>,
    /// "Klavyede dene" is showing a short run on the keys.
    pub trying: bool,
    /// An agent started this series as one focus block of this many
    /// minutes (`keymotion-cli focus start --for 50m`), without changing
    /// `settings`: the series is that block alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub once: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroCardKind {
    /// A phase started.
    Phase,
    /// The timer stopped: a summary that stays true afterwards.
    Done,
}

/// A card the UI should draw and hand to the live mode
/// (`pomodoro-card` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroCard {
    pub kind: PomodoroCardKind,
    pub phase: PomodoroPhase,
    /// The focus block this one is or follows, of the focus blocks in one
    /// run (the ring on the card).
    pub round: u8,
    pub rounds: u8,
    /// The run of the sequence and how many there are (0: until stopped).
    pub cycle: u8,
    pub cycles: u8,
    /// Phase length in minutes.
    pub minutes: u16,
    /// The block that comes next, `None` when this is the last.
    pub next: Option<PomodoroBlock>,
    pub focus_done: u8,
    pub focus_min: u32,
    /// Summary: every round was done, the long break included.
    pub complete: bool,
    pub ended_ms: f64,
}

// ------------------------------------------------------------ system gauges

/// How a gauge bar is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GaugeStyle {
    /// Green, then yellow, then red towards the end of the bar: the load
    /// reads at a glance (owner's choice, 2026-11-10).
    #[default]
    Heat,
    /// Each gauge in its own colour.
    Own,
}

/// What the keys outside the gauges show while the gauges are on. A live
/// frame covers the whole keyboard, so the saved effect cannot show under
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GaugeBase {
    #[default]
    Dark,
    /// A faint colour (`GaugesSettings::tint`).
    Tint,
}

/// System gauges settings (persisted in `gauges.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct GaugesSettings {
    /// The gauges are on the keys (RAM only, no flash write).
    pub enabled: bool,
    /// Processor load on the number row (1 … 0).
    pub cpu: bool,
    /// Graphics load on the Q row (Q … P).
    pub gpu: bool,
    /// Memory in use on the home row (A … ;).
    pub ram: bool,
    /// The volume on the F row for a moment when it changes (the knob).
    pub volume: bool,
    /// A key lit red while the microphone is muted in Windows.
    pub mic: bool,
    /// Id of that key in the model's geometry.
    pub mic_key: String,
    pub style: GaugeStyle,
    /// The empty part of a bar is faintly lit, so its length shows.
    pub track: bool,
    pub base: GaugeBase,
    /// The faint colour of `GaugeBase::Tint` (UI colour, sRGB).
    pub tint: [u8; 3],
    /// Light brightness in the keyboard's own steps, 1–5.
    pub brightness: u8,
}

impl Default for GaugesSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            cpu: true,
            gpu: true,
            ram: false,
            volume: true,
            mic: true,
            mic_key: "delete".into(),
            style: GaugeStyle::default(),
            track: true,
            base: GaugeBase::default(),
            tint: [124, 92, 255],
            brightness: 4,
        }
    }
}

impl GaugesSettings {
    pub fn sanitized(self) -> Self {
        let key_ok = !self.mic_key.is_empty()
            && self.mic_key.len() <= 32
            && self.mic_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        Self {
            brightness: self.brightness.clamp(1, 5),
            mic_key: if key_ok { self.mic_key } else { Self::default().mic_key },
            ..self
        }
    }
}

/// The latest readings (`None`: not read, or not there on this PC).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GaugeReadings {
    /// Processor load, 0–100.
    pub cpu: Option<f32>,
    /// Graphics load, 0–100 (the busiest card).
    pub gpu: Option<f32>,
    /// Memory in use, 0–100.
    pub ram: Option<f32>,
    /// Speaker volume, 0–100.
    pub volume: Option<f32>,
    pub muted: Option<bool>,
    pub mic_muted: Option<bool>,
}

/// System gauges state (`gauges` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GaugesState {
    pub settings: GaugesSettings,
    pub readings: GaugeReadings,
    /// What the gauges put on the keys now, alerts included (UI colours,
    /// before brightness). Empty when nothing would show.
    pub frame: Vec<LedColor>,
    /// The volume bar shows (for a moment after a change).
    pub volume_showing: bool,
}

// -------------------------------------------------------------- music mode

/// How the music lights move (owner's choice of all four, 2026-11-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MusicMode {
    /// Columns from left (bass) to right (treble), each filling from the
    /// bottom row up, with a mark at its peak for a moment.
    #[default]
    Bars,
    /// The whole keyboard in one colour, pulsing on each beat, a ring
    /// spreading from the middle on a strong beat.
    Beat,
    /// How loud it is, as a band opening from the middle to both sides.
    Level,
    /// Each bass beat sends a wave of light across the keys.
    Wave,
}

/// The music lights' colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MusicPalette {
    /// Green, yellow, red: from the bottom row up (bars), from the middle
    /// out (level).
    #[default]
    Heat,
    Rainbow,
    /// `MusicSettings::color`.
    One,
    /// From `color` to `color2`.
    Two,
}

/// The keys the music does not light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MusicRest {
    #[default]
    Dark,
    /// Faintly in the palette's colour.
    Dim,
}

/// What the keys show once it has been quiet for a few seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MusicSilence {
    /// The music lights step aside: the keyboard's own light (or the layer
    /// under them, such as the gauges) shows.
    #[default]
    Saved,
    /// Every key faintly lit.
    Dim,
    /// Every key off.
    Dark,
}

/// Music mode settings (persisted in `music.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct MusicSettings {
    /// The music lights are on the keys (RAM only, no flash write).
    pub enabled: bool,
    pub mode: MusicMode,
    pub palette: MusicPalette,
    /// The colours of `One` and `Two` (UI colours, sRGB).
    pub color: [u8; 3],
    pub color2: [u8; 3],
    /// Bars and Wave start in the middle: bass in the middle, treble at
    /// both ends.
    pub center: bool,
    /// How strongly the lights react, in percent (50–200).
    pub sensitivity: u16,
    pub rest: MusicRest,
    pub silence: MusicSilence,
    /// The music lights wait while a game is in front.
    pub pause_in_games: bool,
    /// Light brightness in the keyboard's own steps, 1–5.
    pub brightness: u8,
}

impl Default for MusicSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: MusicMode::default(),
            palette: MusicPalette::default(),
            color: [0, 200, 255],
            color2: [255, 45, 149],
            center: false,
            sensitivity: 100,
            rest: MusicRest::default(),
            silence: MusicSilence::default(),
            pause_in_games: true,
            brightness: 4,
        }
    }
}

impl MusicSettings {
    pub fn sanitized(self) -> Self {
        Self { sensitivity: self.sensitivity.clamp(50, 200), brightness: self.brightness.clamp(1, 5), ..self }
    }
}

/// Where the music lights stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MusicStatus {
    /// Music mode is off.
    Off,
    /// No keyboard on the USB cable (the only connection that takes the
    /// lights).
    NoUsb,
    /// Windows' sound system could not be started.
    Unavailable,
    /// No speakers or headphones.
    NoOutput,
    /// A game is in front (and the lights wait for games), or a game wants
    /// the keyboard left alone.
    Game,
    /// Nothing plays.
    Waiting,
    /// The lights move with the sound.
    Playing,
}

/// Music mode state (`music` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MusicState {
    pub settings: MusicSettings,
    pub status: MusicStatus,
    /// The music lights are on the keys now (while sound plays, and while
    /// it's quiet with "Dim color" or "Dark"): the layers under them wait.
    pub covers: bool,
    /// What the music lights put on the keys now (UI colours, before
    /// brightness); on the catalogue's model without a keyboard. Empty
    /// when they show nothing.
    pub frame: Vec<LedColor>,
}

// ---------------------------------------------------------------- calendar

/// Where a published calendar comes from (help text and icon only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CalendarProvider {
    Google,
    Outlook,
    Apple,
    Other,
}

/// How long a meeting's card stays on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CalendarUntil {
    /// Until the meeting starts.
    Start,
    /// Until 5 minutes after it starts (owner's choice, 2026-11-11).
    #[default]
    Shortly,
    /// Until it ends.
    End,
}

/// Calendar settings (persisted in `calendar.json` with the calendars).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSettings {
    /// A card on the screen before a meeting (through the live mode).
    pub screen: bool,
    /// Minutes before the start the card comes (1–60).
    pub lead_min: u16,
    pub until: CalendarUntil,
    /// A key blinks in the minute before a meeting starts (RAM only).
    pub reminder: bool,
    /// Id of that key in the model's geometry.
    pub reminder_key: String,
}

impl Default for CalendarSettings {
    fn default() -> Self {
        Self { screen: true, lead_min: 10, until: CalendarUntil::default(), reminder: true, reminder_key: "esc".into() }
    }
}

impl CalendarSettings {
    pub fn sanitized(self) -> Self {
        let key_ok = !self.reminder_key.is_empty()
            && self.reminder_key.len() <= 32
            && self.reminder_key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        Self {
            lead_min: self.lead_min.clamp(1, 60),
            reminder_key: if key_ok { self.reminder_key } else { Self::default().reminder_key },
            ..self
        }
    }
}

/// One calendar the user added. Its address is a secret and stays in the
/// backend; the UI only sees where it is hosted.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSourceInfo {
    pub id: String,
    pub name: String,
    pub provider: CalendarProvider,
    pub host: String,
    pub color: [u8; 3],
    pub enabled: bool,
    /// Last successful read.
    pub last_ok_ms: Option<f64>,
    /// Why the last read failed (none when it worked).
    pub error: Option<String>,
    /// Events read for the next two days.
    pub events: u32,
}

/// One meeting (or an instance of a repeating one).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarEvent {
    pub title: String,
    pub location: String,
    pub start_ms: f64,
    pub end_ms: f64,
    pub all_day: bool,
    /// Its calendar's colour.
    pub color: [u8; 3],
    pub source: String,
}

/// Calendar state (`calendar` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarState {
    pub settings: CalendarSettings,
    pub sources: Vec<CalendarSourceInfo>,
    /// From now (running ones included) to a day ahead, at most 12.
    pub upcoming: Vec<CalendarEvent>,
    /// The meeting whose card time it is now.
    pub showing: Option<CalendarEvent>,
    /// When the next card would come.
    pub next_card_ms: Option<f64>,
    /// A calendar is being read now.
    pub reading: bool,
}

/// A card the UI should draw and hand to the live mode
/// (`calendar-card` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarCard {
    pub event: CalendarEvent,
    /// The meeting after it the same day.
    pub next: Option<CalendarEvent>,
}

// --------------------------------------------------------------- reminders

/// The picture on a reminder's card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReminderIcon {
    #[default]
    Bell,
    Drop,
    Stand,
    Eye,
    Pill,
}

/// When a reminder goes off. Times of day are minutes after local
/// midnight; `days`: bit 0 Monday … bit 6 Sunday.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum ReminderWhen {
    /// Once, at this moment (ms since the epoch).
    Once { at: f64 },
    /// At this time on the chosen days.
    Daily { minute: u16, days: u8 },
    /// Every `every` minutes from `from` to `to` on the chosen days: the
    /// first `every` after `from`, the last at `to` at the latest. Counted
    /// from the last time it went off (or from the user's return).
    Every { every: u16, from: u16, to: u16, days: u8 },
}

impl Default for ReminderWhen {
    fn default() -> Self {
        Self::Daily { minute: 9 * 60, days: 0b001_1111 }
    }
}

/// One reminder. The light always shows; the card and the sound are the
/// user's choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Reminder {
    /// Empty when adding.
    pub id: String,
    pub title: String,
    /// A short line under the title ("20 sn uzağa bak").
    pub note: String,
    pub icon: ReminderIcon,
    pub color: [u8; 3],
    pub when: ReminderWhen,
    pub enabled: bool,
    /// A card on the screen too (through the live mode).
    pub card: bool,
    /// The Windows notification sound.
    pub sound: bool,
}

impl Default for Reminder {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            note: String::new(),
            icon: ReminderIcon::default(),
            color: [255, 176, 32],
            when: ReminderWhen::default(),
            enabled: true,
            card: true,
            sound: false,
        }
    }
}

/// Reminder settings (persisted in `reminders.json` with the reminders).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct ReminderSettings {
    /// Reminders go off at all.
    pub enabled: bool,
    /// Cards on the screen (through the live mode).
    pub screen: bool,
    /// Minutes a card stays on the screen: 1, 2 or 5.
    pub stay_min: u8,
    /// Repeating reminders wait while nobody is at the computer and count
    /// again from the return (owner's choice, 2026-11-11).
    pub away_wait: bool,
    /// A Windows notification each time, too. Without the keyboard on the
    /// USB cable reminders always come as a notification, since the keys
    /// cannot light up.
    pub notify: bool,
}

impl Default for ReminderSettings {
    fn default() -> Self {
        Self { enabled: true, screen: true, stay_min: 2, away_wait: true, notify: false }
    }
}

impl ReminderSettings {
    pub fn sanitized(self) -> Self {
        Self { stay_min: if [1, 2, 5].contains(&self.stay_min) { self.stay_min } else { 2 }, ..self }
    }
}

/// A reminder and when it goes off next.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReminderInfo {
    pub reminder: Reminder,
    /// `None`: off, or a one-time reminder that has gone by.
    pub next_ms: Option<f64>,
}

/// Reminder state (`reminders` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReminderState {
    pub settings: ReminderSettings,
    pub reminders: Vec<ReminderInfo>,
    /// The reminder whose card holds the screen now, and until when.
    pub showing: Option<String>,
    pub showing_until_ms: Option<f64>,
    /// Nobody has touched the computer for a while: repeating reminders wait.
    pub away: bool,
    /// The last reminder whose card could not go on the screen, and why;
    /// cleared when a reminder's card shows (K5).
    pub card_skip: Option<CardSkip>,
}

/// Why a reminder's card did not go on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CardSkipReason {
    /// "Show cards on the screen" is off on the Reminders page.
    CardsOff,
    /// The live screen is off.
    LiveOff,
    /// The keyboard is not on the USB cable.
    NoCable,
    /// A meeting's card holds the screen.
    Meeting,
    /// No upload left in the budget.
    Budget,
}

/// A reminder's card that did not go on the screen.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CardSkip {
    pub id: String,
    pub reason: CardSkipReason,
    /// When the reminder went off.
    pub at_ms: f64,
}

/// A card the UI should draw and hand to the live mode
/// (`reminder-card` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReminderCard {
    pub reminder: Reminder,
    /// When it went off (the time on the card).
    pub at_ms: f64,
    /// When it goes off next, if it repeats.
    pub next_ms: Option<f64>,
}

/// Updates (updates.rs): what the app knows about a newer version.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateState {
    /// The version running now ("0.1.0").
    pub current: String,
    /// What the app does about new versions by itself.
    pub mode: UpdateMode,
    /// When GitHub was last asked (ms since the epoch).
    pub last_check_ms: Option<f64>,
    pub status: UpdateStatus,
    /// Installed from the Microsoft Store, which updates it: the app's
    /// own updater stays off.
    pub store: bool,
    /// The Store build, with the GitHub copy installed beside it: it no
    /// longer opens (it starts this one instead) and can be removed.
    pub github_copy: bool,
}

/// What the app does about new versions by itself (Settings › About).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateMode {
    /// Look once a day, download a new version and install it while the
    /// computer is not in use.
    Auto,
    /// Look once a day and say so: a Windows notification, the tray menu,
    /// the card at the bottom of the sidebar. The user installs it.
    #[default]
    Notify,
    /// Never look by itself; the "Check for updates" button still does.
    Off,
}

/// Where an update stands.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateStatus {
    /// Not asked in this run.
    Idle,
    Checking,
    UpToDate,
    /// A newer version is on GitHub. `notes`: the release notes as the
    /// release pipeline wrote them (a JSON object with "en" and maybe "tr").
    #[serde(rename_all = "camelCase")]
    Available {
        version: String,
        notes: Option<String>,
        date_ms: Option<f64>,
    },
    /// Downloaded and checked in the background ("update by itself"): it
    /// installs once the computer is not in use, or now on request.
    #[serde(rename_all = "camelCase")]
    Ready {
        version: String,
        notes: Option<String>,
        date_ms: Option<f64>,
    },
    /// Bytes so far; `total` when the server said.
    Downloading {
        version: String,
        done: f64,
        total: Option<f64>,
    },
    /// The installer starts; the app closes and opens again when it is done.
    Installing {
        version: String,
    },
    Failed {
        reason: UpdateFailure,
    },
}

/// Why asking or installing did not work, in words the UI can say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum UpdateFailure {
    /// No connection to GitHub.
    Offline,
    /// GitHub has no release yet.
    NoRelease,
    /// The download did not match the signature: not installed.
    Signature,
    /// A write to the keyboard did not finish in time.
    KeyboardBusy,
    Other,
}

// ------------------------------------------------------------ game session

/// How the keys look while a game session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GameLights {
    /// The keyboard's own saved light: nothing is sent.
    #[default]
    Own,
    /// The chosen keys in one colour, the others dark or dim (RAM only).
    Keys,
    /// A profile's per-key design (RAM only).
    Profile,
}

/// The keys not chosen, with `GameLights::Keys`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GameRest {
    Dark,
    #[default]
    Dim,
}

/// A program the user said is a game, or is not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameMark {
    /// Full path of the program.
    pub path: String,
    pub name: String,
    pub game: bool,
}

/// Game session settings (persisted in `games.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct GamesSettings {
    /// Game sessions are noticed (the program in front is watched).
    pub enabled: bool,
    /// The launchers' own lists (Steam, Epic, GOG, Xbox, Riot, Game Bar)
    /// tell which programs are games.
    pub launchers: bool,
    /// A card on the keyboard's screen when a session starts (live screen).
    pub start_card: bool,
    /// A summary card when a session of 5 minutes or more ends.
    pub summary_card: bool,
    pub lights: GameLights,
    /// The profile whose per-key design shows with `GameLights::Profile`.
    pub lights_profile: Option<String>,
    /// Keys lit with `GameLights::Keys` (ids of the model's geometry).
    pub keys: Vec<String>,
    /// Their colour (UI colour, sRGB).
    pub color: [u8; 3],
    pub rest: GameRest,
    /// Light brightness in the keyboard's own steps, 1–5.
    pub brightness: u8,
    /// While a game runs, nothing is sent to the keyboard: no lights, no
    /// cards. For a game whose anti-cheat software might mind.
    pub quiet: bool,
    /// … but the meetings' and reminders' lights and cards still come.
    pub quiet_reminders: bool,
    /// Games (their keys) that always get the quiet mode.
    pub quiet_games: Vec<String>,
    /// The user's own word on programs.
    pub marks: Vec<GameMark>,
    /// League of Legends: lights from the game's own data.
    pub lol: LolSettings,
}

/// League of Legends lights (Riot's Live Client Data API, read on this
/// computer while a match runs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct LolSettings {
    /// League of Legends' match lights instead of the plain game lights.
    pub enabled: bool,
    /// Health, and mana (or energy) on the F row.
    pub bars: bool,
    /// Dead: the keys go dark red, the light comes back as respawn nears.
    pub death: bool,
    /// Short animations: level up, kills, an ace, dragons, Baron, Herald,
    /// the end.
    pub events: bool,
    /// Where health shows.
    pub layout: LolLayout,
    /// How the whole keyboard's light recedes as health drops.
    pub motion: PomodoroMotion,
    /// One colour for health; `None`: green, amber, red by how much is left.
    pub health_color: Option<[u8; 3]>,
    /// Mana (energy, fury) on the F row.
    pub mana: bool,
    /// Mana's colour (energy stays yellow, fury red).
    pub mana_color: [u8; 3],
    /// Each event's animation and colour.
    pub lights: LolEventLights,
    /// Dragons in their kind's colour, not `lights.dragon.color`.
    pub dragon_kind_color: bool,
    /// Dragons, Herald and Baron only when the player's team takes them.
    pub team_only: bool,
}

impl Default for LolSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            bars: true,
            death: true,
            events: true,
            layout: LolLayout::default(),
            motion: PomodoroMotion::default(),
            health_color: None,
            mana: true,
            mana_color: [61, 139, 255],
            lights: LolEventLights::default(),
            dragon_kind_color: true,
            team_only: true,
        }
    }
}

/// Where League of Legends shows health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LolLayout {
    /// The whole keyboard: the light recedes as health drops.
    #[default]
    Keyboard,
    /// A bar on the number row (1 … 0).
    Row,
}

/// A short animation for a match event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LolAnim {
    /// A band crossing from left to right.
    #[default]
    Wave,
    /// The whole keyboard flashes twice.
    Flash,
    /// The whole keyboard breathes twice.
    Pulse,
    /// Keys twinkle here and there.
    Sparkle,
    /// A ring spreading from the middle.
    Ring,
    /// The keyboard lights up, then goes dark from the top row down.
    Drain,
}

/// The events that get a light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LolEvent {
    Win,
    Lose,
    LevelUp,
    Kill,
    Ace,
    Dragon,
    Baron,
    Herald,
}

/// One event's light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct LolEventLight {
    pub on: bool,
    pub anim: LolAnim,
    /// UI colour, sRGB.
    pub color: [u8; 3],
}

impl Default for LolEventLight {
    fn default() -> Self {
        Self { on: true, anim: LolAnim::Wave, color: LOL_GOLD }
    }
}

const LOL_GOLD: [u8; 3] = [240, 199, 94];

/// Every event's light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct LolEventLights {
    pub win: LolEventLight,
    pub lose: LolEventLight,
    pub level_up: LolEventLight,
    pub kill: LolEventLight,
    pub ace: LolEventLight,
    pub dragon: LolEventLight,
    pub baron: LolEventLight,
    pub herald: LolEventLight,
}

impl Default for LolEventLights {
    fn default() -> Self {
        let l = |anim, color| LolEventLight { on: true, anim, color };
        Self {
            win: l(LolAnim::Sparkle, [52, 199, 89]),
            lose: l(LolAnim::Drain, [255, 59, 48]),
            level_up: l(LolAnim::Wave, LOL_GOLD),
            kill: l(LolAnim::Flash, LOL_GOLD),
            ace: l(LolAnim::Ring, LOL_GOLD),
            dragon: l(LolAnim::Flash, [255, 90, 30]),
            baron: l(LolAnim::Pulse, [155, 93, 229]),
            herald: l(LolAnim::Wave, [196, 120, 255]),
        }
    }
}

impl LolEventLights {
    pub fn get(&self, e: LolEvent) -> LolEventLight {
        match e {
            LolEvent::Win => self.win,
            LolEvent::Lose => self.lose,
            LolEvent::LevelUp => self.level_up,
            LolEvent::Kill => self.kill,
            LolEvent::Ace => self.ace,
            LolEvent::Dragon => self.dragon,
            LolEvent::Baron => self.baron,
            LolEvent::Herald => self.herald,
        }
    }
}

/// What the League of Legends preview (and "Try on the keyboard") shows.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum LolScene {
    /// Health and mana, 0–1.
    Bars { health: f32, mana: f32 },
    /// Dead, until respawn.
    Death,
    /// An event's animation, over full health.
    Event { event: LolEvent },
}

/// How a League of Legends match went (summary card, history).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LolResult {
    /// `None` when the end was not seen (left early, the client closed).
    pub win: Option<bool>,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    pub cs: u32,
    pub champion: String,
    /// The game's own name of the mode (CLASSIC, ARAM, …).
    pub mode: String,
}

/// Keys lit by default: movement and the keys next to it.
pub const GAME_KEYS_DEFAULT: [&str; 7] = ["w", "a", "s", "d", "lshift", "lctrl", "space"];

impl Default for GamesSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            launchers: true,
            start_card: true,
            summary_card: true,
            lights: GameLights::default(),
            lights_profile: None,
            keys: GAME_KEYS_DEFAULT.iter().map(|k| (*k).to_string()).collect(),
            color: [255, 138, 61],
            rest: GameRest::default(),
            brightness: 4,
            quiet: false,
            quiet_reminders: true,
            quiet_games: Vec::new(),
            marks: Vec::new(),
            lol: LolSettings::default(),
        }
    }
}

impl GamesSettings {
    pub const MAX_MARKS: usize = 500;
    pub const NAME_MAX: usize = 80;

    pub fn sanitized(mut self) -> Self {
        let key_ok =
            |k: &String| !k.is_empty() && k.len() <= 32 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        self.keys.retain(key_ok);
        self.keys.dedup();
        self.keys.truncate(128);
        self.brightness = self.brightness.clamp(1, 5);
        self.marks.retain(|m| !m.path.trim().is_empty() && m.path.len() <= 1024);
        for m in &mut self.marks {
            m.name = m.name.trim().chars().take(Self::NAME_MAX).collect();
        }
        let mut seen = std::collections::HashSet::new();
        self.marks.retain(|m| seen.insert(m.path.to_lowercase()));
        if self.marks.len() > Self::MAX_MARKS {
            let drop = self.marks.len() - Self::MAX_MARKS;
            self.marks.drain(..drop);
        }
        self.quiet_games.retain(|k| !k.trim().is_empty() && k.len() <= 1024);
        let mut seen = std::collections::HashSet::new();
        self.quiet_games.retain(|k| seen.insert(k.to_lowercase()));
        self.quiet_games.truncate(Self::MAX_MARKS);
        self
    }
}

/// Where a game was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GameSource {
    Steam,
    Epic,
    Gog,
    Xbox,
    Riot,
    /// Xbox Game Bar remembers it as a game.
    Windows,
    /// The user said so.
    Mine,
}

/// A game on the Game session page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameEntry {
    /// Stable id: the program's path, or the install folder.
    pub key: String,
    pub name: String,
    pub source: GameSource,
    /// Played in total (from the history), ms.
    pub total_ms: f64,
    /// Last played (end of the last session).
    pub last_ms: Option<f64>,
    /// Played at least once: a game from a launcher's list that was never
    /// played is listed only on request.
    pub played: bool,
    /// Its program file, for its icon (`program_icon`), when known.
    pub exe: Option<String>,
}

/// A program that looked like a game: "Is this a game?".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameSuggestion {
    pub path: String,
    pub name: String,
}

/// The session that runs now.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameSessionInfo {
    pub key: String,
    pub name: String,
    pub source: GameSource,
    pub started_ms: f64,
    pub gpu_avg: Option<f32>,
    pub gpu_peak: Option<f32>,
    pub cpu_avg: Option<f32>,
    pub cpu_peak: Option<f32>,
    pub temp_peak: Option<f32>,
    /// Quiet mode is on for this session (for every game, or this one).
    pub quiet: bool,
    /// Its start card on the keyboard's screen.
    pub card: GameCardStatus,
}

/// Where the running session's start card is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GameCardStatus {
    /// No start card: turned off, or the live screen is off.
    Off,
    /// Quiet mode: nothing goes to the keyboard.
    Quiet,
    /// Waiting for the screen (its upload allowance, or another upload).
    Waiting,
    /// On the keyboard's screen.
    Shown,
    /// Something else is on the screen now (a meeting's card, the song).
    Other,
}

/// A finished session (history, `game-history.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameRecord {
    pub key: String,
    pub name: String,
    pub source: GameSource,
    pub start_ms: f64,
    pub end_ms: f64,
    #[serde(default)]
    pub gpu_avg: Option<f32>,
    #[serde(default)]
    pub gpu_peak: Option<f32>,
    #[serde(default)]
    pub cpu_avg: Option<f32>,
    #[serde(default)]
    pub cpu_peak: Option<f32>,
    #[serde(default)]
    pub temp_peak: Option<f32>,
    /// A League of Legends match: how it went.
    #[serde(default)]
    pub lol: Option<LolResult>,
}

/// Game session, settings and history (`games` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GamesState {
    /// Counts up with every state made: the UI drops one older than what it
    /// has (an event and a reply can cross).
    pub rev: u32,
    pub settings: GamesSettings,
    pub session: Option<GameSessionInfo>,
    pub games: Vec<GameEntry>,
    pub suggestions: Vec<GameSuggestion>,
    /// Played today and in the last 7 days (the session that runs included).
    pub today_ms: f64,
    pub week_ms: f64,
    /// The latest sessions, newest first.
    pub recent: Vec<GameRecord>,
    /// The graphics card reports its temperature on this computer.
    pub temperature: bool,
    /// Programs lately in front that aren't games: one can be added as a
    /// game by hand. Newest first.
    pub programs: Vec<GameSuggestion>,
}

/// What a game card is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum GameCardKind {
    Start,
    Summary,
}

/// A card the UI draws for the live screen (`game-card` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameCard {
    pub kind: GameCardKind,
    pub name: String,
    /// Steam's id: its cover can be asked for (`games_cover`).
    pub steam_app: Option<u32>,
    /// The game's program file: its icon when there's no cover
    /// (`program_icon`).
    pub exe: Option<String>,
    pub started_ms: f64,
    pub ended_ms: Option<f64>,
    pub gpu_avg: Option<f32>,
    pub gpu_peak: Option<f32>,
    pub cpu_avg: Option<f32>,
    pub cpu_peak: Option<f32>,
    pub temp_peak: Option<f32>,
    /// Played today, this session included.
    pub today_ms: f64,
    /// A League of Legends match: how it went (summary card).
    pub lol: Option<LolResult>,
}

// ------------------------------------------------------- per-app profiles

/// The rule that stands for every game (game session's list).
pub const AUTO_RULE_GAMES: &str = "games";

/// A program (or every game) and the profile it gets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutoRule {
    /// Full path of the program, or `games` for every game.
    pub path: String,
    pub name: String,
    /// Profile id.
    pub profile: String,
    /// Also the key mapping and macros, and an effect's lighting: written
    /// to the keyboard (flash), after the program has been in front 10 s,
    /// a limited number of times a day. Off: only a per-key design's
    /// lighting changes, in RAM.
    pub writes: bool,
}

/// Per-app profiles settings (persisted in `auto-profiles.json`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AutoProfileSettings {
    pub enabled: bool,
    /// Turned off for now from the tray menu; the rules stay.
    pub paused: bool,
    pub rules: Vec<AutoRule>,
    /// What goes back on the keyboard after a written switch, once its game
    /// ends or a program without a written rule comes in front.
    pub back: AutoBack,
    /// The profile for `AutoBack::Profile`.
    pub base: Option<String>,
}

/// After a written switch (per-app profiles).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum AutoBack {
    /// What the keyboard had before the first written switch.
    #[default]
    Previous,
    /// The profile in `base`.
    Profile,
    /// The written parts stay.
    Stay,
}

impl AutoProfileSettings {
    pub const MAX_RULES: usize = 100;

    pub fn sanitized(mut self) -> Self {
        self.rules.retain(|r| !r.path.trim().is_empty() && r.path.len() <= 1024 && !r.profile.is_empty());
        for r in &mut self.rules {
            r.name = r.name.trim().chars().take(GamesSettings::NAME_MAX).collect();
        }
        let mut seen = std::collections::HashSet::new();
        self.rules.retain(|r| seen.insert(r.path.to_lowercase()));
        self.rules.truncate(Self::MAX_RULES);
        self
    }
}

/// The program in front, offered on the page to add a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutoProgram {
    pub path: String,
    pub name: String,
}

/// What the per-app profiles do now (`auto-profile` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutoProfileState {
    /// Counts up with every state made: the UI drops one older than what it
    /// has (an event and a reply can cross).
    pub rev: u32,
    pub settings: AutoProfileSettings,
    /// The rule in force: its path (or `games`) and profile.
    pub active: Option<AutoRule>,
    /// The program that brought it in front (its name).
    pub because: Option<String>,
    /// Profile switches written to the keyboard today, and the limit.
    pub writes_today: u32,
    pub writes_max: u32,
    /// The last error of a written switch, in the backend's words.
    pub last_error: Option<String>,
    /// Programs used lately (newest first), to add a rule for.
    pub recent: Vec<AutoProgram>,
}

// ------------------------------------------------------------------ agents

/// How an agent's signal moves: the look the user picks for each signal
/// (agents may override it per call with one of the five simple ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentEffect {
    #[default]
    Solid,
    Pulse,
    Blink,
    Wave,
    /// Quick flashes, like a car's headlights (never more than three a
    /// second).
    Flash,
    /// One wave, then steady.
    WaveThenSolid,
    /// Three quick flashes, then a slow pulse.
    FlashThenPulse,
    /// Two blinks, then steady.
    BlinkTwiceThenSolid,
    /// No light.
    Off,
}

/// How one signal looks on the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentLook {
    /// UI colour (sRGB).
    pub color: [u8; 3],
    pub effect: AgentEffect,
    /// 1 (slow) to 5 (fast).
    pub speed: u8,
    /// How long it stays, in seconds (1 s to 2 h).
    pub duration_s: u32,
    /// It stays until the user answers (the agent takes it away), at most
    /// `duration_s`.
    pub until_answered: bool,
}

impl Default for AgentLook {
    fn default() -> Self {
        Self::new([255, 255, 255], AgentEffect::Solid, 3, 60)
    }
}

impl AgentLook {
    pub const fn new(color: [u8; 3], effect: AgentEffect, speed: u8, duration_s: u32) -> Self {
        Self { color, effect, speed, duration_s, until_answered: false }
    }

    pub fn sanitized(self) -> Self {
        Self { speed: self.speed.clamp(1, 5), duration_s: self.duration_s.clamp(1, 2 * 60 * 60), ..self }
    }
}

/// The ready-made signals' looks (owner's defaults, 2026-11-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSignalLooks {
    /// Green, a wave then steady, 2 minutes.
    pub done: AgentLook,
    /// Orange, three quick flashes then a slow pulse, until the user
    /// answers (at most 30 minutes).
    pub attention: AgentLook,
    /// Red, two blinks then steady, 10 minutes.
    pub error: AgentLook,
    /// No light: only takes the agent's earlier signals away.
    pub busy: AgentLook,
    /// A blue wave, 10 seconds.
    pub info: AgentLook,
}

impl Default for AgentSignalLooks {
    fn default() -> Self {
        Self {
            done: AgentLook::new([0, 255, 0], AgentEffect::WaveThenSolid, 3, 2 * 60),
            attention: AgentLook {
                until_answered: true,
                ..AgentLook::new([255, 128, 0], AgentEffect::FlashThenPulse, 2, 30 * 60)
            },
            error: AgentLook::new([255, 0, 0], AgentEffect::BlinkTwiceThenSolid, 3, 10 * 60),
            busy: AgentLook::new([0, 160, 255], AgentEffect::Off, 3, 60),
            info: AgentLook::new([0, 0, 255], AgentEffect::Wave, 3, 10),
        }
    }
}

impl AgentSignalLooks {
    pub fn sanitized(self) -> Self {
        Self {
            done: self.done.sanitized(),
            attention: self.attention.sanitized(),
            error: self.error.sanitized(),
            busy: self.busy.sanitized(),
            info: self.info.sanitized(),
        }
    }
}

/// A: the agent ran into its usage limit (told apart from an error). The
/// hook events that say so come with the command line tool's hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentUsageLimit {
    pub on: bool,
    pub look: AgentLook,
}

impl Default for AgentUsageLimit {
    fn default() -> Self {
        Self { on: true, look: AgentLook::new([128, 0, 255], AgentEffect::Pulse, 1, 30 * 60) }
    }
}

/// B: with two or more agent sessions, the waiting one's number lights on
/// the number row (1–9). Comes with the hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSessions {
    pub on: bool,
    pub color: [u8; 3],
}

impl Default for AgentSessions {
    fn default() -> Self {
        Self { on: true, color: [255, 255, 255] }
    }
}

/// C: what finished or started waiting while nobody was at the computer
/// flashes once when the user is back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentAway {
    pub on: bool,
    /// No input this long is "away", in seconds (1–60 minutes).
    pub after_s: u32,
}

impl Default for AgentAway {
    fn default() -> Self {
        Self { on: true, after_s: 5 * 60 }
    }
}

/// D: an agent's question nobody answered flashes again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentUnanswered {
    pub on: bool,
    /// Every this many seconds (1–30 minutes)…
    pub every_s: u32,
    /// …for at most this long after it came (1 minute to 2 hours).
    pub up_to_s: u32,
}

impl Default for AgentUnanswered {
    fn default() -> Self {
        Self { on: true, every_s: 5 * 60, up_to_s: 30 * 60 }
    }
}

/// The row a progress bar uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentProgressKeys {
    #[default]
    FRow,
    Numbers,
}

/// E: a progress bar an agent asks for (`progress`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentProgress {
    pub keys: AgentProgressKeys,
    pub color: [u8; 3],
}

impl Default for AgentProgress {
    fn default() -> Self {
        Self { keys: AgentProgressKeys::FRow, color: [0, 200, 255] }
    }
}

/// F: a card on the screen when a long task finishes ("Done · 14 min").
/// Needs the screen permission; the card itself comes in a later step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentLongTask {
    pub on: bool,
    /// A task this long or longer counts, in seconds (1 minute to 4 hours).
    pub after_s: u32,
    /// The card stays this long, in seconds (10 s to 30 minutes).
    pub show_s: u32,
}

impl Default for AgentLongTask {
    fn default() -> Self {
        Self { on: true, after_s: 10 * 60, show_s: 5 * 60 }
    }
}

/// What the agents' lights do at night.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentNightMode {
    /// Faint.
    #[default]
    Dim,
    /// None at all.
    Off,
}

/// G: night quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentNight {
    pub on: bool,
    /// From and to, in minutes after midnight (local time); `from` after
    /// `to` runs over midnight.
    pub from_min: u16,
    pub to_min: u16,
    pub mode: AgentNightMode,
}

impl Default for AgentNight {
    fn default() -> Self {
        Self { on: false, from_min: 23 * 60, to_min: 8 * 60, mode: AgentNightMode::Dim }
    }
}

/// H: a helper agent (Claude Code's subagent) finished: a short wave on
/// the F row. Comes with the hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentHelper {
    pub on: bool,
    pub color: [u8; 3],
}

impl Default for AgentHelper {
    fn default() -> Self {
        Self { on: false, color: [0, 0, 255] }
    }
}

/// The agents' settings (persisted in `agents.json`). Access, the
/// permissions, "agents may change these settings", "hide while playing"
/// and the screen's hourly limit are the user's alone; agents may change
/// the looks and the behaviour (`settings_set`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSettings {
    /// Agent access: off until the user turns it on.
    pub enabled: bool,
    /// Agents may put cards on the screen (a later step).
    pub screen: bool,
    /// Agents may use the focus timer (a later step).
    pub focus: bool,
    /// Agents may save a profile's lighting (a later step).
    pub profiles: bool,
    /// Agents may change the looks and the behaviour below.
    pub agents_may_edit: bool,
    /// The agents' lights wait while a game wants the keyboard left alone.
    pub hide_in_game: bool,
    /// Screen cards all agents together may show in an hour (1–20).
    pub screen_per_hour: u8,
    pub signals: AgentSignalLooks,
    pub usage_limit: AgentUsageLimit,
    pub sessions: AgentSessions,
    pub away: AgentAway,
    pub unanswered: AgentUnanswered,
    pub progress: AgentProgress,
    pub long_task: AgentLongTask,
    pub night: AgentNight,
    pub helper: AgentHelper,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            screen: false,
            focus: false,
            profiles: false,
            agents_may_edit: true,
            hide_in_game: true,
            screen_per_hour: 4,
            signals: AgentSignalLooks::default(),
            usage_limit: AgentUsageLimit::default(),
            sessions: AgentSessions::default(),
            away: AgentAway::default(),
            unanswered: AgentUnanswered::default(),
            progress: AgentProgress::default(),
            long_task: AgentLongTask::default(),
            night: AgentNight::default(),
            helper: AgentHelper::default(),
        }
    }
}

impl AgentSettings {
    pub fn sanitized(self) -> Self {
        const MIN: u32 = 60;
        let n = self.night;
        Self {
            screen_per_hour: self.screen_per_hour.clamp(1, 20),
            signals: self.signals.sanitized(),
            usage_limit: AgentUsageLimit { look: self.usage_limit.look.sanitized(), ..self.usage_limit },
            away: AgentAway { after_s: self.away.after_s.clamp(MIN, 60 * MIN), ..self.away },
            unanswered: AgentUnanswered {
                every_s: self.unanswered.every_s.clamp(MIN, 30 * MIN),
                up_to_s: self.unanswered.up_to_s.clamp(MIN, 120 * MIN),
                ..self.unanswered
            },
            long_task: AgentLongTask {
                after_s: self.long_task.after_s.clamp(MIN, 240 * MIN),
                show_s: self.long_task.show_s.clamp(10, 30 * MIN),
                ..self.long_task
            },
            night: AgentNight { from_min: n.from_min.min(24 * 60 - 1), to_min: n.to_min.min(24 * 60 - 1), ..n },
            ..self
        }
    }
}

/// An agent's light, as the page lists it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSignalInfo {
    /// Who sent it (`claude-code:3f9a`, `cli`).
    pub source: String,
    pub id: String,
    /// `done`, `attention`, `error`, `busy`, `info`, `light`, `progress`
    /// or `try` (the page's "Try on the keyboard").
    pub kind: String,
    pub color: [u8; 3],
    /// The keys, as the agent named them (`all`, `f1-f12`).
    pub keys: String,
    /// It shows on at least one key now (a light of higher rank may cover
    /// it).
    pub shown: bool,
    pub expires_in_ms: f64,
    /// A progress bar's percent and label.
    pub percent: Option<f64>,
    pub label: Option<String>,
}

/// A setting an agent changed, as text (`#00ff00`, `2m`, `true`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSettingChange {
    pub key: String,
    pub before: String,
    pub after: String,
}

/// One agent request that changed (or tried to change) something. Never
/// what the agent's user typed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentLogEntry {
    /// Counts up; `agents_undo` names an entry by it.
    pub seq: u32,
    pub at_ms: f64,
    pub source: String,
    pub command: String,
    /// `None` when it went through, else the error code (`rate_limited`).
    pub error: Option<String>,
    /// The light it named, if any.
    pub id: Option<String>,
    /// Settings it changed (`settings_set`): "Undo" puts `before` back.
    pub changes: Vec<AgentSettingChange>,
    pub undone: bool,
    /// Keys "Undo" left as they were: they had changed again since.
    pub kept: Vec<String>,
}

/// Whether the agents' lights show as they are now, and why not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentLights {
    /// They show.
    Shown,
    /// No keyboard is connected.
    NoKeyboard,
    /// The keyboard is not on the USB cable (lights need it).
    NeedsUsb,
    /// A game wants the keyboard left alone and "hide while playing" is on.
    Game,
    /// Night quiet, faint.
    NightDim,
    /// Night quiet, none.
    NightOff,
}

/// The pipe the command line tool talks to the app over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentServer {
    pub running: bool,
    /// Why it is not running (another program holds its name), in the
    /// backend's words.
    pub error: Option<String>,
    pub connections: u32,
}

/// A coding agent's session, known from its hooks (B: its number lights
/// on the number row while it waits, with two or more sessions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSessionInfo {
    /// 1 to 9, in the order the sessions first reached Keymotion; `None`
    /// when all nine were taken.
    pub number: Option<u8>,
    /// Its source, e.g. `claude-code:3f9a0c1d`.
    pub source: String,
    /// `claude-code` or `codex`.
    pub agent: Option<String>,
    /// The project folder's name.
    pub label: Option<String>,
    /// It waits for the user now.
    pub waiting: bool,
}

/// The agents page's state (`agents` event payload).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentsState {
    pub settings: AgentSettings,
    /// Every agent's lights now, highest rank first.
    pub signals: Vec<AgentSignalInfo>,
    /// The coding agents' sessions, by number.
    pub sessions: Vec<AgentSessionInfo>,
    /// The last agent requests, newest first.
    pub log: Vec<AgentLogEntry>,
    pub server: AgentServer,
    pub lights: AgentLights,
}

// ------------------------------------------------- agents: coding apps

/// A coding app Keymotion connects to: its hooks tell the keyboard when
/// the agent finished, waits or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentApp {
    ClaudeCode,
    Codex,
}

/// How a coding app stands with Keymotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentAppState {
    /// Not on this computer (for this Windows user).
    NotFound,
    /// There, without Keymotion's hooks.
    NotConnected,
    Connected,
    /// Codex: connected, but the user has not trusted Keymotion's hooks in
    /// Codex yet (`/hooks`, or Settings > Hooks in the app).
    NeedsApproval,
    /// Some of Keymotion's hooks are missing or name another keymotion-cli:
    /// connecting again brings them up to date.
    Outdated,
    /// Its settings file does not read (`error` says why): left alone.
    Unreadable,
}

/// A coding app, as the AI agents page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentAppStatus {
    pub app: AgentApp,
    /// "Claude Code", "Codex".
    pub name: String,
    /// Its settings folder is there.
    pub found: bool,
    pub state: AgentAppState,
    /// Keymotion's hooks in its settings, of how many.
    pub hooks: u32,
    pub hooks_wanted: u32,
    /// Codex: of Keymotion's hooks, how many the user trusted.
    pub trusted: Option<u32>,
    /// Claude Code: the oldest copy found here (`2.1.140`).
    pub version: Option<String>,
    /// Claude Code is older than `min_version`: Keymotion cannot connect it
    /// until it is updated (`claude update`).
    pub too_old: bool,
    /// Claude Code: the oldest version the hooks work with (`2.1.139`).
    pub min_version: Option<String>,
    /// Its settings folder (`C:\Users\…\.claude`).
    pub home: String,
    pub hooks_file: String,
    pub skill_file: String,
    /// The keymotion-cli its hooks run.
    pub runs: Option<String>,
    /// Why its settings could not be read, in the backend's words.
    pub error: Option<String>,
}

/// Which keymotion-cli this copy of Keymotion comes with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentCliKind {
    /// The Microsoft Store copy's alias, on PATH by itself.
    Store,
    /// The GitHub copy's, in its installation's `bin` folder.
    Installed,
    /// A development build's, beside it.
    Dev,
}

/// The keymotion-cli the hooks run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentCli {
    pub path: String,
    pub kind: AgentCliKind,
    /// The file is there (a development build may not have built it).
    pub present: bool,
}

/// The GitHub copy's `bin` folder and the user's PATH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentPathInfo {
    pub dir: String,
    /// It is on the user's PATH: terminals started from now on find
    /// keymotion-cli by its name.
    pub on_path: bool,
    /// The PATH could not be read (it is not text, access was refused):
    /// nothing will be written to it. In the backend's words.
    pub error: Option<String>,
}

/// The coding apps and the command line tool (`agents_connections`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentConnections {
    /// Claude Code, then Codex.
    pub apps: Vec<AgentAppStatus>,
    pub cli: Option<AgentCli>,
    /// The GitHub copy only (the Store copy's alias is on PATH already).
    pub path: Option<AgentPathInfo>,
}

/// What a file is, for a short line on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentFileKind {
    /// The app's settings file, with Keymotion's hooks (`hooks` of them).
    Hooks,
    /// The keymotion skill: "keymotion-cli is here; run its guide first".
    Skill,
    /// Codex: a rule that lets the agent's own keymotion-cli calls out of
    /// its sandbox.
    Rule,
    /// Keymotion's own notes beside the skill (what it made, for taking it
    /// away); "Details" only.
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentFileAction {
    Create,
    Update,
    Remove,
}

/// One file connecting or disconnecting changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentFileChange {
    pub kind: AgentFileKind,
    pub action: AgentFileAction,
    pub path: String,
    /// For people, in English ("add 16 hooks that run keymotion-cli").
    pub summary: String,
    /// `hooks`: how many of Keymotion's hooks the file holds afterwards
    /// (connecting) or loses (disconnecting).
    pub hooks: Option<u32>,
    /// Where a copy of the file as it was is kept first (another program's
    /// file).
    pub backup: Option<String>,
}

/// Why a coding app could not be connected or disconnected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentProblemKind {
    /// Its settings file is not valid JSON (or not what it should be):
    /// left alone; the user fixes it.
    Invalid,
    /// Reading or writing failed.
    Io,
    /// Claude Code is too old for the hooks: update it first.
    Unsupported,
    /// It kept writing its settings while Keymotion tried: try again.
    Changed,
    /// keymotion-cli is not where it should be (a development build that
    /// did not build it).
    NoCli,
    /// Writing failed part way and not all of it could be put back: some
    /// of Keymotion's entries stayed; disconnecting removes them.
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentProblem {
    pub kind: AgentProblemKind,
    /// In the backend's words (English).
    pub message: String,
}

/// What connecting (or disconnecting) changes for one coding app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentAppPlan {
    pub app: AgentApp,
    pub name: String,
    /// Its settings folder.
    pub home: String,
    /// Empty: nothing to change (connected already, or nothing of
    /// Keymotion's to take away).
    pub changes: Vec<AgentFileChange>,
    /// The line every hook runs, as people read it.
    pub command_line: Option<String>,
    /// Codex: the user trusts the new hooks once in Codex afterwards.
    pub needs_approval: bool,
    /// Notes in English (a development build's path).
    pub warnings: Vec<String>,
    /// It cannot be done: why. Nothing is written for this app.
    pub problem: Option<AgentProblem>,
}

/// What turning agent access on would add (`agents_connect_plan`), shown
/// before the user approves it once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentConnectPlan {
    /// Hand it back to `agents_connect`: what is written is what was shown.
    pub token: String,
    /// The coding apps found (and asked for), Claude Code first.
    pub apps: Vec<AgentAppPlan>,
    /// The coding apps not on this computer.
    pub not_found: Vec<AgentApp>,
    /// The GitHub copy: this folder goes on the user's PATH (absent when it
    /// is there already, and for the Store copy).
    pub path: Option<AgentPathInfo>,
    pub cli: Option<AgentCli>,
    /// Nothing to write: approving only turns access on.
    pub empty: bool,
}

/// What came of `agents_connect`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentConnectOutcome {
    /// Written as shown, and agent access is on. `false`: a file changed
    /// since the plan was shown (a coding app wrote its settings); nothing
    /// was written, access is as it was, and `plan` is what would change
    /// now, to show again.
    pub applied: bool,
    pub plan: Option<AgentConnectPlan>,
    /// The coding apps connected now.
    pub connected: Vec<AgentApp>,
    /// The coding apps that could not be, and why (the others went ahead).
    pub failed: Vec<AgentAppPlan>,
    /// The `bin` folder went on the user's PATH.
    pub path_added: bool,
    /// Why it did not, in the backend's words.
    pub path_error: Option<String>,
    /// Agent access could not be saved as on (the files above were
    /// written all the same), in the backend's words.
    pub save_error: Option<String>,
    pub connections: AgentConnections,
    /// `settings.enabled`: on when at least one coding app connected, or
    /// none had anything to connect; off when every one failed.
    pub state: AgentsState,
}

/// What came of `agents_disconnect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentDisconnectOutcome {
    /// What was taken away (its `problem` set when it could not be).
    pub plan: AgentAppPlan,
    pub connections: AgentConnections,
}

// ------------------------------------------------------ agents: the screen

/// What an agent's screen card says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentCardKind {
    /// The agent's own: `title`, `text` and/or `image`, `color`.
    Text,
    /// F: a long task finished ("Done · 14 min"): `minutes` and the
    /// project folder's name (`label`); the words are the page's, in the
    /// user's language.
    LongTask,
}

/// A picture for a card: a PNG, JPEG, GIF or WebP of at most 40 KiB, sides
/// 8 to 2048 pixels (checked). Drawn as a still: an animated one shows its
/// first frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentCardImage {
    /// `image/png`, `image/jpeg`, `image/gif`, `image/webp`.
    pub mime: String,
    /// The file, base64.
    pub data: String,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
}

/// An agent's card for the keyboard's screen (`agent-card` event): the page
/// draws it like the other live cards and hands it back with
/// `agents_card_drawn` (header `x-card: <id>`, the picture as the body,
/// like `live_submit`), or says why it could not with `agents_card_failed`.
/// It must be drawn within 15 seconds. The page first says it listens:
/// `agents_card_drawer_ready` (no argument, or `{ ready: false }` when it
/// stops); until then agents get no card (a reload or a closed window
/// resets it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentCard {
    pub id: String,
    pub kind: AgentCardKind,
    /// Who asked (`claude-code`, `cli`): for the page's log, not the card.
    pub agent: String,
    /// At most 24 characters, one line.
    pub title: Option<String>,
    /// At most 60 characters on at most 2 lines (`\n` between them).
    pub text: Option<String>,
    pub image: Option<AgentCardImage>,
    /// An accent colour (sRGB) the agent chose.
    pub color: Option<[u8; 3]>,
    /// `longTask`: how long the task took, in whole minutes.
    pub minutes: Option<u32>,
    /// `longTask`: the project folder's name.
    pub label: Option<String>,
    /// How long the card stays before the user's own picture comes back.
    pub show_s: u32,
}
