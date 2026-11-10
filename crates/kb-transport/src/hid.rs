//! HID interface enumeration and report I/O.

use crate::{Result, TransportError};
use hidapi::{BusType, HidApi, HidDevice};
use std::ffi::CString;

/// Bus an interface was found on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Bluetooth,
    Other,
}

/// One HID top-level collection as reported by the OS.
///
/// On Windows every top-level collection is a separate device path, so a
/// single keyboard shows up as several of these.
#[derive(Debug, Clone)]
pub struct HidInterface {
    pub vid: u16,
    pub pid: u16,
    pub release: u16,
    pub usage_page: u16,
    pub usage: u16,
    /// USB interface number, or -1 when the OS does not report one (Bluetooth).
    pub interface_number: i32,
    pub bus: Bus,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub path: String,
}

impl HidInterface {
    /// Vendor-defined usage pages (0xFF00..=0xFFFF) carry proprietary
    /// command or data channels.
    pub fn is_vendor_defined(&self) -> bool {
        self.usage_page >= 0xFF00
    }
}

/// Shared handle to the OS HID subsystem.
pub struct Hid {
    api: HidApi,
}

impl Hid {
    pub fn new() -> Result<Self> {
        Ok(Self { api: HidApi::new()? })
    }

    /// Re-scan devices (hot-plug).
    pub fn refresh(&mut self) -> Result<()> {
        self.api.refresh_devices()?;
        Ok(())
    }

    /// Every HID collection currently present.
    pub fn interfaces(&self) -> Vec<HidInterface> {
        self.api
            .device_list()
            .map(|d| HidInterface {
                vid: d.vendor_id(),
                pid: d.product_id(),
                release: d.release_number(),
                usage_page: d.usage_page(),
                usage: d.usage(),
                interface_number: d.interface_number(),
                bus: match d.bus_type() {
                    BusType::Usb => Bus::Usb,
                    BusType::Bluetooth => Bus::Bluetooth,
                    _ => Bus::Other,
                },
                manufacturer: d.manufacturer_string().map(str::to_owned),
                product: d.product_string().map(str::to_owned),
                serial: d.serial_number().map(str::to_owned),
                path: d.path().to_string_lossy().into_owned(),
            })
            .collect()
    }

    /// Open one collection for report I/O.
    pub fn open(&self, iface: &HidInterface) -> Result<HidChannel> {
        let path = CString::new(iface.path.as_bytes()).map_err(|_| TransportError::BadPath)?;
        let device = self.api.open_path(&path)?;
        Ok(HidChannel { device, info: iface.clone() })
    }
}

/// An open HID collection.
pub struct HidChannel {
    device: HidDevice,
    info: HidInterface,
}

/// Upper bound for one report including the report-ID byte. The largest
/// report we know of is the 4096-byte display channel.
const MAX_REPORT: usize = 4096 + 1;

impl HidChannel {
    pub fn info(&self) -> &HidInterface {
        &self.info
    }

    /// Raw report descriptor as reconstructed by the OS (read-only).
    pub fn report_descriptor(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
        let n = self.device.get_report_descriptor(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Send a feature report. `report` starts with the report ID (0 when the
    /// device uses unnumbered reports).
    pub fn send_feature(&self, report: &[u8]) -> Result<()> {
        check_len(report.len())?;
        tracing::trace!(len = report.len(), data = %hex(report), "SET_FEATURE");
        self.device.send_feature_report(report)?;
        Ok(())
    }

    /// Read a feature report of `len` bytes (including the report-ID byte).
    pub fn get_feature(&self, report_id: u8, len: usize) -> Result<Vec<u8>> {
        check_len(len)?;
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        let n = self.device.get_feature_report(&mut buf)?;
        buf.truncate(n);
        tracing::trace!(len = n, data = %hex(&buf), "GET_FEATURE");
        Ok(buf)
    }

    /// Send an output report. `report` starts with the report ID.
    pub fn write_output(&self, report: &[u8]) -> Result<usize> {
        check_len(report.len())?;
        tracing::trace!(len = report.len(), data = %hex(report), "OUTPUT");
        Ok(self.device.write(report)?)
    }

    /// Wait up to `timeout_ms` for one input report.
    pub fn read_input(&self, max_len: usize, timeout_ms: i32) -> Result<Vec<u8>> {
        check_len(max_len)?;
        let mut buf = vec![0u8; max_len];
        let n = self.device.read_timeout(&mut buf, timeout_ms)?;
        if n == 0 {
            return Err(TransportError::Timeout(timeout_ms));
        }
        buf.truncate(n);
        tracing::trace!(len = n, data = %hex(&buf), "INPUT");
        Ok(buf)
    }
}

fn check_len(len: usize) -> Result<()> {
    if len > MAX_REPORT {
        return Err(TransportError::ReportTooLong { len, max: MAX_REPORT });
    }
    Ok(())
}

/// Space-separated lowercase hex, for logs.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02x}"));
    }
    s
}
