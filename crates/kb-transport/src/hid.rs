//! HID interface enumeration and report I/O.

use crate::{descriptor, Result, TransportError};
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

    /// Open one collection for report I/O. Its output limit is the longest
    /// output report its report descriptor declares
    /// ([`descriptor::output_limit`]); when the descriptor cannot be read,
    /// the channel writes nothing.
    pub fn open(&self, iface: &HidInterface) -> Result<HidChannel> {
        let path = CString::new(iface.path.as_bytes()).map_err(|_| TransportError::BadPath)?;
        let device = self.api.open_path(&path)?;
        let output_limit = match read_descriptor(&device) {
            Ok(desc) => descriptor::output_limit(&desc),
            Err(e) => {
                tracing::warn!(usage_page = format_args!("{:04X}", iface.usage_page), "descriptor unreadable: {e}");
                0
            }
        };
        Ok(HidChannel { device, info: iface.clone(), output_limit })
    }

    /// The report descriptor of the collection at `path`, as the OS rebuilds
    /// it. Nothing is sent to the device: Windows answers from what it read
    /// when the device was plugged in.
    pub fn report_descriptor_at(&self, path: &str) -> Result<Vec<u8>> {
        let path = CString::new(path.as_bytes()).map_err(|_| TransportError::BadPath)?;
        read_descriptor(&self.api.open_path(&path)?)
    }
}

fn read_descriptor(device: &HidDevice) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
    let n = device.get_report_descriptor(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// An open HID collection.
pub struct HidChannel {
    device: HidDevice,
    info: HidInterface,
    /// Longest output report this channel sends, report-ID byte included.
    output_limit: usize,
}

/// The "04 xx" family's command channel: the only collection feature
/// reports go to or come from.
pub const FEATURE_USAGE_PAGE: u16 = 0xFF13;

/// SONiX display channel (screen image pages).
pub const DISPLAY_USAGE_PAGE: u16 = 0xFF68;

/// Upper bound for one report including the report-ID byte. The largest
/// report we know of is the "AA" family's FF67 screen chunk: an 8-byte
/// header and 4096 bytes of image.
pub const MAX_REPORT: usize = 4104 + 1;

impl HidChannel {
    pub fn info(&self) -> &HidInterface {
        &self.info
    }

    /// Raw report descriptor as reconstructed by the OS (read-only).
    pub fn report_descriptor(&self) -> Result<Vec<u8>> {
        read_descriptor(&self.device)
    }

    /// Longest output report this channel sends, report-ID byte included.
    pub fn output_limit(&self) -> usize {
        self.output_limit
    }

    /// Change the longest output report this channel sends (report-ID byte
    /// included). [`Hid::open`] already sets it from the report descriptor;
    /// a caller can only confirm it after its own check of the descriptor
    /// or lower it. At most [`MAX_REPORT`].
    pub fn set_output_limit(&mut self, len: usize) -> Result<()> {
        check_len(len, MAX_REPORT)?;
        self.output_limit = len;
        Ok(())
    }

    /// Send a feature report. `report` starts with the report ID (0 when the
    /// device uses unnumbered reports).
    ///
    /// Only on the "04 xx" command channel (usage page 0xFF13): the "AA"
    /// family uses no feature reports, and its FF67 screen collection
    /// declares one that nothing may touch.
    pub fn send_feature(&self, report: &[u8]) -> Result<()> {
        feature_allowed(self.info.usage_page, "SET_FEATURE outside the FF13 command channel")?;
        check_len(report.len(), MAX_REPORT)?;
        tracing::trace!(len = report.len(), data = %hex(report), "SET_FEATURE");
        self.device.send_feature_report(report)?;
        Ok(())
    }

    /// Read a feature report of `len` bytes (including the report-ID byte).
    ///
    /// Only on the "04 xx" command channel (usage page 0xFF13). Above all
    /// never on a screen channel: a GET_FEATURE on FF68 locks the firmware
    /// until a power cycle (sibling AK35i; docs site: developers/protocol).
    pub fn get_feature(&self, report_id: u8, len: usize) -> Result<Vec<u8>> {
        feature_allowed(self.info.usage_page, "GET_FEATURE outside the FF13 command channel")?;
        check_len(len, MAX_REPORT)?;
        let mut buf = vec![0u8; len];
        buf[0] = report_id;
        let n = self.device.get_feature_report(&mut buf)?;
        buf.truncate(n);
        tracing::trace!(len = n, data = %hex(&buf), "GET_FEATURE");
        Ok(buf)
    }

    /// Send an output report. `report` starts with the report ID. At most
    /// [`Self::output_limit`] bytes.
    ///
    /// On Windows hidapi (2.6, C backend) writes with overlapped I/O and
    /// waits at most 1 s, its default, which the crate cannot change. A
    /// write that times out is reported as an error but stays pending, and
    /// the next write reuses its OVERLAPPED: after a failed write, drop the
    /// channel (closing it cancels pending I/O) and open it again.
    pub fn write_output(&self, report: &[u8]) -> Result<usize> {
        check_len(report.len(), self.output_limit)?;
        tracing::trace!(len = report.len(), data = %hex(report), "OUTPUT");
        Ok(self.device.write(report)?)
    }

    /// Wait up to `timeout_ms` for one input report. A negative timeout
    /// waits without limit (hidapi): pass a bound.
    pub fn read_input(&self, max_len: usize, timeout_ms: i32) -> Result<Vec<u8>> {
        check_len(max_len, MAX_REPORT)?;
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

fn check_len(len: usize, max: usize) -> Result<()> {
    if len > max {
        return Err(TransportError::ReportTooLong { len, max });
    }
    Ok(())
}

/// Feature reports go only to the "04 xx" command channel.
fn feature_allowed(usage_page: u16, refusal: &'static str) -> Result<()> {
    if usage_page == FEATURE_USAGE_PAGE {
        Ok(())
    } else {
        Err(TransportError::Refused(refusal))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_limits() {
        // A 64-byte command channel takes 65 bytes (report ID included), the
        // "04 xx" screen 4097 and the "AA" screen 4105 (limits from their
        // descriptors: descriptor.rs); nothing goes past MAX_REPORT.
        assert_eq!(MAX_REPORT, 4105);
        assert!(check_len(65, 65).is_ok());
        assert!(matches!(check_len(66, 65), Err(TransportError::ReportTooLong { len: 66, max: 65 })));
        assert!(check_len(4097, 4097).is_ok());
        assert!(matches!(check_len(4105, 4097), Err(TransportError::ReportTooLong { len: 4105, max: 4097 })));
        assert!(check_len(4105, MAX_REPORT).is_ok());
        assert!(matches!(check_len(4106, MAX_REPORT), Err(TransportError::ReportTooLong { len: 4106, max: 4105 })));
        assert!(matches!(check_len(1, 0), Err(TransportError::ReportTooLong { len: 1, max: 0 })), "unknown layout");
    }

    #[test]
    fn feature_reports_only_on_the_ff13_command_channel() {
        assert!(feature_allowed(0xFF13, "x").is_ok());
        // FF68 is the "04 xx" screen and the "AA" command channel, FF67 the
        // "AA" screen (its declared feature report is never touched), FF60
        // the receiver; standard collections take none either.
        for page in [0xFF68, 0xFF67, 0xFF60, 0xFFFF, 0x0001, 0x000C] {
            assert!(matches!(feature_allowed(page, "refused"), Err(TransportError::Refused("refused"))), "{page:04X}");
        }
    }

    /// Needs a SONiX keyboard on the cable (run with `--ignored`): every
    /// collection's descriptor reads through `report_descriptor_at`, the
    /// keyboard collection too (Windows opens it without read or write
    /// access). Nothing is sent to the keyboard.
    #[test]
    #[ignore = "needs a SONiX keyboard on the cable"]
    fn descriptors_of_every_collection_read() {
        let hid = Hid::new().expect("HID");
        let sonix: Vec<HidInterface> = hid.interfaces().into_iter().filter(|i| i.vid == 0x0C45).collect();
        assert!(!sonix.is_empty(), "no SONiX device found");
        for iface in &sonix {
            let d = hid.report_descriptor_at(&iface.path);
            println!(
                "{:04X}:{:04X} page {:04X} usage {:04X} if {}: {}",
                iface.vid,
                iface.pid,
                iface.usage_page,
                iface.usage,
                iface.interface_number,
                match &d {
                    Ok(b) => format!("{} bytes, {}", b.len(), hex(&b[..b.len().min(8)])),
                    Err(e) => format!("error {e}"),
                }
            );
            assert!(d.is_ok_and(|b| !b.is_empty()), "descriptor of {:04X}/{:04X}", iface.usage_page, iface.usage);
        }
    }
}
