//! keymotion-cli must never open the keyboard (the app's device actor is
//! its only owner), and it starts on every hook call, so it stays small:
//! no HID stack, no Tauri, no async runtime, here or in any workspace crate
//! it builds on.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Crates that open devices, belong to the app, or start a runtime.
const FORBIDDEN: [&str; 5] = ["kb-transport", "kb-core", "kb-devices", "hidapi", "tokio"];

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("crates/").to_path_buf()
}

/// The crates a manifest builds with: `[dependencies]`,
/// `[build-dependencies]` and their per-target forms (not
/// dev-dependencies, which never reach the program).
fn dependencies(manifest: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(manifest).unwrap_or_else(|e| panic!("{}: {e}", manifest.display()));
    let doc: toml::Table = toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", manifest.display()));
    let mut tables = vec![doc.get("dependencies"), doc.get("build-dependencies")];
    if let Some(targets) = doc.get("target").and_then(|t| t.as_table()) {
        for target in targets.values() {
            tables.push(target.get("dependencies"));
            tables.push(target.get("build-dependencies"));
        }
    }
    tables.into_iter().flatten().filter_map(|t| t.as_table()).flat_map(|t| t.keys().cloned()).collect()
}

#[test]
fn keymotion_cli_never_reaches_the_keyboard() {
    let mut seen = BTreeSet::new();
    let mut queue = vec!["keymotion-cli".to_string()];
    while let Some(krate) = queue.pop() {
        if !seen.insert(krate.clone()) {
            continue;
        }
        for dep in dependencies(&crates_dir().join(&krate).join("Cargo.toml")) {
            assert!(
                !FORBIDDEN.contains(&dep.as_str()) && !dep.starts_with("tauri"),
                "{krate} depends on {dep}: keymotion-cli must not open the keyboard or start a runtime"
            );
            if dep.starts_with("kb-") {
                queue.push(dep);
            }
        }
    }
    assert!(seen.contains("kb-agent") && seen.contains("kb-winsys"), "{seen:?}");
}

#[test]
fn the_check_would_catch_the_hid_stack() {
    let core = dependencies(&crates_dir().join("kb-core").join("Cargo.toml"));
    assert!(core.contains("kb-transport"), "{core:?}");
}
