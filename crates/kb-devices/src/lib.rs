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
use std::fmt;
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

/// Command family a keyboard speaks on its vendor channels. Boards with the
/// same IDs and name can speak either (another firmware), and sending one
/// family's commands to the other can erase the screen slot, so the family
/// is also told apart by the channels' report layout (kb_core `shape`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, serde::Serialize)]
pub enum Protocol {
    /// SONiX "04 xx" family: 64-byte feature reports on FF13 for commands,
    /// 4096-byte output reports on FF68 for screen pages.
    #[serde(rename = "sonix-ff13")]
    SonixFf13,
    /// AJAZZ "AA" family (AK980 PRO firmware 769 and others): 64-byte
    /// output and input reports on FF68 for commands, 4104-byte output
    /// reports on FF67 for the screen. No feature reports.
    #[serde(rename = "ajazz-aa")]
    AjazzAa,
}

impl Protocol {
    /// The name model files use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SonixFf13 => "sonix-ff13",
            Self::AjazzAa => "ajazz-aa",
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
    /// Command family in this mode, when it is not the model's
    /// ([`Model::protocol_of`] resolves it).
    #[serde(default)]
    pub protocol: Option<Protocol>,
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
    /// Firmware versions (USB `bcdDevice`, e.g. `0x0106`) this project has
    /// tested writes on. Writing to the keyboard's memory on any other
    /// version needs the user's confirmation first.
    #[serde(default)]
    pub verified_firmware: Vec<u16>,
}

impl ModeSpec {
    /// Whether writes were tested on this firmware version.
    pub fn firmware_verified(&self, release: u16) -> bool {
        self.verified_firmware.contains(&release)
    }
}

/// A firmware version (USB `bcdDevice`, binary-coded decimal) as people
/// read it: `0x0106` is "1.06".
pub fn firmware_text(release: u16) -> String {
    format!("{:x}.{:02x}", release >> 8, release & 0xFF)
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
    /// The key's index in the "AA" command family (keymap slot and per-key
    /// table entry), which is not `led` shifted: needed by every model with
    /// a mode that speaks it.
    #[serde(default)]
    pub aa: Option<u8>,
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
    /// Command family of the model's modes, unless a mode names its own.
    pub protocol: Protocol,
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
    /// The same LED's index in the "AA" command family ([`KeySpec::aa`]).
    #[serde(default)]
    pub aa: Option<u8>,
    /// What it shows, e.g. "Num Lock".
    pub name: String,
    /// Colour when lit, as the LED is driven (not affected by brightness).
    pub color: [u8; 3],
    pub verified: bool,
}

impl Model {
    /// The first mode of `kind` in the model file. A model can list more
    /// than one mode of a kind, one per command family behind the same
    /// connection: [`Model::modes_of`] lists them all.
    pub fn mode(&self, kind: ConnectionKind) -> Option<&ModeSpec> {
        self.modes.iter().find(|m| m.kind == kind)
    }

    /// Every mode of `kind`, in model file order.
    pub fn modes_of(&self, kind: ConnectionKind) -> impl Iterator<Item = &ModeSpec> + '_ {
        self.modes.iter().filter(move |m| m.kind == kind)
    }

    /// The command family `mode` (one of this model's) speaks: its own,
    /// else the model's.
    pub fn protocol_of(&self, mode: &ModeSpec) -> Protocol {
        mode.protocol.unwrap_or(self.protocol)
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
    /// The mode's command family ([`Model::protocol_of`]).
    pub protocol: Protocol,
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
/// also requires the product string to contain the mode's
/// `product_contains`: a USB mode without one, or a board with no product
/// string (a bootloader keeps the IDs and drops the name), never matches.
/// The dongle matches the product string when the model declares one.
/// Bluetooth and the 2.4G dongle report the same VID/PID and are told apart
/// by bus.
///
/// A match says only that the IDs and the name fit: whether the vendor
/// channels have the protocol's shape is checked by the caller, which can
/// read their report descriptors (kb_core `classify`).
///
/// This is the first matching mode in catalogue order. When a model lists
/// modes of several command families behind the same IDs and name, only
/// the report layout tells them apart: [`match_interfaces`] lists them all.
pub fn match_interface<'m>(models: &'m [Model], probe: &InterfaceProbe<'_>) -> Option<InterfaceMatch<'m>> {
    for model in models {
        for mode in &model.modes {
            if mode_matches(mode, probe) {
                return Some(interface_match(model, mode, probe));
            }
        }
    }
    None
}

/// Every mode one HID interface could belong to, in catalogue order (the
/// first is [`match_interface`]'s).
pub fn match_interfaces<'m>(models: &'m [Model], probe: &InterfaceProbe<'_>) -> Vec<InterfaceMatch<'m>> {
    models
        .iter()
        .flat_map(|model| model.modes.iter().map(move |mode| (model, mode)))
        .filter(|(_, mode)| mode_matches(mode, probe))
        .map(|(model, mode)| interface_match(model, mode, probe))
        .collect()
}

fn mode_matches(mode: &ModeSpec, probe: &InterfaceProbe<'_>) -> bool {
    if mode.vid != probe.vid || mode.pid != probe.pid {
        return false;
    }
    let bus_ok = match mode.kind {
        ConnectionKind::Bluetooth => probe.bus == Bus::Bluetooth,
        ConnectionKind::Usb | ConnectionKind::Dongle => probe.bus == Bus::Usb,
    };
    if !bus_ok {
        return false;
    }
    match mode.product_contains.as_deref().map(str::trim) {
        Some(needle) if !needle.is_empty() => {
            probe.product.is_some_and(|p| p.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()))
        }
        // A board on the cable must name itself: a bootloader (ISP mode)
        // can keep the keyboard's IDs and report no product string, and
        // nothing may be sent to it. A USB mode without a name to look for
        // never matches.
        _ => mode.kind != ConnectionKind::Usb,
    }
}

fn interface_match<'m>(model: &'m Model, mode: &'m ModeSpec, probe: &InterfaceProbe<'_>) -> InterfaceMatch<'m> {
    let role = if Some(probe.usage_page) == mode.command_usage_page {
        InterfaceRole::Command
    } else if Some(probe.usage_page) == mode.display_usage_page {
        InterfaceRole::Display
    } else {
        InterfaceRole::Standard
    };
    InterfaceMatch { model, mode, role, protocol: model.protocol_of(mode) }
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
    fn wired_name_is_the_measured_ak980_pro() {
        let probe = |product| InterfaceProbe { vid: 0x0C45, pid: 0x8009, usage_page: 0xFF13, product, bus: Bus::Usb };
        // The product string the owner's keyboard reports (maker "SONiX" apart).
        assert!(match_interface(models(), &probe(Some("AK980 PRO"))).is_some());
        assert!(match_interface(models(), &probe(Some("ak980 pro"))).is_some(), "case does not matter");
        // Other AK980s share the IDs but not the name: they are offered
        // "Request support" instead of being driven as this one.
        for other in ["AK980", "SONiX AK980", "AJAZZ AK980 V1", "AK980PRO"] {
            assert!(match_interface(models(), &probe(Some(other))).is_none(), "{other} must not match");
        }
        assert!(match_interface(models(), &probe(None)).is_none(), "no name, no match");
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
    fn firmware_versions() {
        assert_eq!(firmware_text(0x0106), "1.06");
        assert_eq!(firmware_text(0x0138), "1.38");
        assert_eq!(firmware_text(0x1210), "12.10");
        assert_eq!(firmware_text(0x0009), "0.09");
        let usb = models()[0].mode(ConnectionKind::Usb).unwrap();
        assert!(usb.firmware_verified(0x0106), "the owner's keyboard");
        assert!(!usb.firmware_verified(0x0107));
        let dongle = models()[0].mode(ConnectionKind::Dongle).unwrap();
        assert!(!dongle.firmware_verified(0x0106), "no list: nothing is verified");
    }

    #[test]
    fn ble_name_prefix() {
        assert!(match_ble_name(models(), "AK980 PRO-1").is_some());
        assert!(match_ble_name(models(), "AJAZZ BT-1").is_none());
    }

    /// A board with both command families behind the same IDs and name (a
    /// test model: no model file lists an "AA" mode yet).
    const TWO_FAMILIES: &str = r#"
        id = "test-two-families"
        name = "Test"
        vendor = "Test"
        protocol = "sonix-ff13"
        layout = "98"
        key_count = 0
        has_knob = false
        has_side_light = false

        [[modes]]
        kind = "usb"
        vid = 0x0C45
        pid = 0x8009
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF13
        display_usage_page = 0xFF68
        verified = false

        [[modes]]
        kind = "usb"
        protocol = "ajazz-aa"
        vid = 0x0C45
        pid = 0x8009
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF68
        display_usage_page = 0xFF67
        verified = false
    "#;

    #[test]
    fn protocol_names_are_checked() {
        let ak980 = models().iter().find(|m| m.id == "ajazz-ak980-pro").unwrap();
        assert_eq!(ak980.protocol, Protocol::SonixFf13);
        assert_eq!(ak980.protocol_of(ak980.mode(ConnectionKind::Usb).unwrap()), Protocol::SonixFf13);
        assert_eq!(Protocol::SonixFf13.to_string(), "sonix-ff13");
        assert_eq!(Protocol::AjazzAa.to_string(), "ajazz-aa");

        assert!(toml::from_str::<Model>(TWO_FAMILIES).is_ok());
        let unknown_model = TWO_FAMILIES.replacen("protocol = \"sonix-ff13\"", "protocol = \"sonix\"", 1);
        assert!(toml::from_str::<Model>(&unknown_model).is_err(), "unknown model family");
        let unknown_mode = TWO_FAMILIES.replacen("protocol = \"ajazz-aa\"", "protocol = \"aa\"", 1);
        assert!(toml::from_str::<Model>(&unknown_mode).is_err(), "unknown mode family");
    }

    #[test]
    fn one_connection_can_have_a_mode_per_family() {
        let model: Model = toml::from_str(TWO_FAMILIES).unwrap();
        let usb: Vec<&ModeSpec> = model.modes_of(ConnectionKind::Usb).collect();
        assert_eq!(usb.len(), 2);
        assert!(model.modes_of(ConnectionKind::Dongle).next().is_none());
        // `mode` keeps meaning the first one.
        assert!(std::ptr::eq(model.mode(ConnectionKind::Usb).unwrap(), usb[0]));
        assert_eq!(model.protocol_of(usb[0]), Protocol::SonixFf13, "inherited from the model");
        assert_eq!(model.protocol_of(usb[1]), Protocol::AjazzAa, "the mode's own");

        // FF68 is the screen in one family and the command channel in the other.
        let models = std::slice::from_ref(&model);
        let probe = |usage_page| InterfaceProbe {
            vid: 0x0C45,
            pid: 0x8009,
            usage_page,
            product: Some("AK980 PRO"),
            bus: Bus::Usb,
        };
        let ff68 = match_interfaces(models, &probe(0xFF68));
        let seen: Vec<_> = ff68.iter().map(|m| (m.protocol, m.role)).collect();
        assert_eq!(seen, [(Protocol::SonixFf13, InterfaceRole::Display), (Protocol::AjazzAa, InterfaceRole::Command)]);
        let first = match_interface(models, &probe(0xFF68)).unwrap();
        assert_eq!((first.protocol, first.role), (Protocol::SonixFf13, InterfaceRole::Display));
        let ff67 = match_interfaces(models, &probe(0xFF67));
        let seen: Vec<_> = ff67.iter().map(|m| (m.protocol, m.role)).collect();
        assert_eq!(seen, [(Protocol::SonixFf13, InterfaceRole::Standard), (Protocol::AjazzAa, InterfaceRole::Display)]);
        assert!(match_interfaces(models, &InterfaceProbe { product: Some("AK820 PRO"), ..probe(0xFF68) }).is_empty());
    }

    #[test]
    fn a_board_without_a_name_never_matches_a_usb_mode() {
        // In ISP (bootloader) mode a board can keep its PID and report an
        // empty product string. Neither family's USB mode may match it.
        let two: Model = toml::from_str(TWO_FAMILIES).unwrap();
        for models in [models(), std::slice::from_ref(&two)] {
            for usage_page in [0xFF13, 0xFF68, 0xFF67, 0x0001] {
                for product in [None, Some(""), Some("   ")] {
                    let probe = InterfaceProbe { vid: 0x0C45, pid: 0x8009, usage_page, product, bus: Bus::Usb };
                    assert!(match_interfaces(models, &probe).is_empty(), "{usage_page:04X} {product:?}");
                }
            }
        }
        // Every shipped USB mode names the product it is for.
        for m in models() {
            for mode in m.modes_of(ConnectionKind::Usb) {
                let name = mode.product_contains.as_deref().unwrap_or("");
                assert!(!name.trim().is_empty(), "{}: a USB mode needs `product_contains`", m.id);
            }
        }
        // A USB mode without one, or with a blank one, matches nothing.
        let nameless = TWO_FAMILIES.replace("product_contains = \"AK980 PRO\"", "");
        let blank = TWO_FAMILIES.replace("product_contains = \"AK980 PRO\"", "product_contains = \" \"");
        for src in [nameless, blank] {
            let model: Model = toml::from_str(&src).unwrap();
            let probe = InterfaceProbe {
                vid: 0x0C45,
                pid: 0x8009,
                usage_page: 0xFF68,
                product: Some("AK980 PRO"),
                bus: Bus::Usb,
            };
            assert!(match_interfaces(std::slice::from_ref(&model), &probe).is_empty());
        }
    }

    #[test]
    fn aa_indices_are_complete_and_unique() {
        for m in models() {
            let Some(g) = &m.geometry else { continue };
            let speaks_aa = m.modes.iter().any(|mode| m.protocol_of(mode) == Protocol::AjazzAa);
            let given: Vec<u8> = g.keys.iter().filter_map(|k| k.aa).collect();
            if given.is_empty() {
                assert!(!speaks_aa, "{}: an \"AA\" mode needs every key's `aa` index", m.id);
                continue;
            }
            assert_eq!(given.len(), g.keys.len(), "{}: `aa` on some keys only", m.id);
            let mut unique = given.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), given.len(), "{}: duplicate `aa` index", m.id);
            // 128 keymap slots (512 bytes, 4 per key).
            assert!(given.iter().all(|&i| i < 128), "{}: `aa` index out of range", m.id);
            // A status light sits on a key's LED: same key, same index.
            for ind in &m.indicators {
                let key = g.keys.iter().find(|k| k.led == ind.led).expect("indicator on a key");
                assert_eq!(ind.aa, key.aa, "{}: {} indicator", m.id, ind.name);
            }
        }
    }

    #[test]
    fn ak980_pro_aa_indices() {
        let m = models().iter().find(|m| m.id == "ajazz-ak980-pro").unwrap();
        let aa = |id: &str| m.geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap().aa;
        // Spot checks against the vendor's tables (hub key table, WebHID key list).
        assert_eq!(aa("esc"), Some(0));
        assert_eq!(aa("grave"), Some(16));
        assert_eq!(aa("w"), Some(34));
        assert_eq!(aa("numlock"), Some(29));
        assert_eq!(aa("enter"), Some(76));
        assert_eq!(aa("fn"), Some(85));
        assert_eq!(aa("kp-plus"), Some(110));
        // Not the LED index shifted: Esc 1→0, Fn 96→85.
        assert_eq!(m.indicators.iter().map(|i| i.aa).collect::<Vec<_>>(), [Some(29), Some(85)]);
    }
}
