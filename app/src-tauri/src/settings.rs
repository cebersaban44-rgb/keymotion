//! Settings the backend owns and persists in the app config directory
//! (`%APPDATA%\dev.klavye.studio`). UI-only preferences (theme, language,
//! the current lighting design) stay in the webview's local storage.

use kb_core::color::LedCalibration;
use kb_core::keymap::LayerMap;
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

const CALIBRATION_FILE: &str = "calibration.json";
const CALIBRATION_VERSION: u32 = 1;
const CLOCK_FILE: &str = "clock.json";
const CLOCK_VERSION: u32 = 1;
const KEYMAP_FILE: &str = "keymap.json";
const KEYMAP_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Versioned<T> {
    version: u32,
    #[serde(flatten)]
    value: T,
}

/// Read `file` if it exists, has `version` and parses. Anything else is
/// logged and gives `None`, so a damaged file cannot break the app.
fn load<T: DeserializeOwned>(dir: &Path, file: &str, version: u32) -> Option<T> {
    let path = dir.join(file);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("{file} unreadable ({}): {e}", path.display());
            return None;
        }
    };
    match serde_json::from_str::<Versioned<T>>(&text) {
        Ok(s) if s.version == version => Some(s.value),
        Ok(s) => {
            tracing::warn!(version = s.version, "{file}: unknown version; using defaults");
            None
        }
        Err(e) => {
            tracing::warn!("{file} is damaged; using defaults: {e}");
            None
        }
    }
}

/// Write `value` atomically (temp file, then rename).
fn save<T: Serialize>(dir: &Path, file: &str, version: u32, value: T) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(&Versioned { version, value }).map_err(io::Error::other)?;
    let tmp = dir.join(format!("{file}.tmp"));
    fs::write(&tmp, json)?;
    fs::rename(&tmp, dir.join(file))
}

/// The saved calibration, or the default when none was saved yet or the
/// file cannot be read. Values are always sanitized.
pub fn load_calibration(dir: &Path) -> LedCalibration {
    load::<LedCalibration>(dir, CALIBRATION_FILE, CALIBRATION_VERSION).unwrap_or_default().sanitized()
}

pub fn save_calibration(dir: &Path, calibration: LedCalibration) -> io::Result<()> {
    save(dir, CALIBRATION_FILE, CALIBRATION_VERSION, calibration.sanitized())
}

/// Keyboard clock sync preferences and history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockSettings {
    /// Set the keyboard clock when it connects over USB, at most once a day.
    pub auto_sync: bool,
    /// Last successful sync, Unix time in milliseconds.
    pub last_sync_ms: Option<u64>,
}

impl Default for ClockSettings {
    fn default() -> Self {
        Self { auto_sync: true, last_sync_ms: None }
    }
}

pub fn load_clock(dir: &Path) -> ClockSettings {
    load(dir, CLOCK_FILE, CLOCK_VERSION).unwrap_or_default()
}

pub fn save_clock(dir: &Path, clock: ClockSettings) -> io::Result<()> {
    save(dir, CLOCK_FILE, CLOCK_VERSION, clock)
}

/// What this app last wrote to each model's keymap. The keyboard cannot
/// report its keymap, so this file is the only record of it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymaps {
    /// Model id → layers.
    #[serde(default)]
    pub models: BTreeMap<String, StoredKeymap>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredKeymap {
    #[serde(default)]
    pub base: StoredLayer,
    #[serde(default, rename = "fn")]
    pub fn_layer: StoredLayer,
}

impl StoredKeymap {
    pub fn layer_mut(&mut self, layer: Layer) -> &mut StoredLayer {
        match layer {
            Layer::Base => &mut self.base,
            Layer::Fn => &mut self.fn_layer,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredLayer {
    /// LED index (decimal) → table entry as 8 hex digits (`02000400` = A).
    /// Raw bytes keep the file independent of how the UI names actions.
    /// Keys are strings: the versioned wrapper cannot read numeric map keys.
    #[serde(default)]
    pub entries: BTreeMap<String, String>,
    /// When this app last wrote the layer (Unix ms). `None`: never, so the
    /// keyboard is taken to be at its factory default.
    pub written_ms: Option<u64>,
}

impl StoredLayer {
    pub fn new(map: &LayerMap, written_ms: u64) -> Self {
        let entries = map
            .iter()
            .filter_map(|(&led, a)| {
                let e = a.encode().ok()?;
                Some((led.to_string(), format!("{:02x}{:02x}{:02x}{:02x}", e[0], e[1], e[2], e[3])))
            })
            .collect();
        Self { entries, written_ms: Some(written_ms) }
    }

    /// The stored actions; entries that do not decode are skipped.
    pub fn actions(&self) -> LayerMap {
        self.entries
            .iter()
            .filter_map(|(led, hex)| {
                let entry = led.parse::<u8>().ok().zip(decode_entry(hex));
                if entry.is_none() {
                    tracing::warn!(led, hex, "keymap.json: unknown entry skipped");
                }
                entry
            })
            .collect()
    }
}

fn decode_entry(hex: &str) -> Option<KeyAction> {
    if hex.len() != 8 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    KeyAction::decode(v.to_be_bytes())
}

pub fn load_keymaps(dir: &Path) -> Keymaps {
    load(dir, KEYMAP_FILE, KEYMAP_VERSION).unwrap_or_default()
}

pub fn save_keymaps(dir: &Path, keymaps: &Keymaps) -> io::Result<()> {
    save(dir, KEYMAP_FILE, KEYMAP_VERSION, keymaps)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("klavye-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_file_gives_default() {
        assert_eq!(load_calibration(&temp_dir("missing")), LedCalibration::DEFAULT);
        assert_eq!(load_clock(&temp_dir("missing")), ClockSettings::default());
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("round-trip");
        let cal = LedCalibration { gamma: [2.0, 2.4, 1.8], gain: [0.85, 1.0, 0.9] };
        save_calibration(&dir, cal).unwrap();
        assert_eq!(load_calibration(&dir), cal);
        let clock = ClockSettings { auto_sync: false, last_sync_ms: Some(1_794_000_000_000) };
        save_clock(&dir, clock).unwrap();
        assert_eq!(load_clock(&dir), clock);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn keymap_round_trip() {
        use kb_protocol::sonix::keymap::MouseButton;
        let dir = temp_dir("keymap");
        assert_eq!(load_keymaps(&dir), Keymaps::default());
        let map = LayerMap::from([
            (33, KeyAction::key(0x04)),
            (34, KeyAction::Consumer(0xE9)),
            (123, KeyAction::Mouse(MouseButton::Back)),
        ]);
        let mut km = Keymaps::default();
        km.models.entry("ajazz-ak980-pro".into()).or_default().fn_layer = StoredLayer::new(&map, 1_794_000_000_000);
        save_keymaps(&dir, &km).unwrap();
        let loaded = load_keymaps(&dir);
        assert_eq!(loaded, km);
        let stored = &loaded.models["ajazz-ak980-pro"];
        assert_eq!(stored.fn_layer.actions(), map);
        assert_eq!(stored.fn_layer.entries["33"], "02000400");
        assert_eq!(stored.base.written_ms, None, "base layer never written");
        let text = fs::read_to_string(dir.join(KEYMAP_FILE)).unwrap();
        assert!(text.contains(r#""fn""#), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_keymap_entries_are_skipped() {
        let layer = StoredLayer {
            entries: BTreeMap::from([
                ("33".into(), "02000400".into()),
                ("34".into(), "06020105".into()),
                ("35".into(), "zz".into()),
                ("x".into(), "02000400".into()),
            ]),
            written_ms: Some(1),
        };
        assert_eq!(layer.actions(), LayerMap::from([(33, KeyAction::key(0x04))]));
    }

    #[test]
    fn calibration_file_format_is_unchanged() {
        // Files written before the shared helpers must still load.
        let dir = temp_dir("format");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CALIBRATION_FILE), r#"{"version":1,"gamma":[1.0,1.0,1.0],"gain":[1.0,1.0,1.0]}"#).unwrap();
        assert_eq!(load_calibration(&dir), LedCalibration::RAW);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn damaged_or_out_of_range_files_are_safe() {
        let dir = temp_dir("damaged");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CALIBRATION_FILE), "{ not json").unwrap();
        assert_eq!(load_calibration(&dir), LedCalibration::DEFAULT);

        fs::write(dir.join(CALIBRATION_FILE), r#"{"version":1,"gamma":[9,2.2,2.2],"gain":[1,1,-3]}"#).unwrap();
        let cal = load_calibration(&dir);
        assert_eq!(cal.gamma[0], kb_core::color::GAMMA_MAX);
        assert_eq!(cal.gain[2], 0.0);

        fs::write(dir.join(CLOCK_FILE), r#"{"version":7,"autoSync":false}"#).unwrap();
        assert_eq!(load_clock(&dir), ClockSettings::default(), "unknown version");
        let _ = fs::remove_dir_all(&dir);
    }
}
