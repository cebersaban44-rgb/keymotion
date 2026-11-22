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
//! 2. The Store copy is installed and new enough (the policy of one
//!    Keymotion per computer prefers it): the same, by the Store package's
//!    AUMID. New enough: at least `kb_agent::BACKGROUND_START_SINCE` and
//!    this CLI's version. An older Store copy would open its window and has
//!    no pipe, so it is never started.
//! 3. The `keymotion:` address scheme opens the installed GitHub copy (which
//!    registers it; a development build's own registration does not count,
//!    see `installed_link_program`): `explorer.exe keymotion://background`, with none of
//!    this process's handles. Explorer opens the address the way a click on
//!    a link would. A GitHub copy beside an old Store copy stays hidden
//!    itself instead of handing over (`handoff.rs`).
//! 4. None of these: keymotion-cli cannot start it and says so at once
//!    (69): "update Keymotion from the Microsoft Store" when only an old
//!    Store copy is there.
//!
//! The app takes `--background` and `keymotion://background` as "run with
//! the tray icon only"; neither counts as a start with Windows, and a
//! running app that gets one shows nothing (`app/src-tauri/src/tray.rs`,
//! `handoff.rs`).

use std::path::{Path, PathBuf};
use std::time::Duration;

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
    /// The Store copy's version, when it is installed for this user.
    pub store_version: Option<[u16; 4]>,
    /// The program `keymotion:` addresses open, when it is the installed
    /// GitHub copy ([`installed_link_program`]).
    pub scheme_exe: Option<PathBuf>,
}

/// The GitHub installer's names (`handoff.rs` `MANUFACTURER`, `PRODUCT`,
/// `EXE`).
const GITHUB_MANUFACTURER: &str = "Keymotion";
const GITHUB_PRODUCT: &str = "Keymotion";
const APP_EXE: &str = "keymotion.exe";

/// The program `keymotion:` addresses open, only when it is the installed
/// GitHub copy. A development build registers the scheme for itself, and
/// run from inside another app's box its registration stays in that box,
/// where Explorer cannot see it: Explorer would then ask the user which app
/// to use. The installed copy registers it from outside, where it counts.
fn installed_link_program(command: Option<&str>, github_dir: Option<&Path>) -> Option<PathBuf> {
    let program = command.and_then(program_of)?;
    let dir = github_dir?;
    kb_winsys::copies::runs_from(dir, &program).then_some(program)
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

/// This keymotion-cli's version, `[major, minor, patch]`.
pub fn cli_version() -> [u16; 3] {
    kb_agent::version_triple(crate::VERSION).unwrap_or([0, 0, 0])
}

/// Whether a Keymotion of `version` can be started for this keymotion-cli:
/// it must take `--background` (an older one opens its window and has no
/// pipe) and be at least as new as the CLI.
pub fn new_enough(version: [u16; 3], cli: [u16; 3]) -> bool {
    kb_agent::starts_in_background(version) && version >= cli
}

fn dotted(v: &[u16]) -> String {
    v.iter().map(u16::to_string).collect::<Vec<_>>().join(".")
}

/// An old copy that cannot start without a window: 69, at once.
pub fn too_old(version: [u16; 3], cli: [u16; 3]) -> AgentError {
    let needed = cli.max(kb_agent::BACKGROUND_START_SINCE);
    AgentError::new(
        ErrorCode::AppNotRunning,
        format!(
            "Keymotion from the Microsoft Store is {}; keymotion-cli {} needs {} or newer, which starts without a window and answers agents.",
            dotted(&version),
            crate::VERSION,
            dotted(&needed)
        ),
    )
    .with_hint("Update Keymotion from the Microsoft Store.")
}

/// The first way that works ([module docs](self)) for a keymotion-cli of
/// version `cli`.
pub fn choose(f: &Facts, cli: [u16; 3]) -> Result<Method, AgentError> {
    if let Some(family) = &f.own_family {
        // The package that holds this very program: the same version.
        return Ok(Method::Package { aumid: format!("{family}!{APP_ID}"), own: true });
    }
    let store = f.store_version.map(|v| [v[0], v[1], v[2]]);
    if store.is_some_and(|v| new_enough(v, cli)) {
        return Ok(Method::Package { aumid: format!("{STORE_FAMILY}!{APP_ID}"), own: false });
    }
    // An old Store copy is left alone; the GitHub copy, through its link,
    // stays hidden instead of handing over to it (handoff.rs).
    if let Some(exe) = &f.scheme_exe {
        return Ok(Method::Link { exe: exe.clone() });
    }
    if let Some(old) = store {
        return Err(too_old(old, cli));
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
        store_version: kb_winsys::package::installed_version(STORE_FAMILY),
        scheme_exe: installed_link_program(
            kb_winsys::protocol::command(SCHEME).as_deref(),
            kb_winsys::copies::nsis_install(GITHUB_MANUFACTURER, GITHUB_PRODUCT, APP_EXE).as_deref(),
        )
        .filter(|p| p.is_file()),
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

/// The version of a running Store copy too old to answer agents, if one
/// runs (read from its install folder): waiting for its pipe would be in
/// vain.
pub fn old_store_copy_running() -> Option<[u16; 3]> {
    let cli = cli_version();
    kb_winsys::foreground::process_names()
        .into_iter()
        .filter(|(_, name)| name.eq_ignore_ascii_case("keymotion.exe"))
        .filter_map(|(pid, _)| kb_winsys::foreground::image_path(pid))
        .filter_map(|path| kb_winsys::package::version_in_install_path(&path))
        .map(|v| [v[0], v[1], v[2]])
        .find(|v| !new_enough(*v, cli))
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
        // Explorer gets none of this process's handles: an agent reading
        // this program's output to the end would otherwise wait for
        // Explorer too. Explorer hands the address to the shell and ends;
        // its exit code says nothing.
        Method::Link { .. } => kb_winsys::shell::run_alone(&explorer(), BACKGROUND_LINK, Duration::from_secs(3))
            .map_err(|e| failed(format!("explorer.exe did not run ({e})"))),
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
        let cli = [1, 2, 0];
        let all = Facts {
            own_family: Some("Test.Keymotion_abc".into()),
            store_version: Some([1, 2, 0, 0]),
            scheme_exe: Some(PathBuf::from(r"C:\Keymotion\keymotion.exe")),
        };
        assert_eq!(
            choose(&all, cli).unwrap(),
            Method::Package { aumid: "Test.Keymotion_abc!Keymotion".into(), own: true }
        );
        let store = Facts { own_family: None, ..all.clone() };
        assert_eq!(
            choose(&store, cli).unwrap(),
            Method::Package { aumid: "cebersaban.Keymotion_pa98x8fw006yg!Keymotion".into(), own: false }
        );
        let github = Facts { store_version: None, ..store.clone() };
        assert_eq!(choose(&github, cli).unwrap(), Method::Link { exe: PathBuf::from(r"C:\Keymotion\keymotion.exe") });
        let e = choose(&Facts::default(), cli).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));
        assert!(e.hint.is_some());
    }

    #[test]
    fn an_old_store_copy_is_never_started() {
        let cli = [1, 2, 0];
        // 1.1.0 knows no --background: it would open its window.
        let old = Facts { store_version: Some([1, 1, 0, 0]), ..Facts::default() };
        let e = choose(&old, cli).unwrap_err();
        assert_eq!((e.code, e.exit_code()), (ErrorCode::AppNotRunning, 69));
        assert!(e.message.contains("1.1.0"), "{}", e.message);
        assert_eq!(e.hint.as_deref(), Some("Update Keymotion from the Microsoft Store."));
        // With the GitHub copy beside it, the link starts that one.
        let both = Facts { scheme_exe: Some(PathBuf::from(r"C:\K\keymotion.exe")), ..old };
        assert!(matches!(choose(&both, cli).unwrap(), Method::Link { .. }));
        // Older than this CLI, even if it knows --background.
        let behind = Facts { store_version: Some([1, 2, 0, 0]), ..Facts::default() };
        assert!(choose(&behind, [1, 3, 0]).is_err());
        assert!(choose(&behind, [1, 2, 0]).is_ok());
        let ahead = Facts { store_version: Some([1, 4, 2, 0]), ..Facts::default() };
        assert!(choose(&ahead, [1, 3, 0]).is_ok());
        assert!(!new_enough([1, 1, 9], [1, 1, 0]), "below BACKGROUND_START_SINCE");
        assert!(new_enough([1, 2, 0], [1, 2, 0]));
        assert_eq!(cli_version(), kb_agent::version_triple(env!("CARGO_PKG_VERSION")).unwrap());
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
    fn only_the_installed_github_copys_link_is_used() {
        let dir = Path::new(r"C:\Users\A B\AppData\Local\Keymotion");
        let installed = r#""C:\Users\A B\AppData\Local\Keymotion\keymotion.exe" "%1""#;
        assert_eq!(installed_link_program(Some(installed), Some(dir)), Some(dir.join("keymotion.exe")));
        // A development build's registration, perhaps only inside a box.
        let dev = r#""C:\dev\klavye\target\debug\keymotion.exe" "%1""#;
        assert_eq!(installed_link_program(Some(dev), Some(dir)), None);
        assert_eq!(installed_link_program(Some(installed), None), None);
        assert_eq!(installed_link_program(None, Some(dir)), None);
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
        assert!(handoff.contains(&format!("const MANUFACTURER: &str = \"{GITHUB_MANUFACTURER}\";")));
        assert!(handoff.contains(&format!("const PRODUCT: &str = \"{GITHUB_PRODUCT}\";")));
        assert!(handoff.contains(&format!("const EXE: &str = \"{APP_EXE}\";")));
        assert!(read("tray.rs").contains(&format!("pub const ARG_BACKGROUND: &str = \"{ARG_BACKGROUND}\";")));
        let updates = read("updates.rs");
        assert!(updates.contains(&format!("pub const SCHEME: &str = \"{SCHEME}\";")));
        assert!(updates.contains("\"background\" => Link::Background"));
        assert_eq!(BACKGROUND_LINK, format!("{SCHEME}://background"));
    }
}
