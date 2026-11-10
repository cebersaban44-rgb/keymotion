//! Raw device access for keyboard control.
//!
//! * [`hid`]: enumerate HID interfaces and exchange feature/output/input
//!   reports. Used for the wired (USB) and 2.4G dongle command channels.
//! * [`ble`]: read-only Bluetooth LE access (battery level). The keyboard
//!   exposes no vendor command channel over Bluetooth, and its BLE OTA
//!   service must never be written to, so this module offers no write API
//!   at all.
//!
//! This crate moves bytes. Which bytes are safe to send is decided by the
//! protocol layer, never here.

pub mod hid;

#[cfg(windows)]
pub mod ble;

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("device path contains an interior NUL byte")]
    BadPath,
    #[error("report of {len} bytes exceeds the {max}-byte limit")]
    ReportTooLong { len: usize, max: usize },
    #[error("timed out after {0} ms waiting for an input report")]
    Timeout(i32),
    #[cfg(windows)]
    #[error("Windows error: {0}")]
    Windows(#[from] windows::core::Error),
    #[error("Bluetooth: {0}")]
    Bluetooth(String),
}

pub type Result<T> = std::result::Result<T, TransportError>;
