//! The user's own `PATH` (`HKCU\Environment`, value `Path`): whether a
//! folder is on it, adding one, taking it away.
//!
//! The GitHub copy of Keymotion puts `keymotion-cli` in its `bin` folder and
//! adds that folder here when the user turns agent access on, so terminals
//! and coding agents find `keymotion-cli` by its name (plan: `ajan-cli-
//! plan.md` §12.1, row 6). The Store copy needs none of it: its alias lives
//! in a folder Windows keeps on PATH.
//!
//! * The value keeps its type: `REG_EXPAND_SZ` (Windows' own, with
//!   `%USERPROFILE%`-style entries left unexpanded) stays so; a value made
//!   here is `REG_EXPAND_SZ` too. Entries are never reordered or expanded;
//!   ours goes at the end, and only ours is ever taken out.
//! * Entries compare as Windows reads them: any case, either slash, with or
//!   without a trailing backslash or quotes, `%NAME%` expanded.
//! * After a change, open programs are told (`WM_SETTINGCHANGE`,
//!   "Environment"); Explorer passes it on to what it starts next. A
//!   terminal that is already open keeps its old PATH until it restarts.

use std::path::Path;

/// Where Windows keeps the user's environment variables.
const ENVIRONMENT: &str = "Environment";
/// The value's name.
const PATH: &str = "Path";
/// Windows' longest environment value, in UTF-16 units, without the end.
const MAX_CHARS: usize = 32_766;

/// The folder is on the user's PATH.
pub fn has(dir: &Path) -> bool {
    sys::read(ENVIRONMENT, PATH).is_some_and(|(value, _)| contains(&value, &dir.display().to_string()))
}

/// Put the folder at the end of the user's PATH, if it is not on it yet,
/// and tell open programs. Whether it changed.
pub fn add(dir: &Path) -> Result<bool, String> {
    let changed = add_under(ENVIRONMENT, &dir.display().to_string())?;
    if changed {
        sys::announce();
    }
    Ok(changed)
}

/// Take the folder off the user's PATH (every entry that names it), and
/// tell open programs. Whether it changed.
pub fn remove(dir: &Path) -> Result<bool, String> {
    let changed = remove_under(ENVIRONMENT, &dir.display().to_string())?;
    if changed {
        sys::announce();
    }
    Ok(changed)
}

fn add_under(key: &str, dir: &str) -> Result<bool, String> {
    // A new value is REG_EXPAND_SZ, as Windows makes it.
    let (value, expand) = sys::read(key, PATH).unwrap_or((String::new(), true));
    let Some(next) = with_added(&value, dir) else { return Ok(false) };
    if next.encode_utf16().count() > MAX_CHARS {
        return Err("PATH is full: Windows takes no more than 32,766 characters in it".into());
    }
    sys::write(key, PATH, &next, expand)?;
    Ok(true)
}

fn remove_under(key: &str, dir: &str) -> Result<bool, String> {
    let Some((value, expand)) = sys::read(key, PATH) else { return Ok(false) };
    let Some(next) = without(&value, dir) else { return Ok(false) };
    sys::write(key, PATH, &next, expand)?;
    Ok(true)
}

/// An entry as Windows compares folders: `%NAME%` expanded, no quotes, one
/// kind of slash, no trailing backslash, any case.
fn normal(entry: &str) -> String {
    let expanded = expand(entry.trim().trim_matches('"'));
    let mut s = expanded.replace('/', "\\");
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    s.to_lowercase()
}

/// `%NAME%` replaced by the variable's value where it is set.
fn expand(entry: &str) -> String {
    let mut out = String::with_capacity(entry.len());
    let mut rest = entry;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Some entry of `value` (a PATH) names `dir`.
pub fn contains(value: &str, dir: &str) -> bool {
    let want = normal(dir);
    !want.is_empty() && value.split(';').any(|e| normal(e) == want)
}

/// `value` with `dir` at its end; `None` when it is on it already.
pub fn with_added(value: &str, dir: &str) -> Option<String> {
    if contains(value, dir) {
        return None;
    }
    let dir = dir.trim();
    Some(if value.is_empty() {
        dir.to_string()
    } else if value.ends_with(';') {
        format!("{value}{dir}")
    } else {
        format!("{value};{dir}")
    })
}

/// `value` without the entries that name `dir`; `None` when none does. The
/// other entries stay as they were, empty ones included.
pub fn without(value: &str, dir: &str) -> Option<String> {
    if !contains(value, dir) {
        return None;
    }
    let want = normal(dir);
    let kept: Vec<&str> = value.split(';').filter(|e| normal(e) != want).collect();
    let mut next = kept.join(";");
    // The `;` that followed our entry at the end goes with it.
    if !value.ends_with(';') {
        while next.ends_with(';') {
            next.pop();
        }
    }
    Some(next)
}

#[cfg(windows)]
mod sys {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::Registry::{
        RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE, RRF_NOEXPAND,
        RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    /// The text and whether it is `REG_EXPAND_SZ`; `None` when it is not
    /// there.
    pub fn read(key: &str, name: &str) -> Option<(String, bool)> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut len = 0u32;
        // SAFETY: no buffer; the API only writes the size needed, in bytes.
        if unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, &name, flags, None, None, Some(&mut len)) }.is_err() {
            return None;
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2).max(1)];
        let mut len = (buf.len() * 2) as u32;
        let mut kind = REG_VALUE_TYPE::default();
        // SAFETY: a valid output buffer and its size in bytes, and a valid
        // out pointer for the type.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                flags,
                Some(&mut kind),
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        if err.is_err() {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        let text = String::from_utf16_lossy(&buf[..chars]).trim_end_matches('\0').to_string();
        Some((text, kind == REG_EXPAND_SZ))
    }

    pub fn write(key: &str, name: &str, value: &str, expand: bool) -> Result<(), String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let kind = if expand { REG_EXPAND_SZ } else { REG_SZ };
        // SAFETY: `data` is NUL-terminated UTF-16 and `cbdata` its size in
        // bytes, NUL included, as REG_SZ and REG_EXPAND_SZ ask; the key is
        // made if missing.
        let err = unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, &key, &name, kind.0, Some(data.as_ptr().cast()), (data.len() * 2) as u32)
        };
        if err.is_err() {
            return Err(format!("PATH not changed: {}", windows::core::Error::from(err.to_hresult()).message()));
        }
        Ok(())
    }

    /// Tell open programs the environment changed, waiting at most a few
    /// seconds for any that hangs.
    pub fn announce() {
        let what = HSTRING::from("Environment");
        // SAFETY: `what` outlives the call; the result and the answer are
        // not needed (a hung window is skipped after the time-out).
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(PCWSTR(what.as_ptr()).0 as isize),
                SMTO_ABORTIFHUNG,
                3_000,
                None,
            );
        }
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn read(_key: &str, _name: &str) -> Option<(String, bool)> {
        None
    }
    pub fn write(_key: &str, _name: &str, _value: &str, _expand: bool) -> Result<(), String> {
        Err("PATH can be changed on Windows only".into())
    }
    pub fn announce() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = r"C:\Users\A B\AppData\Local\Keymotion\bin";

    #[test]
    fn entries_compare_as_windows_reads_them() {
        for same in [
            BIN,
            r"c:\users\a b\appdata\local\keymotion\bin",
            r"C:/Users/A B/AppData/Local/Keymotion/bin/",
            r#""C:\Users\A B\AppData\Local\Keymotion\bin\""#,
            r"  C:\Users\A B\AppData\Local\Keymotion\bin  ",
        ] {
            assert!(contains(&format!(r"C:\x;{same};D:\y"), BIN), "{same}");
        }
        assert!(!contains(r"C:\Users\A B\AppData\Local\Keymotion\bin2;C:\x", BIN));
        assert!(!contains(r"C:\Users\A B\AppData\Local\Keymotion", BIN));
        assert!(!contains("", BIN));
        assert!(!contains(";;", ""), "an empty folder is on no PATH");
        // A variable Windows would expand.
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        if !home.is_empty() {
            let ours = format!(r"{home}\AppData\Local\Keymotion\bin");
            assert!(contains(r"%USERPROFILE%\AppData\Local\Keymotion\bin;C:\x", &ours));
            assert!(contains(r"%NOT_A_VARIABLE_HERE%;%USERPROFILE%\AppData\Local\Keymotion\bin", &ours));
        }
    }

    #[test]
    fn ours_goes_at_the_end_and_comes_off_alone() {
        assert_eq!(with_added("", BIN).as_deref(), Some(BIN));
        assert_eq!(with_added(r"C:\x", BIN), Some(format!(r"C:\x;{BIN}")));
        assert_eq!(with_added(r"C:\x;", BIN), Some(format!(r"C:\x;{BIN}")));
        assert_eq!(with_added(&format!(r"C:\x;{BIN}\;D:\y"), BIN), None, "on it already");

        let theirs = r"%USERPROFILE%\.cargo\bin;;C:\x";
        let ours = with_added(theirs, BIN).unwrap();
        assert_eq!(without(&ours, BIN).as_deref(), Some(theirs), "the user's entries as they were");
        assert_eq!(without(theirs, BIN), None);
        assert_eq!(without(&format!(r"{BIN};C:\x;{BIN}\;"), BIN).as_deref(), Some(r"C:\x;"), "every entry naming it");
        assert_eq!(without(BIN, BIN).as_deref(), Some(""));
    }

    #[cfg(windows)]
    #[test]
    fn adds_and_removes_in_the_registry_keeping_the_type() {
        use windows::core::HSTRING;
        use windows::Win32::System::Registry::{RegDeleteKeyW, RegDeleteTreeW, HKEY_CURRENT_USER};
        // A key of its own, removed afterwards: the real Environment is
        // never touched.
        let key = format!(r"Software\Keymotion-test-path-{}", std::process::id());
        let run = || -> Result<(), String> {
            // No value yet: made as REG_EXPAND_SZ.
            assert!(add_under(&key, BIN)?);
            assert_eq!(sys::read(&key, PATH), Some((BIN.to_string(), true)));
            assert!(!add_under(&key, BIN)?, "twice changes nothing");
            assert!(remove_under(&key, BIN)?);
            assert_eq!(sys::read(&key, PATH), Some((String::new(), true)));
            // A plain REG_SZ value of the user's stays plain.
            sys::write(&key, PATH, r"C:\x", false)?;
            assert!(add_under(&key, BIN)?);
            assert_eq!(sys::read(&key, PATH), Some((format!(r"C:\x;{BIN}"), false)));
            assert!(remove_under(&key, &BIN.to_uppercase())?);
            assert_eq!(sys::read(&key, PATH), Some((r"C:\x".to_string(), false)));
            assert!(!remove_under(&key, BIN)?);
            Ok(())
        };
        let result = run();
        let h = HSTRING::from(key.as_str());
        // SAFETY: a plain string naming the key the test made.
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &h);
            let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &h);
        }
        result.unwrap();
    }
}
