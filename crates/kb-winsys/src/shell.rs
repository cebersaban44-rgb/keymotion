//! Opening a web page in the user's default browser, and starting a
//! program that inherits nothing from this process ([`run_alone`]).
//!
//! `explorer.exe <address>` takes an address with a query (`?a=b&c=d`) for a
//! path, finds nothing and opens the Documents folder instead (the owner's
//! install test, 2026-11-12). `ShellExecuteW` with the "open" verb hands the
//! address to the browser registered for it, as a click on a link does.

/// Open `url` in the default browser. Only `https://` and `http://`
/// addresses are opened, nothing that could start a program.
pub fn open_url(url: &str) -> Result<(), String> {
    if !web_address(url) {
        return Err("only web pages are opened".into());
    }
    sys::open(url)
}

/// Windows' own pages the app opens: fixed addresses, nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsPage {
    /// Settings › Apps › Startup: where a startup task turned off by the
    /// user is turned on again (the Store build).
    StartupApps,
    /// Microsoft Store › Downloads and updates (the Store build's updates).
    StoreUpdates,
    /// Settings › Apps › Installed apps: where the user removes a program
    /// (the GitHub copy, beside the Store copy).
    InstalledApps,
}

impl WindowsPage {
    pub fn address(self) -> &'static str {
        match self {
            Self::StartupApps => "ms-settings:startupapps",
            Self::StoreUpdates => "ms-windows-store://downloadsandupdates",
            Self::InstalledApps => "ms-settings:appsfeatures",
        }
    }
}

/// Open one of Windows' own pages.
pub fn open_windows_page(page: WindowsPage) -> Result<(), String> {
    sys::open(page.address())
}

/// Start `program` (a full path) with one `argument`, handing it none of
/// this process's handles, and wait up to `wait` for it to end.
///
/// `std::process::Command` always creates processes that inherit every
/// inheritable handle, the standard handles included: a program started
/// that way keeps a caller's output pipe open, and a caller reading to the
/// end (an AI agent running keymotion-cli) waits for it as well.
pub fn run_alone(program: &std::path::Path, argument: &str, wait: std::time::Duration) -> Result<(), String> {
    if !program.is_absolute() || argument.contains(['"', '\0']) {
        return Err("a program by its full path and a plain argument only".into());
    }
    sys::run_alone(program, argument, wait)
}

/// Start `program` (a full path) with `args` and leave it running on its
/// own: none of this process's handles, no console window, outside this
/// process's job when the job allows it (a hook runner that ends its job
/// would end it too). Returns once it started.
///
/// keymotion-cli's hook mode hands an event to such a copy of itself when
/// Keymotion is not running, so the coding agent does not wait for the app
/// to start.
pub fn spawn_detached(program: &std::path::Path, args: &[&str]) -> Result<(), String> {
    if !program.is_absolute() || args.iter().any(|a| a.is_empty() || a.contains(['"', '\0'])) {
        return Err("a program by its full path and plain arguments only".into());
    }
    sys::spawn_detached(program, args)
}

fn web_address(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://")) && !url.chars().any(char::is_whitespace)
}

#[cfg(windows)]
mod sys {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    pub fn open(url: &str) -> Result<(), String> {
        let url = HSTRING::from(url);
        // SAFETY: NUL-terminated wide strings that outlive the call; no
        // owner window, parameters or working folder.
        let result = unsafe { ShellExecuteW(None, w!("open"), &url, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
        // Windows reports success as a value above 32 in the handle.
        let code = result.0 as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(format!("page not opened (Windows code {code})"))
        }
    }

    pub fn run_alone(program: &std::path::Path, argument: &str, wait: std::time::Duration) -> Result<(), String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            CreateProcessW, WaitForSingleObject, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
        };
        let program = HSTRING::from(program.as_os_str());
        let mut line: Vec<u16> = format!("\"{program}\" \"{argument}\"").encode_utf16().chain([0]).collect();
        let startup = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
        let mut process = PROCESS_INFORMATION::default();
        // SAFETY: NUL-terminated program and a writable, NUL-terminated
        // command line, both alive for the call; no security attributes,
        // environment or folder of its own; `startup` and `process` are
        // valid. Nothing is inherited (bInheritHandles FALSE).
        unsafe {
            CreateProcessW(
                &program,
                Some(windows::core::PWSTR(line.as_mut_ptr())),
                None,
                None,
                false,
                PROCESS_CREATION_FLAGS(0),
                None,
                PCWSTR::null(),
                &startup,
                &mut process,
            )
        }
        .map_err(|e| format!("not started: {}", e.message()))?;
        let ms = u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: the handles CreateProcessW just gave, each closed once.
        unsafe {
            let _ = WaitForSingleObject(process.hProcess, ms);
            let _ = CloseHandle(process.hThread);
            let _ = CloseHandle(process.hProcess);
        }
        Ok(())
    }

    pub fn spawn_detached(program: &std::path::Path, args: &[&str]) -> Result<(), String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            CreateProcessW, CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
            PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
        };
        let program = HSTRING::from(program.as_os_str());
        let mut text = format!("\"{program}\"");
        for a in args {
            text.push_str(&format!(" \"{a}\""));
        }
        let start = |flags: PROCESS_CREATION_FLAGS| -> windows::core::Result<()> {
            let mut line: Vec<u16> = text.encode_utf16().chain([0]).collect();
            let startup = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
            let mut process = PROCESS_INFORMATION::default();
            // SAFETY: as in `run_alone`: NUL-terminated program and a
            // writable, NUL-terminated command line alive for the call;
            // nothing is inherited. The two handles it gives are closed at
            // once: the program runs on.
            unsafe {
                CreateProcessW(
                    &program,
                    Some(windows::core::PWSTR(line.as_mut_ptr())),
                    None,
                    None,
                    false,
                    flags,
                    None,
                    PCWSTR::null(),
                    &startup,
                    &mut process,
                )?;
                let _ = CloseHandle(process.hThread);
                let _ = CloseHandle(process.hProcess);
            }
            Ok(())
        };
        let alone = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
        // A job that does not allow breaking away refuses the flag: then
        // inside it.
        start(alone | CREATE_BREAKAWAY_FROM_JOB)
            .or_else(|_| start(alone))
            .map_err(|e| format!("not started: {}", e.message()))
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn open(_url: &str) -> Result<(), String> {
        Err("opening pages is only available on Windows".into())
    }

    pub fn run_alone(_program: &std::path::Path, _argument: &str, _wait: std::time::Duration) -> Result<(), String> {
        Err("starting programs is only available on Windows".into())
    }

    pub fn spawn_detached(_program: &std::path::Path, _args: &[&str]) -> Result<(), String> {
        Err("starting programs is only available on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_addresses_are_opened() {
        assert!(web_address("https://github.com/o/r/issues/new?template=bug.yml&title=A%20B"));
        assert!(web_address("http://example.com/"));
        assert!(!web_address("C:/Windows/System32/cmd.exe"));
        assert!(!web_address("file:///C:/x"));
        assert!(!web_address("ms-settings:"));
        assert!(!web_address("https://a.b/ x"));
        assert!(open_url("calc.exe").is_err());
    }

    #[test]
    fn only_full_paths_and_plain_arguments_run() {
        let wait = std::time::Duration::from_secs(1);
        assert!(run_alone(std::path::Path::new("explorer.exe"), "x", wait).is_err());
        assert!(run_alone(std::path::Path::new(r"C:\Windows\explorer.exe"), "a\" b", wait).is_err());
        assert!(spawn_detached(std::path::Path::new("cmd.exe"), &["/c"]).is_err());
        assert!(spawn_detached(std::path::Path::new(r"C:\Windows\System32\cmd.exe"), &["a\"b"]).is_err());
        assert!(spawn_detached(std::path::Path::new(r"C:\Windows\System32\cmd.exe"), &[""]).is_err());
    }

    /// A detached program runs on its own: here one that writes a file.
    #[cfg(windows)]
    #[test]
    fn a_detached_program_runs_on_its_own() {
        let powershell = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        let target = std::env::temp_dir().join(format!("kb-winsys detached {}.txt", std::process::id()));
        let _ = std::fs::remove_file(&target);
        let script = format!("Set-Content -LiteralPath '{}' -Value ok", target.display());
        spawn_detached(&powershell, &["-NoProfile", "-Command", &script]).unwrap();
        let started = std::time::Instant::now();
        while !target.exists() && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(target.exists(), "the detached program ran");
        let _ = std::fs::remove_file(&target);
    }

    /// A program started this way holds none of the caller's handles: a
    /// child that inherited this test's stdout would keep it open.
    #[cfg(windows)]
    #[test]
    fn a_program_runs_and_ends() {
        let cmd = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
            .join("System32")
            .join("cmd.exe");
        // `"cmd.exe" "/c"` runs nothing and ends at once.
        run_alone(&cmd, "/c", std::time::Duration::from_secs(5)).unwrap();
    }
}
