//! Types shared with the UI. `cargo test` exports them to
//! `app/src/bindings/*.ts` through ts-rs, so the UI never re-declares them.

use kb_core::DetectedKeyboard;
use kb_devices::{Capability, ConnectionKind, Model};
use serde::Serialize;
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
    use super::{EffectInfo, GeometryInfo};

    /// Writes each model's geometry and the effect catalog for the UI's
    /// browser-only mock backend, so the mock never drifts from Rust.
    #[test]
    fn export_mock_geometry() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings");
        std::fs::create_dir_all(&dir).unwrap();
        let catalog = serde_json::to_string_pretty(&EffectInfo::catalog()).unwrap();
        std::fs::write(dir.join("lighting-catalog.json"), catalog + "\n").unwrap();
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
    pub factory_default: bool,
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
