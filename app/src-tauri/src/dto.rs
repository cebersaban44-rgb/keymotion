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

/// What one connection mode of a model offers, for "switch to USB to
/// unlock" guidance in the UI.
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Battery {
    pub percent: u8,
    /// Unix epoch milliseconds of the reading.
    #[ts(type = "number")]
    pub read_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KeyboardInfo {
    /// Stable identity: model id + connection.
    pub key: String,
    pub model_id: String,
    pub model_name: String,
    pub vendor: String,
    pub layout: String,
    pub key_count: u16,
    pub has_knob: bool,
    pub has_side_light: bool,
    pub screen: Option<ScreenInfo>,
    pub connection: Connection,
    pub product: Option<String>,
    /// Features usable right now in this connection mode.
    pub features: Vec<Feature>,
    /// What every connection mode of this model offers.
    pub modes: Vec<ModeInfo>,
    pub has_command_channel: bool,
    pub has_display_channel: bool,
    pub ble_name: Option<String>,
    pub ble_address: Option<String>,
    pub battery: Option<Battery>,
    pub geometry: Option<GeometryInfo>,
}

impl KeyboardInfo {
    pub fn from_detected(k: &DetectedKeyboard, battery: Option<Battery>) -> Self {
        let m: &Model = k.model;
        Self {
            key: k.key(),
            model_id: m.id.clone(),
            model_name: m.name.clone(),
            vendor: m.vendor.clone(),
            layout: m.layout.clone(),
            key_count: m.key_count,
            has_knob: m.has_knob,
            has_side_light: m.has_side_light,
            screen: m.screen.as_ref().map(|s| ScreenInfo { width: s.width, height: s.height, verified: s.verified }),
            connection: k.connection.into(),
            product: k.product.clone(),
            features: k.capabilities().into_iter().map(Feature::from).collect(),
            modes: m
                .modes
                .iter()
                .map(|mode| ModeInfo {
                    connection: mode.kind.into(),
                    features: mode.capabilities.iter().copied().map(Feature::from).collect(),
                    verified: mode.verified,
                })
                .collect(),
            has_command_channel: k.command.is_some(),
            has_display_channel: k.display.is_some(),
            ble_name: k.ble.as_ref().map(|b| b.name.clone()),
            ble_address: k.ble.as_ref().map(|b| b.address_text.clone()),
            battery,
            geometry: m.geometry.as_ref().map(GeometryInfo::from_model),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectInfo, GeometryInfo, KeymapCatalog};

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

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UiEnvironment {
    pub app_version: String,
    /// Windows 11 Mica backdrop is active behind the webview.
    pub mica: bool,
    pub os_build: u32,
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
}

impl From<&StoredLayer> for KeymapLayerState {
    fn from(l: &StoredLayer) -> Self {
        Self {
            assignments: l.actions().into_iter().map(|(led, a)| KeyAssignment { led, action: a.into() }).collect(),
            written_ms: l.written_ms.map(|ms| ms as f64),
        }
    }
}

/// What the app last wrote to a model's keymap. The keyboard cannot report
/// its keymap, so this is the app's own record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct KeymapState {
    pub base: KeymapLayerState,
    #[serde(rename = "fn")]
    pub fn_layer: KeymapLayerState,
}

impl From<&StoredKeymap> for KeymapState {
    fn from(k: &StoredKeymap) -> Self {
        Self { base: (&k.base).into(), fn_layer: (&k.fn_layer).into() }
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

/// The panel and upload limits (kb-protocol `screen`).
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
            page_ms: 145,
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
    /// Memory limit: `(steps + slots) × 8` bytes may not exceed this.
    pub budget: u32,
    pub max_slots: u32,
    pub min_delay_ms: u16,
    pub max_name: u32,
}

impl From<&StoredMacros> for MacrosState {
    fn from(m: &StoredMacros) -> Self {
        use kb_protocol::sonix::macros::{BUDGET, MAX_MACROS, MIN_DELAY_MS};
        Self {
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
/// wear budget, docs/research/ekran-ve-veri-kaynaklari.md §3.4).
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
    pub uploading: Option<LiveItem>,
    pub last: Option<LiveShown>,
    pub last_error: Option<String>,
    /// Source switch: the song playing.
    pub now_playing: bool,
    pub track: Option<TrackInfo>,
    /// A live card is on the GIF page and this library item was there
    /// before it.
    pub restore: Option<LiveRestore>,
    /// The Pomodoro holds the screen while it runs: "Çalan şarkı" waits.
    pub held: bool,
    /// What "Çalan şarkı" does when the music stops.
    pub song_stop: SongStop,
    /// The user's own picture: what the screen goes back to when no source
    /// has anything to show (the last library item uploaded by hand).
    pub own: Option<LiveRestore>,
    /// The screen goes back to the user's own picture at this time.
    pub going_back: Option<LiveBack>,
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

/// Pomodoro phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PomodoroPhase {
    #[default]
    Focus,
    ShortBreak,
    LongBreak,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct PomodoroSettings {
    /// Minutes of one focus phase (1–120).
    pub focus_min: u16,
    /// Minutes of a short break (1–60).
    pub short_min: u16,
    /// Minutes of the long break that ends a series (1–60).
    pub long_min: u16,
    /// Focus phases in one series (1–8).
    pub rounds: u8,
    /// Progress on the key lights (RAM only, no flash write).
    pub lights: bool,
    /// A card on the screen when a phase starts (through the live mode).
    pub screen: bool,
    pub motion: PomodoroMotion,
    /// Phase colours (UI colours, sRGB).
    pub focus_color: [u8; 3],
    pub short_color: [u8; 3],
    pub long_color: [u8; 3],
    /// Light brightness in the keyboard's own steps, 1–5.
    pub brightness: u8,
    /// The light that is left breathes in the last minute of a phase.
    pub pulse: bool,
    pub transition: PomodoroTransition,
    /// The Windows notification sound when a phase ends by itself.
    pub sound: bool,
    /// What the screen shows when a series ends or is stopped.
    pub end_screen: PomodoroEndScreen,
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
            focus_min: 25,
            short_min: 5,
            long_min: 15,
            rounds: 4,
            lights: true,
            screen: true,
            motion: PomodoroMotion::default(),
            focus_color: [255, 80, 20],
            short_color: [30, 220, 100],
            long_color: [40, 140, 255],
            brightness: 4,
            pulse: true,
            transition: PomodoroTransition::default(),
            sound: false,
            end_screen: PomodoroEndScreen::default(),
        }
    }
}

impl PomodoroSettings {
    pub fn sanitized(self) -> Self {
        Self {
            focus_min: self.focus_min.clamp(1, 120),
            short_min: self.short_min.clamp(1, 60),
            long_min: self.long_min.clamp(1, 60),
            rounds: self.rounds.clamp(1, 8),
            brightness: self.brightness.clamp(1, 5),
            ..self
        }
    }

    pub fn color(&self, phase: PomodoroPhase) -> [u8; 3] {
        match phase {
            PomodoroPhase::Focus => self.focus_color,
            PomodoroPhase::ShortBreak => self.short_color,
            PomodoroPhase::LongBreak => self.long_color,
        }
    }

    pub fn minutes(&self, phase: PomodoroPhase) -> u16 {
        match phase {
            PomodoroPhase::Focus => self.focus_min,
            PomodoroPhase::ShortBreak => self.short_min,
            PomodoroPhase::LongBreak => self.long_min,
        }
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
    /// Focus round the current phase belongs to (1-based).
    pub round: u8,
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
    pub round: u8,
    pub rounds: u8,
    /// Phase length in minutes.
    pub minutes: u16,
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
