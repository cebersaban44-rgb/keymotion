//! One Keymotion per computer when both channels are installed (owner's
//! decision, 2026-11-16; Microsoft's guidance on moving users to the
//! Store-packaged app).
//!
//! The GitHub copy (per-user NSIS install) and the Microsoft Store copy
//! share the settings folder and the single-instance lock, so only one
//! runs. Left alone, both would start with Windows and the first to come
//! would win: one day the GitHub copy its own updater raised, the next day
//! the Store's. So the installed GitHub copy steps aside once the Store
//! copy is installed: it starts the Store copy instead and hands over its
//! start with Windows. The Store copy tells the user they can remove the
//! GitHub copy (Settings › About).
//!
//! A development build and a release build run from the repository never
//! step aside: only the program in the GitHub install's folder does. Nor
//! does a pre-release (a beta, `1.3.0-beta.1`): its tester installed it to
//! try it, so it starts itself, and it leaves the start with Windows as it
//! is. The Store never ships a pre-release.

use crate::tray;
use std::path::PathBuf;

/// The Store package's family name (Partner Center › Product identity).
pub const STORE_FAMILY: &str = "cebersaban.Keymotion_pa98x8fw006yg";
/// The package's application (`store/AppxManifest.xml`, `Application Id`).
const STORE_APP: &str = "Keymotion";
/// The GitHub copy started with Windows: the Store copy turns its own
/// startup task on in its place.
pub const ARG_TAKE_STARTUP: &str = "--take-startup";
/// tauri-plugin-single-instance's lock, `<identifier>-sim` (2.5, without
/// its `semver` feature): both copies take the same one.
const INSTANCE_LOCK: &str = "dev.keymotion-sim";
/// The GitHub installer's names (tauri.conf.json `bundle.publisher`,
/// `productName`) and its program.
const MANUFACTURER: &str = "Keymotion";
const PRODUCT: &str = "Keymotion";
const EXE: &str = "keymotion.exe";
/// The app's identifier: its log folder is `%LOCALAPPDATA%\<identifier>\logs`.
const IDENTIFIER: &str = "dev.keymotion";

/// What the program found when it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Facts {
    /// This program is the installed GitHub copy.
    github_copy: bool,
    /// The Store copy is installed for this user.
    store_installed: bool,
    /// Started with Windows (`--hidden`, or the Store copy's startup task).
    sign_in: bool,
    /// keymotion-cli started it for an agent (`--background` or
    /// `keymotion://background`): no window, but not a start with Windows.
    background: bool,
    /// The installed Store copy takes `--background`
    /// (`kb_agent::BACKGROUND_START_SINCE`); an older one would open its
    /// window, and has no pipe for agents.
    store_background: bool,
    /// The GitHub copy asked this one to take over its start with Windows.
    take_startup: bool,
    /// A copy already runs.
    running: bool,
    /// This copy's `Run` value: `Some(on)` when there is one.
    run_entry: Option<bool>,
    /// This copy's version has a pre-release part (`1.3.0-beta.1`).
    pre_release: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Plan {
    /// Start as usual.
    Start,
    /// Started with Windows while a copy already runs: end quietly (the
    /// single-instance plugin would bring its window forward).
    End,
    /// Start the Store copy with `args` and end; once it started, remove
    /// this copy's `Run` value when there is one.
    HandOver { args: Vec<&'static str>, drop_run_entry: bool },
}

fn plan(f: Facts) -> Plan {
    // A start for an agent stays with this copy, hidden, rather than go to
    // a Store copy too old to start without a window.
    let store_takes_it = !f.background || f.store_background;
    if f.github_copy && f.store_installed && store_takes_it && !f.pre_release {
        let mut args = Vec::new();
        if f.sign_in {
            args.push(tray::ARG_HIDDEN);
        }
        if f.background {
            args.push(tray::ARG_BACKGROUND);
        }
        if f.run_entry == Some(true) {
            args.push(ARG_TAKE_STARTUP);
        }
        return Plan::HandOver { args, drop_run_entry: f.run_entry.is_some() };
    }
    if f.sign_in && f.running && !f.take_startup {
        return Plan::End;
    }
    Plan::Start
}

/// A version with a pre-release part (semver: `1.3.0-beta.1`). Build
/// metadata alone (`1.3.0+abc`) is not one.
fn is_pre_release(version: &str) -> bool {
    version.split('+').next().is_some_and(|v| v.contains('-'))
}

/// The installed GitHub copy's folder, when there is one.
pub fn github_install() -> Option<PathBuf> {
    kb_winsys::copies::nsis_install(MANUFACTURER, PRODUCT, EXE)
}

/// Before anything else starts: `true` when this process should end here.
pub fn before_start() -> bool {
    let args: Vec<String> = std::env::args().collect();
    let has = |a: &str| args.iter().any(|x| x == a);
    let store_build = crate::channel::store();
    let github_copy = !store_build
        && !tauri::is_dev()
        && std::env::current_exe()
            .ok()
            .zip(github_install())
            .is_some_and(|(exe, dir)| kb_winsys::copies::runs_from(&dir, &exe));
    let running = kb_winsys::copies::running(INSTANCE_LOCK);
    let background = tray::background_start(&args);
    let facts = Facts {
        github_copy,
        store_installed: github_copy && kb_winsys::package::installed(STORE_FAMILY),
        // The startup task passes no arguments; Windows tells how it
        // started (asked only when it matters).
        sign_in: has(tray::ARG_HIDDEN) || (store_build && running && kb_winsys::package::started_at_sign_in()),
        background,
        store_background: background
            && kb_winsys::package::installed_version(STORE_FAMILY)
                .is_some_and(|v| kb_agent::starts_in_background([v[0], v[1], v[2]])),
        take_startup: has(ARG_TAKE_STARTUP),
        running,
        run_entry: kb_winsys::startup::entry(tray::RUN_NAME).map(|e| !e.disabled),
        pre_release: is_pre_release(env!("CARGO_PKG_VERSION")),
    };
    match plan(facts) {
        Plan::Start => {
            if store_build && github_install().is_some() {
                tracing::info!("the GitHub copy is installed too: Settings › About offers to remove it");
            }
            false
        }
        Plan::End => {
            keep_log();
            tracing::info!("started with Windows while Keymotion already runs: nothing more to do");
            true
        }
        Plan::HandOver { args, drop_run_entry } => {
            keep_log();
            let aumid = format!("{STORE_FAMILY}!{STORE_APP}");
            match kb_winsys::package::activate(&aumid, &args.join(" ")) {
                Ok(pid) => {
                    tracing::info!(
                        pid,
                        ?args,
                        "the Microsoft Store copy is installed: started it instead of this GitHub copy"
                    );
                    if drop_run_entry {
                        match kb_winsys::startup::remove(tray::RUN_NAME) {
                            Ok(()) => tracing::info!("this GitHub copy no longer starts with Windows"),
                            Err(e) => tracing::warn!("start with Windows not removed: {e}"),
                        }
                    }
                    true
                }
                Err(e) => {
                    tracing::warn!("the Microsoft Store copy did not start ({e}); this GitHub copy runs instead");
                    false
                }
            }
        }
    }
}

/// A process that ends before the app's setup still leaves its lines in
/// the log (setup would name the folder otherwise).
fn keep_log() {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let dir = PathBuf::from(local).join(IDENTIFIER).join("logs");
        crate::logging::set_dir(&dir, env!("CARGO_PKG_VERSION"));
    }
}

/// The Store copy, when the GitHub copy is installed beside it: the
/// settings page offers to remove the GitHub copy.
pub fn github_copy_beside() -> bool {
    crate::channel::store() && github_install().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(sign_in: bool, run_entry: Option<bool>) -> Facts {
        Facts { github_copy: true, store_installed: true, sign_in, run_entry, ..Facts::default() }
    }

    #[test]
    fn the_github_copy_starts_the_store_copy_instead() {
        // Opened by the user, not starting with Windows.
        assert_eq!(plan(github(false, None)), Plan::HandOver { args: vec![], drop_run_entry: false });
        // Started with Windows: the Store copy starts hidden too and takes
        // the start with Windows over.
        assert_eq!(
            plan(github(true, Some(true))),
            Plan::HandOver { args: vec![tray::ARG_HIDDEN, ARG_TAKE_STARTUP], drop_run_entry: true }
        );
        // Opened by the user while it starts with Windows.
        assert_eq!(
            plan(github(false, Some(true))),
            Plan::HandOver { args: vec![ARG_TAKE_STARTUP], drop_run_entry: true }
        );
        // A value turned off in Task Manager is removed, not handed over.
        assert_eq!(plan(github(false, Some(false))), Plan::HandOver { args: vec![], drop_run_entry: true });
    }

    #[test]
    fn without_the_store_copy_nothing_changes() {
        let f = Facts { store_installed: false, ..github(true, Some(true)) };
        assert_eq!(plan(f), Plan::Start);
        // A build that is not the installed GitHub copy never steps aside.
        let f = Facts { github_copy: false, ..github(false, Some(true)) };
        assert_eq!(plan(f), Plan::Start);
    }

    #[test]
    fn a_pre_release_github_copy_starts_itself() {
        // A beta never hands over to the Store copy, opened by the user or
        // started with Windows, and keeps its Run value.
        for f in [github(false, None), github(true, Some(true)), github(false, Some(false))] {
            assert_eq!(plan(Facts { pre_release: true, ..f }), Plan::Start, "{f:?}");
        }
        let agent = Facts { background: true, store_background: true, pre_release: true, ..github(false, None) };
        assert_eq!(plan(agent), Plan::Start);
        // Started with Windows while the Store copy already runs: one
        // Keymotion, as for any second start.
        let running = Facts { running: true, pre_release: true, ..github(true, Some(true)) };
        assert_eq!(plan(running), Plan::End);
        // A release still hands over.
        assert!(matches!(plan(github(false, None)), Plan::HandOver { .. }));
    }

    #[test]
    fn pre_releases_are_told_by_their_version() {
        assert!(is_pre_release("1.3.0-beta.1"));
        assert!(is_pre_release("1.3.0-rc.2+build.5"));
        assert!(!is_pre_release("1.2.1"));
        assert!(!is_pre_release("1.3.0+build.5"));
    }

    #[test]
    fn a_second_start_with_windows_ends_quietly() {
        let f = Facts { sign_in: true, running: true, ..Facts::default() };
        assert_eq!(plan(f), Plan::End);
        // Unless it carries the GitHub copy's start with Windows to the
        // running copy.
        assert_eq!(plan(Facts { take_startup: true, ..f }), Plan::Start);
        // Opened by the user: the running copy's window comes forward.
        assert_eq!(plan(Facts { sign_in: false, ..f }), Plan::Start);
        assert_eq!(plan(Facts { running: false, ..f }), Plan::Start);
    }

    #[test]
    fn a_start_for_an_agent_stays_windowless_and_is_not_a_start_with_windows() {
        // The GitHub copy hands the start to the Store copy, still without
        // a window, and does not hand over its start with Windows.
        let f = Facts { background: true, store_background: true, ..github(false, None) };
        assert_eq!(plan(f), Plan::HandOver { args: vec![tray::ARG_BACKGROUND], drop_run_entry: false });
        // A Store copy too old for --background would open its window: this
        // copy starts instead, hidden, and keeps its start with Windows.
        let old = Facts { store_background: false, run_entry: Some(true), ..f };
        assert_eq!(plan(old), Plan::Start);
        // An ordinary start still goes to an old Store copy.
        assert!(matches!(plan(Facts { background: false, ..old }), Plan::HandOver { .. }));
        // With a copy already running, it goes to that copy (which shows
        // nothing), unlike a second start with Windows.
        let f = Facts { background: true, running: true, ..Facts::default() };
        assert_eq!(plan(f), Plan::Start);
    }
}
