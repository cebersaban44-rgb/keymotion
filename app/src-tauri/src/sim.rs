//! A simulated AK980 PRO on firmware 769 for developing the "AA" family
//! without one (plan K-11): `KEYMOTION_SIM=aa769` in a debug build made
//! with the `sim` feature (`pnpm tauri dev --features sim`).
//!
//! The device actor then lists this keyboard and nothing else: no HID
//! device is enumerated or opened and Bluetooth is not scanned, so a real
//! keyboard on the same computer is never reached (driver.rs `Ports::Sim`).
//! The app also runs under its own identifier (`dev.keymotion.sim`), with
//! its own data folder, log folder and single-instance lock, beside an
//! installed copy.
//!
//! `KEYMOTION_SIM_FAULTS` (comma-separated) makes the keyboard misbehave:
//! `insane` (an unknown effect mode: read-only), `update-pending`
//! (firmware status 1), `silent` (no answer to the device information),
//! `color-unknown` (another tool wrote the effect, so its colour reads back
//! white).
//!
//! Release builds never contain this module: it needs the `sim` feature,
//! which does not build without debug assertions (lib.rs).

use crate::aa::AaPort;
use kb_core::aa::sim::{Faults, Sim};
use kb_core::aa::UsbIdentity;
use kb_transport::hid::{Bus, HidInterface};

/// The environment variable that turns the simulator on.
pub const ENV: &str = "KEYMOTION_SIM";
/// Faults to simulate.
pub const ENV_FAULTS: &str = "KEYMOTION_SIM_FAULTS";
/// The identifier the app runs under meanwhile.
pub const IDENTIFIER: &str = "dev.keymotion.sim";

const COMMAND: &str = "sim:aa769:ff68";
const DISPLAY: &str = "sim:aa769:ff67";
const KEYBOARD: &str = "sim:aa769:keyboard";

// Firmware 769's collections, from its "Request support" report (#154).
const FF68: &str = "06 68 FF 09 61 A1 01 09 62 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 95 40 91 \
                    02 C0";
const FF67: &str = "06 67 FF 09 61 A1 01 09 64 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 96 08 10 \
                    91 02 09 62 15 00 26 FF 00 75 08 95 40 B1 02 C0";

fn hex(s: &str) -> Vec<u8> {
    s.split_whitespace().filter_map(|b| u8::from_str_radix(b, 16).ok()).collect()
}

/// A simulated keyboard on the cable.
pub struct SimBoard {
    sim: Sim,
}

impl SimBoard {
    /// The keyboard `name` asks for (`aa769`), misbehaving as `faults`
    /// says.
    pub fn new(name: &str, faults: &str) -> Result<Self, String> {
        if name != "aa769" {
            return Err(format!("{ENV}={name}: the simulator knows only `aa769`"));
        }
        let sim = Sim::ak980();
        {
            let mut s = sim.state();
            for fault in faults.split(',').map(str::trim).filter(|f| !f.is_empty()) {
                match fault {
                    // An effect mode the sanity check does not know.
                    "insane" => s.effect[0] = 99,
                    // `firmwareStatus` 1.
                    "update-pending" => s.info[32] = 1,
                    "silent" => s.faults = Faults { silent_info: true, ..s.faults.clone() },
                    "color-unknown" => {
                        let e = s.effect;
                        s.set_effect_by_another_tool(e);
                    }
                    other => return Err(format!("{ENV_FAULTS}: unknown fault `{other}`")),
                }
            }
        }
        Ok(Self { sim })
    }

    /// The keyboard from the environment, if it asks for one.
    pub fn from_env() -> Option<Result<Self, String>> {
        Self::from_vars(std::env::var(ENV).ok(), std::env::var(ENV_FAULTS).ok())
    }

    fn from_vars(name: Option<String>, faults: Option<String>) -> Option<Result<Self, String>> {
        let name = name.filter(|v| !v.trim().is_empty())?;
        Some(Self::new(name.trim(), &faults.unwrap_or_default()))
    }

    /// The simulated keyboard (what it holds and received).
    #[cfg(test)]
    pub fn sim(&self) -> &Sim {
        &self.sim
    }

    pub fn usb(&self) -> UsbIdentity {
        Sim::usb()
    }

    /// Its collections, as the OS would list them: the keyboard, FF68 and
    /// FF67, named "AK980 PRO", `0C45:8039` at release 0x0301.
    pub fn interfaces(&self) -> Vec<HidInterface> {
        let usb = self.usb();
        [(0x0001, 0x0006, KEYBOARD), (0xFF68, 0x0061, COMMAND), (0xFF67, 0x0061, DISPLAY)]
            .into_iter()
            .map(|(usage_page, usage, path)| HidInterface {
                vid: 0x0C45,
                pid: usb.pid,
                release: usb.release,
                usage_page,
                usage,
                interface_number: -1,
                bus: Bus::Usb,
                manufacturer: Some("SONiX".into()),
                product: Some("AK980 PRO".into()),
                serial: None,
                path: path.into(),
            })
            .collect()
    }

    pub fn report_descriptor_at(&self, path: &str) -> Result<Vec<u8>, String> {
        match path {
            COMMAND => Ok(hex(FF68)),
            DISPLAY => Ok(hex(FF67)),
            _ => Err("no descriptor".into()),
        }
    }

    pub fn command_port(&self, path: &str) -> Result<AaPort, String> {
        match path {
            COMMAND => Ok(AaPort::new(self.sim.command())),
            _ => Err("keyboard disconnected".into()),
        }
    }

    pub fn display_port(&self, path: &str) -> Result<AaPort, String> {
        match path {
            DISPLAY => Ok(AaPort::new(self.sim.display())),
            _ => Err("keyboard disconnected".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_simulated_keyboard_is_the_ak980_pro_on_firmware_769() {
        let board = SimBoard::new("aa769", "").unwrap();
        let found = kb_core::classify(&board.interfaces(), &[], |i| board.report_descriptor_at(&i.path).ok());
        assert!(found.unknown.is_empty());
        assert_eq!(found.keyboards.len(), 1);
        let k = &found.keyboards[0];
        assert_eq!(k.protocol(), kb_devices::Protocol::AjazzAa);
        assert_eq!((k.mode.pid, k.firmware), (0x8039, Some(0x0301)));
        assert_eq!(k.record_id(), "ajazz-ak980-pro.aa");
        assert_eq!(k.command.as_ref().map(|c| c.path.as_str()), Some(COMMAND));
        assert_eq!(k.display.as_ref().map(|c| c.path.as_str()), Some(DISPLAY));
        assert!(board.command_port(DISPLAY).is_err() && board.display_port(COMMAND).is_err());
    }

    #[test]
    fn the_environment_names_the_keyboard() {
        assert!(SimBoard::from_vars(None, Some("insane".into())).is_none(), "no simulator asked for");
        assert!(SimBoard::from_vars(Some(" ".into()), None).is_none());
        let b = SimBoard::from_vars(Some("aa769".into()), Some("update-pending".into())).unwrap().unwrap();
        assert_eq!(b.sim().state().info[32], 1);
        assert!(SimBoard::from_vars(Some("aa769".into()), Some("nope".into())).unwrap().is_err());
        // The real variables: not set where tests run.
        if std::env::var(ENV).is_err() {
            assert!(SimBoard::from_env().is_none());
        }
        // Not the installed app's identifier: its own folders and lock.
        assert_ne!(IDENTIFIER, "dev.keymotion");
        assert!(IDENTIFIER.starts_with("dev.keymotion."));
    }

    #[test]
    fn faults_are_named() {
        assert!(SimBoard::new("aa769", "insane, update-pending,silent,color-unknown").is_ok());
        assert!(SimBoard::new("aa769", "loud").is_err());
        assert!(SimBoard::new("ff13", "").is_err(), "only the 769 board");
        let b = SimBoard::new("aa769", "update-pending").unwrap();
        assert_eq!(b.sim().state().info[32], 1);
    }
}
