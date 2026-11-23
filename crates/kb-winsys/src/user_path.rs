//! The user's own `PATH` (`HKCU\Environment`, value `Path`): whether a
//! folder is on it, adding one, taking it away.
//!
//! The GitHub copy of Keymotion puts `keymotion-cli` in its `bin` folder and
//! adds that folder here when the user turns agent access on, so terminals
//! and coding agents find `keymotion-cli` by its name (plan: `ajan-cli-
//! plan.md` §12.1, row 6). The Store copy needs none of it: its alias lives
//! in a folder Windows keeps on PATH.
//!
//! * Nothing of the user's is lost. The value is read and written back as
//!   the UTF-16 it is (no text conversion on the way), in the type it has
//!   (`REG_EXPAND_SZ`, with `%USERPROFILE%`-style entries left unexpanded,
//!   stays so; a value made here is `REG_EXPAND_SZ` too); entries are never
//!   reordered or expanded; ours goes at the end. Only a value that is not
//!   there counts as empty: any other failure to read it (a type that is not
//!   text, access refused, a value that keeps growing) is an error and
//!   nothing is written. Before a change, the value as it was is kept beside
//!   our record ([`RECORD_KEY`], `PathBefore`).
//! * Entries compare as Windows compares names: `%NAME%` expanded, either
//!   slash, with or without a trailing backslash or quotes, ordinal and
//!   ignoring case (`CompareStringOrdinal`, so a Turkish `ı` or `İ` is no
//!   other letter's twin). A folder with `;` in it goes in quotes.
//! * Only what we added is ever taken away: [`add`] records the folder
//!   (`PathAdded`), [`remove`] takes it off only when that record names it,
//!   so a folder the user put there themselves stays.
//! * After a change, tell open programs ([`announce`], `WM_SETTINGCHANGE`
//!   "Environment"); it may wait a few seconds for a program that hangs, so
//!   callers run it on a thread of its own. A terminal that is already open
//!   keeps its old PATH until it restarts.

use std::path::Path;

/// Where Windows keeps the user's environment variables.
const ENVIRONMENT: &str = "Environment";
/// The value's name.
const PATH: &str = "Path";
/// Where Keymotion records the folder it added and the value before it.
pub const RECORD_KEY: &str = r"Software\Keymotion\UserPath";
const ADDED: &str = "PathAdded";
const BEFORE: &str = "PathBefore";
/// Windows' longest environment value, in UTF-16 units, without the end.
const MAX_UNITS: usize = 32_766;
const SEMI: u16 = b';' as u16;

/// A text value as the registry holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raw {
    /// UTF-16, without the terminating NUL.
    pub units: Vec<u16>,
    /// `REG_EXPAND_SZ` rather than `REG_SZ`.
    pub expand: bool,
}

/// Where the PATH and our record are (the real ones, or a test's).
#[derive(Debug, Clone, Copy)]
struct Keys<'a> {
    environment: &'a str,
    record: &'a str,
}

const REAL: Keys<'static> = Keys { environment: ENVIRONMENT, record: RECORD_KEY };

/// The folder is on the user's PATH. An error when the PATH cannot be read.
pub fn has(dir: &Path) -> Result<bool, String> {
    has_in(REAL, &dir.display().to_string())
}

/// Put the folder at the end of the user's PATH, if it is not on it yet,
/// and record that we did. Whether it changed; call [`announce`] then.
pub fn add(dir: &Path) -> Result<bool, String> {
    add_in(REAL, &dir.display().to_string())
}

/// Take the folder off the user's PATH, if we added it (every entry that
/// names it). Whether it changed; call [`announce`] then.
pub fn remove(dir: &Path) -> Result<bool, String> {
    remove_in(REAL, &dir.display().to_string())
}

/// Tell open programs the environment changed. It may take a few seconds
/// (each hung window is given up on after a timeout): run it on a thread of
/// its own.
pub fn announce() {
    sys::announce();
}

fn has_in(keys: Keys<'_>, dir: &str) -> Result<bool, String> {
    Ok(sys::read(keys.environment, PATH)?.is_some_and(|raw| contains(&raw.units, dir)))
}

fn add_in(keys: Keys<'_>, dir: &str) -> Result<bool, String> {
    let current = sys::read(keys.environment, PATH)?;
    let raw = current.clone().unwrap_or(Raw { units: Vec::new(), expand: true });
    let Some(next) = with_added(&raw.units, dir) else { return Ok(false) };
    if next.len() > MAX_UNITS {
        return Err("PATH is full: Windows takes no more than 32,766 characters in it".into());
    }
    // The value as it was, and then the folder, recorded before the change:
    // a record without the change is harmless, a change without it is not.
    if let Some(before) = &current {
        sys::write(keys.record, BEFORE, before)?;
    }
    sys::write(keys.record, ADDED, &Raw { units: dir.encode_utf16().collect(), expand: false })?;
    sys::write(keys.environment, PATH, &Raw { units: next, expand: raw.expand })?;
    Ok(true)
}

fn remove_in(keys: Keys<'_>, dir: &str) -> Result<bool, String> {
    let ours = sys::read(keys.record, ADDED)?.is_some_and(|r| same(&String::from_utf16_lossy(&r.units), dir));
    if !ours {
        return Ok(false);
    }
    let changed = match sys::read(keys.environment, PATH)? {
        Some(raw) => match without(&raw.units, dir) {
            Some(next) => {
                sys::write(keys.environment, PATH, &Raw { units: next, expand: raw.expand })?;
                true
            }
            None => false,
        },
        None => false,
    };
    sys::delete(keys.record, ADDED)?;
    sys::delete(keys.record, BEFORE)?;
    Ok(changed)
}

/// An entry as Windows compares folders: `%NAME%` expanded, no quotes, one
/// kind of slash, no trailing backslash.
fn normal(entry: &str) -> String {
    let expanded = expand(entry.trim().trim_matches('"'));
    let mut s = expanded.trim_matches('"').replace('/', "\\");
    while s.len() > 3 && s.ends_with('\\') {
        s.pop();
    }
    s
}

/// Two folders are the same, as Windows compares names.
fn same(a: &str, b: &str) -> bool {
    let (a, b) = (normal(a), normal(b));
    !a.is_empty() && sys::equal_ignoring_case(&a, &b)
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

/// The entries of a PATH, as UTF-16 slices (empty ones included): split at
/// each `;` outside double quotes, as Windows reads it.
fn entries(value: &[u16]) -> Vec<&[u16]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    for (i, &c) in value.iter().enumerate() {
        if c == u16::from(b'"') {
            quoted = !quoted;
        } else if c == SEMI && !quoted {
            out.push(&value[start..i]);
            start = i + 1;
        }
    }
    out.push(&value[start..]);
    out
}

/// Some entry of `value` (a PATH) names `dir`.
pub fn contains(value: &[u16], dir: &str) -> bool {
    entries(value).into_iter().any(|e| same(&String::from_utf16_lossy(e), dir))
}

/// The entry for `dir`: in quotes when it has a `;`, which would otherwise
/// split it in two.
fn entry_for(dir: &str) -> Vec<u16> {
    let dir = dir.trim();
    let text = if dir.contains(';') { format!("\"{dir}\"") } else { dir.to_string() };
    text.encode_utf16().collect()
}

/// `value` with `dir` at its end; `None` when it is on it already.
pub fn with_added(value: &[u16], dir: &str) -> Option<Vec<u16>> {
    if contains(value, dir) {
        return None;
    }
    let mut next = value.to_vec();
    if !next.is_empty() && next.last() != Some(&SEMI) {
        next.push(SEMI);
    }
    next.extend(entry_for(dir));
    Some(next)
}

/// `value` without the entries that name `dir`; `None` when none does. The
/// other entries stay exactly as they were, empty ones included.
pub fn without(value: &[u16], dir: &str) -> Option<Vec<u16>> {
    if !contains(value, dir) {
        return None;
    }
    let kept: Vec<&[u16]> = entries(value).into_iter().filter(|e| !same(&String::from_utf16_lossy(e), dir)).collect();
    let mut next = kept.join(&SEMI);
    // The `;` that went before our entry at the end goes with it.
    if value.last() != Some(&SEMI) {
        while next.last() == Some(&SEMI) {
            next.pop();
        }
    }
    Some(next)
}

#[cfg(windows)]
mod sys {
    use super::Raw;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, LPARAM, WIN32_ERROR, WPARAM};
    use windows::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
        RRF_NOEXPAND, RRF_RT_ANY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    fn fail(what: &str, err: WIN32_ERROR) -> String {
        format!("{what}: {}", windows::core::Error::from(err.to_hresult()).message())
    }

    /// The value; `None` only when it is not there. A value that is not
    /// text, or that cannot be read, is an error.
    pub fn read(key: &str, name: &str) -> Result<Option<Raw>, String> {
        let (key_h, name_h) = (HSTRING::from(key), HSTRING::from(name));
        // A value that grows between asking its size and reading it is read
        // again, a few times.
        for _ in 0..5 {
            let mut len = 0u32;
            let mut kind = REG_VALUE_TYPE::default();
            // SAFETY: no buffer; the API writes the type and the size needed.
            let err = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    &key_h,
                    &name_h,
                    RRF_RT_ANY | RRF_NOEXPAND,
                    Some(&mut kind),
                    None,
                    Some(&mut len),
                )
            };
            if err == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if err.is_err() {
                return Err(fail(&format!("{name} not read"), err));
            }
            if kind != REG_SZ && kind != REG_EXPAND_SZ {
                return Err(format!("{name} is not text in the registry (type {}); nothing was changed", kind.0));
            }
            let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
            let mut got = (buf.len() * 2) as u32;
            // SAFETY: a valid output buffer and its size in bytes.
            let err = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    &key_h,
                    &name_h,
                    RRF_RT_ANY | RRF_NOEXPAND,
                    Some(&mut kind),
                    Some(buf.as_mut_ptr().cast()),
                    Some(&mut got),
                )
            };
            if err == ERROR_MORE_DATA {
                continue;
            }
            if err == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if err.is_err() {
                return Err(fail(&format!("{name} not read"), err));
            }
            if kind != REG_SZ && kind != REG_EXPAND_SZ {
                return Err(format!("{name} is not text in the registry (type {}); nothing was changed", kind.0));
            }
            buf.truncate((got as usize / 2).min(buf.len()));
            while buf.last() == Some(&0) {
                buf.pop();
            }
            return Ok(Some(Raw { units: buf, expand: kind == REG_EXPAND_SZ }));
        }
        Err(format!("{name} kept changing while it was read; nothing was changed"))
    }

    pub fn write(key: &str, name: &str, value: &Raw) -> Result<(), String> {
        let (key, name_h) = (HSTRING::from(key), HSTRING::from(name));
        let data: Vec<u16> = value.units.iter().copied().chain(std::iter::once(0)).collect();
        let kind = if value.expand { REG_EXPAND_SZ } else { REG_SZ };
        // SAFETY: `data` is NUL-terminated UTF-16 and `cbdata` its size in
        // bytes, NUL included, as REG_SZ and REG_EXPAND_SZ ask; the key is
        // made if missing.
        let err = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &name_h,
                kind.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        };
        if err.is_err() {
            return Err(fail(&format!("{name} not written"), err));
        }
        Ok(())
    }

    pub fn delete(key: &str, name: &str) -> Result<(), String> {
        let (key, name_h) = (HSTRING::from(key), HSTRING::from(name));
        // SAFETY: plain strings; a value or key that is not there reports
        // ERROR_FILE_NOT_FOUND.
        let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, &name_h) };
        if err.is_ok() || err == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(fail(&format!("{name} not removed"), err))
        }
    }

    /// Windows' own comparison of names: ordinal, ignoring case.
    pub fn equal_ignoring_case(a: &str, b: &str) -> bool {
        let (a, b): (Vec<u16>, Vec<u16>) = (a.encode_utf16().collect(), b.encode_utf16().collect());
        // SAFETY: two valid slices; the API reads them only.
        unsafe { CompareStringOrdinal(&a, &b, true) == CSTR_EQUAL }
    }

    /// Tell open programs the environment changed, giving up on each one
    /// that hangs.
    pub fn announce() {
        let what = HSTRING::from("Environment");
        // SAFETY: `what` outlives the call; the result and the answer are
        // not needed.
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
    use super::Raw;

    pub fn read(_key: &str, _name: &str) -> Result<Option<Raw>, String> {
        Err("PATH can be read on Windows only".into())
    }
    pub fn write(_key: &str, _name: &str, _value: &Raw) -> Result<(), String> {
        Err("PATH can be changed on Windows only".into())
    }
    pub fn delete(_key: &str, _name: &str) -> Result<(), String> {
        Ok(())
    }
    pub fn equal_ignoring_case(a: &str, b: &str) -> bool {
        a.to_uppercase() == b.to_uppercase()
    }
    pub fn announce() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = r"C:\Users\A B\AppData\Local\Keymotion\bin";

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn s(units: &[u16]) -> String {
        String::from_utf16_lossy(units)
    }

    #[test]
    fn entries_compare_as_windows_reads_them() {
        for same in [
            BIN,
            r"c:\users\a b\appdata\local\keymotion\bin",
            r"C:/Users/A B/AppData/Local/Keymotion/bin/",
            r#""C:\Users\A B\AppData\Local\Keymotion\bin\""#,
            r"  C:\Users\A B\AppData\Local\Keymotion\bin  ",
        ] {
            assert!(contains(&u(&format!(r"C:\x;{same};D:\y")), BIN), "{same}");
        }
        assert!(!contains(&u(r"C:\Users\A B\AppData\Local\Keymotion\bin2;C:\x"), BIN));
        assert!(!contains(&u(r"C:\Users\A B\AppData\Local\Keymotion"), BIN));
        assert!(!contains(&u(""), BIN));
        assert!(!contains(&u(";;"), ""), "an empty folder is on no PATH");
        // A variable Windows would expand.
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        if !home.is_empty() {
            let ours = format!(r"{home}\AppData\Local\Keymotion\bin");
            assert!(contains(&u(r"%USERPROFILE%\AppData\Local\Keymotion\bin;C:\x"), &ours));
            assert!(contains(&u(r"%NOT_A_VARIABLE_HERE%;%USERPROFILE%\AppData\Local\Keymotion\bin"), &ours));
        }
    }

    #[cfg(windows)]
    #[test]
    fn case_is_ignored_the_way_windows_ignores_it() {
        // Ordinal: the Turkish dotless ı is not an i, nor İ an I.
        assert!(contains(&u(r"C:\Users\ŞAHİN\bin"), r"c:\users\şahİn\BIN"));
        assert!(!contains(&u(r"C:\Users\Işık\bin"), r"C:\Users\Isik\bin"));
        assert!(!contains(&u(r"C:\Users\İrem\bin"), r"C:\Users\Irem\bin"));
    }

    #[test]
    fn ours_goes_at_the_end_and_comes_off_alone() {
        assert_eq!(with_added(&[], BIN).map(|v| s(&v)).as_deref(), Some(BIN));
        assert_eq!(with_added(&u(r"C:\x"), BIN).map(|v| s(&v)), Some(format!(r"C:\x;{BIN}")));
        assert_eq!(with_added(&u(r"C:\x;"), BIN).map(|v| s(&v)), Some(format!(r"C:\x;{BIN}")));
        assert_eq!(with_added(&u(&format!(r"C:\x;{BIN}\;D:\y")), BIN), None, "on it already");

        let theirs = u(r"%USERPROFILE%\.cargo\bin;;C:\x");
        let ours = with_added(&theirs, BIN).unwrap();
        assert_eq!(without(&ours, BIN), Some(theirs.clone()), "the user's entries as they were");
        assert_eq!(without(&theirs, BIN), None);
        assert_eq!(without(&u(&format!(r"{BIN};C:\x;{BIN}\;")), BIN).map(|v| s(&v)).as_deref(), Some(r"C:\x;"));
        assert_eq!(without(&u(BIN), BIN), Some(Vec::new()));
    }

    #[test]
    fn a_folder_with_a_semicolon_goes_in_quotes() {
        let odd = r"C:\Users\a;b\Keymotion\bin";
        let next = with_added(&u(r"C:\x"), odd).unwrap();
        assert_eq!(s(&next), format!("C:\\x;\"{odd}\""));
        // Read back as one entry, as Windows reads it: added once, taken
        // off whole.
        assert_eq!(with_added(&next, odd), None);
        assert_eq!(without(&next, odd).map(|v| s(&v)).as_deref(), Some(r"C:\x"));
    }

    #[test]
    fn what_is_not_text_survives_untouched() {
        // An unpaired surrogate in someone else's entry comes back as it was.
        let mut theirs = u(r"C:\x");
        theirs.push(0xD800);
        theirs.extend(u(r";C:\y"));
        let ours = with_added(&theirs, BIN).unwrap();
        assert_eq!(without(&ours, BIN), Some(theirs));
    }

    #[cfg(windows)]
    mod registry {
        use super::super::*;
        use super::{s, u, BIN};
        use windows::core::HSTRING;
        use windows::Win32::System::Registry::{
            RegDeleteKeyW, RegDeleteTreeW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY, REG_MULTI_SZ,
        };

        /// A key of its own, removed afterwards: the real Environment and
        /// the real record are never touched.
        struct Scratch(String);

        impl Scratch {
            fn new(name: &str) -> Self {
                Scratch(format!(r"Software\Keymotion-test-path-{name}-{}", std::process::id()))
            }
            fn keys(&self) -> (String, String) {
                (format!(r"{}\Environment", self.0), format!(r"{}\Record", self.0))
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let h = HSTRING::from(self.0.as_str());
                // SAFETY: a plain string naming the key the test made.
                unsafe {
                    let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &h);
                    let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &h);
                }
            }
        }

        fn raw_value(key: &str, kind: u32, data: &[u8]) {
            let (k, n) = (HSTRING::from(key), HSTRING::from("Path"));
            // SAFETY: plain strings and a byte buffer of the given length.
            let err = unsafe {
                RegSetKeyValueW(HKEY_CURRENT_USER, &k, &n, kind, Some(data.as_ptr().cast()), data.len() as u32)
            };
            assert!(err.is_ok());
        }

        #[test]
        fn adds_and_removes_keeping_the_type_and_a_copy() {
            let scratch = Scratch::new("round");
            let (env, rec) = scratch.keys();
            let keys = Keys { environment: &env, record: &rec };
            // No value yet: made as REG_EXPAND_SZ.
            assert!(add_in(keys, BIN).unwrap());
            assert_eq!(sys::read(&env, PATH).unwrap(), Some(Raw { units: u(BIN), expand: true }));
            assert!(!add_in(keys, BIN).unwrap(), "twice changes nothing");
            assert!(has_in(keys, BIN).unwrap());
            assert!(remove_in(keys, BIN).unwrap());
            assert_eq!(sys::read(&env, PATH).unwrap(), Some(Raw { units: Vec::new(), expand: true }));
            assert_eq!(sys::read(&rec, ADDED).unwrap(), None, "the record goes with it");
            // A plain REG_SZ value of the user's stays plain; what it was is kept.
            sys::write(&env, PATH, &Raw { units: u(r"C:\x"), expand: false }).unwrap();
            assert!(add_in(keys, BIN).unwrap());
            assert_eq!(
                sys::read(&env, PATH).unwrap().map(|r| (s(&r.units), r.expand)),
                Some((format!(r"C:\x;{BIN}"), false))
            );
            assert_eq!(sys::read(&rec, BEFORE).unwrap().map(|r| s(&r.units)).as_deref(), Some(r"C:\x"));
            assert!(remove_in(keys, &BIN.to_uppercase()).unwrap());
            assert_eq!(sys::read(&env, PATH).unwrap().map(|r| s(&r.units)).as_deref(), Some(r"C:\x"));
            assert!(!remove_in(keys, BIN).unwrap());
        }

        #[test]
        fn a_folder_the_user_put_there_stays() {
            let scratch = Scratch::new("theirs");
            let (env, rec) = scratch.keys();
            let keys = Keys { environment: &env, record: &rec };
            sys::write(&env, PATH, &Raw { units: u(&format!(r"C:\x;{BIN}")), expand: true }).unwrap();
            assert!(!add_in(keys, BIN).unwrap(), "already there: nothing added, nothing recorded");
            assert!(!remove_in(keys, BIN).unwrap());
            assert_eq!(sys::read(&env, PATH).unwrap().map(|r| s(&r.units)), Some(format!(r"C:\x;{BIN}")));
        }

        #[test]
        fn a_path_that_does_not_read_is_never_written() {
            let scratch = Scratch::new("odd");
            let (env, rec) = scratch.keys();
            let keys = Keys { environment: &env, record: &rec };
            // Another type than text: refused, left as it was.
            let multi: Vec<u8> = "C:\\x\0C:\\y\0\0".encode_utf16().flat_map(u16::to_le_bytes).collect();
            raw_value(&env, REG_MULTI_SZ.0, &multi);
            assert!(add_in(keys, BIN).unwrap_err().contains("not text"));
            assert!(has_in(keys, BIN).is_err());
            raw_value(&env, REG_BINARY.0, &[1, 2, 3]);
            assert!(add_in(keys, BIN).is_err());
            assert_eq!(sys::read(&rec, ADDED).unwrap(), None, "nothing recorded either");
        }
    }
}
