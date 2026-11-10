//! Turn raw HID collections and paired BLE devices into a list of keyboards.

use kb_devices::{
    catalogue, match_ble_name, match_interface, Bus, Capability, ConnectionKind, InterfaceProbe, InterfaceRole, Model,
};
use kb_transport::hid::{self, HidInterface};
use serde::Serialize;

/// Minimal view of a paired Bluetooth LE device, independent of the
/// platform module that produced it.
#[derive(Debug, Clone, Serialize)]
pub struct BlePeer {
    pub name: String,
    pub address: u64,
    pub address_text: String,
    pub connected: bool,
}

/// One physical keyboard in one connection mode.
#[derive(Debug, Clone)]
pub struct DetectedKeyboard {
    pub model: &'static Model,
    pub connection: ConnectionKind,
    /// USB product string when the OS reports a meaningful one.
    pub product: Option<String>,
    /// Vendor command channel (wired or dongle).
    pub command: Option<HidInterface>,
    /// Vendor display channel (wired only).
    pub display: Option<HidInterface>,
    /// Number of standard HID collections (keyboard, consumer, mouse...).
    pub standard_collections: usize,
    /// Matching paired BLE device, for Bluetooth connections.
    pub ble: Option<BlePeer>,
}

impl DetectedKeyboard {
    /// Features usable right now, given the connection mode and the vendor
    /// interfaces the OS actually exposed.
    pub fn capabilities(&self) -> Vec<Capability> {
        let Some(mode) = self.model.mode(self.connection) else {
            return Vec::new();
        };
        mode.capabilities
            .iter()
            .copied()
            .filter(|c| match c {
                Capability::Battery => self.connection != ConnectionKind::Bluetooth || self.ble.is_some(),
                Capability::Screen => self.display.is_some(),
                _ => self.connection == ConnectionKind::Bluetooth || self.command.is_some(),
            })
            .collect()
    }

    /// Stable identity for UI keys and change detection.
    pub fn key(&self) -> String {
        format!("{}:{:?}", self.model.id, self.connection)
    }
}

/// Group matching HID collections per (model, mode) and attach BLE peers.
///
/// Placeholder product strings that some Bluetooth stacks report
/// ("Model_Number") are dropped.
pub fn detect(interfaces: &[HidInterface], ble: &[BlePeer]) -> Vec<DetectedKeyboard> {
    let Ok(models) = catalogue() else {
        tracing::error!("embedded model catalogue failed to parse");
        return Vec::new();
    };
    let mut out: Vec<DetectedKeyboard> = Vec::new();

    for iface in interfaces {
        let probe = InterfaceProbe {
            vid: iface.vid,
            pid: iface.pid,
            usage_page: iface.usage_page,
            product: iface.product.as_deref(),
            bus: match iface.bus {
                hid::Bus::Usb => Bus::Usb,
                hid::Bus::Bluetooth => Bus::Bluetooth,
                hid::Bus::Other => Bus::Other,
            },
        };
        let Some(m) = match_interface(models, &probe) else { continue };
        let idx = match out.iter().position(|k| k.model.id == m.model.id && k.connection == m.mode.kind) {
            Some(i) => i,
            None => {
                out.push(DetectedKeyboard {
                    model: m.model,
                    connection: m.mode.kind,
                    product: iface.product.clone().filter(|p| is_meaningful_product(p)),
                    command: None,
                    display: None,
                    standard_collections: 0,
                    ble: None,
                });
                out.len() - 1
            }
        };
        let k = &mut out[idx];
        match m.role {
            InterfaceRole::Command => k.command = Some(iface.clone()),
            InterfaceRole::Display => k.display = Some(iface.clone()),
            InterfaceRole::Standard => k.standard_collections += 1,
        }
    }

    for peer in ble {
        let Some((model, _)) = match_ble_name(models, &peer.name) else { continue };
        if let Some(k) = out.iter_mut().find(|k| k.model.id == model.id && k.connection == ConnectionKind::Bluetooth) {
            k.ble = Some(peer.clone());
        } else if peer.connected {
            out.push(DetectedKeyboard {
                model,
                connection: ConnectionKind::Bluetooth,
                product: None,
                command: None,
                display: None,
                standard_collections: 0,
                ble: Some(peer.clone()),
            });
        }
    }

    out.sort_by_key(|k| (k.model.id.clone(), k.connection as u8));
    out
}

fn is_meaningful_product(p: &str) -> bool {
    let p = p.trim();
    !p.is_empty() && !p.eq_ignore_ascii_case("Model_Number") && !p.eq_ignore_ascii_case("Model Number")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(pid: u16, up: u16, bus: hid::Bus, product: &str) -> HidInterface {
        HidInterface {
            vid: 0x0C45,
            pid,
            release: 0x0138,
            usage_page: up,
            usage: 1,
            interface_number: -1,
            bus,
            manufacturer: None,
            product: Some(product.into()),
            serial: None,
            path: format!("path-{pid:x}-{up:x}"),
        }
    }

    #[test]
    fn wired_groups_command_and_display() {
        let list = [
            iface(0x8009, 0x0001, hid::Bus::Usb, "SONiX AK980 PRO"),
            iface(0x8009, 0xFF13, hid::Bus::Usb, "SONiX AK980 PRO"),
            iface(0x8009, 0xFF68, hid::Bus::Usb, "SONiX AK980 PRO"),
        ];
        let found = detect(&list, &[]);
        assert_eq!(found.len(), 1);
        let k = &found[0];
        assert_eq!(k.connection, ConnectionKind::Usb);
        assert!(k.command.is_some() && k.display.is_some());
        assert_eq!(k.standard_collections, 1);
        assert!(k.capabilities().contains(&Capability::Screen));
    }

    #[test]
    fn bluetooth_merges_ble_peer_and_hides_placeholder_name() {
        let list = [iface(0xFEFE, 0x0001, hid::Bus::Bluetooth, "Model_Number")];
        let peer = BlePeer {
            name: "AK980 PRO-1".into(),
            address: 0xAABBCCDDEEFF,
            address_text: "AA:BB:CC:DD:EE:FF".into(),
            connected: true,
        };
        let found = detect(&list, &[peer]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].product, None);
        assert_eq!(found[0].capabilities(), vec![Capability::Battery]);
    }
}
