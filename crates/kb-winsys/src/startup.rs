//! Starting with Windows: a value in the per-user `Run` key, the list Task
//! Manager shows under "Startup apps". No administrator rights needed.
//!
//! Task Manager and Settings › Apps › Startup turn an entry off without
//! removing it: they write a flag under `Explorer\StartupApproved\Run`
//! (first byte odd = off). [`entry`] reports that; [`set`] clears it, since
//! the user just asked for the app to start.

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// A program's entry in the `Run` key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEntry {
    /// The command line stored, such as `"C:\…\keymotion.exe" --hidden`.
    pub command: String,
    /// Turned off in Task Manager or Settings; Windows skips it.
    pub disabled: bool,
}

/// The entry named `name`; `None` when there is none.
pub fn entry(name: &str) -> Option<RunEntry> {
    sys::entry(RUN, APPROVED, name)
}

/// Start `command` when the user signs in (replaces an older entry, and
/// turns it on if Task Manager had turned it off).
pub fn set(name: &str, command: &str) -> Result<(), String> {
    sys::set(RUN, APPROVED, name, command)
}

/// Remove the entry; nothing to remove is fine.
pub fn remove(name: &str) -> Result<(), String> {
    sys::remove(RUN, APPROVED, name)
}

/// The command line for `exe` with its arguments, quoted for the `Run` key.
pub fn command_line(exe: &std::path::Path, args: &[&str]) -> String {
    let mut line = format!("\"{}\"", exe.display());
    for a in args {
        line.push(' ');
        line.push_str(a);
    }
    line
}

#[cfg(windows)]
mod sys {
    use super::RunEntry;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    fn fail(what: &str, err: WIN32_ERROR) -> String {
        format!("{what}: {}", windows::core::Error::from(err.to_hresult()).message())
    }

    fn text(key: &str, name: &str) -> Option<String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut len = 0u32;
        // SAFETY: no buffer; the API only writes the size needed, in bytes.
        let err = unsafe { RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_SZ, None, None, Some(&mut len)) };
        if err.is_err() {
            return None;
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2).max(1)];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: a valid output buffer and its size in bytes; REG_SZ data
        // comes back NUL-terminated within it.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
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
        Some(String::from_utf16_lossy(&buf[..chars]).trim_end_matches('\0').to_string())
    }

    /// Turned off by Task Manager: the flag's first byte is odd.
    fn turned_off(key: &str, name: &str) -> bool {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut buf = [0u8; 32];
        let mut len = buf.len() as u32;
        // SAFETY: a valid output buffer and its size in bytes.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                RRF_RT_REG_BINARY,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut len),
            )
        };
        err.is_ok() && len > 0 && buf[0] & 1 == 1
    }

    fn delete(key: &str, name: &str) -> Result<(), String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        // SAFETY: plain strings; deleting a value that is not there reports
        // ERROR_FILE_NOT_FOUND (as does a key that is not there).
        let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &key, &name) };
        if err.is_ok() || err == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(fail("startup entry not removed", err))
        }
    }

    pub fn entry(run: &str, approved: &str, name: &str) -> Option<RunEntry> {
        let command = text(run, name)?;
        Some(RunEntry { command, disabled: turned_off(approved, name) })
    }

    pub fn set(run: &str, approved: &str, name: &str, command: &str) -> Result<(), String> {
        let (key, value) = (HSTRING::from(run), HSTRING::from(name));
        let data: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `data` is NUL-terminated UTF-16 and `cbdata` its size in
        // bytes, NUL included, as REG_SZ asks; the key is made if missing.
        let err = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &value,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        };
        if err.is_err() {
            return Err(fail("startup entry not saved", err));
        }
        delete(approved, name)
    }

    pub fn remove(run: &str, approved: &str, name: &str) -> Result<(), String> {
        delete(run, name)?;
        delete(approved, name)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::System::Registry::{RegDeleteTreeW, REG_BINARY};

        /// A key of its own under HKCU, removed afterwards: the real `Run`
        /// key is never touched by the tests.
        struct Scratch(String);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let key = HSTRING::from(self.0.as_str());
                // SAFETY: a plain string naming a key the test made.
                let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &key) };
                let _ = unsafe { windows::Win32::System::Registry::RegDeleteKeyW(HKEY_CURRENT_USER, &key) };
            }
        }

        #[test]
        fn set_read_turn_off_and_remove() {
            let root = Scratch(format!(r"Software\Keymotion-test-{}", std::process::id()));
            let run = format!(r"{}\Run", root.0);
            let approved = format!(r"{}\Approved", root.0);
            let name = "Keymotion";
            assert_eq!(entry(&run, &approved, name), None);

            let cmd = r#""C:\Program Files\Keymotion\keymotion.exe" --hidden"#;
            set(&run, &approved, name, cmd).unwrap();
            assert_eq!(entry(&run, &approved, name), Some(RunEntry { command: cmd.into(), disabled: false }));

            // Task Manager turns it off: 03 00 00 00 + a time stamp.
            let flag = [3u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
            let (key, value) = (HSTRING::from(approved.as_str()), HSTRING::from(name));
            // SAFETY: a valid buffer and its size in bytes.
            let err = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    &key,
                    &value,
                    REG_BINARY.0,
                    Some(flag.as_ptr().cast()),
                    flag.len() as u32,
                )
            };
            assert!(err.is_ok());
            assert!(entry(&run, &approved, name).unwrap().disabled);

            // Turning it on again clears Task Manager's flag.
            set(&run, &approved, name, cmd).unwrap();
            assert!(!entry(&run, &approved, name).unwrap().disabled);

            remove(&run, &approved, name).unwrap();
            assert_eq!(entry(&run, &approved, name), None);
            remove(&run, &approved, name).unwrap();
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::RunEntry;

    pub fn entry(_run: &str, _approved: &str, _name: &str) -> Option<RunEntry> {
        None
    }

    pub fn set(_run: &str, _approved: &str, _name: &str, _command: &str) -> Result<(), String> {
        Err("starting with the system is only available on Windows".into())
    }

    pub fn remove(_run: &str, _approved: &str, _name: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::command_line;
    use std::path::Path;

    #[test]
    fn quotes_the_program_path() {
        assert_eq!(
            command_line(Path::new(r"C:\Users\A B\AppData\Local\Keymotion\keymotion.exe"), &["--hidden"]),
            r#""C:\Users\A B\AppData\Local\Keymotion\keymotion.exe" --hidden"#
        );
        assert_eq!(command_line(Path::new(r"C:\k.exe"), &[]), r#""C:\k.exe""#);
    }
}
