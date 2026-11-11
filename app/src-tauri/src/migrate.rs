//! The app was called "Klavye Stüdyo" (identifier `dev.klavye.studio`) until
//! 2026-11-11 and is now Keymotion (`dev.keymotion`). Tauri keeps settings in
//! `%APPDATA%\<identifier>` and the WebView's storage (theme, language) in
//! `%LOCALAPPDATA%\<identifier>`, so on the first start after the rename the
//! old folders are moved to the new names. Runs before anything opens them.

use std::path::Path;

pub const OLD_IDENTIFIER: &str = "dev.klavye.studio";
pub const IDENTIFIER: &str = "dev.keymotion";

/// Move the old data folders to the new identifier, once. Errors are only
/// printed: logging is not set up yet, and the app still starts (with empty
/// settings) if a folder cannot be moved.
pub fn move_old_data() {
    #[cfg(windows)]
    for var in ["APPDATA", "LOCALAPPDATA"] {
        let Some(base) = std::env::var_os(var).map(std::path::PathBuf::from) else { continue };
        if let Err(e) = move_dir(&base.join(OLD_IDENTIFIER), &base.join(IDENTIFIER)) {
            eprintln!("could not move {var}\\{OLD_IDENTIFIER} to {IDENTIFIER}: {e}");
        }
    }
}

/// Rename `old` to `new` when `old` exists and `new` does not. Returns
/// whether it moved anything.
fn move_dir(old: &Path, new: &Path) -> std::io::Result<bool> {
    if !old.is_dir() || new.exists() {
        return Ok(false);
    }
    std::fs::rename(old, new)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_matches_the_tauri_config() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], IDENTIFIER);
    }

    #[test]
    fn moves_once_and_never_over_new_data() {
        let base = std::env::temp_dir().join(format!("keymotion-migrate-{}", std::process::id()));
        let (old, new) = (base.join("old"), base.join("new"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("keymap.json"), "{}").unwrap();

        assert!(move_dir(&old, &new).unwrap());
        assert!(new.join("keymap.json").is_file() && !old.exists());
        // Nothing left to move.
        assert!(!move_dir(&old, &new).unwrap());

        // Existing new data is never replaced.
        std::fs::create_dir_all(&old).unwrap();
        assert!(!move_dir(&old, &new).unwrap());
        assert!(old.is_dir());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
