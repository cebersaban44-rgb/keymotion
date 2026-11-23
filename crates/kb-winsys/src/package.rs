//! The Microsoft Store build: the same program, installed from an MSIX
//! package. Windows gives such a process a package identity, and with it a
//! few differences the app follows (docs site: developers/microsoft-store):
//!
//! * the Store updates it, so the app's own updater stays off;
//! * "start with Windows" is the package's startup task, not a `Run` value;
//! * notifications carry the package's name and logo by themselves;
//! * a folder it makes directly under `%APPDATA%` or `%LOCALAPPDATA%` lands
//!   in the package's own folder (`%LOCALAPPDATA%\Packages\<family>\
//!   LocalCache\…`), which Windows shows the app at the usual path and
//!   removes when the app is uninstalled. A folder that was there before (a
//!   GitHub install's `dev.keymotion`) stays where it is and is shared
//!   (tried on Windows 11, 2026-11-15).
//!
//! A program started by another packaged app (the development build run
//! from a packaged editor or assistant) inherits that app's identity. Only
//! a package that holds this very program counts as Keymotion's own.

use std::path::{Path, PathBuf};

/// The package Keymotion was installed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Name, version, architecture and publisher id, such as
    /// `Keymotion_0.1.0.0_x64__8wekyb3d8bbwe`.
    pub full_name: String,
    /// Name and publisher id (`Keymotion_8wekyb3d8bbwe`): names the
    /// package's data folder.
    pub family_name: String,
    /// The folder the package is installed in (read only).
    pub install_dir: PathBuf,
}

/// Keymotion's own package; `None` for the GitHub build, the development
/// build and a copied program.
pub fn current() -> Option<Package> {
    let package = sys::current()?;
    let exe = std::env::current_exe().ok()?;
    holds(&package.install_dir, &exe).then_some(package)
}

/// The package this process runs with, whichever app's it is (for
/// keymotion-cli's `doctor`). A program another packaged app started may
/// carry that app's identity, or none and still have its AppData writes
/// redirected into that app's folder, as programs started from the Claude
/// desktop app do (measured 2026-11-22): `None` does not prove a program
/// runs outside a package.
pub fn identity() -> Option<Package> {
    sys::current()
}

/// Whether `exe` lies inside `dir` (letters compared without case, as
/// Windows does).
fn holds(dir: &Path, exe: &Path) -> bool {
    let dir = dir.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase();
    let exe = exe.to_string_lossy().to_lowercase();
    !dir.is_empty() && exe.strip_prefix(&dir).is_some_and(|rest| rest.starts_with(['\\', '/']))
}

/// Where the package keeps its own copy of `dir`, a folder under
/// `%APPDATA%` (`roaming`) or `%LOCALAPPDATA%` (`local`): the folder to show
/// the user, since Explorer sees only the real one. `None` when `dir` is
/// under neither.
pub fn private_copy(family_name: &str, local: &Path, roaming: &Path, dir: &Path) -> Option<PathBuf> {
    let cache = local.join("Packages").join(family_name).join("LocalCache");
    // %LOCALAPPDATA%\Packages itself is the package's, not a copy.
    if within(dir, &local.join("Packages")).is_some() {
        return None;
    }
    if let Some(rest) = within(dir, roaming) {
        return Some(cache.join("Roaming").join(rest));
    }
    within(dir, local).map(|rest| cache.join("Local").join(rest))
}

/// `dir` relative to `base`, when it is inside (letters compared without case).
fn within(dir: &Path, base: &Path) -> Option<PathBuf> {
    let (d, b) = (dir.to_string_lossy(), base.to_string_lossy());
    let b = b.trim_end_matches(['\\', '/']);
    if b.is_empty() || d.len() < b.len() || !d[..b.len()].eq_ignore_ascii_case(b) {
        return None;
    }
    let rest = &d[b.len()..];
    if rest.is_empty() {
        return Some(PathBuf::new());
    }
    rest.starts_with(['\\', '/']).then(|| PathBuf::from(rest.trim_start_matches(['\\', '/'])))
}

/// Windows started the app through the package's startup task, at sign-in
/// (the Store build's "start with Windows"; it opens hidden, in the tray).
pub fn started_at_sign_in() -> bool {
    sys::started_at_sign_in()
}

/// A package of the family `family_name` is installed for this user. Any
/// program can ask, packaged or not (the GitHub build looks for the Store
/// build this way).
pub fn installed(family_name: &str) -> bool {
    sys::installed(family_name)
}

/// The newest version of the family `family_name` installed for this user
/// (`[major, minor, build, revision]`), from the packages' full names;
/// `None` when none is installed. Any program can ask.
pub fn installed_version(family_name: &str) -> Option<[u16; 4]> {
    sys::installed_full_names(family_name).iter().filter_map(|n| version_in_full_name(n)).max()
}

/// The version in a package's full name,
/// `Name_1.2.3.0_x64__publisherid` → `[1, 2, 3, 0]`.
pub fn version_in_full_name(full_name: &str) -> Option<[u16; 4]> {
    let version = full_name.split('_').nth(1)?;
    let mut parts = version.split('.').map(|p| p.parse::<u16>().ok());
    let out = [parts.next()??, parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(out)
}

/// The version of a packaged program from its path, when it runs from a
/// package's install folder (`…\WindowsApps\Name_1.2.3.0_x64__id\x.exe`):
/// tells an old Store copy that is running apart from a new one.
pub fn version_in_install_path(path: &str) -> Option<[u16; 4]> {
    let mut parts = path.rsplit(['\\', '/']);
    let _program = parts.next()?;
    let folder = parts.next()?;
    let parent = parts.next()?;
    parent.eq_ignore_ascii_case("WindowsApps").then(|| version_in_full_name(folder)).flatten()
}

/// Start a packaged app, `<family>!<application id>`, with `arguments` on
/// its command line; its process id. A link to it (`shell:AppsFolder\…`)
/// cannot pass arguments.
pub fn activate(app_user_model_id: &str, arguments: &str) -> Result<u32, String> {
    sys::activate(app_user_model_id, arguments)
}

/// Come back after an update. When the Store installs a new version, Windows
/// closes the running app; a process registered here is started again
/// afterwards with `arguments`
/// (learn.microsoft.com: desktop-to-uwp-extensions, "Updates"). Only after
/// an update: not after a crash, a hang or a restart of the computer.
pub fn restart_after_update(arguments: &str) -> Result<(), String> {
    sys::restart_after_update(arguments)
}

#[cfg(windows)]
mod sys {
    use super::Package;
    use std::path::PathBuf;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, WIN32_ERROR};
    use windows::Win32::Storage::Packaging::Appx::{
        GetCurrentPackageFamilyName, GetCurrentPackageFullName, GetCurrentPackagePath,
    };

    /// One of the `GetCurrentPackage…` calls: asks for the length, then
    /// fills a buffer of that length. `None` without a package.
    fn read(call: unsafe fn(*mut u32, Option<PWSTR>) -> WIN32_ERROR) -> Option<String> {
        let mut len = 0u32;
        // SAFETY: no buffer; the API writes the length needed, in
        // characters with the NUL, and answers ERROR_INSUFFICIENT_BUFFER
        // (or APPMODEL_ERROR_NO_PACKAGE without a package).
        if unsafe { call(&mut len, None) } != ERROR_INSUFFICIENT_BUFFER || len == 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        // SAFETY: a buffer of `len` characters, as the API asked for.
        if unsafe { call(&mut len, Some(PWSTR(buf.as_mut_ptr()))) }.is_err() {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    pub fn current() -> Option<Package> {
        Some(Package {
            full_name: read(GetCurrentPackageFullName)?,
            family_name: read(GetCurrentPackageFamilyName)?,
            install_dir: PathBuf::from(read(GetCurrentPackagePath)?),
        })
    }

    pub fn started_at_sign_in() -> bool {
        use windows::ApplicationModel::Activation::ActivationKind;
        use windows::ApplicationModel::AppInstance;
        // Without a package there are no activation arguments (an error).
        AppInstance::GetActivatedEventArgs()
            .and_then(|args| args.Kind())
            .is_ok_and(|kind| kind == ActivationKind::StartupTask)
    }

    // Declared here rather than through the `windows` crate: its
    // `Win32_System_Recovery` feature for one function. It lives in
    // kernel32, which every program links anyway.
    #[link(name = "kernel32")]
    extern "system" {
        fn RegisterApplicationRestart(command_line: *const u16, flags: u32) -> windows::core::HRESULT;
    }

    pub fn restart_after_update(arguments: &str) -> Result<(), String> {
        // RESTART_NO_CRASH | RESTART_NO_HANG | RESTART_NO_REBOOT: only an
        // update (Restart Manager's "patch") brings the app back.
        const ONLY_AFTER_UPDATE: u32 = 1 | 2 | 8;
        let wide: Vec<u16> = arguments.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated UTF-16 shorter than the 32,767
        // characters allowed; Windows copies it during the call.
        unsafe { RegisterApplicationRestart(wide.as_ptr(), ONLY_AFTER_UPDATE) }
            .ok()
            .map_err(|e| format!("restart after an update not registered: {}", e.message()))
    }

    pub fn installed(family: &str) -> bool {
        use windows::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;
        let family = windows::core::HSTRING::from(family);
        let (mut count, mut len) = (0u32, 0u32);
        // SAFETY: no buffers; the API writes how many packages there are
        // and the length their names would need.
        let err = unsafe { GetPackagesByPackageFamily(&family, &mut count, None, &mut len, None) };
        (err.is_ok() || err == ERROR_INSUFFICIENT_BUFFER) && count > 0
    }

    pub fn installed_full_names(family: &str) -> Vec<String> {
        use windows::Win32::Storage::Packaging::Appx::GetPackagesByPackageFamily;
        let family = windows::core::HSTRING::from(family);
        // A package installed between the two calls makes the second one
        // too small: ask again.
        for _ in 0..3 {
            let (mut count, mut len) = (0u32, 0u32);
            // SAFETY: no buffers; the API writes how many packages there
            // are and the length their names need, NULs included.
            let err = unsafe { GetPackagesByPackageFamily(&family, &mut count, None, &mut len, None) };
            if !(err.is_ok() || err == ERROR_INSUFFICIENT_BUFFER) || count == 0 || len == 0 {
                return Vec::new();
            }
            let mut names = vec![PWSTR::null(); count as usize];
            let mut buf = vec![0u16; len as usize];
            // SAFETY: an array of `count` name pointers and a buffer of
            // `len` characters, the sizes the first call asked for; the
            // pointers it writes point into `buf`.
            let err = unsafe {
                GetPackagesByPackageFamily(
                    &family,
                    &mut count,
                    Some(names.as_mut_ptr()),
                    &mut len,
                    Some(PWSTR(buf.as_mut_ptr())),
                )
            };
            if err == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            if err.is_err() {
                return Vec::new();
            }
            return names
                .iter()
                .take(count as usize)
                .filter(|p| !p.is_null())
                // SAFETY: NUL-terminated names inside `buf`, which is alive.
                .filter_map(|p| unsafe { p.to_string() }.ok())
                .collect();
        }
        Vec::new()
    }

    pub fn activate(aumid: &str, arguments: &str) -> Result<u32, String> {
        use windows::core::HSTRING;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{ApplicationActivationManager, IApplicationActivationManager, AO_NONE};
        // SAFETY: COM for this thread, as the UI thread later wants it
        // (single-threaded); undone below when this call started it.
        let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let started = (|| {
            // SAFETY: Windows' own class, asked for the interface it has.
            let manager: IApplicationActivationManager =
                unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER) }
                    .map_err(|e| format!("activation manager: {}", e.message()))?;
            // SAFETY: NUL-terminated strings that outlive the call.
            unsafe { manager.ActivateApplication(&HSTRING::from(aumid), &HSTRING::from(arguments), AO_NONE) }
                .map_err(|e| format!("not started: {}", e.message()))
        })();
        if init.is_ok() {
            // SAFETY: balances the successful CoInitializeEx above; the
            // manager is already released.
            unsafe { CoUninitialize() };
        }
        started
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Package;

    pub fn current() -> Option<Package> {
        None
    }

    pub fn started_at_sign_in() -> bool {
        false
    }

    pub fn installed(_family: &str) -> bool {
        false
    }

    pub fn installed_full_names(_family: &str) -> Vec<String> {
        Vec::new()
    }

    pub fn activate(_aumid: &str, _arguments: &str) -> Result<u32, String> {
        Err("packaged apps are only available on Windows".into())
    }

    pub fn restart_after_update(_arguments: &str) -> Result<(), String> {
        Err("restart after an update is only available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn restart_after_an_update_can_be_registered() {
        // Only this test process is registered, and it ends with the run.
        restart_after_update("--hidden").unwrap();
    }

    #[test]
    fn versions_come_from_full_names_and_install_folders() {
        assert_eq!(version_in_full_name("cebersaban.Keymotion_1.1.0.0_x64__pa98x8fw006yg"), Some([1, 1, 0, 0]));
        assert_eq!(version_in_full_name("Keymotion_12.3.45.6_neutral_~_abc"), Some([12, 3, 45, 6]));
        for bad in [
            "Keymotion",
            "Keymotion_1.1.0_x64__a",
            "Keymotion_1.1.0.0.0_x64__a",
            "K_a.b.c.d_x64__a",
            "K_99999.0.0.0_x64",
        ] {
            assert_eq!(version_in_full_name(bad), None, "{bad}");
        }
        assert_eq!(
            version_in_install_path(
                r"C:\Program Files\WindowsApps\cebersaban.Keymotion_1.1.0.0_x64__pa98x8fw006yg\keymotion.exe"
            ),
            Some([1, 1, 0, 0])
        );
        assert_eq!(version_in_install_path(r"C:\Users\a\AppData\Local\Keymotion\keymotion.exe"), None);
        assert_eq!(version_in_install_path(r"C:\dev\Keymotion_1.1.0.0_x64__a\keymotion.exe"), None);
        // Nothing is installed under a made-up family.
        assert_eq!(installed_version("Nobody.Nothing_0000000000000"), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_versions_of_an_installed_family_can_be_read() {
        // Windows' own Store app family on a normal install; when it is
        // missing (a stripped image), there is simply nothing to read.
        if installed("Microsoft.WindowsStore_8wekyb3d8bbwe") {
            let v = installed_version("Microsoft.WindowsStore_8wekyb3d8bbwe").expect("a version");
            assert!(v[0] > 0, "{v:?}");
        }
    }

    #[test]
    fn only_a_program_inside_the_package_is_the_package_s() {
        let dir = Path::new(r"C:\Program Files\WindowsApps\Keymotion_0.1.0.0_x64__abc");
        assert!(holds(dir, Path::new(r"C:\Program Files\WindowsApps\Keymotion_0.1.0.0_x64__abc\keymotion.exe")));
        assert!(holds(dir, Path::new(r"c:\program files\windowsapps\KEYMOTION_0.1.0.0_x64__abc\keymotion.exe")));
        // The development build under a packaged editor's identity.
        assert!(!holds(dir, Path::new(r"C:\dev\klavye\target\debug\keymotion.exe")));
        // A sibling folder whose name only starts the same.
        assert!(!holds(dir, Path::new(r"C:\Program Files\WindowsApps\Keymotion_0.1.0.0_x64__abcd\keymotion.exe")));
        assert!(!holds(Path::new(""), Path::new(r"C:\keymotion.exe")));
    }

    #[test]
    fn data_folders_map_to_the_package_s_copy() {
        let local = Path::new(r"C:\Users\A B\AppData\Local");
        let roaming = Path::new(r"C:\Users\A B\AppData\Roaming");
        let fam = "Keymotion_abc";
        assert_eq!(
            private_copy(fam, local, roaming, Path::new(r"C:\Users\A B\AppData\Roaming\dev.keymotion")),
            Some(PathBuf::from(r"C:\Users\A B\AppData\Local\Packages\Keymotion_abc\LocalCache\Roaming\dev.keymotion"))
        );
        assert_eq!(
            private_copy(fam, local, roaming, Path::new(r"c:\users\a b\appdata\local\dev.keymotion\logs")),
            Some(PathBuf::from(
                r"C:\Users\A B\AppData\Local\Packages\Keymotion_abc\LocalCache\Local\dev.keymotion\logs"
            ))
        );
        assert_eq!(private_copy(fam, local, roaming, Path::new(r"C:\Users\A B\Downloads")), None);
        assert_eq!(private_copy(fam, local, roaming, Path::new(r"C:\Users\A B\AppData\LocalLow\x")), None);
        assert_eq!(
            private_copy(
                fam,
                local,
                roaming,
                Path::new(r"C:\Users\A B\AppData\Local\Packages\Keymotion_abc\LocalState")
            ),
            None
        );
    }

    /// Prints what this process sees; `--nocapture` shows it. Under a
    /// packaged parent the raw identity is the parent's, never Keymotion's.
    #[test]
    fn the_test_runner_is_not_keymotion_s_package() {
        eprintln!("raw package: {:?}", sys::current());
        assert_eq!(current(), None);
        assert!(!started_at_sign_in());
    }

    /// A family no one has: not installed, never an error or a panic.
    #[test]
    fn an_unknown_family_is_not_installed() {
        assert!(!installed("Keymotion.NoSuchPackage_0000000000000"));
        assert!(!installed(""));
    }
}
