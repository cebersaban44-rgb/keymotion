//! The name and logo Windows shows on the app's notifications.
//!
//! Windows names a notification after its app id. An installed app gets
//! its id from its Start menu shortcut, but a program run from a folder (the
//! development build, a copied exe) has none, and Windows then shows no name
//! or icon. Registering the id under `HKCU\Software\Classes\AppUserModelId`
//! with a display name and an icon file fixes both, without a shortcut.
//! Nothing here needs administrator rights.

use std::path::Path;

const ROOT: &str = r"Software\Classes\AppUserModelId";

/// Show notifications of `id` as `name` with the PNG at `icon`.
pub fn register(id: &str, name: &str, icon: &Path) -> Result<(), String> {
    sys::register(&format!(r"{ROOT}\{id}"), name, icon)
}

#[cfg(windows)]
mod sys {
    use std::path::Path;
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    fn set(key: &str, name: &str, value: &str) -> Result<(), String> {
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
            return Err(format!("app id not registered: {}", windows::core::Error::from(err.to_hresult()).message()));
        }
        Ok(())
    }

    pub fn register(key: &str, name: &str, icon: &Path) -> Result<(), String> {
        set(key, "DisplayName", name)?;
        set(key, "IconUri", &icon.display().to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows::Win32::System::Registry::{RegDeleteTreeW, RegGetValueW, RRF_RT_REG_SZ};

        fn read(key: &str, name: &str) -> Option<String> {
            let (key, name) = (HSTRING::from(key), HSTRING::from(name));
            let mut buf = [0u16; 512];
            let mut len = std::mem::size_of_val(&buf) as u32;
            // SAFETY: a valid output buffer and its size in bytes.
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
            err.is_ok().then(|| String::from_utf16_lossy(&buf[..len as usize / 2]).trim_end_matches('\0').to_string())
        }

        #[test]
        fn writes_the_name_and_the_icon() {
            // A key of its own, removed afterwards: the real list is never touched.
            let root = format!(r"Software\Keymotion-test-appid-{}", std::process::id());
            let key = format!(r"{root}\dev.keymotion");
            let result = register(&key, "Keymotion", Path::new(r"C:\Users\A B\AppData\Local\dev.keymotion\icon.png"));
            let name = read(&key, "DisplayName");
            let icon = read(&key, "IconUri");
            let root_h = HSTRING::from(root.as_str());
            // SAFETY: a plain string naming the key the test made.
            unsafe {
                let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &root_h);
                let _ = windows::Win32::System::Registry::RegDeleteKeyW(HKEY_CURRENT_USER, &root_h);
            }
            result.unwrap();
            assert_eq!(name.as_deref(), Some("Keymotion"));
            assert_eq!(icon.as_deref(), Some(r"C:\Users\A B\AppData\Local\dev.keymotion\icon.png"));
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use std::path::Path;

    pub fn register(_key: &str, _name: &str, _icon: &Path) -> Result<(), String> {
        Err("notification app ids are only available on Windows".into())
    }
}
