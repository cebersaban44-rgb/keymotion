//! Keyboard model definitions and device matching.
//!
//! Every supported keyboard is described by a TOML file in `models/`, embedded
//! at compile time. Supporting a new model that speaks an existing protocol
//! family means adding one file here; no code changes.
//!
//! This crate performs no I/O: callers hand it plain descriptions of the HID
//! interfaces they found and get back which model and connection mode they
//! belong to.

use serde::Deserialize;
use std::sync::OnceLock;

/// Embedded model files. Keep sorted by id.
const MODEL_SOURCES: &[(&str, &str)] = &[("ajazz-ak980-pro", include_str!("../models/ajazz-ak980-pro.toml"))];

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("model file `{file}` is invalid: {source}")]
    Parse {
        file: &'static str,
        #[source]
        source: toml::de::Error,
    },
    #[error("model file `{file}` declares id `{found}`, expected `{file}`")]
    IdMismatch { file: &'static str, found: String },
}

/// How the keyboard is connected to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnectionKind {
    Usb,
    Dongle,
    Bluetooth,
}

impl ConnectionKind {
    pub fn label_tr(self) -> &'static str {
        match self {
            Self::Usb => "USB kablo",
            Self::Dongle => "2.4G alıcı",
            Self::Bluetooth => "Bluetooth",
        }
    }
}

/// A feature the keyboard offers in a given connection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Lighting,
    PerKeyRgb,
    Keymap,
    Macros,
    Options,
    Screen,
    Clock,
    Battery,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenSpec {
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeSpec {
    pub kind: ConnectionKind,
    pub vid: u16,
    pub pid: u16,
    /// Case-insensitive substring the USB product string must contain.
    #[serde(default)]
    pub product_contains: Option<String>,
    #[serde(default)]
    pub command_usage_page: Option<u16>,
    #[serde(default)]
    pub display_usage_page: Option<u16>,
    #[serde(default)]
    pub ble_name_prefix: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    pub verified: bool,
}

fn one() -> f32 {
    1.0
}

/// One key: where it sits and which LED/keymap index drives it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeySpec {
    pub id: String,
    pub label: String,
    /// LED index; equals the keymap index on SONiX boards.
    pub led: u8,
    /// Default HID usage (keyboard page 0x07; 0xAF is the vendor Fn key).
    pub hid: u8,
    pub x: f32,
    pub y: f32,
    #[serde(default = "one")]
    pub w: f32,
    #[serde(default = "one")]
    pub h: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knob {
    pub x: f32,
    pub y: f32,
    pub d: f32,
}

/// Physical geometry in key units.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    pub source: String,
    #[serde(default)]
    pub screen: Option<Rect>,
    #[serde(default)]
    pub knob: Option<Knob>,
    pub keys: Vec<KeySpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub protocol: String,
    pub layout: String,
    pub key_count: u16,
    pub has_knob: bool,
    pub has_side_light: bool,
    #[serde(default)]
    pub screen: Option<ScreenSpec>,
    pub modes: Vec<ModeSpec>,
    #[serde(default)]
    pub geometry: Option<Geometry>,
    /// Status lights driven by the firmware itself (Num Lock, Fn, …). They
    /// ignore effects, per-key tables and live frames.
    #[serde(default)]
    pub indicators: Vec<Indicator>,
}

/// A firmware status light on one LED.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Indicator {
    pub led: u8,
    /// What it shows, e.g. "Num Lock".
    pub name: String,
    /// Colour when lit, as the LED is driven (not affected by brightness).
    pub color: [u8; 3],
    pub verified: bool,
}

impl Model {
    pub fn mode(&self, kind: ConnectionKind) -> Option<&ModeSpec> {
        self.modes.iter().find(|m| m.kind == kind)
    }
}

/// Bus an HID interface was found on, as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Bluetooth,
    Other,
}

/// What the transport layer knows about one HID interface.
#[derive(Debug, Clone)]
pub struct InterfaceProbe<'a> {
    pub vid: u16,
    pub pid: u16,
    pub usage_page: u16,
    pub product: Option<&'a str>,
    pub bus: Bus,
}

/// Result of matching one HID interface against the model catalogue.
#[derive(Debug, Clone, Copy)]
pub struct InterfaceMatch<'m> {
    pub model: &'m Model,
    pub mode: &'m ModeSpec,
    pub role: InterfaceRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceRole {
    /// Vendor command channel (lighting, keymap, options...).
    Command,
    /// Vendor bulk channel for screen frames.
    Display,
    /// Standard HID collection (keyboard, consumer, mouse...): input only.
    Standard,
}

/// All models compiled into this build, parsed once.
pub fn catalogue() -> Result<&'static [Model], &'static ModelError> {
    static CATALOGUE: OnceLock<Result<Vec<Model>, ModelError>> = OnceLock::new();
    CATALOGUE
        .get_or_init(|| {
            MODEL_SOURCES
                .iter()
                .map(|(file, src)| {
                    let model: Model = toml::from_str(src).map_err(|source| ModelError::Parse { file, source })?;
                    if model.id != *file {
                        return Err(ModelError::IdMismatch { file, found: model.id });
                    }
                    Ok(model)
                })
                .collect()
        })
        .as_deref()
}

/// Match one HID interface against `models`.
///
/// USB and dongle share VID/PIDs with unrelated SONiX boards, so a USB match
/// also requires the product string when the model declares one. Bluetooth
/// and the 2.4G dongle report the same VID/PID and are told apart by bus.
pub fn match_interface<'m>(models: &'m [Model], probe: &InterfaceProbe<'_>) -> Option<InterfaceMatch<'m>> {
    for model in models {
        for mode in &model.modes {
            if mode.vid != probe.vid || mode.pid != probe.pid {
                continue;
            }
            let bus_ok = match mode.kind {
                ConnectionKind::Bluetooth => probe.bus == Bus::Bluetooth,
                ConnectionKind::Usb | ConnectionKind::Dongle => probe.bus == Bus::Usb,
            };
            if !bus_ok {
                continue;
            }
            if let Some(needle) = &mode.product_contains {
                let hit = probe.product.is_some_and(|p| p.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()));
                if !hit {
                    continue;
                }
            }
            let role = if Some(probe.usage_page) == mode.command_usage_page {
                InterfaceRole::Command
            } else if Some(probe.usage_page) == mode.display_usage_page {
                InterfaceRole::Display
            } else {
                InterfaceRole::Standard
            };
            return Some(InterfaceMatch { model, mode, role });
        }
    }
    None
}

/// Find the model whose Bluetooth name starts with a declared prefix.
pub fn match_ble_name<'m>(models: &'m [Model], name: &str) -> Option<(&'m Model, &'m ModeSpec)> {
    models.iter().find_map(|model| {
        model.modes.iter().find_map(|mode| {
            let prefix = mode.ble_name_prefix.as_deref()?;
            name.starts_with(prefix).then_some((model, mode))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn models() -> &'static [Model] {
        catalogue().expect("embedded models must parse")
    }

    #[test]
    fn catalogue_parses() {
        let m = &models()[0];
        assert_eq!(m.id, "ajazz-ak980-pro");
        assert_eq!(m.screen.as_ref().unwrap().width, 240);
        assert!(m.mode(ConnectionKind::Usb).is_some());
    }

    #[test]
    fn wired_requires_product_string() {
        let ak980 = InterfaceProbe {
            vid: 0x0C45,
            pid: 0x8009,
            usage_page: 0xFF13,
            product: Some("SONiX AK980 PRO"),
            bus: Bus::Usb,
        };
        let m = match_interface(models(), &ak980).unwrap();
        assert_eq!(m.mode.kind, ConnectionKind::Usb);
        assert_eq!(m.role, InterfaceRole::Command);

        let ak820 = InterfaceProbe { product: Some("SONiX AK820 PRO"), ..ak980.clone() };
        assert!(match_interface(models(), &ak820).is_none(), "AK820 shares the PID and must not match");
    }

    #[test]
    fn dongle_and_bluetooth_split_by_bus() {
        let base = InterfaceProbe { vid: 0x0C45, pid: 0xFEFE, usage_page: 0xFF60, product: None, bus: Bus::Usb };
        let dongle = match_interface(models(), &base).unwrap();
        assert_eq!(dongle.mode.kind, ConnectionKind::Dongle);
        assert_eq!(dongle.role, InterfaceRole::Command);

        let ble = InterfaceProbe { usage_page: 0x0001, bus: Bus::Bluetooth, ..base };
        let ble = match_interface(models(), &ble).unwrap();
        assert_eq!(ble.mode.kind, ConnectionKind::Bluetooth);
        assert_eq!(ble.role, InterfaceRole::Standard);
    }

    #[test]
    fn geometry_is_consistent() {
        for m in models() {
            let Some(g) = &m.geometry else { continue };
            assert_eq!(g.keys.len(), m.key_count as usize, "{}: key count", m.id);
            let mut leds: Vec<u8> = g.keys.iter().map(|k| k.led).collect();
            leds.sort_unstable();
            leds.dedup();
            assert_eq!(leds.len(), g.keys.len(), "{}: duplicate LED index", m.id);
            let mut ids: Vec<&str> = g.keys.iter().map(|k| k.id.as_str()).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), g.keys.len(), "{}: duplicate key id", m.id);
            let mut hids: Vec<u8> = g.keys.iter().map(|k| k.hid).collect();
            hids.sort_unstable();
            hids.dedup();
            assert_eq!(hids.len(), g.keys.len(), "{}: duplicate HID usage", m.id);
            for k in &g.keys {
                assert!(k.x >= 0.0 && k.x + k.w <= g.width + 0.01, "{}: {} out of bounds", m.id, k.id);
                assert!(k.y >= 0.0 && k.y + k.h <= g.height + 0.01, "{}: {} out of bounds", m.id, k.id);
            }
            // No two keys overlap.
            for (i, a) in g.keys.iter().enumerate() {
                for b in &g.keys[i + 1..] {
                    let overlap = a.x < b.x + b.w - 0.01
                        && b.x < a.x + a.w - 0.01
                        && a.y < b.y + b.h - 0.01
                        && b.y < a.y + a.h - 0.01;
                    assert!(!overlap, "{}: {} overlaps {}", m.id, a.id, b.id);
                }
            }
        }
    }

    #[test]
    fn ble_name_prefix() {
        assert!(match_ble_name(models(), "AK980 PRO-1").is_some());
        assert!(match_ble_name(models(), "AJAZZ BT-1").is_none());
    }
}
