//! "Copy diagnostics": a plain-text report the user can paste into a
//! message to the developer, and opening the app's folders in Explorer.
//!
//! The report holds versions (app, Windows, WebView2), the keyboards the
//! app sees (model, connection, firmware when known), the names and sizes
//! of the settings files, and the last lines of the log. It never holds a
//! calendar address, a reminder title or a meeting name: the log does not
//! carry them, and every log line passes [`crate::logging::redact`] again.
//! The report is English: it is read by the developer, not shown in the UI.
//!
//! A problem report adds what happened (an error, a crash note) at the top.
//! A support request for a keyboard the app doesn't know yet describes that
//! keyboard instead ([`keyboard_details`]).

use crate::dto::KeyboardInfo;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Log lines in a report.
pub const LOG_LINES: usize = 150;
/// Longest "what happened" text in a report.
const CONTEXT_MAX: usize = 4_000;

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
    /// What happened, for a problem report (an error, a crash note).
    pub context: Option<String>,
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
    if let Some(context) = f.context.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        let context: String = crate::logging::redact(context).chars().take(CONTEXT_MAX).collect();
        s.push_str("\nWhat happened:\n");
        for line in context.lines() {
            s.push_str(&format!("  {line}\n"));
        }
    }
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

/// The start of a support request: the app and Windows versions.
pub fn support_request(app_name: &str, app_version: &str, windows: &str, details: &str) -> String {
    format!("{app_name} keyboard support request\nApp version: {app_version}\nWindows: {windows}\n\n{details}")
}

/// A keyboard the app doesn't know yet, for a support request: its ids and
/// names, and each HID collection with its report descriptor (`descriptors`,
/// in the order of `k.collections`). The serial number and the device paths
/// stay out: they identify the user's own keyboard, not the model.
pub fn keyboard_details(k: &kb_core::UnknownKeyboard, descriptors: &[Result<Vec<u8>, String>]) -> String {
    let mut s = String::new();
    s.push_str(&format!("Keyboard: {}\n", k.product.as_deref().unwrap_or("(no name)")));
    if let Some(m) = &k.manufacturer {
        s.push_str(&format!("Maker: {m}\n"));
    }
    s.push_str(&format!("USB ids: VID {:04X}, PID {:04X}, release 0x{:04X}\n", k.vid, k.pid, k.release));
    let bus = match k.bus {
        kb_transport::hid::Bus::Usb => "USB",
        kb_transport::hid::Bus::Bluetooth => "Bluetooth",
        kb_transport::hid::Bus::Other => "other",
    };
    s.push_str(&format!("Connection: {bus}\n"));
    s.push_str(&format!("\nCollections ({}):\n", k.collections.len()));
    for (i, &(page, usage, interface)) in k.collections.iter().enumerate() {
        s.push_str(&format!("  {}. usage page 0x{page:04X}, usage 0x{usage:04X}", i + 1));
        if let Some(name) = usage_name(page, usage) {
            s.push_str(&format!(" ({name})"));
        }
        if interface >= 0 {
            s.push_str(&format!(", interface {interface}"));
        }
        s.push('\n');
        match descriptors.get(i) {
            Some(Ok(bytes)) => {
                s.push_str(&format!("     descriptor, {} bytes:\n", bytes.len()));
                for chunk in bytes.chunks(16) {
                    let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02X}")).collect();
                    s.push_str(&format!("     {}\n", hex.join(" ")));
                }
            }
            Some(Err(e)) => s.push_str(&format!("     descriptor not readable: {e}\n")),
            None => s.push_str("     descriptor not read\n"),
        }
    }
    s
}

/// What a HID collection is, for the common ones.
fn usage_name(page: u16, usage: u16) -> Option<&'static str> {
    Some(match (page, usage) {
        (0x0001, 0x0002) => "mouse",
        (0x0001, 0x0006) => "keyboard",
        (0x0001, 0x0080) => "system control",
        (0x000C, 0x0001) => "consumer control",
        (p, _) if p >= 0xFF00 => "vendor-defined",
        _ => return None,
    })
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
            context: None,
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
            context: None,
            keyboards: vec![],
            data_files: vec![],
            log_lines: vec![],
        };
        let r = report(&f);
        assert!(r.contains("Keyboards:\n  none found\n") && r.contains("Data files:\n  none\n"), "{r}");
    }

    #[test]
    fn a_problem_report_says_what_happened_first() {
        let f = Facts {
            app_name: "Keymotion".into(),
            app_version: "0.1.0".into(),
            time: "t".into(),
            windows: "w".into(),
            webview: "v".into(),
            ui_language: "en".into(),
            context: Some(
                r"action failed: not saved at C:\Users\someone\x
second line"
                    .into(),
            ),
            keyboards: vec![],
            data_files: vec![],
            log_lines: vec![],
        };
        let r = report(&f);
        let what = r.find("What happened:\n  action failed: not saved").expect("context");
        assert!(what < r.find("Keyboards:").unwrap(), "{r}");
        assert!(r.contains("\n  second line\n") && !r.contains("someone"), "{r}");
    }

    #[test]
    fn keyboard_details_carry_ids_collections_and_descriptors() {
        let k = kb_core::UnknownKeyboard {
            vid: 0x0C45,
            pid: 0x8009,
            bus: kb_transport::hid::Bus::Usb,
            release: 0x0138,
            manufacturer: Some("SONiX".into()),
            product: Some("SONiX AK820 PRO".into()),
            collections: vec![(0x0001, 0x0006, 0), (0xFF13, 0x0001, 3)],
            paths: vec![r"\\?\hid#vid_0c45&pid_8009&mi_00#7&serial".into(), "p2".into()],
        };
        let descriptor: Vec<u8> = (0u8..20).collect();
        let r = keyboard_details(&k, &[Ok(descriptor), Err("access denied".into())]);
        for part in [
            "Keyboard: SONiX AK820 PRO\n",
            "Maker: SONiX\n",
            "USB ids: VID 0C45, PID 8009, release 0x0138\n",
            "Connection: USB\n",
            "1. usage page 0x0001, usage 0x0006 (keyboard), interface 0\n",
            "descriptor, 20 bytes:\n     00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F\n     10 11 12 13\n",
            "2. usage page 0xFF13, usage 0x0001 (vendor-defined), interface 3\n",
            "descriptor not readable: access denied",
        ] {
            assert!(r.contains(part), "{part:?} missing:\n{r}");
        }
        assert!(!r.contains("hid#") && !r.contains("serial"), "paths stay out:\n{r}");
        assert!(
            support_request("Keymotion", "0.1.0", "Windows 11", &r).starts_with("Keymotion keyboard support request\n")
        );
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
