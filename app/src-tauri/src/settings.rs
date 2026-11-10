//! Settings the backend owns and persists in the app config directory
//! (`%APPDATA%\dev.klavye.studio`). UI-only preferences (theme, language,
//! the current lighting design) stay in the webview's local storage.

use kb_core::color::LedCalibration;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

const CALIBRATION_FILE: &str = "calibration.json";
const CALIBRATION_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct StoredCalibration {
    version: u32,
    #[serde(flatten)]
    calibration: LedCalibration,
}

/// The saved calibration, or the default when none was saved yet or the
/// file cannot be read. Values are always sanitized.
pub fn load_calibration(dir: &Path) -> LedCalibration {
    let path = dir.join(CALIBRATION_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return LedCalibration::default(),
        Err(e) => {
            tracing::warn!("calibration unreadable ({}): {e}", path.display());
            return LedCalibration::default();
        }
    };
    match serde_json::from_str::<StoredCalibration>(&text) {
        Ok(s) if s.version == CALIBRATION_VERSION => s.calibration.sanitized(),
        Ok(s) => {
            tracing::warn!(version = s.version, "unknown calibration version; using default");
            LedCalibration::default()
        }
        Err(e) => {
            tracing::warn!("calibration file is damaged; using default: {e}");
            LedCalibration::default()
        }
    }
}

/// Write the calibration atomically (temp file, then rename).
pub fn save_calibration(dir: &Path, calibration: LedCalibration) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let stored = StoredCalibration { version: CALIBRATION_VERSION, calibration: calibration.sanitized() };
    let json = serde_json::to_string_pretty(&stored).map_err(io::Error::other)?;
    let tmp = dir.join(format!("{CALIBRATION_FILE}.tmp"));
    fs::write(&tmp, json)?;
    fs::rename(&tmp, dir.join(CALIBRATION_FILE))
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
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("round-trip");
        let cal = LedCalibration { gamma: [2.0, 2.4, 1.8], gain: [0.85, 1.0, 0.9] };
        save_calibration(&dir, cal).unwrap();
        assert_eq!(load_calibration(&dir), cal);
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
        let _ = fs::remove_dir_all(&dir);
    }
}
