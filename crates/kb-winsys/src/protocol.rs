//! An address scheme of the app's own (`keymotion:`), so a notification's
//! button can reach the app.
//!
//! A program installed without a package has two ways to hear a click on
//! its notification. One is the click event, which only fires while the
//! notification is still on screen. The other is an address the button
//! opens. Windows starts the scheme's program with the address, and the
//! running app hears of it through its single-instance lock. This works
//! from the notification centre too, after the notification has gone.
//! Microsoft lists it as supported for unpackaged apps without a COM
//! activator, and PowerToys uses it for its update buttons.
//!
//! The scheme lives under `HKCU\Software\Classes`, per user, so it needs no
//! administrator rights. The uninstaller removes it
//! (`windows/installer-hooks.nsh`).

use std::path::Path;

const CLASSES: &str = r"Software\Classes";

/// Open `scheme:` addresses with `"exe" "<address>"`. `name` is what
/// Windows shows if it ever asks which program to use. Writes only what
/// differs, so calling this at every start is cheap.
pub fn register(scheme: &str, name: &str, exe: &Path) -> Result<(), String> {
    register_under(CLASSES, scheme, name, exe)
}

/// The command `scheme:` addresses open now, if any.
pub fn command(scheme: &str) -> Option<String> {
    crate::startup::user_text(&format!(r"{CLASSES}\{scheme}\shell\open\command"), "")
}

/// The command line Windows runs for an address.
fn command_for(exe: &Path) -> String {
    format!("\"{}\" \"%1\"", exe.display())
}

fn register_under(root: &str, scheme: &str, name: &str, exe: &Path) -> Result<(), String> {
    let key = format!(r"{root}\{scheme}");
    let command = command_for(exe);
    let open = format!(r"{key}\shell\open\command");
    if crate::startup::user_text(&open, "").as_deref() == Some(command.as_str())
        && crate::startup::user_text(&key, "URL Protocol").is_some()
    {
        return Ok(());
    }
    sys::set(&key, "", &format!("URL:{name}"))?;
    sys::set(&key, "URL Protocol", "")?;
    sys::set(&format!(r"{key}\DefaultIcon"), "", &format!("\"{}\",0", exe.display()))?;
    sys::set(&open, "", &command)
}

#[cfg(windows)]
mod sys {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    /// Write a text value; the key is made if missing. An empty `name` is
    /// the key's default value.
    pub fn set(key: &str, name: &str, value: &str) -> Result<(), String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `data` is NUL-terminated UTF-16 and `cbdata` its size in
        // bytes, NUL included, as REG_SZ asks; the key is made if missing.
        let err = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &key,
                &name,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        };
        if err.is_err() {
            return Err(format!(
                "address scheme not registered: {}",
                windows::core::Error::from(err.to_hresult()).message()
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use std::path::Path;
        use windows::core::HSTRING;
        use windows::Win32::System::Registry::{RegDeleteKeyW, RegDeleteTreeW, HKEY_CURRENT_USER};

        #[test]
        fn writes_the_scheme_and_its_command() {
            // A key of its own, removed afterwards: the real classes are never touched.
            let root = format!(r"Software\Keymotion-test-protocol-{}", std::process::id());
            let exe = Path::new(r"C:\Users\A B\AppData\Local\Keymotion\keymotion.exe");
            let result = super::super::register_under(&root, "keymotion-test", "Keymotion", exe);
            let read = |key: &str, name: &str| crate::startup::user_text(&format!(r"{root}\keymotion-test{key}"), name);
            let (url, marker, icon, command) =
                (read("", ""), read("", "URL Protocol"), read(r"\DefaultIcon", ""), read(r"\shell\open\command", ""));
            // Again: nothing differs, nothing is written.
            let again = super::super::register_under(&root, "keymotion-test", "Keymotion", exe);
            let root_h = HSTRING::from(root.as_str());
            // SAFETY: a plain string naming the key the test made.
            unsafe {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &root_h);
                let _ = RegDeleteKeyW(HKEY_CURRENT_USER, &root_h);
            }
            result.unwrap();
            again.unwrap();
            assert_eq!(url.as_deref(), Some("URL:Keymotion"));
            assert_eq!(marker.as_deref(), Some(""));
            assert_eq!(icon.as_deref(), Some(r#""C:\Users\A B\AppData\Local\Keymotion\keymotion.exe",0"#));
            assert_eq!(command.as_deref(), Some(r#""C:\Users\A B\AppData\Local\Keymotion\keymotion.exe" "%1""#));
        }
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn set(_key: &str, _name: &str, _value: &str) -> Result<(), String> {
        Err("address schemes are only available on Windows".into())
    }
}
