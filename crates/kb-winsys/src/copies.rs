//! Other copies of the program on this computer: one installed per user by
//! an NSIS installer (the GitHub build), and one already running.

use std::path::{Path, PathBuf};

/// The folder a per-user NSIS installer put `product` in, when its program
/// `exe` is there. Tauri's installer writes the folder as the default value
/// of `HKCU\Software\<manufacturer>\<product>`, and again, in quotes, as
/// `InstallLocation` of its `Uninstall\<product>` entry.
pub fn nsis_install(manufacturer: &str, product: &str, exe: &str) -> Option<PathBuf> {
    let own = crate::startup::user_text(&format!(r"Software\{manufacturer}\{product}"), "");
    let listed = || {
        crate::startup::user_text(
            &format!(r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{product}"),
            "InstallLocation",
        )
    };
    let dir = folder(own.as_deref()).or_else(|| folder(listed().as_deref()))?;
    dir.join(exe).is_file().then_some(dir)
}

/// A registry value naming a folder, quoted or not.
fn folder(value: Option<&str>) -> Option<PathBuf> {
    let v = value?.trim().trim_matches('"').trim();
    (!v.is_empty()).then(|| PathBuf::from(v))
}

/// `exe` lies directly in `dir` (letters compared without case, as Windows
/// does).
pub fn runs_from(dir: &Path, exe: &Path) -> bool {
    let Some(parent) = exe.parent() else { return false };
    let (a, b) = (parent.to_string_lossy(), dir.to_string_lossy());
    let (a, b) = (a.trim_end_matches(['\\', '/']), b.trim_end_matches(['\\', '/']));
    !b.is_empty() && a.eq_ignore_ascii_case(b)
}

/// Another program in this session holds the named mutex `name`: a copy
/// is already running.
pub fn running(name: &str) -> bool {
    sys::running(name)
}

#[cfg(windows)]
mod sys {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED};
    use windows::Win32::System::Threading::{OpenMutexW, SYNCHRONIZATION_SYNCHRONIZE};

    pub fn running(name: &str) -> bool {
        // SAFETY: a NUL-terminated name that outlives the call.
        match unsafe { OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, &HSTRING::from(name)) } {
            Ok(handle) => {
                // SAFETY: the handle just opened, closed once.
                let _ = unsafe { CloseHandle(handle) };
                true
            }
            // There, but not ours to open.
            Err(e) => e.code() == ERROR_ACCESS_DENIED.to_hresult(),
        }
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn running(_name: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_come_quoted_or_not() {
        assert_eq!(
            folder(Some(r"C:\Users\A B\AppData\Local\Keymotion")),
            Some(PathBuf::from(r"C:\Users\A B\AppData\Local\Keymotion"))
        );
        assert_eq!(
            folder(Some(r#""C:\Users\A B\AppData\Local\Keymotion""#)),
            Some(PathBuf::from(r"C:\Users\A B\AppData\Local\Keymotion"))
        );
        assert_eq!(folder(Some("  ")), None);
        assert_eq!(folder(Some(r#""""#)), None);
        assert_eq!(folder(None), None);
    }

    #[test]
    fn only_the_installed_program_runs_from_the_install() {
        let dir = Path::new(r"C:\Users\A B\AppData\Local\Keymotion");
        assert!(runs_from(dir, Path::new(r"C:\Users\A B\AppData\Local\Keymotion\keymotion.exe")));
        assert!(runs_from(
            Path::new(r"c:\users\a b\appdata\local\keymotion\"),
            Path::new(r"C:\Users\A B\AppData\Local\Keymotion\keymotion.exe")
        ));
        // A release build run from the repository, a copy elsewhere.
        assert!(!runs_from(dir, Path::new(r"C:\dev\klavye\target\release\keymotion.exe")));
        assert!(!runs_from(dir, Path::new(r"C:\Users\A B\AppData\Local\Keymotion\sub\keymotion.exe")));
        assert!(!runs_from(Path::new(""), Path::new(r"C:\keymotion.exe")));
    }

    #[test]
    fn no_one_holds_an_unknown_mutex() {
        assert!(!running("keymotion-test-no-such-mutex-7f3a"));
    }

    /// Prints what this computer has; `--nocapture` shows it.
    #[test]
    fn an_installed_copy_is_found_or_not_without_error() {
        eprintln!("GitHub copy: {:?}", nsis_install("Keymotion", "Keymotion", "keymotion.exe"));
        assert_eq!(nsis_install("Keymotion-test-none", "Nothing", "x.exe"), None);
    }
}
