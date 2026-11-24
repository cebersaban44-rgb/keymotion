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
//!
//! A report stays short enough to paste into a GitHub issue (65,536
//! characters) whatever happened: what happened, the "AA" keyboard section
//! and every log line have a limit, the log fills what is left of
//! [`REPORT_MAX`] (newest lines first), and anything cut says so
//! ("[… 120 characters left out]"). Many reasons that differ only in their
//! numbers read as one line ([`summarize_reasons`]).

use crate::dto::KeyboardInfo;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use ts_rs::TS;

/// Log lines read for a report, the newest: repeats are merged, then the
/// newest that fit are shown ("Last N log lines" never says more).
pub const LOG_LINES: usize = 150;
/// Longest log line in a report.
const LOG_LINE_MAX: usize = 400;
/// Longest "what happened" text in a report, its indentation included.
const CONTEXT_MAX: usize = 3_000;
/// Longest "AA" keyboard section.
const AA_MAX: usize = 6_000;
/// Longest line of the "AA" keyboard section or the keyboard list.
const LINE_MAX: usize = 600;
/// Keyboards and data files listed at most.
const LIST_MAX: usize = 40;
/// Longest report, well under GitHub's 65,536 characters for an issue.
pub const REPORT_MAX: usize = 20_000;

/// Lines a list of reasons is grouped into, at most; the rest is counted
/// ("and 12 more").
const REASON_LINES: usize = 8;
/// Longest reason line.
const REASON_LINE_MAX: usize = 240;
/// Ranges of numbers written out in one reason line, at most.
const RUNS_MAX: usize = 8;
/// Reasons that differ in more than their first changing number go on
/// lines of their own up to this many; beyond, the other numbers are
/// counted.
const VARIANTS_MAX: usize = 3;

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
    /// About the "AA" keyboard on the cable, when there was one (aa.rs
    /// `AaDriver::diagnostics`, aa/report.rs): the device information, the
    /// options and the effect (its colour included) whole; the keymaps,
    /// the per-key table and the macro table counted (entries, the values
    /// of their first byte, macro addresses); the first and last bytes of
    /// a table that failed the sanity check, which can be a keymap, the
    /// per-key colours or the macro table's addresses. Never macro
    /// contents.
    pub aa: Vec<String>,
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

/// The report text, at most [`REPORT_MAX`] characters.
pub fn report(f: &Facts) -> String {
    use crate::logging::redact;
    let mut s = String::new();
    s.push_str(&format!("{} diagnostics\n", f.app_name));
    s.push_str(&format!("Created: {}\n", f.time));
    s.push_str(&format!("App version: {}\n", f.app_version));
    s.push_str(&format!("Windows: {}\n", f.windows));
    s.push_str(&format!("WebView2: {}\n", f.webview));
    s.push_str(&format!("UI language: {}\n", f.ui_language));
    if let Some(context) = f.context.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        s.push_str("\nWhat happened:\n");
        // Cut with its indentation: many short lines count too.
        let indented: Vec<String> = redact(context).lines().map(|l| format!("  {l}")).collect();
        s.push_str(&cut(&indented.join("\n"), CONTEXT_MAX));
        s.push('\n');
    }
    s.push_str("\nKeyboards:\n");
    if f.keyboards.is_empty() {
        s.push_str("  none found\n");
    }
    for k in f.keyboards.iter().take(LIST_MAX) {
        s.push_str(&format!("  {}\n", cut(&redact(k), LINE_MAX)));
    }
    if f.keyboards.len() > LIST_MAX {
        s.push_str(&format!("  and {} more\n", f.keyboards.len() - LIST_MAX));
    }
    if !f.aa.is_empty() {
        s.push_str("\n\"AA\" keyboard:\n");
        s.push_str(&aa_section(&f.aa));
    }
    s.push_str("\nData files:\n");
    if f.data_files.is_empty() {
        s.push_str("  none\n");
    }
    for (name, bytes) in f.data_files.iter().take(LIST_MAX) {
        s.push_str(&format!("  {} ({})\n", cut(&redact(name), LINE_MAX), size(*bytes)));
    }
    if f.data_files.len() > LIST_MAX {
        s.push_str(&format!("  and {} more\n", f.data_files.len() - LIST_MAX));
    }
    s.push_str(&log_section(&f.log_lines, REPORT_MAX.saturating_sub(chars(&s))));
    if chars(&s) > REPORT_MAX {
        s = cut(&s, REPORT_MAX);
        s.push('\n');
    }
    s
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// The "AA" keyboard section's lines, indented, each redacted and at most
/// [`LINE_MAX`] characters, the whole at most [`AA_MAX`]: the lines that
/// come last (the hex excerpts, the errors) go first.
fn aa_section(lines: &[String]) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let line = format!("  {}\n", cut(&crate::logging::redact(line), LINE_MAX));
        if chars(&out) + chars(&line) > AA_MAX {
            let left: usize = lines[i..].iter().map(|l| chars(l) + 3).sum();
            out.push_str(&format!("  [… {} more lines, {left} characters, left out]\n", lines.len() - i));
            break;
        }
        out.push_str(&line);
    }
    out
}

/// The log part: the newest of `lines` (at most [`LOG_LINES`]) that fit in
/// `room` characters, each redacted and at most [`LOG_LINE_MAX`]
/// characters, repeats merged.
fn log_section(lines: &[String], room: usize) -> String {
    let lines = &lines[lines.len().saturating_sub(LOG_LINES)..];
    let entries = merge_repeats(lines);
    // Heading and the note about older lines.
    let mut room = room.saturating_sub(160);
    let mut shown: Vec<&(String, usize)> = Vec::new();
    for entry in entries.iter().rev() {
        let len = chars(&entry.0) + 1;
        if len > room {
            break;
        }
        room -= len;
        shown.push(entry);
    }
    shown.reverse();
    let covered: usize = shown.iter().map(|(_, n)| n).sum();
    let mut s = format!("\nLast {covered} log lines (addresses and user names removed):\n");
    if covered < lines.len() {
        s.push_str(&format!("[… {} older lines left out to keep the report short]\n", lines.len() - covered));
    }
    for (text, _) in shown {
        s.push_str(text);
        s.push('\n');
    }
    s
}

/// Log lines as report entries (text, lines it stands for): redacted, cut
/// to [`LOG_LINE_MAX`], and a line the same as the one before but for its
/// time merged into it ("… repeated 12 more times").
fn merge_repeats(lines: &[String]) -> Vec<(String, usize)> {
    // The line without its leading time stamp.
    fn body(line: &str) -> &str {
        let starts_with_time = line.as_bytes().first().is_some_and(u8::is_ascii_digit);
        match line.split_once(char::is_whitespace) {
            Some((time, rest)) if starts_with_time && time.contains('T') => rest,
            _ => line,
        }
    }
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let first = &lines[i];
        let same = lines[i + 1..].iter().take_while(|l| body(l) == body(first)).count();
        let mut text = cut(&crate::logging::redact(first), LOG_LINE_MAX);
        if same > 0 {
            let last = &lines[i + same];
            let time = last.split_whitespace().next().filter(|_| body(last) != last.as_str());
            let times = if same == 1 { "once more".to_string() } else { format!("{same} more times") };
            match time {
                Some(t) => text.push_str(&format!("\n  … repeated {times}, the last at {t}")),
                None => text.push_str(&format!("\n  … repeated {times}")),
            }
        }
        out.push((text, same + 1));
        i += same + 1;
    }
    out
}

/// `text` cut to at most `max` characters, on a word boundary where there
/// is one nearby, with a marker that says how much was left out.
pub fn cut(text: &str, max: usize) -> String {
    let total = chars(text);
    if total <= max {
        return text.to_string();
    }
    let marker = |left: usize| format!(" [… {left} characters left out]");
    let keep = max.saturating_sub(chars(&marker(total)));
    let end = text.char_indices().nth(keep).map_or(text.len(), |(i, _)| i);
    let mut head = &text[..end];
    // Back to the last space, unless that loses much (hex, a long path).
    if !text[end..].starts_with(char::is_whitespace) {
        if let Some(space) = head.rfind(char::is_whitespace).filter(|&at| at >= end * 2 / 3) {
            head = &head[..space];
        }
    }
    let head = head.trim_end();
    format!("{head}{}", marker(total - chars(head)))
}

/// Bytes as `40 00 0C …`.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Bytes as hex lines of 16 with their offsets (`010: …`): all of them, or
/// the first `head` and the last `tail` with a line that counts the rest.
pub fn hex_excerpt(bytes: &[u8], head: usize, tail: usize) -> Vec<String> {
    let lines = |from: usize, part: &[u8]| -> Vec<String> {
        part.chunks(16).enumerate().map(|(i, c)| format!("{:03X}: {}", from + i * 16, hex(c))).collect()
    };
    if bytes.len() <= head + tail {
        return lines(0, bytes);
    }
    let tail_at = bytes.len() - tail;
    let mut out = lines(0, &bytes[..head]);
    out.push(format!("… {} bytes left out", tail_at - head));
    out.extend(lines(tail_at, &bytes[tail_at..]));
    out
}

// ------------------------------------------------------------- reasons

/// A number in a reason, as it was written.
#[derive(Debug, Clone)]
struct Num {
    value: u64,
    text: String,
}

/// A reason split at its numbers: `parts` has one more item than `nums`.
/// A number is a whole word of digits, or `0x` and hex digits.
fn split_numbers(s: &str) -> (Vec<String>, Vec<Num>) {
    let mut parts = vec![String::new()];
    let mut nums = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if !c.is_ascii_alphanumeric() {
            parts.last_mut().expect("never empty").push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        let end = rest.find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_').unwrap_or(rest.len());
        let word = &rest[..end];
        let value = match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
            Some(h) if !h.is_empty() && h.len() <= 16 => u64::from_str_radix(h, 16).ok(),
            _ if word.len() <= 19 && word.bytes().all(|b| b.is_ascii_digit()) => word.parse().ok(),
            _ => None,
        };
        match value {
            Some(value) => {
                nums.push(Num { value, text: word.to_string() });
                parts.push(String::new());
            }
            None => parts.last_mut().expect("never empty").push_str(word),
        }
        rest = &rest[end..];
    }
    (parts, nums)
}

/// Numbers as sorted runs: `1-127`, `3, 4`, `0x0E`; past [`RUNS_MAX`] runs
/// the rest is counted.
fn ranges(nums: &[&Num]) -> String {
    let mut sorted: Vec<&Num> = nums.to_vec();
    sorted.sort_by_key(|n| n.value);
    sorted.dedup_by_key(|n| n.value);
    let mut runs: Vec<(&Num, &Num)> = Vec::new();
    for n in sorted.iter().copied() {
        match runs.last_mut() {
            Some((_, last)) if last.value + 1 == n.value => *last = n,
            _ => runs.push((n, n)),
        }
    }
    let text = |(a, b): &(&Num, &Num)| match b.value - a.value {
        0 => a.text.clone(),
        1 => format!("{}, {}", a.text, b.text),
        _ => format!("{}-{}", a.text, b.text),
    };
    let mut out: Vec<String> = runs.iter().take(RUNS_MAX).map(text).collect();
    if runs.len() > RUNS_MAX {
        out.push(format!("… ({} in all)", sorted.len()));
    }
    out.join(", ")
}

/// Numbers as sorted runs (`0-3, 7, 9, 10`), as in a reason line.
pub fn number_ranges(values: &[u64]) -> String {
    let nums: Vec<Num> = values.iter().map(|&value| Num { value, text: value.to_string() }).collect();
    ranges(&nums.iter().collect::<Vec<_>>())
}

/// One reason line from its parts and what goes in each number's place.
fn fill(parts: &[String], slots: &[String]) -> String {
    let mut s = parts[0].clone();
    for (slot, part) in slots.iter().zip(&parts[1..]) {
        s.push_str(slot);
        s.push_str(part);
    }
    s
}

/// The lines for reasons that read the same but for their numbers
/// (`members`: their numbers, in order), with how many reasons each covers.
fn group_lines(parts: &[String], members: &[&[Num]]) -> Vec<(String, usize)> {
    let n = members.len();
    let slots = parts.len() - 1;
    let varies = |k: usize| members.iter().any(|m| m[k].value != members[0][k].value);
    let Some(key) = (0..slots).find(|&k| varies(k)) else {
        let line = fill(parts, &members[0].iter().map(|x| x.text.clone()).collect::<Vec<_>>());
        return vec![(if n > 1 { format!("{line} ({n} times)") } else { line }, n)];
    };
    // Reasons with the same other numbers share a line.
    let mut variants: Vec<(Vec<String>, Vec<&Num>)> = Vec::new();
    for m in members {
        let others: Vec<String> = (0..slots).filter(|&k| k != key).map(|k| m[k].text.clone()).collect();
        match variants.iter_mut().find(|(o, _)| *o == others) {
            Some((_, keys)) => keys.push(&m[key]),
            None => variants.push((others, vec![&m[key]])),
        }
    }
    if variants.len() <= VARIANTS_MAX {
        return variants
            .iter()
            .map(|(others, keys)| {
                let mut filled: Vec<String> = others.clone();
                filled.insert(key, ranges(keys));
                (fill(parts, &filled), keys.len())
            })
            .collect();
    }
    let filled: Vec<String> = (0..slots)
        .map(|k| {
            let all: Vec<&Num> = members.iter().map(|m| &m[k]).collect();
            if k == key {
                ranges(&all)
            } else if varies(k) {
                let mut values: Vec<u64> = all.iter().map(|x| x.value).collect();
                values.sort_unstable();
                values.dedup();
                format!("[{} different values]", values.len())
            } else {
                all[0].text.clone()
            }
        })
        .collect();
    vec![(fill(parts, &filled), n)]
}

/// Reasons (the sanity check's, say) as a few lines: reasons that differ
/// only in their numbers share a line, the numbers that change as ranges
/// ("per-key entry 1-127 starts with 0"). At most [`REASON_LINES`] lines of
/// at most [`REASON_LINE_MAX`] characters, then "and N more"; the order is
/// the order the reasons came in.
pub fn summarize_reasons(reasons: &[String]) -> Vec<String> {
    let mut groups: Vec<(Vec<String>, Vec<Vec<Num>>)> = Vec::new();
    for r in reasons {
        let (parts, nums) = split_numbers(r);
        match groups.iter_mut().find(|(p, _)| *p == parts) {
            Some((_, members)) => members.push(nums),
            None => groups.push((parts, vec![nums])),
        }
    }
    let lines: Vec<(String, usize)> = groups
        .iter()
        .flat_map(|(parts, members)| {
            let members: Vec<&[Num]> = members.iter().map(Vec::as_slice).collect();
            group_lines(parts, &members)
        })
        .collect();
    let mut out: Vec<String> =
        lines.iter().take(REASON_LINES).map(|(line, _)| cut(line.trim(), REASON_LINE_MAX)).collect();
    let more: usize = lines.iter().skip(REASON_LINES).map(|(_, n)| n).sum();
    if more > 0 {
        out.push(format!("and {more} more"));
    }
    out
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

    fn strings(v: impl IntoIterator<Item = String>) -> Vec<String> {
        v.into_iter().collect()
    }

    fn facts(context: Option<String>, log_lines: Vec<String>) -> Facts {
        Facts {
            app_name: "Keymotion".into(),
            app_version: "1.3.0".into(),
            time: "t".into(),
            windows: "w".into(),
            webview: "v".into(),
            ui_language: "en".into(),
            context,
            keyboards: vec![],
            aa: vec![],
            data_files: vec![],
            log_lines,
        }
    }

    #[test]
    fn reasons_that_differ_in_numbers_share_a_line() {
        // The 1.3.0-beta.1 report: 127 of these, six times over.
        let per_key = strings((1..128).map(|i| format!("per-key entry {i} starts with 0")));
        assert_eq!(summarize_reasons(&per_key), ["per-key entry 1-127 starts with 0"]);

        // Gaps and pairs; hex numbers keep their spelling.
        let mixed = strings(
            [0, 1, 2, 5, 6, 9]
                .iter()
                .map(|i| format!("base layer slot {i}: unknown entry page 0x0E"))
                .chain([3, 4].iter().map(|i| format!("base layer slot {i}: unknown entry page 0xFF")))
                .chain(["unknown effect mode 99".to_string()]),
        );
        assert_eq!(
            summarize_reasons(&mixed),
            [
                "base layer slot 0-2, 5, 6, 9: unknown entry page 0x0E",
                "base layer slot 3, 4: unknown entry page 0xFF",
                "unknown effect mode 99",
            ]
        );

        // Other numbers that keep changing are counted, not listed.
        let noisy = strings((0..128).map(|i| format!("per-key entry {i} starts with {}", (i * 37 + 1) % 251)));
        assert_eq!(summarize_reasons(&noisy), ["per-key entry 0-127 starts with [128 different values]"]);

        // The same reason twice, words that hold digits, an empty list.
        let same = strings(vec!["the keyboard reported a reset".to_string(); 2]);
        assert_eq!(summarize_reasons(&same), ["the keyboard reported a reset (2 times)"]);
        assert_eq!(
            summarize_reasons(&strings(["vendor id 0C46, expected 0C45".to_string()])),
            ["vendor id 0C46, expected 0C45"]
        );
        assert!(summarize_reasons(&[]).is_empty());
    }

    #[test]
    fn a_long_list_of_reasons_ends_with_a_count() {
        let many = strings((0..20).map(|i| format!("reason kind {} here", "x".repeat(i + 1))));
        let lines = summarize_reasons(&many);
        assert_eq!(lines.len(), REASON_LINES + 1);
        assert_eq!(lines.last().unwrap(), "and 12 more");
        let scattered = strings((0..40).map(|i| format!("macro {} is gone", i * 3)));
        assert_eq!(summarize_reasons(&scattered), ["macro 0, 3, 6, 9, 12, 15, 18, 21, … (40 in all) is gone"]);
        let long = strings([format!("a reason {}", "word ".repeat(100))]);
        let line = &summarize_reasons(&long)[0];
        assert!(line.chars().count() <= REASON_LINE_MAX && line.ends_with("characters left out]"), "{line}");
    }

    #[test]
    fn cuts_fall_on_a_word_and_say_how_much_went() {
        assert_eq!(cut("short", 10), "short");
        let text = format!("{} end", "lorem ipsum ".repeat(50));
        let c = cut(&text, 100);
        assert!(c.chars().count() <= 100, "{c}");
        assert!(c.starts_with("lorem ipsum lorem"), "{c}");
        let kept = c.split(" [… ").next().unwrap();
        assert!(kept.ends_with("lorem") || kept.ends_with("ipsum"), "on a word: {c}");
        let left: usize = c.split("[… ").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        assert_eq!(kept.chars().count() + left, text.chars().count());
        // No space nearby: a hard cut.
        let hex = "AB".repeat(200);
        assert!(cut(&hex, 50).starts_with("ABAB") && cut(&hex, 50).chars().count() <= 50);
        // Not inside a character.
        assert!(cut(&"ş".repeat(300), 60).starts_with("şş"));
    }

    #[test]
    fn hex_excerpts_show_the_start_and_the_end() {
        let bytes: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
        let lines = hex_excerpt(&bytes, 32, 16);
        assert_eq!(
            lines,
            [
                "000: 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F",
                "010: 10 11 12 13 14 15 16 17 18 19 1A 1B 1C 1D 1E 1F",
                "… 464 bytes left out",
                "1F0: F0 F1 F2 F3 F4 F5 F6 F7 F8 F9 FA FB FC FD FE FF",
            ]
        );
        assert_eq!(hex_excerpt(&[1, 2, 3], 128, 16), ["000: 01 02 03"]);
        assert_eq!(number_ranges(&[7, 1, 2, 3, 9, 10]), "1-3, 7, 9, 10");
    }

    #[test]
    fn what_happened_is_cut_with_a_marker() {
        let what = format!("The keyboard does not look as expected: {}", "per-key entry 1 starts with 0; ".repeat(400));
        let r = report(&facts(Some(what), vec![]));
        let part = &r[r.find("What happened:").unwrap()..r.find("\nKeyboards:").unwrap()];
        assert!(part.chars().count() <= CONTEXT_MAX + 40, "{}", part.chars().count());
        assert!(part.contains("characters left out]"), "{part}");
        assert!(!part.contains("per-key e\n"), "not mid-word");

        // Many short lines: their indentation counts too.
        let r = report(&facts(Some("- x\n".repeat(3_000)), vec![]));
        let part = &r[r.find("What happened:\n").unwrap() + 15..r.find("\nKeyboards:").unwrap()];
        assert!(part.chars().count() <= CONTEXT_MAX + 1, "{}", part.chars().count());
        assert!(part.starts_with("  - x\n  - x\n") && part.contains("characters left out]"), "{part}");
    }

    #[test]
    fn log_lines_are_short_merged_and_fit_the_report() {
        let mut lines = vec!["2026-11-24T03:42:17.754222Z  INFO keymotion_lib: app started".to_string()];
        lines.extend((0..12).map(|i| {
            format!("2026-11-24T03:43:{i:02}.000000Z  INFO keymotion_lib::device: keyboards changed count=1")
        }));
        lines.push(format!("2026-11-24T03:44:00.000000Z  WARN kb_core::aa: gate why={}", "\"x\", ".repeat(2_000)));
        let r = report(&facts(None, lines.clone()));
        assert!(r.contains("Last 14 log lines"), "{r}");
        assert!(
            r.contains(
                "keyboards changed count=1\n  … repeated 11 more times, the last at 2026-11-24T03:43:11.000000Z\n"
            ),
            "{r}"
        );
        let last = r.lines().find(|l| l.contains("kb_core::aa: gate")).unwrap();
        assert!(last.chars().count() <= LOG_LINE_MAX && last.ends_with("characters left out]"), "{last}");

        // A long log: the newest lines that fit, and a note about the rest.
        let long: Vec<String> =
            (0..LOG_LINES).map(|i| format!("2026-11-24T03:00:00Z  INFO line {i} {}", "y".repeat(300))).collect();
        let r = report(&facts(None, long));
        assert!(r.chars().count() <= REPORT_MAX, "{}", r.chars().count());
        assert!(r.contains(&format!("line {} ", LOG_LINES - 1)), "the newest line is in");
        assert!(r.contains("older lines left out to keep the report short"), "{r}");

        // More lines than a report takes: the newest 150, never more.
        let many: Vec<String> = (0..400).map(|i| format!("2026-11-24T03:00:00Z  INFO short {i}")).collect();
        let r = report(&facts(None, many));
        assert!(r.contains("Last 150 log lines") && !r.contains("older lines left out"), "{r}");
        assert!(r.contains("INFO short 250\n") && !r.contains("INFO short 249\n"), "{r}");
    }

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
            aa: vec![],
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
        assert!(!r.contains("\"AA\" keyboard"), "no section without one");
    }

    #[test]
    fn an_aa_keyboard_gets_its_section() {
        let f = Facts {
            app_name: "Keymotion".into(),
            app_version: "1.3.0".into(),
            time: "t".into(),
            windows: "w".into(),
            webview: "v".into(),
            ui_language: "en".into(),
            context: None,
            keyboards: vec!["AJAZZ AK980 PRO (ajazz-ak980-pro), usb, firmware 3.01".into()],
            aa: vec!["ajazz-ak980-pro.aa on USB 8039, firmware 3.01".into(), "writes: allowed".into()],
            data_files: vec![],
            log_lines: vec![],
        };
        let r = report(&f);
        let at = r.find("\"AA\" keyboard:\n  ajazz-ak980-pro.aa on USB 8039").expect("section");
        assert!(at > r.find("Keyboards:").unwrap() && at < r.find("Data files:").unwrap(), "{r}");
        assert!(r.contains("\n  writes: allowed\n"), "{r}");
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
            aa: vec![],
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
            aa: vec![],
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
