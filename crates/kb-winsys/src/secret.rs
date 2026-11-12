//! Secrets kept on disk for the current Windows user: Windows Data
//! Protection (DPAPI, `CryptProtectData`). Only this user on this computer
//! (or the same user's roaming profile on a domain) can open them again.
//! Other systems have no such store; callers keep the value as it is there.

use std::fmt;

/// Why a secret could not be sealed or opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// This system has no data protection (not Windows).
    Unsupported,
    /// Windows refused: another user's or another computer's data, or
    /// damaged bytes. The Windows error code.
    Failed(i32),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("data protection is not available on this system"),
            Self::Failed(code) => write!(f, "data protection failed (0x{:08X})", *code as u32),
        }
    }
}

impl std::error::Error for SecretError {}

/// Whether [`protect`] can work on this system.
pub const AVAILABLE: bool = cfg!(windows);

/// Seal `plain` for the current user. `entropy` binds the result to its
/// purpose: [`unprotect`] needs the same bytes.
pub fn protect(plain: &[u8], entropy: &[u8]) -> Result<Vec<u8>, SecretError> {
    sys::protect(plain, entropy)
}

/// Open what [`protect`] sealed (same user, same `entropy`).
pub fn unprotect(sealed: &[u8], entropy: &[u8]) -> Result<Vec<u8>, SecretError> {
    sys::unprotect(sealed, entropy)
}

#[cfg(windows)]
mod sys {
    use super::SecretError;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        // The APIs take `*mut` but do not write to input blobs.
        CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr().cast_mut() }
    }

    /// Copy an output blob and free it (the API allocates it with `LocalAlloc`).
    fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if out.pbData.is_null() {
            return Vec::new();
        }
        // SAFETY: on success the API set `pbData` to `cbData` readable bytes.
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        // SAFETY: `pbData` came from the API's `LocalAlloc` and is freed once.
        unsafe { LocalFree(Some(HLOCAL(out.pbData.cast()))) };
        bytes
    }

    pub fn protect(plain: &[u8], entropy: &[u8]) -> Result<Vec<u8>, SecretError> {
        let input = blob(plain);
        let extra = blob(entropy);
        let mut out = CRYPT_INTEGER_BLOB::default();
        // SAFETY: valid input blobs that outlive the call, a valid out blob;
        // no prompt (`UI_FORBIDDEN`), so it never blocks on a dialog.
        unsafe { CryptProtectData(&input, None, Some(&extra), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
            .map_err(|e| SecretError::Failed(e.code().0))?;
        Ok(take(out))
    }

    pub fn unprotect(sealed: &[u8], entropy: &[u8]) -> Result<Vec<u8>, SecretError> {
        let input = blob(sealed);
        let extra = blob(entropy);
        let mut out = CRYPT_INTEGER_BLOB::default();
        // SAFETY: as in `protect`; the description out-pointer is not asked for.
        unsafe { CryptUnprotectData(&input, None, Some(&extra), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }
            .map_err(|e| SecretError::Failed(e.code().0))?;
        Ok(take(out))
    }
}

#[cfg(not(windows))]
mod sys {
    use super::SecretError;

    pub fn protect(_plain: &[u8], _entropy: &[u8]) -> Result<Vec<u8>, SecretError> {
        Err(SecretError::Unsupported)
    }

    pub fn unprotect(_sealed: &[u8], _entropy: &[u8]) -> Result<Vec<u8>, SecretError> {
        Err(SecretError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn sealed_secrets_open_only_with_the_same_entropy() {
        let plain = "https://calendar.google.com/calendar/ical/x%40gmail.com/private-abc/basic.ics";
        let sealed = protect(plain.as_bytes(), b"purpose").unwrap();
        assert!(!sealed.windows(plain.len()).any(|w| w == plain.as_bytes()), "not readable on disk");
        assert_eq!(unprotect(&sealed, b"purpose").unwrap(), plain.as_bytes());
        assert!(matches!(unprotect(&sealed, b"other"), Err(SecretError::Failed(_))));
        let mut damaged = sealed.clone();
        let mid = damaged.len() / 2;
        damaged[mid] ^= 0xFF;
        assert!(unprotect(&damaged, b"purpose").is_err());
        assert_eq!(unprotect(&protect(b"", b"purpose").unwrap(), b"purpose").unwrap(), b"");
    }

    #[test]
    #[cfg(not(windows))]
    fn other_systems_say_so() {
        assert_eq!(protect(b"x", b"y"), Err(SecretError::Unsupported));
        assert!(!AVAILABLE);
    }
}
