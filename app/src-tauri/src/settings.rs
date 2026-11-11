//! Settings the backend owns and persists in the app config directory
//! (`%APPDATA%\dev.keymotion`). UI-only preferences (theme, language,
//! the current lighting design) stay in the webview's local storage.
//!
//! A file that cannot be used (damaged, unreadable, or written by a newer
//! version of the app) gives the defaults, but it is never lost: it is first
//! moved aside as `<name>.bak-<YYYYMMDD-HHMMSS>` and the reset is recorded,
//! so the UI can say once which records went back to their defaults. This
//! matters most for `keymap.json` and `macros.json`: the keyboard cannot
//! report either, so the file is the only record of what it holds.

use kb_core::color::LedCalibration;
use kb_core::keymap::LayerMap;
use kb_protocol::sonix::keymap::{KeyAction, Layer};
use kb_protocol::sonix::macros::{MacroStep, StepKind};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const CALIBRATION_FILE: &str = "calibration.json";
const CALIBRATION_VERSION: u32 = 1;
const CLOCK_FILE: &str = "clock.json";
const CLOCK_VERSION: u32 = 1;
const KEYMAP_FILE: &str = "keymap.json";
const KEYMAP_VERSION: u32 = 1;
const MACROS_FILE: &str = "macros.json";
const MACROS_VERSION: u32 = 1;
const BATTERY_FILE: &str = "battery.json";
const BATTERY_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Versioned<T> {
    version: u32,
    #[serde(flatten)]
    value: T,
}

/// A settings file that could not be used when it was loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reset {
    pub dir: PathBuf,
    /// File name, e.g. `keymap.json`.
    pub file: String,
    /// Name of the copy kept beside it; `None` while it could not be moved
    /// aside (then the file itself is not overwritten, see [`save`]).
    pub backup: Option<String>,
    /// The UI has shown the notice about it.
    pub shown: bool,
}

/// Resets of this run, oldest first.
static RESETS: Mutex<Vec<Reset>> = Mutex::new(Vec::new());

fn resets_lock() -> std::sync::MutexGuard<'static, Vec<Reset>> {
    RESETS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Resets the UI has not shown yet.
pub fn unshown_resets() -> Vec<Reset> {
    resets_lock().iter().filter(|r| !r.shown).cloned().collect()
}

/// The UI showed its notice: it is not shown again in this run. (The file
/// is gone from its place, so the next run does not report it either.)
pub fn mark_resets_shown() {
    for r in resets_lock().iter_mut() {
        r.shown = true;
    }
}

/// Name of the copy of `file` kept at `time`: `keymap.json.bak-20261111-143005`.
pub(crate) fn backup_name(file: &str, time: chrono::NaiveDateTime) -> String {
    format!("{file}.bak-{}", time.format("%Y%m%d-%H%M%S"))
}

/// Move `dir/file` aside under a backup name not taken yet (copy when it
/// cannot be moved). Returns the backup's name.
fn keep_aside(dir: &Path, file: &str) -> Option<String> {
    let base = backup_name(file, chrono::Local::now().naive_local());
    let name =
        (1..100).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|n| !dir.join(n).exists())?;
    let (from, to) = (dir.join(file), dir.join(&name));
    match fs::rename(&from, &to).or_else(|_| fs::copy(&from, &to).map(|_| ())) {
        Ok(()) => Some(name),
        Err(e) => {
            tracing::warn!("{file} could not be kept aside: {e}");
            None
        }
    }
}

/// `file` cannot be used: keep it aside and record the reset.
fn reset(dir: &Path, file: &str) {
    let backup = keep_aside(dir, file);
    match &backup {
        Some(b) => tracing::warn!("{file} kept as {b}; using defaults"),
        None => tracing::warn!("{file} left in place and not overwritten; using defaults"),
    }
    resets_lock().push(Reset { dir: dir.to_path_buf(), file: file.to_string(), backup, shown: false });
}

/// Read `file` if it exists, has `version` and parses. Anything else gives
/// `None` (the defaults), so a damaged file cannot break the app; a file
/// that exists but cannot be used is kept aside first (see the module docs).
pub(crate) fn load<T: DeserializeOwned>(dir: &Path, file: &str, version: u32) -> Option<T> {
    let path = dir.join(file);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!("{file} unreadable ({}): {e}", path.display());
            reset(dir, file);
            return None;
        }
    };
    match serde_json::from_str::<Versioned<T>>(&text) {
        Ok(s) if s.version == version => Some(s.value),
        Ok(s) => {
            tracing::warn!(version = s.version, "{file}: unknown version");
            reset(dir, file);
            None
        }
        Err(e) => {
            tracing::warn!("{file} is damaged: {e}");
            reset(dir, file);
            None
        }
    }
}

/// A reset file that is still in its place must be kept aside before it is
/// overwritten; if that still fails, it is not overwritten at all.
fn guard_overwrite(dir: &Path, file: &str) -> io::Result<()> {
    let mut resets = resets_lock();
    let Some(r) = resets.iter_mut().find(|r| r.backup.is_none() && r.dir == dir && r.file == file) else {
        return Ok(());
    };
    if !dir.join(file).exists() {
        return Ok(());
    }
    r.backup = keep_aside(dir, file);
    match &r.backup {
        Some(_) => Ok(()),
        None => Err(io::Error::other(format!("{file} could not be kept aside, so it was not overwritten"))),
    }
}

/// Write `value` atomically (temp file, then rename).
pub(crate) fn save<T: Serialize>(dir: &Path, file: &str, version: u32, value: T) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    guard_overwrite(dir, file)?;
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

/// The last battery reading of each model (by model id). Only Bluetooth
/// reports the battery; over the USB cable the keyboard cannot, so the app
/// shows the last reading with its age.
pub fn load_batteries(dir: &Path) -> BTreeMap<String, crate::dto::Battery> {
    load(dir, BATTERY_FILE, BATTERY_VERSION).unwrap_or_default()
}

pub fn save_batteries(dir: &Path, batteries: &BTreeMap<String, crate::dto::Battery>) -> io::Result<()> {
    save(dir, BATTERY_FILE, BATTERY_VERSION, batteries)
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

/// What this app last wrote to each model's macro memory. The keyboard
/// cannot report its macros (or their names), so this is the only record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroRecord {
    /// Model id → macros.
    #[serde(default)]
    pub models: BTreeMap<String, StoredMacros>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMacros {
    /// Slot `i` is macro `i` on the keyboard; `null` is an unused slot.
    #[serde(default)]
    pub slots: Vec<Option<StoredMacro>>,
    /// When this app last wrote the macros (Unix ms); `None`: never.
    pub written_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredMacro {
    pub name: String,
    #[serde(default)]
    pub steps: Vec<StoredStep>,
}

/// One step as `["down", 4, 20]`: action, key usage or mouse mask, pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "(String, u8, u16)", into = "(String, u8, u16)")]
pub struct StoredStep(pub MacroStep);

const STEP_NAMES: [(StepKind, &str); 4] = [
    (StepKind::KeyDown, "down"),
    (StepKind::KeyUp, "up"),
    (StepKind::MouseDown, "mouse-down"),
    (StepKind::MouseUp, "mouse-up"),
];

impl From<(String, u8, u16)> for StoredStep {
    fn from((kind, code, delay_ms): (String, u8, u16)) -> Self {
        // An unknown action reads as a key release, which is harmless.
        let kind = STEP_NAMES.iter().find(|(_, n)| *n == kind).map_or(StepKind::KeyUp, |(k, _)| *k);
        Self(MacroStep { kind, code, delay_ms })
    }
}

impl From<StoredStep> for (String, u8, u16) {
    fn from(StoredStep(s): StoredStep) -> Self {
        let name = STEP_NAMES.iter().find(|(k, _)| *k == s.kind).map_or("up", |(_, n)| n);
        (name.to_string(), s.code, s.delay_ms)
    }
}

impl StoredMacros {
    /// Steps per slot, as the keyboard stores them (unused slots are empty).
    pub fn image_slots(&self) -> Vec<Vec<MacroStep>> {
        self.slots
            .iter()
            .map(|m| m.as_ref().map(|m| m.steps.iter().map(|s| s.0).collect()).unwrap_or_default())
            .collect()
    }
}

pub fn load_macros(dir: &Path) -> MacroRecord {
    load(dir, MACROS_FILE, MACROS_VERSION).unwrap_or_default()
}

pub fn save_macros(dir: &Path, macros: &MacroRecord) -> io::Result<()> {
    save(dir, MACROS_FILE, MACROS_VERSION, macros)
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
    fn macro_record_round_trip() {
        let dir = temp_dir("macros");
        assert_eq!(load_macros(&dir), MacroRecord::default());
        let ok = StoredMacro {
            name: "ok".into(),
            steps: vec![
                StoredStep(MacroStep::new(StepKind::KeyDown, 0x12, 20)),
                StoredStep(MacroStep::new(StepKind::MouseUp, 0x02, 300)),
            ],
        };
        let mut rec = MacroRecord::default();
        rec.models.insert(
            "ajazz-ak980-pro".into(),
            StoredMacros { slots: vec![Some(ok), None], written_ms: Some(1_794_000_000_000) },
        );
        save_macros(&dir, &rec).unwrap();
        assert_eq!(load_macros(&dir), rec);
        let text = fs::read_to_string(dir.join(MACROS_FILE)).unwrap();
        assert!(text.contains(r#""down""#) && text.contains(r#""mouse-up""#), "{text}");
        let slots = rec.models["ajazz-ak980-pro"].image_slots();
        assert_eq!(slots.len(), 2);
        assert!(slots[1].is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_keymap_entries_are_skipped() {
        let layer = StoredLayer {
            entries: BTreeMap::from([
                ("33".into(), "02000400".into()),
                ("34".into(), "07040506".into()),
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

    fn resets_in(dir: &Path) -> Vec<Reset> {
        resets_lock().iter().filter(|r| r.dir == dir).cloned().collect()
    }

    fn files_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> =
            fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn backup_names_carry_the_time() {
        let t = chrono::NaiveDate::from_ymd_opt(2026, 11, 11).unwrap().and_hms_opt(9, 5, 7).unwrap();
        assert_eq!(backup_name("keymap.json", t), "keymap.json.bak-20261111-090507");
    }

    #[test]
    fn a_damaged_record_is_kept_before_it_is_overwritten() {
        let dir = temp_dir("reset-damaged");
        fs::create_dir_all(&dir).unwrap();
        let damaged = r#"{"version":1,"models":{"ajazz-ak980-pro":{"base":"#;
        fs::write(dir.join(KEYMAP_FILE), damaged).unwrap();

        assert_eq!(load_keymaps(&dir), Keymaps::default(), "defaults");
        let resets = resets_in(&dir);
        assert_eq!(resets.len(), 1);
        assert_eq!(resets[0].file, KEYMAP_FILE);
        let backup = resets[0].backup.clone().expect("kept aside");
        assert!(backup.starts_with("keymap.json.bak-") && backup.len() == "keymap.json.bak-20261111-090507".len());
        assert_eq!(fs::read_to_string(dir.join(&backup)).unwrap(), damaged, "the old file, byte for byte");
        assert!(!dir.join(KEYMAP_FILE).exists(), "moved, not copied");

        // The next save writes a new record; the backup stays.
        let mut km = Keymaps::default();
        km.models.insert("ajazz-ak980-pro".into(), StoredKeymap::default());
        save_keymaps(&dir, &km).unwrap();
        assert_eq!(load_keymaps(&dir), km);
        assert_eq!(fs::read_to_string(dir.join(&backup)).unwrap(), damaged);
        assert_eq!(files_in(&dir), vec![KEYMAP_FILE.to_string(), backup]);
        assert_eq!(resets_in(&dir).len(), 1, "a good file is no reset");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_record_from_a_newer_version_is_kept_too() {
        let dir = temp_dir("reset-newer");
        fs::create_dir_all(&dir).unwrap();
        let newer = r#"{"version":2,"models":{}}"#;
        fs::write(dir.join(MACROS_FILE), newer).unwrap();
        assert_eq!(load_macros(&dir), MacroRecord::default());
        let backup = resets_in(&dir)[0].backup.clone().unwrap();
        assert!(backup.starts_with("macros.json.bak-"));
        assert_eq!(fs::read_to_string(dir.join(backup)).unwrap(), newer);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_reset_in_the_same_second_gets_its_own_name() {
        let dir = temp_dir("reset-twice");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CLOCK_FILE), "first").unwrap();
        let _ = load_clock(&dir);
        fs::write(dir.join(CLOCK_FILE), "second").unwrap();
        let _ = load_clock(&dir);
        let backups: Vec<String> = resets_in(&dir).into_iter().filter_map(|r| r.backup).collect();
        assert_eq!(backups.len(), 2);
        assert_ne!(backups[0], backups[1]);
        let mut kept: Vec<String> = backups.iter().map(|b| fs::read_to_string(dir.join(b)).unwrap()).collect();
        kept.sort();
        assert_eq!(kept, vec!["first".to_string(), "second".to_string()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_notice_is_shown_once() {
        let dir = temp_dir("reset-notice");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(CALIBRATION_FILE), "{").unwrap();
        let _ = load_calibration(&dir);
        assert!(unshown_resets().iter().any(|r| r.dir == dir && r.file == CALIBRATION_FILE));
        mark_resets_shown();
        assert!(!unshown_resets().iter().any(|r| r.dir == dir));
        // The file was moved aside: the next run finds nothing to report.
        assert_eq!(load_calibration(&dir), LedCalibration::DEFAULT);
        assert_eq!(resets_in(&dir).len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reset_file_still_in_place_is_kept_before_the_save() {
        // Moving it aside failed at load time (say, it was locked).
        let dir = temp_dir("reset-later");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MACROS_FILE), "old").unwrap();
        resets_lock().push(Reset { dir: dir.clone(), file: MACROS_FILE.into(), backup: None, shown: false });
        save_macros(&dir, &MacroRecord::default()).unwrap();
        let backup = resets_in(&dir)[0].backup.clone().expect("kept before the save");
        assert_eq!(fs::read_to_string(dir.join(backup)).unwrap(), "old");
        assert_eq!(load_macros(&dir), MacroRecord::default());
        let _ = fs::remove_dir_all(&dir);
    }
}
