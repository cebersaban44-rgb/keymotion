//! Opening a web page in the user's default browser.
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
}

#[cfg(not(windows))]
mod sys {
    pub fn open(_url: &str) -> Result<(), String> {
        Err("opening pages is only available on Windows".into())
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
}
