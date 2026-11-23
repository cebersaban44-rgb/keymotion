//! `doctor`: what stands between an agent and the keyboard's lights. It
//! reports and never fixes, and exits 0 whatever it finds.
//!
//! Checks: this program; whether Keymotion runs and answers; which process
//! serves the pipe (expected: `keymotion.exe`) and that the pipe is this
//! user's; versions and protocol; agent access; the keyboard and its USB
//! cable; how keymotion-cli would start the app; whether `keymotion-cli` is
//! on PATH and in the Store's alias folder; and whether this process runs
//! inside another app's package box (a small probe file written under
//! `%APPDATA%`, looked for under `%LOCALAPPDATA%\Packages\*\LocalCache\
//! Roaming`, deleted again), since Windows' own package API does not show
//! such a box.

use std::path::{Path, PathBuf};

use kb_agent::ErrorCode;
use serde_json::{json, Value};

use crate::cli;
use crate::launch;
use crate::output::Done;
use crate::session::{self, Ctx, IfNotRunning};

/// The program the pipe's server should be.
const APP_EXE: &str = "keymotion.exe";
const CLI_EXE: &str = "keymotion-cli.exe";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    /// Something to look at.
    Warn,
    /// Something that stops agents.
    Fail,
    /// For the record.
    Info,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
            Self::Info => "info",
        }
    }
}

#[derive(Default)]
struct Report {
    checks: Vec<(&'static str, Status, String)>,
}

impl Report {
    fn add(&mut self, id: &'static str, status: Status, detail: impl Into<String>) {
        self.checks.push((id, status, detail.into()));
    }

    fn done(self) -> Done {
        let problems = self.checks.iter().filter(|(_, s, _)| matches!(s, Status::Warn | Status::Fail)).count();
        let checks: Vec<Value> = self
            .checks
            .iter()
            .map(|(id, status, detail)| json!({ "id": id, "status": status.as_str(), "detail": detail }))
            .collect();
        let data = json!({ "checks": checks, "problems": problems });
        let text = crate::human::doctor(&data);
        Done::new(data, text)
    }
}

pub fn run(ctx: &Ctx) -> Done {
    let mut r = Report::default();
    let exe = std::env::current_exe().ok();
    r.add(
        "cli",
        Status::Ok,
        format!(
            "keymotion-cli {} (protocol {}), {}",
            crate::VERSION,
            kb_agent::PROTOCOL_VERSION,
            exe.as_deref().map_or_else(|| "its path unknown".to_string(), |p| p.display().to_string())
        ),
    );
    app(ctx, &mut r);
    match ctx.start_method() {
        Ok(method) => r.add(
            "start",
            Status::Info,
            format!("when Keymotion is not running, commands start it through {}", method.describe()),
        ),
        Err(e) => r.add("start", Status::Warn, e.message),
    }
    path(exe.as_deref(), &mut r);
    package(&mut r);
    r.done()
}

/// The app, its pipe, versions, access and the keyboard.
fn app(ctx: &Ctx, r: &mut Report) {
    let mut conn = match session::connect(ctx, IfNotRunning::Report) {
        Ok(conn) => conn,
        Err(e) if e.code == ErrorCode::AppNotRunning => {
            if ctx.app_running() {
                r.add(
                    "app",
                    Status::Warn,
                    "Keymotion runs but its agent pipe is not open (starting, or a version without agent support)",
                );
            } else {
                r.add("app", Status::Warn, "Keymotion is not running");
            }
            return;
        }
        Err(e) => {
            let mut detail = e.message.clone();
            if let Some(hint) = &e.hint {
                detail = format!("{detail} {hint}");
            }
            r.add("app", Status::Fail, detail);
            return;
        }
    };
    let a = conn.app().clone();
    r.add("app", Status::Ok, format!("Keymotion {} ({}) answers, protocol {}", a.version, channel(&a), a.protocol));
    pipe_server(&conn.client, r);
    if a.version == crate::VERSION {
        r.add("versions", Status::Ok, format!("Keymotion and keymotion-cli are both {}", a.version));
    } else {
        r.add(
            "versions",
            Status::Warn,
            format!(
                "Keymotion is {}, keymotion-cli {}: they work together (protocol {}), but update the older one",
                a.version,
                crate::VERSION,
                a.protocol
            ),
        );
    }
    let left = conn.left();
    let status = match conn.client.call_within(&cli::status_command(), left) {
        Ok(status) => status,
        Err(e) => {
            r.add("status", Status::Fail, format!("status failed: {}", e.message));
            return;
        }
    };
    if status["agents"]["enabled"] == json!(true) {
        r.add("access", Status::Ok, "agent access is on");
    } else {
        r.add("access", Status::Warn, "agent access is off: the user turns it on in Keymotion > Tools > AI agents");
    }
    keyboard(&status, r);
}

fn channel(a: &kb_agent::HelloData) -> String {
    serde_json::to_value(a.channel).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default()
}

/// Which process serves the pipe, and whose pipe it is.
fn pipe_server(client: &kb_agent::transport::Client, r: &mut Report) {
    match client.server_process_id() {
        Some(pid) => {
            let path = kb_winsys::foreground::image_path(pid);
            let name = path.as_deref().map(kb_winsys::foreground::file_name).unwrap_or("");
            if name.eq_ignore_ascii_case(APP_EXE) {
                r.add("pipe", Status::Ok, format!("served by {APP_EXE} (process {pid})"));
            } else if name.is_empty() {
                r.add("pipe", Status::Warn, format!("served by process {pid}, whose program could not be read"));
            } else {
                r.add("pipe", Status::Warn, format!("served by {name} (process {pid}), not {APP_EXE}"));
            }
        }
        None => r.add("pipe", Status::Warn, "the process serving the pipe could not be read"),
    }
    let user = kb_winsys::pipe::current_user_sid().ok();
    match (client.pipe_owner_sid(), user) {
        (Some(owner), Some(user)) if owner == user => {
            r.add("owner", Status::Ok, "the pipe belongs to this Windows user")
        }
        (Some(owner), _) => r.add("owner", Status::Fail, format!("the pipe belongs to {owner}, not to this user")),
        (None, _) => r.add("owner", Status::Warn, "the pipe's owner could not be read"),
    }
}

fn keyboard(status: &Value, r: &mut Report) {
    let keyboards = status["keyboards"].as_array().map(Vec::as_slice).unwrap_or_default();
    let text = |k: &Value, key: &str| k[key].as_str().unwrap_or("").to_string();
    match keyboards.iter().find(|k| k["lights"] == json!(true)) {
        Some(k) => r.add("keyboard", Status::Ok, format!("{} on the USB cable: lights work", text(k, "name"))),
        None => match keyboards.first() {
            Some(k) => r.add(
                "keyboard",
                Status::Warn,
                format!("{} over {}: lights need the USB cable", text(k, "name"), text(k, "connection")),
            ),
            None => r.add("keyboard", Status::Warn, "no keyboard is connected"),
        },
    }
    match status["lights"].as_str() {
        Some("game") => r.add("lights", Status::Info, "agents' lights wait while a game wants the keyboard left alone"),
        Some("night_dim" | "night_off") => r.add("lights", Status::Info, "the user's night quiet is on now"),
        _ => {}
    }
}

/// `keymotion-cli` on PATH, and the Store's alias.
fn path(exe: Option<&Path>, r: &mut Report) {
    let found = std::env::var_os("PATH")
        .and_then(|p| std::env::split_paths(&p).map(|dir| dir.join(CLI_EXE)).find(|c| c.symlink_metadata().is_ok()));
    let same = |a: &Path, b: &Path| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy());
    match (found, exe) {
        (Some(found), Some(exe)) if same(&found, exe) => {
            r.add("path", Status::Ok, format!("keymotion-cli on PATH is this one, {}", found.display()))
        }
        (Some(found), _) => r.add("path", Status::Ok, format!("keymotion-cli on PATH: {}", found.display())),
        (None, _) => r.add(
            "path",
            Status::Warn,
            "keymotion-cli is not on PATH: agents must call it by its full path (a new terminal may be needed after installing)",
        ),
    }
    if let Some(alias) = launch::alias_dir().map(|d| d.join(CLI_EXE)).filter(|a| a.symlink_metadata().is_ok()) {
        r.add("alias", Status::Info, format!("the Microsoft Store copy's alias: {}", alias.display()));
    }
}

/// The package identity, and the box a calling app may keep this process in.
fn package(r: &mut Report) {
    match kb_winsys::package::identity() {
        Some(p) if kb_winsys::package::current().is_some() => {
            r.add("package", Status::Info, format!("runs from Keymotion's package {}", p.full_name))
        }
        Some(p) => r.add("package", Status::Info, format!("runs with another app's package identity: {}", p.full_name)),
        None => r.add("package", Status::Info, "runs without a package identity"),
    }
    match box_probe() {
        Ok(None) => r.add("box", Status::Ok, "files written to AppData stay where they are written: not inside another app's box"),
        Ok(Some(family)) => r.add(
            "box",
            Status::Info,
            format!(
                "runs inside {family}'s box: its AppData writes land in that app's folder. keymotion-cli never starts Keymotion as its own child, so Keymotion stays outside it"
            ),
        ),
        Err(e) => r.add("box", Status::Info, format!("could not tell whether it runs inside another app's box: {e}")),
    }
}

/// Write a probe under `%APPDATA%` and look for it in every package's
/// redirected Roaming folder: the package family whose folder got it.
fn box_probe() -> Result<Option<String>, String> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from).ok_or_else(|| format!("{name} is not set"));
    let (roaming, local) = (var("APPDATA")?, var("LOCALAPPDATA")?);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let name = format!("keymotion-cli-probe-{}-{nanos}.tmp", std::process::id());
    let probe = roaming.join(&name);
    std::fs::write(&probe, b"keymotion-cli doctor: a probe, deleted right away")
        .map_err(|e| format!("no probe written to {}: {e}", roaming.display()))?;
    let packages = local.join("Packages");
    let found = std::fs::read_dir(&packages).ok().and_then(|entries| {
        entries.flatten().find_map(|entry| {
            let copy = entry.path().join("LocalCache").join("Roaming").join(&name);
            copy.symlink_metadata().is_ok().then(|| (entry.file_name().to_string_lossy().into_owned(), copy))
        })
    });
    let _ = std::fs::remove_file(&probe);
    if let Some((_, copy)) = &found {
        // Removed through the box already; this only catches a leftover.
        let _ = std::fs::remove_file(copy);
    }
    Ok(found.map(|(family, _)| family))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_leaves_nothing_behind() {
        let before = std::env::var_os("APPDATA").map(|r| {
            std::fs::read_dir(r)
                .map(|d| {
                    d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("keymotion-cli-probe-")).count()
                })
                .unwrap_or(0)
        });
        // Whatever box the tests run in, the probe answers.
        assert!(box_probe().is_ok());
        let after = std::env::var_os("APPDATA").map(|r| {
            std::fs::read_dir(r)
                .map(|d| {
                    d.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("keymotion-cli-probe-")).count()
                })
                .unwrap_or(0)
        });
        assert_eq!(before, after);
    }

    #[test]
    fn problems_count_warnings_and_failures() {
        let mut r = Report::default();
        r.add("a", Status::Ok, "fine");
        r.add("b", Status::Warn, "look");
        r.add("c", Status::Fail, "broken");
        r.add("d", Status::Info, "note");
        let done = r.done();
        assert_eq!(done.data["problems"], json!(2));
        assert_eq!(done.data["checks"][1], json!({"id": "b", "status": "warn", "detail": "look"}));
        assert!(done.text.ends_with("2 problems to look at."));
    }
}
