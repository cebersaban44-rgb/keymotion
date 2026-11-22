//! keymotion-cli must never open the keyboard (the app's device actor is
//! its only owner), and it starts on every hook call, so it stays small:
//! no HID stack, no Tauri, no async runtime, anywhere below it.
//!
//! The check walks Cargo.lock from keymotion-cli, so a crate that comes in
//! through another, or under another name in a manifest, is caught too.
//! Cargo.lock lists every dependency kind of a package together, so the
//! walk also covers the workspace crates' dev-dependencies: stricter than
//! needed, never looser.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Crates that open devices, belong to the app, or start a runtime.
const FORBIDDEN: [&str; 6] = ["kb-transport", "kb-core", "kb-devices", "hidapi", "tokio", "mio"];
/// Prefixes of the app's runtime: Tauri, its windowing and its web view.
const FORBIDDEN_PREFIXES: [&str; 4] = ["tauri", "tao", "wry", "webview2"];

fn lock_file() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock")
}

/// Every package in Cargo.lock: (name, version) → its dependencies as
/// written there (`name`, or `name version` when several versions exist).
fn packages() -> BTreeMap<(String, String), Vec<String>> {
    let text = std::fs::read_to_string(lock_file()).expect("Cargo.lock at the workspace root");
    let doc: toml::Table = toml::from_str(&text).expect("Cargo.lock is TOML");
    let list = doc.get("package").and_then(|p| p.as_array()).expect("[[package]] entries");
    list.iter()
        .filter_map(|p| p.as_table())
        .map(|p| {
            let field = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let deps = p
                .get("dependencies")
                .and_then(|d| d.as_array())
                .map(|d| d.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            ((field("name"), field("version")), deps)
        })
        .collect()
}

/// Every package reachable from `root`, by name.
fn reachable(root: &str) -> BTreeSet<String> {
    let all = packages();
    let resolve = |dep: &str| -> Vec<(String, String)> {
        let mut words = dep.split_whitespace();
        let name = words.next().unwrap_or_default();
        let version = words.next();
        all.keys().filter(|(n, v)| n == name && version.is_none_or(|w| w == v)).cloned().collect()
    };
    let mut seen = BTreeSet::new();
    let mut queue: Vec<(String, String)> = all.keys().filter(|(n, _)| n == root).cloned().collect();
    assert!(!queue.is_empty(), "{root} is not in Cargo.lock");
    while let Some(key) = queue.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        for dep in all.get(&key).into_iter().flatten() {
            queue.extend(resolve(dep));
        }
    }
    seen.into_iter().map(|(name, _)| name).collect()
}

fn forbidden(name: &str) -> bool {
    FORBIDDEN.contains(&name) || FORBIDDEN_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[test]
fn keymotion_cli_never_reaches_the_keyboard() {
    let below = reachable("keymotion-cli");
    let bad: Vec<&String> = below.iter().filter(|n| forbidden(n)).collect();
    assert!(bad.is_empty(), "keymotion-cli reaches {bad:?}: it must not open the keyboard or start a runtime");
    for expected in ["kb-agent", "kb-winsys", "clap", "serde_json"] {
        assert!(below.contains(expected), "{expected} missing from {below:?}");
    }
}

#[test]
fn the_check_would_catch_the_hid_stack_and_the_app() {
    // Real crates that do reach them, also through others.
    assert!(reachable("kb-core").contains("hidapi"));
    let app = reachable("keymotion");
    assert!(app.contains("tokio") && app.contains("tauri") && app.contains("kb-transport"));
}
