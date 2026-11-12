//! The app's log. The installed app has no console, so besides the console
//! (development builds) every line also goes to a file in the app's log
//! folder (Tauri's `app_log_dir`: `%LOCALAPPDATA%\<identifier>\logs`).
//!
//! * One file per local day, `YYYY-MM-DD.log`. The newest [`KEEP_FILES`]
//!   are kept; a file stops growing at [`MAX_FILE_BYTES`] (a last line says
//!   so), so the folder never holds more than a few tens of megabytes.
//! * Every line is redacted before it is written ([`redact`]): web addresses
//!   keep only their host (a calendar address is a secret) and the Windows
//!   user name in paths is replaced. Code never logs reminder titles or
//!   meeting names in the first place.
//! * A panic writes a short crash note (`crash-….txt`) next to the logs:
//!   version, time, thread, message and source location. The next start
//!   offers the newest one once for reporting ([`unseen_crash`]).
//! * Lines logged before the folder is known (start-up) wait in memory.

use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Daily log files kept.
pub const KEEP_FILES: usize = 7;
/// A day's file stops growing here.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Start-up lines held until the folder is known.
const PENDING_MAX: usize = 256 * 1024;
/// Crash notes kept.
const KEEP_CRASHES: usize = 10;
/// Longest panic message written to a crash note.
const CRASH_MESSAGE_MAX: usize = 2_000;
/// Crash notes older than this are not offered for reporting any more.
const CRASH_OFFER_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Holds the name of the newest crash note offered so far.
const CRASH_SEEN: &str = "crash.seen";
const FULL_NOTE: &str = "-- today's log reached its size limit; later lines are left out --\n";

struct OpenFile {
    date: String,
    file: File,
    size: u64,
    full: bool,
}

struct Sink {
    dir: Option<PathBuf>,
    pending: Vec<u8>,
    open: Option<OpenFile>,
}

static SINK: Mutex<Sink> = Mutex::new(Sink { dir: None, pending: Vec::new(), open: None });
/// Log folder and app version, for crash notes.
static CRASH: OnceLock<(PathBuf, String)> = OnceLock::new();
/// When this run started writing logs: crash notes from before it are
/// from an earlier run.
static STARTED: OnceLock<SystemTime> = OnceLock::new();

thread_local! {
    /// This thread is inside the sink: a panic there must not log again.
    static IN_SINK: Cell<bool> = const { Cell::new(false) };
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn log_name(date: &str) -> String {
    format!("{date}.log")
}

/// `YYYY-MM-DD.log`: a daily file of this module.
fn is_log_name(name: &str) -> bool {
    let Some(date) = name.strip_suffix(".log") else { return false };
    let b = date.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

fn is_crash_name(name: &str) -> bool {
    name.starts_with("crash-") && name.ends_with(".txt")
}

/// Remove all but the newest `keep` files whose names `matches` (names sort
/// by date).
fn prune(dir: &Path, keep: usize, matches: fn(&str) -> bool) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut names: Vec<String> =
        entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| matches(n)).collect();
    names.sort();
    let extra = names.len().saturating_sub(keep);
    for name in &names[..extra] {
        let _ = fs::remove_file(dir.join(name));
    }
}

impl Sink {
    fn write(&mut self, text: &[u8], date: &str) {
        let Some(dir) = self.dir.clone() else {
            if self.pending.len() + text.len() <= PENDING_MAX {
                self.pending.extend_from_slice(text);
            }
            return;
        };
        if self.open.as_ref().is_none_or(|o| o.date != date) {
            self.open = None;
            let path = dir.join(log_name(date));
            if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
                let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                self.open = Some(OpenFile { date: date.to_string(), file, size, full: size >= MAX_FILE_BYTES });
            }
            prune(&dir, KEEP_FILES, is_log_name);
        }
        let Some(open) = self.open.as_mut() else { return };
        if open.full {
            return;
        }
        if open.size + text.len() as u64 > MAX_FILE_BYTES {
            let _ = open.file.write_all(FULL_NOTE.as_bytes());
            open.full = true;
            return;
        }
        if open.file.write_all(text).is_ok() {
            open.size += text.len() as u64;
        }
    }
}

/// Redact one formatted line and append it to today's file.
fn write_line(bytes: &[u8]) {
    let text = redact(&String::from_utf8_lossy(bytes));
    IN_SINK.with(|f| f.set(true));
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    sink.write(text.as_bytes(), &today());
    drop(sink);
    IN_SINK.with(|f| f.set(false));
}

/// `MakeWriter` for the file layer: each event is formatted into one
/// buffer and handed to the sink as a whole line when the writer drops.
#[derive(Clone, Copy)]
struct FileWriter;

struct LineWriter(Vec<u8>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileWriter {
    type Writer = LineWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LineWriter(Vec::with_capacity(256))
    }
}

impl Write for LineWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for LineWriter {
    fn drop(&mut self) {
        if !self.0.is_empty() {
            write_line(&self.0);
        }
    }
}

/// Install the global subscriber (console in development builds, the daily
/// file always) and the panic hook. `RUST_LOG` overrides the `info` level.
pub fn init() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let console = cfg!(debug_assertions).then(tracing_subscriber::fmt::layer);
    let file = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(FileWriter);
    tracing_subscriber::registry().with(filter).with(console).with(file).init();
    install_panic_hook();
}

/// Start writing to `dir` (created if needed): lines that waited since
/// start-up go first. Old files beyond the limits are removed.
pub fn set_dir(dir: &Path, version: &str) {
    if let Err(e) = fs::create_dir_all(dir) {
        tracing::warn!("log folder not available: {e}");
        return;
    }
    let _ = CRASH.set((dir.to_path_buf(), version.to_string()));
    STARTED.get_or_init(SystemTime::now);
    prune(dir, KEEP_CRASHES, is_crash_name);
    let mut sink = SINK.lock().unwrap_or_else(|e| e.into_inner());
    sink.dir = Some(dir.to_path_buf());
    let pending = std::mem::take(&mut sink.pending);
    if !pending.is_empty() {
        sink.write(&pending, &today());
    }
}

/// The last `n` lines of the newest log files (the previous day's when
/// today's has fewer), redacted again for safety.
pub fn recent_lines(dir: &Path, n: usize) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> =
        entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| is_log_name(n)).collect();
    names.sort();
    let mut out: Vec<String> = Vec::new();
    for name in names.iter().rev() {
        let Ok(bytes) = fs::read(dir.join(name)) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let mut lines: Vec<String> = text.lines().filter(|l| !l.trim().is_empty()).map(redact).collect();
        lines.append(&mut out);
        out = lines;
        if out.len() >= n {
            break;
        }
    }
    let skip = out.len().saturating_sub(n);
    out.split_off(skip)
}

// ------------------------------------------------------------------ panics

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(no message)".to_string()
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = panic_message(info.payload());
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        // A panic inside the sink must not wait for the sink again.
        if !IN_SINK.with(Cell::get) {
            tracing::error!(%thread, location = %redact(&location), "panic: {}", redact(&message));
        }
        if let Some((dir, version)) = CRASH.get() {
            let now = chrono::Local::now();
            let note = crash_note(version, &now.to_rfc3339(), &thread, &location, &message);
            let path = dir.join(format!("crash-{}.txt", now.format("%Y-%m-%d-%H%M%S")));
            let _ = fs::write(path, note);
        }
        previous(info);
    }));
}

/// The text of a crash note (redacted, message shortened).
fn crash_note(version: &str, time: &str, thread: &str, location: &str, message: &str) -> String {
    let message: String = redact(message).chars().take(CRASH_MESSAGE_MAX).collect();
    format!(
        "Crash note\nVersion: {version}\nTime: {time}\nThread: {thread}\nLocation: {}\nMessage: {message}\n",
        redact(location)
    )
}

/// The newest crash note from an earlier run that wasn't offered yet and is
/// at most a week old: its file name and text (redacted).
pub fn unseen_crash(dir: &Path) -> Option<(String, String)> {
    let seen = fs::read_to_string(dir.join(CRASH_SEEN)).ok();
    let before = STARTED.get().copied().unwrap_or_else(SystemTime::now);
    let name = newest_unseen_crash(dir, seen.as_deref().map(str::trim), before)?;
    let text = fs::read_to_string(dir.join(&name)).ok()?;
    Some((name, redact(&text)))
}

fn newest_unseen_crash(dir: &Path, seen: Option<&str>, before: SystemTime) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;
    entries
        .filter_map(|e| {
            let e = e.ok()?;
            let name = e.file_name().into_string().ok()?;
            if !is_crash_name(&name) || seen.is_some_and(|s| name.as_str() <= s) {
                return None;
            }
            let written = e.metadata().ok()?.modified().ok()?;
            let age = before.duration_since(written).ok()?;
            (age <= CRASH_OFFER_AGE).then_some(name)
        })
        .max()
}

/// Remember that the crash note `name` was offered, with the ones before it.
pub fn mark_crash_seen(dir: &Path, name: &str) -> io::Result<()> {
    if !is_crash_name(name) || name.contains(['/', '\\']) || name.contains("..") {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a crash note name"));
    }
    let seen = fs::read_to_string(dir.join(CRASH_SEEN)).unwrap_or_default();
    if seen.trim() >= name {
        return Ok(());
    }
    fs::write(dir.join(CRASH_SEEN), name)
}

// --------------------------------------------------------------- redaction

const URL_SCHEMES: [&str; 4] = ["http", "https", "webcal", "webcals"];
/// Longest Windows user folder name taken as one (with spaces).
const USER_NAME_MAX: usize = 64;

fn url_end(c: char) -> bool {
    c.is_whitespace() || c.is_control() || matches!(c, '"' | '\'' | '<' | '>' | '`' | '\\')
}

/// Web addresses keep their scheme and host; the path, query and any
/// `user:password@` are replaced (a calendar's address is its password).
fn redact_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(sep) = rest.find("://") {
        let before = &rest[..sep];
        let scheme_start = before
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            .last()
            .map_or(sep, |(i, _)| i);
        let scheme = &before[scheme_start..];
        let after = &rest[sep + 3..];
        if !URL_SCHEMES.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
            out.push_str(&rest[..sep + 3]);
            rest = after;
            continue;
        }
        let end = after.find(url_end).unwrap_or(after.len());
        let url = &after[..end];
        let authority_end = url.find(['/', '?', '#']).unwrap_or(url.len());
        let host = url[..authority_end].rsplit('@').next().unwrap_or("");
        out.push_str(&rest[..sep + 3]);
        out.push_str(host);
        if authority_end < url.len() || url[..authority_end].contains('@') {
            out.push_str("/[redacted]");
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// `C:\Users\<name>\…` (also with doubled backslashes from debug output),
/// `/Users/<name>/…` and `/home/<name>/…`: the user name is replaced.
fn redact_user_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' || c == b'/' {
            let sep_end = i + bytes[i..].iter().take_while(|b| **b == b'\\' || **b == b'/').count();
            let word = ["users", "home"].into_iter().find(|w| {
                let end = sep_end + w.len();
                end < bytes.len()
                    && text.is_char_boundary(end)
                    && text[sep_end..end].eq_ignore_ascii_case(w)
                    && (bytes[end] == b'\\' || bytes[end] == b'/')
                    && (*w == "users" || c == b'/')
            });
            if let Some(w) = word {
                let name_start = sep_end + w.len();
                let name_start =
                    name_start + bytes[name_start..].iter().take_while(|b| **b == b'\\' || **b == b'/').count();
                // A user name may hold spaces ("Ayşe Yılmaz"): it ends at the
                // next separator, unless that is far away (not a path then).
                let tail = &text[name_start..];
                let hard =
                    |ch: char| matches!(ch, '\\' | '/' | ':' | '"' | '\'' | '<' | '>' | '|' | '`') || ch.is_control();
                let name_len = match tail.find(hard) {
                    Some(n) if n <= USER_NAME_MAX => n,
                    _ => tail.find(|ch: char| hard(ch) || ch.is_whitespace()).unwrap_or(tail.len()),
                };
                if name_len > 0 {
                    out.push_str(&text[i..name_start]);
                    out.push_str("<user>");
                    i = name_start + name_len;
                    continue;
                }
            }
            out.push_str(&text[i..sep_end]);
            i = sep_end;
            continue;
        }
        let ch_len = text[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    out
}

/// What may be written to the log or copied into a diagnostics report.
pub fn redact(text: &str) -> String {
    redact_user_paths(&redact_urls(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("klavye-logging-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_newest_unseen_crash_from_before_this_run_is_offered_once() {
        let dir = temp_dir("crash-offer");
        let now = SystemTime::now();
        assert_eq!(newest_unseen_crash(&dir, None, now), None);
        fs::write(dir.join("crash-2026-11-10-090000.txt"), "Crash note\n").unwrap();
        fs::write(dir.join("crash-2026-11-11-140322.txt"), "Crash note\nMessage: boom\n").unwrap();
        fs::write(dir.join("2026-11-11.log"), "x").unwrap();
        // Written before this run started: offered.
        let later = now + Duration::from_secs(5);
        assert_eq!(newest_unseen_crash(&dir, None, later).as_deref(), Some("crash-2026-11-11-140322.txt"));
        // Written during this run: not "last time".
        assert_eq!(newest_unseen_crash(&dir, None, now - Duration::from_secs(60)), None);
        // Too old.
        assert_eq!(newest_unseen_crash(&dir, None, now + CRASH_OFFER_AGE + Duration::from_secs(60)), None);

        mark_crash_seen(&dir, "crash-2026-11-11-140322.txt").unwrap();
        let seen = fs::read_to_string(dir.join(CRASH_SEEN)).unwrap();
        assert_eq!(newest_unseen_crash(&dir, Some(&seen), later), None);
        // An older name doesn't move the mark back.
        mark_crash_seen(&dir, "crash-2026-11-10-090000.txt").unwrap();
        assert_eq!(fs::read_to_string(dir.join(CRASH_SEEN)).unwrap(), "crash-2026-11-11-140322.txt");
        assert!(mark_crash_seen(&dir, "../crash-x.txt").is_err());
        assert!(mark_crash_seen(&dir, "notes.txt").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn addresses_keep_only_their_host() {
        assert_eq!(
            redact("reading https://calendar.google.com/calendar/ical/x%40gmail.com/private-abc/basic.ics now"),
            "reading https://calendar.google.com/[redacted] now"
        );
        assert_eq!(
            redact("webcal://p52-caldav.icloud.com/published/2/abc"),
            "webcal://p52-caldav.icloud.com/[redacted]"
        );
        assert_eq!(redact("WEBCALS://Host.example/x?y#z, then"), "WEBCALS://Host.example/[redacted] then");
        assert_eq!(redact("https://user:secret@host.example"), "https://host.example/[redacted]");
        assert_eq!(redact("url=\"https://a.example/p?q=1\" ok"), "url=\"https://a.example/[redacted]\" ok");
        assert_eq!(redact("see https://example.org"), "see https://example.org", "a bare host stays");
        assert_eq!(redact("tauri://localhost/x and ipc://y"), "tauri://localhost/x and ipc://y", "other schemes stay");
        assert_eq!(redact("no address here: 14:00 · İş"), "no address here: 14:00 · İş");
    }

    #[test]
    fn user_names_leave_paths() {
        assert_eq!(
            redact(r"opened C:\Users\someone\AppData\Roaming\x\calendar.json"),
            r"opened C:\Users\<user>\AppData\Roaming\x\calendar.json"
        );
        assert_eq!(redact(r#"path: "C:\\Users\\Ayşe Yılmaz\\AppData""#), r#"path: "C:\\Users\\<user>\\AppData""#);
        assert_eq!(
            redact("/home/ali/.config and /Users/ayşe/Library"),
            "/home/<user>/.config and /Users/<user>/Library"
        );
        assert_eq!(redact(r"C:\users\x"), r"C:\users\<user>");
        assert_eq!(redact("going home/away"), "going home/away");
        assert_eq!(redact(r"C:\Users\"), r"C:\Users\");
        assert_eq!(redact(r"in C:\Users\bob now"), r"in C:\Users\<user> now", "no separator: up to the space");
    }

    #[test]
    fn only_daily_files_count_as_logs() {
        assert!(is_log_name("2026-11-11.log"));
        assert!(!is_log_name("2026-11-1.log"));
        assert!(!is_log_name("notes.log"));
        assert!(!is_log_name("2026-11-11.txt"));
        assert!(is_crash_name("crash-2026-11-11-140322.txt"));
    }

    #[test]
    fn files_rotate_by_day_and_stop_at_the_limit() {
        let dir = temp_dir("rotate");
        let mut sink = Sink { dir: Some(dir.clone()), pending: Vec::new(), open: None };
        for day in 1..=9 {
            sink.write(b"line\n", &format!("2026-11-{day:02}"));
        }
        let mut names: Vec<String> =
            fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names.len(), KEEP_FILES);
        assert_eq!(names[0], "2026-11-03.log", "the oldest went");

        let big = vec![b'x'; (MAX_FILE_BYTES / 2) as usize];
        for _ in 0..3 {
            sink.write(&big, "2026-11-09");
        }
        let text = fs::read_to_string(dir.join("2026-11-09.log")).unwrap();
        assert!(text.len() as u64 <= MAX_FILE_BYTES + FULL_NOTE.len() as u64);
        assert!(text.ends_with(FULL_NOTE));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_up_lines_wait_and_recent_lines_span_days() {
        let dir = temp_dir("recent");
        let mut sink = Sink { dir: None, pending: Vec::new(), open: None };
        sink.write(b"early\n", "2026-11-10");
        assert!(fs::read_dir(&dir).unwrap().next().is_none(), "nothing written before the folder is known");
        sink.dir = Some(dir.clone());
        let pending = std::mem::take(&mut sink.pending);
        sink.write(&pending, "2026-11-10");
        for i in 0..3 {
            sink.write(format!("yesterday {i}\n").as_bytes(), "2026-11-10");
        }
        for i in 0..2 {
            sink.write(format!("today {i} https://x.example/secret\n").as_bytes(), "2026-11-11");
        }
        let lines = recent_lines(&dir, 4);
        assert_eq!(
            lines,
            [
                "yesterday 1",
                "yesterday 2",
                "today 0 https://x.example/[redacted]",
                "today 1 https://x.example/[redacted]"
            ]
        );
        assert_eq!(recent_lines(&dir, 100).first().map(String::as_str), Some("early"));
        assert!(recent_lines(&dir.join("missing"), 5).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn crash_notes_are_short_and_redacted() {
        let note = crash_note(
            "0.1.0",
            "2026-11-11T14:03:22+03:00",
            "device-actor",
            r"C:\Users\someone\.cargo\registry\src\x.rs:1:2",
            "bad https://calendar.google.com/private-abc",
        );
        assert!(note.contains("Version: 0.1.0"));
        assert!(note.contains("Thread: device-actor"));
        assert!(note.contains(r"C:\Users\<user>\.cargo"));
        assert!(!note.contains("private-abc") && !note.contains("someone"), "{note}");
        let long = crash_note("1", "t", "main", "", &"x".repeat(10_000));
        assert!(long.len() < 2_200);
    }
}
