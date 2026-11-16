//! Reading the registry: text values and the names of sub-keys, under the
//! current user or the computer. Nothing here writes.

/// Which part of the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// `HKEY_CURRENT_USER`.
    User,
    /// `HKEY_LOCAL_MACHINE`.
    Machine,
}

/// A text value (`REG_SZ` or `REG_EXPAND_SZ`, not expanded); `None` when
/// it is not there.
pub fn text(root: Root, key: &str, name: &str) -> Option<String> {
    sys::text(root, key, name)
}

/// A `REG_DWORD` value.
pub fn number(root: Root, key: &str, name: &str) -> Option<u32> {
    sys::number(root, key, name)
}

/// The names of the keys right under `key` (empty when it is not there).
pub fn subkeys(root: Root, key: &str) -> Vec<String> {
    sys::subkeys(root, key)
}

#[cfg(windows)]
mod sys {
    use super::Root;
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
        KEY_ENUMERATE_SUB_KEYS, RRF_NOEXPAND, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };

    fn hkey(root: Root) -> HKEY {
        match root {
            Root::User => HKEY_CURRENT_USER,
            Root::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    pub fn text(root: Root, key: &str, name: &str) -> Option<String> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut len = 0u32;
        // SAFETY: no buffer; the API only writes the size needed, in bytes.
        if unsafe { RegGetValueW(hkey(root), &key, &name, flags, None, None, Some(&mut len)) }.is_err() {
            return None;
        }
        let mut buf = vec![0u16; (len as usize).div_ceil(2).max(1)];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: a valid output buffer and its size in bytes.
        let err = unsafe {
            RegGetValueW(hkey(root), &key, &name, flags, None, Some(buf.as_mut_ptr().cast()), Some(&mut len))
        };
        if err.is_err() {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        Some(String::from_utf16_lossy(&buf[..chars]).trim_end_matches('\0').to_string())
    }

    pub fn number(root: Root, key: &str, name: &str) -> Option<u32> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(name));
        let mut value = 0u32;
        let mut len = size_of::<u32>() as u32;
        // SAFETY: a valid output buffer of `len` bytes.
        let err = unsafe {
            RegGetValueW(hkey(root), &key, &name, RRF_RT_REG_DWORD, None, Some((&raw mut value).cast()), Some(&mut len))
        };
        err.is_ok().then_some(value)
    }

    pub fn subkeys(root: Root, key: &str) -> Vec<String> {
        let mut opened = HKEY::default();
        // SAFETY: a valid out pointer; the key is closed below.
        if unsafe { RegOpenKeyExW(hkey(root), &HSTRING::from(key), None, KEY_ENUMERATE_SUB_KEYS, &mut opened) }.is_err()
        {
            return Vec::new();
        }
        let mut out = Vec::new();
        for index in 0u32.. {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            // SAFETY: a valid buffer and its length in characters.
            let err = unsafe {
                RegEnumKeyExW(opened, index, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None)
            };
            if err.is_err() {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]));
        }
        // SAFETY: the key opened above, closed once.
        let _ = unsafe { RegCloseKey(opened) };
        out
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Root;

    pub fn text(_root: Root, _key: &str, _name: &str) -> Option<String> {
        None
    }
    pub fn number(_root: Root, _key: &str, _name: &str) -> Option<u32> {
        None
    }
    pub fn subkeys(_root: Root, _key: &str) -> Vec<String> {
        Vec::new()
    }
}
