//! "Copy diagnostics": a plain-text report the user can paste into a
//! message to the developer, and opening the app's folders in Explorer.
//!
//! The report holds versions (app, Windows, WebView2), the keyboards the
//! app sees (model, connection, firmware when known), the names and sizes
//! of the settings files, and the last lines of the log. It never holds a
//! calendar address, a reminder title or a meeting name: the log does not
//! carry them, and every log line passes [`crate::logging::redact`] again.
//! The report is English: it is read by the developer, not shown in the UI.

use crate::dto::KeyboardInfo;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Log lines in a report.
pub const LOG_LINES: usize = 150;

/// A folder the user can open from Settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export)]
pub enum AppFolder {
    /// Daily logs and crash notes (local to this computer).
    Logs,
    /// Settings, records and the screen library.
    Data,
}

/// Everything a report says, gathered by the caller.
pub struct Facts {
    pub app_name: String,
    pub app_version: String,
    pub time: String,
    pub windows: String,
    pub webview: String,
    pub ui_language: String,
    pub keyboards: Vec<String>,
    pub data_files: Vec<(String, u64)>,
    pub log_lines: Vec<String>,
}

/// One keyboard as a report line. The firmware version is read from the
/// UI's view of the keyboard when it has one (`firmware`), so the report
/// picks it up without depending on how it is stored.
pub fn keyboard_line(k: &KeyboardInfo) -> String {
    let value = serde_json::to_value(k).unwrap_or_default();
    let text = |key: &str| value.get(key).and_then(|v| v.as_str()).map(str::to_string);
    let firmware = firmware_text(value.get("firmware"));
    let mut line =
        format!("{} ({}), {}, firmware {firmware}", k.model_name, k.model_id, text("connection").unwrap_or_default());
    if let Some(p) = &k.product {
        line.push_str(&format!(", product \"{p}\""));
    }
    if !k.has_command_channel {
        line.push_str(", no command channel");
    }
    line
}

/// The firmware version as the keyboard's view gives it: text as it is, a
/// USB `bcdDevice` number in hex, nothing as "unknown".
fn firmware_text(v: Option<&serde_json::Value>) -> String {
    match v {
        None | Some(serde_json::Value::Null) => "unknown".to_string(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.as_u64().map_or_else(|| n.to_string(), |v| format!("0x{v:04X}")),
        Some(other) => other.to_string(),
    }
}

/// Settings files at the top of the data folder, with their sizes.
pub fn data_files(dir: &Path) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<(String, u64)> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| (e.file_name().to_string_lossy().into_owned(), meta.len()))
        })
        .collect();
    files.sort();
    files
}

fn size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// The report text.
pub fn report(f: &Facts) -> String {
    let mut s = String::new();
    s.push_str(&format!("{} diagnostics\n", f.app_name));
    s.push_str(&format!("Created: {}\n", f.time));
    s.push_str(&format!("App version: {}\n", f.app_version));
    s.push_str(&format!("Windows: {}\n", f.windows));
    s.push_str(&format!("WebView2: {}\n", f.webview));
    s.push_str(&format!("UI language: {}\n", f.ui_language));
    s.push_str("\nKeyboards:\n");
    if f.keyboards.is_empty() {
        s.push_str("  none found\n");
    }
    for k in &f.keyboards {
        s.push_str(&format!("  {k}\n"));
    }
    s.push_str("\nData files:\n");
    if f.data_files.is_empty() {
        s.push_str("  none\n");
    }
    for (name, bytes) in &f.data_files {
        s.push_str(&format!("  {name} ({})\n", size(*bytes)));
    }
    s.push_str(&format!("\nLast {} log lines (addresses and user names removed):\n", f.log_lines.len()));
    for line in &f.log_lines {
        s.push_str(&crate::logging::redact(line));
        s.push('\n');
    }
    s
}

/// Open `path` in Explorer; with `select`, open its folder with it selected.
pub fn reveal(path: &Path, select: bool) -> Result<(), String> {
    let mut cmd = explorer(path, select);
    cmd.spawn().map(drop).map_err(|e| format!("folder not opened: {e}"))
}

#[cfg(windows)]
fn explorer(path: &Path, select: bool) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("explorer.exe");
    if select {
        // Explorer reads `/select,"path"` itself, not as standard arguments.
        cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    } else {
        cmd.arg(path);
    }
    cmd
}

#[cfg(target_os = "macos")]
fn explorer(path: &Path, select: bool) -> std::process::Command {
    let mut cmd = std::process::Command::new("open");
    if select {
        cmd.arg("-R");
    }
    cmd.arg(path);
    cmd
}

#[cfg(all(unix, not(target_os = "macos")))]
fn explorer(path: &Path, select: bool) -> std::process::Command {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(if select { path.parent().unwrap_or(path) } else { path });
    cmd
}

/// Where the last export went, for "Show in folder".
#[derive(Default)]
pub struct LastExport(pub std::sync::Mutex<Option<PathBuf>>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_has_versions_and_keeps_secrets_out() {
        let f = Facts {
            app_name: "Keymotion".into(),
            app_version: "0.1.0".into(),
            time: "2026-11-11T14:03:22+03:00".into(),
            windows: "Windows 11 Core 25H2 (build 26200.7171)".into(),
            webview: "142.0.3595.65".into(),
            ui_language: "tr".into(),
            keyboards: vec!["AJAZZ AK980 PRO (ajazz-ak980-pro), usb, firmware unknown".into()],
            data_files: vec![("calendar.json".into(), 2048), ("keymap.json".into(), 12)],
            log_lines: vec![
                "INFO calendar added host=calendar.google.com".into(),
                r"WARN at https://calendar.google.com/private-abc in C:\Users\someone\AppData".into(),
            ],
        };
        let r = report(&f);
        assert!(r.starts_with("Keymotion diagnostics\n"), "{r}");
        for part in
            ["App version: 0.1.0", "WebView2: 142.0.3595.65", "25H2", "calendar.json (2.0 KB)", "keymap.json (12 B)"]
        {
            assert!(r.contains(part), "{part} missing:\n{r}");
        }
        assert!(r.contains("Last 2 log lines"));
        assert!(!r.contains("private-abc") && !r.contains("someone"), "{r}");
    }

    #[test]
    fn an_empty_report_still_reads() {
        let f = Facts {
            app_name: "X".into(),
            app_version: "1".into(),
            time: "t".into(),
            windows: "unknown".into(),
            webview: "unknown".into(),
            ui_language: "en".into(),
            keyboards: vec![],
            data_files: vec![],
            log_lines: vec![],
        };
        let r = report(&f);
        assert!(r.contains("Keyboards:\n  none found\n") && r.contains("Data files:\n  none\n"), "{r}");
    }

    #[test]
    fn firmware_is_shown_however_it_is_known() {
        use serde_json::json;
        assert_eq!(firmware_text(None), "unknown");
        assert_eq!(firmware_text(Some(&json!(null))), "unknown");
        assert_eq!(firmware_text(Some(&json!("1.06"))), "1.06");
        assert_eq!(firmware_text(Some(&json!(0x0106))), "0x0106");
    }

    #[test]
    fn data_files_lists_only_files() {
        let dir = std::env::temp_dir().join(format!("klavye-diag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("screen")).unwrap();
        std::fs::write(dir.join("b.json"), "{}").unwrap();
        std::fs::write(dir.join("a.json"), "{ }").unwrap();
        assert_eq!(data_files(&dir), vec![("a.json".to_string(), 3), ("b.json".to_string(), 2)]);
        assert!(data_files(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
