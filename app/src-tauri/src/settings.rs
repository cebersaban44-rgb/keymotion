//! Settings the backend owns and persists in the app config directory
//! (`%APPDATA%\dev.klavye.studio`). UI-only preferences (theme, language,
//! the current lighting design) stay in the webview's local storage.

use kb_core::color::LedCalibration;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

const CALIBRATION_FILE: &str = "calibration.json";
const CALIBRATION_VERSION: u32 = 1;
const CLOCK_FILE: &str = "clock.json";
const CLOCK_VERSION: u32 = 1;

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
