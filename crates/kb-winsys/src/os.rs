//! Which Windows this is, for the diagnostics report: the release name
//! ("25H2") and the full build ("26200.7171") as Settings → System → About
//! shows them. Read from the registry (`HKLM\…\Windows NT\CurrentVersion`).

/// The Windows release as the user would describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsRelease {
    /// "Windows 11" from build 22000 on, else "Windows 10". The registry's
    /// own product name still says "Windows 10" on Windows 11.
    pub product: &'static str,
    /// Edition, for example "Professional" or "Core" (Home).
    pub edition: Option<String>,
    /// Release name ("25H2"); `None` before Windows 10 20H2.
    pub display_version: Option<String>,
    pub build: u32,
    /// Update build revision: the part after the dot ("26200.7171").
    pub ubr: Option<u32>,
}

impl WindowsRelease {
    /// "Windows 11 Professional 25H2 (build 26200.7171)".
    pub fn describe(&self) -> String {
        let mut s = self.product.to_string();
        for part in [&self.edition, &self.display_version].into_iter().flatten() {
            s.push(' ');
            s.push_str(part);
        }
        match self.ubr {
            Some(ubr) => s.push_str(&format!(" (build {}.{ubr})", self.build)),
            None => s.push_str(&format!(" (build {})", self.build)),
        }
        s
    }
}

/// The product name for a build number.
pub fn product_of(build: u32) -> &'static str {
    if build >= 22_000 {
        "Windows 11"
    } else {
        "Windows 10"
    }
}

/// This computer's Windows release; `None` on other systems or when the
/// registry cannot be read.
pub fn windows_release() -> Option<WindowsRelease> {
    sys::windows_release()
}

#[cfg(windows)]
mod sys {
    use super::{product_of, WindowsRelease};
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};

    const KEY: PCWSTR = w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");

    fn text(name: &str) -> Option<String> {
        let name = HSTRING::from(name);
        let mut buf = [0u16; 128];
        let mut len = std::mem::size_of_val(&buf) as u32;
        // SAFETY: a valid output buffer and its size in bytes; the API
        // writes at most `len` bytes and NUL-terminates REG_SZ data.
        let err = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                KEY,
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        if err.is_err() {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..chars]);
        let s = s.trim_end_matches('\0').trim();
        (!s.is_empty()).then(|| s.to_string())
    }

    fn number(name: &str) -> Option<u32> {
        let name = HSTRING::from(name);
        let mut value = 0u32;
        let mut len = size_of::<u32>() as u32;
        // SAFETY: a valid u32 out pointer and its size, as the API asks.
        let err = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                KEY,
                &name,
                RRF_RT_REG_DWORD,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut len),
            )
        };
        err.is_ok().then_some(value)
    }

    pub fn windows_release() -> Option<WindowsRelease> {
        let build: u32 = text("CurrentBuildNumber")?.parse().ok()?;
        Some(WindowsRelease {
            product: product_of(build),
            edition: text("EditionID"),
            display_version: text("DisplayVersion"),
            build,
            ubr: number("UBR"),
        })
    }
}

#[cfg(not(windows))]
mod sys {
    use super::WindowsRelease;

    pub fn windows_release() -> Option<WindowsRelease> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_the_release_like_windows_does() {
        let r = WindowsRelease {
            product: product_of(26_200),
            edition: Some("Core".into()),
            display_version: Some("25H2".into()),
            build: 26_200,
            ubr: Some(7171),
        };
        assert_eq!(r.describe(), "Windows 11 Core 25H2 (build 26200.7171)");
        let old = WindowsRelease {
            product: product_of(19_045),
            edition: None,
            display_version: None,
            build: 19_045,
            ubr: None,
        };
        assert_eq!(old.describe(), "Windows 10 (build 19045)");
    }

    #[test]
    #[cfg(windows)]
    fn this_computer_has_a_release() {
        let r = windows_release().expect("the registry names the build");
        assert!(r.build >= 10_240, "{r:?}");
    }
}
