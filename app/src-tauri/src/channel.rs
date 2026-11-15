//! Where this copy of Keymotion came from. The program is the same either
//! way; a few things follow the channel:
//!
//! | | GitHub (NSIS installer, development build) | Microsoft Store (MSIX) |
//! |---|---|---|
//! | Updates | the app's updater (updates.rs) | the Store; the updater stays off |
//! | Start with Windows | a `Run` value | the package's startup task |
//! | Notification name and logo | registered at start (notify.rs) | the package's |
//! | "Open data folder" | the real folder | the package's copy (kb_winsys::package) |
//!
//! The Store build is told apart by its package identity, never by a build
//! flag: the program put in the MSIX is the one the release pipeline builds.

use kb_winsys::package::Package;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The startup task the package declares (`store/AppxManifest.xml`,
/// `desktop:StartupTask` `TaskId`).
pub const STARTUP_TASK: &str = "KeymotionStartup";

fn package() -> Option<&'static Package> {
    static PACKAGE: OnceLock<Option<Package>> = OnceLock::new();
    PACKAGE
        .get_or_init(|| {
            let found = kb_winsys::package::current();
            if let Some(p) = &found {
                tracing::info!(package = %p.full_name, "running from the Microsoft Store package");
            }
            found
        })
        .as_ref()
}

/// Installed from the Microsoft Store.
pub fn store() -> bool {
    package().is_some()
}

/// The folder to show for `dir`: in the Store build, the package's own copy
/// of a folder under AppData when it has one (Explorer sees only the real
/// folder); otherwise `dir`.
pub fn shown_folder(dir: &Path) -> PathBuf {
    let Some(p) = package() else { return dir.to_path_buf() };
    let base = |var| std::env::var_os(var).map(PathBuf::from);
    let (Some(local), Some(roaming)) = (base("LOCALAPPDATA"), base("APPDATA")) else {
        return dir.to_path_buf();
    };
    match kb_winsys::package::private_copy(&p.family_name, &local, &roaming, dir) {
        Some(copy) if copy.is_dir() => copy,
        _ => dir.to_path_buf(),
    }
}
