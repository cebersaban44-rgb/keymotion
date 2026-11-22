//! Starting Keymotion when it is not running, without a window.
//!
//! keymotion-cli may itself run inside another packaged app's box: the
//! Claude desktop app's terminal, for one. A program started there as a
//! plain child process stays in that box: its AppData writes land in the
//! other app's folder and it would read settings that are not the user's
//! (measured 2026-11-22: `GetCurrentPackageFullName` says "no package"
//! there, yet `%APPDATA%` is redirected; named pipes do cross the box). So
//! keymotion-cli never starts `keymotion.exe` as its own child. It asks
//! Windows to start it, which starts it outside any box:
//!
//! 1. keymotion-cli runs with Keymotion's package identity (the Store
//!    copy's app execution alias): the package's app, by its AUMID, with
//!    `--background`.
//! 2. The Store copy is installed (the policy of one Keymotion per
//!    computer prefers it): the same, by the Store package's AUMID.
//! 3. The `keymotion:` address scheme is registered (the GitHub copy
//!    registers it): `explorer.exe keymotion://background`. Explorer opens
//!    the address the way a click on a link would.
//! 4. None of these: keymotion-cli cannot start it and says so (69).
//!
//! The app takes `--background` and `keymotion://background` as "run with
//! the tray icon only"; neither counts as a start with Windows, and a
//! running app that gets one shows nothing (`app/src-tauri/src/tray.rs`,
//! `handoff.rs`).

use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::time::{Duration, Instant};

use kb_agent::{AgentError, ErrorCode};

/// The Store package's family name: the same as `handoff.rs`
/// `STORE_FAMILY` (a test compares them).
pub const STORE_FAMILY: &str = "cebersaban.Keymotion_pa98x8fw006yg";
/// The package's application (`store/AppxManifest.xml`, `Application Id`).
pub const APP_ID: &str = "Keymotion";
/// The app's single-instance lock, `<identifier>-sim` (`handoff.rs`
/// `INSTANCE_LOCK`): held while any copy of Keymotion runs.
pub const INSTANCE_LOCK: &str = "dev.keymotion-sim";
/// The app's address scheme (`updates.rs` `SCHEME`).
pub const SCHEME: &str = "keymotion";
/// The app's argument and address for "run without a window, for an agent"
/// (`tray.rs` `ARG_BACKGROUND`, `updates.rs` `Link::Background`).
pub const ARG_BACKGROUND: &str = "--background";
pub const BACKGROUND_LINK: &str = "keymotion://background";

/// How long a started app may take to open its pipe.
pub const START_WAIT: Duration = Duration::from_secs(15);
/// How long to wait for the pipe of an app that already runs (it may be
/// starting).
pub const RUNNING_WAIT: Duration = Duration::from_secs(5);

/// What decides how to start the app.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// keymotion-cli runs from Keymotion's package: its family name.
    pub own_family: Option<String>,
    /// The Store copy is installed for this user.
    pub store_installed: bool,
    /// The program `keymotion:` addresses open, when it exists.
    pub scheme_exe: Option<PathBuf>,
}

/// How to start the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// Activate a packaged app by its AUMID with `--background`.
    Package { aumid: String, own: bool },
    /// `explorer.exe keymotion://background`.
    Link { exe: PathBuf },
    /// Tests: start nothing (`KEYMOTION_CLI_TEST_PIPE`).
    Test,
}

impl Method {
    /// For `start --json` and `doctor`.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Package { own: true, .. } => "package",
            Self::Package { own: false, .. } => "store_package",
            Self::Link { .. } => "link",
            Self::Test => "test",
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Package { aumid, own: true } => format!("its own package, {aumid}, with {ARG_BACKGROUND}"),
            Self::Package { aumid, own: false } => format!("the Microsoft Store copy, {aumid}, with {ARG_BACKGROUND}"),
            Self::Link { exe } => format!("explorer.exe {BACKGROUND_LINK} (opens {})", exe.display()),
            Self::Test => "nothing (a test)".to_string(),
        }
    }
}

/// The first way that works ([module docs](self)).
pub fn choose(f: &Facts) -> Result<Method, AgentError> {
    if let Some(family) = &f.own_family {
        return Ok(Method::Package { aumid: format!("{family}!{APP_ID}"), own: true });
    }
    if f.store_installed {
        return Ok(Method::Package { aumid: format!("{STORE_FAMILY}!{APP_ID}"), own: false });
    }
    if let Some(exe) = &f.scheme_exe {
        return Ok(Method::Link { exe: exe.clone() });
    }
    Err(AgentError::new(
        ErrorCode::AppNotRunning,
        "Keymotion is not running, and keymotion-cli found no way to start it.",
    )
    .with_hint("Start Keymotion, or turn on Settings > Background > Start with Windows, then try again."))
}

/// The facts on this computer.
pub fn facts() -> Facts {
    Facts {
        own_family: kb_winsys::package::current().map(|p| p.family_name),
        store_installed: kb_winsys::package::installed(STORE_FAMILY),
        scheme_exe: kb_winsys::protocol::command(SCHEME).as_deref().and_then(program_of).filter(|p| p.is_file()),
    }
}

/// The program of a command line: `"C:\x\keymotion.exe" "%1"` →
/// `C:\x\keymotion.exe`.
pub fn program_of(command: &str) -> Option<PathBuf> {
    let c = command.trim();
    let program = match c.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => c.split_whitespace().next()?,
    };
    (!program.is_empty()).then(|| PathBuf::from(program))
}

/// Whether a copy of Keymotion runs, by its single-instance lock (whether
/// or not its pipe is open yet).
pub fn app_running() -> bool {
    kb_winsys::copies::running(INSTANCE_LOCK)
}

/// Ask Windows to start the app. Returns once the request is made, not
/// when the app is up.
pub fn start(method: &Method) -> Result<(), AgentError> {
    let failed = |why: String| {
        AgentError::new(ErrorCode::AppNotRunning, format!("Keymotion is not running and did not start: {why}."))
            .with_hint("Start Keymotion, then try again; keymotion-cli doctor shows more.")
    };
    match method {
        Method::Test => Ok(()),
        Method::Package { aumid, .. } => {
            kb_winsys::package::activate(aumid, ARG_BACKGROUND).map(|_| ()).map_err(failed)
        }
        Method::Link { .. } => {
            // No handle of this process reaches Explorer: an agent reading
            // this program's output would otherwise wait for Explorer too.
            let mut child = Process::new(explorer())
                .arg(BACKGROUND_LINK)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| failed(format!("explorer.exe did not run ({e})")))?;
            // Explorer hands the address to the shell and ends; its exit
            // code says nothing.
            let until = Instant::now() + Duration::from_secs(3);
            while Instant::now() < until && matches!(child.try_wait(), Ok(None)) {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }
    }
}

/// `%SystemRoot%\explorer.exe`: by its full path, never through PATH.
fn explorer() -> PathBuf {
    let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("windir"));
    root.map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from).join("explorer.exe")
}

/// Where the Store copy's app execution aliases are, for `doctor`.
pub fn alias_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| Path::new(&l).join("Microsoft").join("WindowsApps"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_own_package_comes_first_then_the_store_copy_then_the_link() {
        let all = Facts {
            own_family: Some("Test.Keymotion_abc".into()),
            store_installed: true,
            scheme_exe: Some(PathBuf::from(r"C:\Keymotion\keymotion.exe")),
        };
        assert_eq!(choose(&all).unwrap(), Method::Package { aumid: "Test.Keymotion_abc!Keymotion".into(), own: true });
        let store = Facts { own_family: None, ..all.clone() };
        assert_eq!(
            choose(&store).unwrap(),
            Method::Package { aumid: "cebersaban.Keymotion_pa98x8fw006yg!Keymotion".into(), own: false }
        );
        let github = Facts { store_installed: false, ..store };
        assert_eq!(choose(&github).unwrap(), Method::Link { exe: PathBuf::from(r"C:\Keymotion\keymotion.exe") });
        let e = choose(&Facts::default()).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));
        assert!(e.hint.is_some());
    }

    #[test]
    fn the_scheme_command_names_its_program() {
        assert_eq!(
            program_of(r#""C:\Users\A B\AppData\Local\Keymotion\keymotion.exe" "%1""#),
            Some(PathBuf::from(r"C:\Users\A B\AppData\Local\Keymotion\keymotion.exe"))
        );
        assert_eq!(program_of(r"C:\K\keymotion.exe %1"), Some(PathBuf::from(r"C:\K\keymotion.exe")));
        assert_eq!(program_of(""), None);
        assert_eq!(program_of(r#""""#), None);
    }

    #[test]
    fn explorer_is_named_by_its_full_path() {
        let e = explorer();
        assert!(e.is_absolute() && e.ends_with("explorer.exe"), "{}", e.display());
    }

    /// The names the app uses, read from its source, so the two cannot
    /// drift apart.
    #[test]
    fn the_names_match_the_apps() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/src-tauri/src");
        let read = |file: &str| std::fs::read_to_string(src.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let handoff = read("handoff.rs");
        assert!(handoff.contains(&format!("pub const STORE_FAMILY: &str = \"{STORE_FAMILY}\";")));
        assert!(handoff.contains(&format!("const STORE_APP: &str = \"{APP_ID}\";")));
        assert!(handoff.contains(&format!("const INSTANCE_LOCK: &str = \"{INSTANCE_LOCK}\";")));
        assert!(read("tray.rs").contains(&format!("pub const ARG_BACKGROUND: &str = \"{ARG_BACKGROUND}\";")));
        let updates = read("updates.rs");
        assert!(updates.contains(&format!("pub const SCHEME: &str = \"{SCHEME}\";")));
        assert!(updates.contains("\"background\" => Link::Background"));
        assert_eq!(BACKGROUND_LINK, format!("{SCHEME}://background"));
    }
}
