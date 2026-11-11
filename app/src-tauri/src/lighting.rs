//! The Lighting page's saved state (`app/src/lib/lighting.ts`): the effect
//! and per-key design being edited, the tools, and what the app last stored
//! on the keyboard. It lived in the webview's local storage until it moved
//! here (`lighting.json`, versioned like the other settings); the UI moves
//! an old local copy over once.
//!
//! The UI owns the fields and checks them when it loads them; the backend
//! keeps the document as it comes and only limits its shape and size.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io;
use std::path::Path;

const FILE: &str = "lighting.json";
const VERSION: u32 = 1;
/// A full per-key design is a few kilobytes; anything near this is not one.
pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Serialize, Deserialize)]
struct Stored {
    state: Map<String, Value>,
}

/// The saved document; `None` when nothing was saved here yet.
pub fn load(dir: &Path) -> Option<Map<String, Value>> {
    crate::settings::load::<Stored>(dir, FILE, VERSION).map(|s| s.state)
}

pub fn save(dir: &Path, state: Map<String, Value>) -> io::Result<()> {
    crate::settings::save(dir, FILE, VERSION, Stored { state })
}

/// The document as the UI sent it: a JSON object of reasonable size.
pub fn check(state: Value) -> Result<Map<String, Value>, String> {
    let Value::Object(map) = state else { return Err("lighting state is not an object".into()) };
    let size = serde_json::to_vec(&map).map_or(usize::MAX, |v| v.len());
    if size > MAX_BYTES {
        return Err("lighting state too large".into());
    }
    Ok(map)
}

/// Lighting from a backup over this computer's own. The keyboard is not
/// written by an import, so what this app last stored on it (`appliedEffect`,
/// `savedPerKey`) stays this computer's; the backup's design comes in as an
/// edit not yet saved to the keyboard (`draft`), which the Lighting page
/// offers to save. Field names follow `lighting.ts`.
pub fn merge_import(local: Option<Map<String, Value>>, backup: Map<String, Value>) -> Map<String, Value> {
    let mut out = backup;
    let local = local.unwrap_or_default();
    for key in ["appliedEffect", "savedPerKey"] {
        out.insert(key.into(), local.get(key).cloned().unwrap_or(Value::Null));
    }
    let per_key = out.get("tab").and_then(Value::as_str) == Some("per-key");
    out.insert("draft".into(), Value::from(if per_key { "per-key" } else { "effect" }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn round_trip_and_shape() {
        let dir = std::env::temp_dir().join(format!("klavye-lighting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load(&dir), None);
        let state = json!({ "tab": "per-key", "perKey": { "33": "#ff8a3d" }, "perKeyBrightness": 4, "draft": null });
        save(&dir, check(state.clone()).unwrap()).unwrap();
        assert_eq!(load(&dir), Some(obj(state)));
        let text = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains(r#""version": 1"#), "{text}");
        assert!(check(json!([1, 2])).is_err());
        assert!(check(json!({ "perKey": "x".repeat(MAX_BYTES) })).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_import_keeps_what_the_keyboard_has() {
        let local = obj(json!({
            "appliedEffect": { "mode": 1, "color": "#ffffff", "rainbow": false, "brightness": 5, "speed": 3, "direction": 0 },
            "savedPerKey": null,
            "effect": { "mode": 1 },
        }));
        let backup = obj(json!({
            "tab": "per-key",
            "perKey": { "40": "#00ff00" },
            "savedPerKey": { "colors": { "40": "#00ff00" }, "brightness": 5 },
            "appliedEffect": null,
            "draft": null,
        }));
        let merged = merge_import(Some(local.clone()), backup);
        assert_eq!(merged["appliedEffect"], local["appliedEffect"]);
        assert_eq!(merged["savedPerKey"], Value::Null, "the backup's record does not claim the keyboard");
        assert_eq!(merged["perKey"], json!({ "40": "#00ff00" }));
        assert_eq!(merged["draft"], json!("per-key"));
        let effect = merge_import(None, obj(json!({ "tab": "effects" })));
        assert_eq!(effect["draft"], json!("effect"));
        assert_eq!(effect["appliedEffect"], Value::Null);
    }
}
