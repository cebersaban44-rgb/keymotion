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
    /// Firmware version (USB `bcdDevice`) of a keyboard on the cable. Over
    /// the receiver or Bluetooth the OS reports the receiver's or the radio's
    /// version instead, so it is left out there.
    pub firmware: Option<u16>,
    /// Vendor channels found per role: (command, display). Identical
    /// keyboards on the same connection share one entry, and each brings its
    /// own channels, so these tell how many there are.
    channels: (usize, usize),
}

/// Features this app drives in `kind`, out of what the model file says the
/// keyboard offers there.
///
/// - USB: the FF13 command channel and the FF68 screen channel. Keyboard
///   options (`04 17`) are not built yet, and the firmware has no battery
///   reading over USB.
/// - Bluetooth: no vendor channel; only the standard GATT battery service.
/// - 2.4 GHz receiver: no driver yet (the other modes come after USB), so
///   nothing is offered even though the model file lists what the receiver
///   is said to carry.
pub fn app_capabilities(model: &Model, kind: ConnectionKind) -> Vec<Capability> {
    let Some(mode) = model.mode(kind) else {
        return Vec::new();
    };
    mode.capabilities
        .iter()
        .copied()
        .filter(|c| match kind {
            ConnectionKind::Usb => !matches!(c, Capability::Options | Capability::Battery),
            ConnectionKind::Bluetooth => *c == Capability::Battery,
            ConnectionKind::Dongle => false,
        })
        .collect()
}

impl DetectedKeyboard {
    /// Features usable right now: what the app drives in this connection
    /// mode, given the interfaces the OS actually exposed.
    pub fn capabilities(&self) -> Vec<Capability> {
        app_capabilities(self.model, self.connection)
            .into_iter()
            .filter(|c| match c {
                Capability::Battery => self.ble.is_some(),
                Capability::Screen => self.display.is_some(),
                _ => self.command.is_some(),
            })
            .collect()
    }

    /// Stable identity for UI keys and change detection.
    pub fn key(&self) -> String {
        format!("{}:{:?}", self.model.id, self.connection)
    }

    /// How many physical keyboards stand behind this entry (at least one):
    /// every keyboard has one command and one display channel at most.
    pub fn units(&self) -> usize {
        self.channels.0.max(self.channels.1).max(1)
    }
}

/// Keyboards on the USB cable, counting identical ones separately. Writes
/// go to one keyboard; with more than one on the cable the app cannot tell
/// which one the user means, so it must not write at all.
pub fn wired_count(found: &[DetectedKeyboard]) -> usize {
    found.iter().filter(|k| k.connection == ConnectionKind::Usb).map(DetectedKeyboard::units).sum()
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
        let Some(m) = match_interface(models, &probe(iface)) else { continue };
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
                    firmware: (m.mode.kind == ConnectionKind::Usb).then_some(iface.release),
                    channels: (0, 0),
                });
                out.len() - 1
            }
        };
        let k = &mut out[idx];
        match m.role {
            InterfaceRole::Command => {
                k.command = Some(iface.clone());
                k.channels.0 += 1;
            }
            InterfaceRole::Display => {
                k.display = Some(iface.clone());
                k.channels.1 += 1;
            }
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
                firmware: None,
                channels: (0, 0),
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

/// SONiX's USB vendor id, which AJAZZ keyboards carry.
pub const SONIX_VID: u16 = 0x0C45;

/// A keyboard that looks like a SONiX or AJAZZ one but matches no model
/// file: the app can't drive it, and the user can ask for support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKeyboard {
    pub vid: u16,
    pub pid: u16,
    pub bus: hid::Bus,
    pub release: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// Its HID collections (usage page, usage, USB interface number), in
    /// the order the OS lists them.
    pub collections: Vec<(u16, u16, i32)>,
    /// The OS paths of those collections, same order (for reading their
    /// descriptors; never shown or sent).
    pub paths: Vec<String>,
}

impl UnknownKeyboard {
    /// Stable identity: ids and bus.
    pub fn key(&self) -> String {
        let bus = match self.bus {
            hid::Bus::Usb => "usb",
            hid::Bus::Bluetooth => "bluetooth",
            hid::Bus::Other => "other",
        };
        format!("{:04X}:{:04X}:{bus}", self.vid, self.pid)
    }

    /// The name to show: the product string, else the maker's.
    pub fn name(&self) -> Option<&str> {
        self.product
            .as_deref()
            .filter(|p| is_meaningful_product(p))
            .or(self.manufacturer.as_deref())
            .map(str::trim)
            .filter(|n| !n.is_empty())
    }
}

/// Keyboards that look like SONiX or AJAZZ ones (SONiX's vendor id, or
/// "AJAZZ" in their names) but match no model. Only devices with a
/// standard keyboard collection count, so a SONiX webcam's buttons don't.
pub fn unknown_keyboards(interfaces: &[HidInterface]) -> Vec<UnknownKeyboard> {
    let Ok(models) = catalogue() else { return Vec::new() };
    unknown_among(interfaces, models, &[])
}

/// [`unknown_keyboards`], with the keyboards in `rejected` (model id and
/// connection) counted as unknown although their IDs and name match.
fn unknown_among(
    interfaces: &[HidInterface],
    models: &'static [Model],
    rejected: &[(&str, ConnectionKind)],
) -> Vec<UnknownKeyboard> {
    let named = |s: &Option<String>| s.as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("ajazz"));
    // Each keyboard, and whether it matched a model whose shape it lacks:
    // that one is a keyboard for sure, keyboard collection or not.
    let mut out: Vec<(UnknownKeyboard, bool)> = Vec::new();
    for iface in interfaces {
        let rejected_model = match match_interface(models, &probe(iface)) {
            Some(m) if rejected.contains(&(m.model.id.as_str(), m.mode.kind)) => true,
            Some(_) => continue,
            None => false,
        };
        if !rejected_model && iface.vid != SONIX_VID && !named(&iface.product) && !named(&iface.manufacturer) {
            continue;
        }
        let same = |k: &UnknownKeyboard| {
            k.vid == iface.vid && k.pid == iface.pid && k.bus == iface.bus && k.product == iface.product
        };
        let (k, known) = match out.iter().position(|(k, _)| same(k)) {
            Some(i) => &mut out[i],
            None => {
                out.push((
                    UnknownKeyboard {
                        vid: iface.vid,
                        pid: iface.pid,
                        bus: iface.bus,
                        release: iface.release,
                        manufacturer: iface.manufacturer.clone(),
                        product: iface.product.clone(),
                        collections: Vec::new(),
                        paths: Vec::new(),
                    },
                    false,
                ));
                out.last_mut().expect("just pushed")
            }
        };
        *known |= rejected_model;
        k.collections.push((iface.usage_page, iface.usage, iface.interface_number));
        k.paths.push(iface.path.clone());
    }
    // Generic desktop (0x01), keyboard (0x06).
    let mut out: Vec<UnknownKeyboard> = out
        .into_iter()
        .filter(|(k, known)| *known || k.collections.iter().any(|&(page, usage, _)| page == 0x0001 && usage == 0x0006))
        .map(|(k, _)| k)
        .collect();
    out.sort_by_key(UnknownKeyboard::key);
    out
}

/// What the app can drive, and what it can only offer "Request support" for.
#[derive(Debug, Clone, Default)]
pub struct Found {
    pub keyboards: Vec<DetectedKeyboard>,
    pub unknown: Vec<UnknownKeyboard>,
}

/// [`detect`] and [`unknown_keyboards`] in one go, with the vendor channels
/// of every keyboard on the cable checked against the protocol's report
/// layout ([`crate::shape`]).
///
/// Other boards share the AK980 PRO's IDs, and some could share its name
/// too (another firmware, another command family). One whose command or
/// screen channel has another layout is not the model: nothing may be sent
/// to it, so it is listed with the unknown keyboards, where the user can
/// ask for support, instead of as a keyboard every action fails on.
///
/// `descriptor` gives a collection's report descriptor, as the OS keeps it
/// (reading it sends nothing to the keyboard). `None` (it could not be
/// read) leaves the keyboard as found: opening the channel checks the
/// layout again before anything is sent.
pub fn classify(
    interfaces: &[HidInterface],
    ble: &[BlePeer],
    mut descriptor: impl FnMut(&HidInterface) -> Option<Vec<u8>>,
) -> Found {
    let Ok(models) = catalogue() else {
        tracing::error!("embedded model catalogue failed to parse");
        return Found::default();
    };
    let mut rejected: Vec<(&str, ConnectionKind)> = Vec::new();
    for iface in interfaces {
        let Some(m) = match_interface(models, &probe(iface)) else { continue };
        let fits: fn(&[u8]) -> bool = match (m.mode.kind, m.role) {
            (ConnectionKind::Usb, InterfaceRole::Command) => crate::shape::is_command_shape,
            (ConnectionKind::Usb, InterfaceRole::Display) => crate::shape::is_display_shape,
            _ => continue,
        };
        let key = (m.model.id.as_str(), m.mode.kind);
        if rejected.contains(&key) {
            continue;
        }
        if let Some(desc) = descriptor(iface) {
            if !fits(&desc) {
                // Every enumeration finds it again; the app logs the change.
                tracing::debug!(
                    model = %m.model.id,
                    usage_page = format_args!("{:04X}", iface.usage_page),
                    shape = ?crate::shape::report_shape(&desc),
                    "name matches, report layout does not: listed as unsupported"
                );
                rejected.push(key);
            }
        }
    }
    let mut keyboards = detect(interfaces, ble);
    keyboards.retain(|k| !rejected.contains(&(k.model.id.as_str(), k.connection)));
    Found { keyboards, unknown: unknown_among(interfaces, models, &rejected) }
}

/// What the model catalogue needs to know about one HID collection.
fn probe(iface: &HidInterface) -> InterfaceProbe<'_> {
    InterfaceProbe {
        vid: iface.vid,
        pid: iface.pid,
        usage_page: iface.usage_page,
        product: iface.product.as_deref(),
        bus: match iface.bus {
            hid::Bus::Usb => Bus::Usb,
            hid::Bus::Bluetooth => Bus::Bluetooth,
            hid::Bus::Other => Bus::Other,
        },
    }
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
    fn wired_reports_firmware() {
        let list = [iface(0x8009, 0xFF13, hid::Bus::Usb, "SONiX AK980 PRO")];
        assert_eq!(detect(&list, &[])[0].firmware, Some(0x0138));
        // The receiver's own version is not the keyboard's.
        let list = [iface(0xFEFE, 0xFF60, hid::Bus::Usb, "")];
        assert_eq!(detect(&list, &[])[0].firmware, None);
    }

    #[test]
    fn two_identical_keyboards_on_the_cable_are_counted() {
        let one = [
            iface(0x8009, 0x0001, hid::Bus::Usb, "SONiX AK980 PRO"),
            iface(0x8009, 0xFF13, hid::Bus::Usb, "SONiX AK980 PRO"),
            iface(0x8009, 0xFF68, hid::Bus::Usb, "SONiX AK980 PRO"),
        ];
        assert_eq!(wired_count(&detect(&one, &[])), 1);

        // A second keyboard of the same model: same IDs, other paths. It
        // merges into the same entry, so only the channel count shows it.
        let second = one.clone().map(|mut i| {
            i.path.push_str("-second");
            i
        });
        let both: Vec<_> = one.iter().chain(second.iter()).cloned().collect();
        let found = detect(&both, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].units(), 2);
        assert_eq!(wired_count(&found), 2);

        // The receiver and Bluetooth do not count: nothing is written there.
        let mut mixed = one.to_vec();
        mixed.push(iface(0xFEFE, 0xFF60, hid::Bus::Usb, ""));
        assert_eq!(wired_count(&detect(&mixed, &[])), 1);

        // A keyboard whose vendor channels did not show up still counts once.
        let bare = [iface(0x8009, 0x0001, hid::Bus::Usb, "SONiX AK980 PRO")];
        assert_eq!(wired_count(&detect(&bare, &[])), 1);
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

    #[test]
    fn wired_offers_only_what_the_app_drives() {
        let list = [
            iface(0x8009, 0xFF13, hid::Bus::Usb, "SONiX AK980 PRO"),
            iface(0x8009, 0xFF68, hid::Bus::Usb, "SONiX AK980 PRO"),
        ];
        let caps = detect(&list, &[])[0].capabilities();
        assert!(!caps.contains(&Capability::Options), "keyboard options are not built");
        assert!(!caps.contains(&Capability::Battery), "no battery reading over USB");
        assert!(caps.contains(&Capability::Lighting) && caps.contains(&Capability::Clock));
    }

    #[test]
    fn receiver_offers_nothing_until_its_driver_exists() {
        // The receiver's vendor channel is found, but the app has no driver
        // for it: no page may unlock and no command may be sent.
        let list = [iface(0xFEFE, 0xFF60, hid::Bus::Usb, "")];
        let found = detect(&list, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].connection, ConnectionKind::Dongle);
        assert!(found[0].command.is_some());
        assert!(found[0].capabilities().is_empty());
    }

    fn collection(pid: u16, page: u16, usage: u16, product: &str) -> HidInterface {
        HidInterface { usage, ..iface(pid, page, hid::Bus::Usb, product) }
    }

    #[test]
    fn unknown_sonix_and_ajazz_keyboards_are_found() {
        // An AK820 PRO shares the AK980's ids but not its name.
        let ak820 = [
            collection(0x8009, 0x0001, 0x0006, "SONiX AK820 PRO"),
            collection(0x8009, 0x000C, 0x0001, "SONiX AK820 PRO"),
            collection(0x8009, 0xFF13, 0x0001, "SONiX AK820 PRO"),
        ];
        let found = unknown_keyboards(&ak820);
        assert_eq!(found.len(), 1);
        let k = &found[0];
        assert_eq!(k.key(), "0C45:8009:usb");
        assert_eq!(k.name(), Some("SONiX AK820 PRO"));
        assert_eq!(k.collections, vec![(0x0001, 0x0006, -1), (0x000C, 0x0001, -1), (0xFF13, 0x0001, -1)]);
        assert_eq!(k.paths.len(), 3);

        // Another maker's id with AJAZZ in the name.
        let other = HidInterface { vid: 0x3151, ..collection(0x4011, 0x0001, 0x0006, "AJAZZ AK35") };
        assert_eq!(unknown_keyboards(&[other])[0].key(), "3151:4011:usb");
    }

    #[test]
    fn supported_and_non_keyboard_devices_are_not_unknown() {
        let ak980 = [
            collection(0x8009, 0x0001, 0x0006, "SONiX AK980 PRO"),
            collection(0x8009, 0xFF13, 0x0001, "SONiX AK980 PRO"),
        ];
        assert!(unknown_keyboards(&ak980).is_empty());
        // A SONiX webcam's button collection is not a keyboard.
        let camera = collection(0x6366, 0x000C, 0x0001, "USB Camera");
        assert!(unknown_keyboards(&[camera]).is_empty());
        // Other makers' keyboards are none of our business.
        let other = HidInterface { vid: 0x046D, ..collection(0xC31C, 0x0001, 0x0006, "USB Keyboard") };
        assert!(unknown_keyboards(&[other]).is_empty());
    }

    #[test]
    fn an_ak980_by_another_name_asks_for_support() {
        // Same ids as the AK980 PRO, but not its name: an AK980 without the
        // screen, another generation, another firmware.
        let ak980 = [collection(0x8009, 0x0001, 0x0006, "AK980"), collection(0x8009, 0xFF13, 0x0001, "AK980")];
        assert!(detect(&ak980, &[]).is_empty());
        let found = unknown_keyboards(&ak980);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name(), Some("AK980"));
        // The name the owner's keyboard reports is the model.
        let pro = ak980.clone().map(|i| HidInterface { product: Some("AK980 PRO".into()), ..i });
        assert_eq!(detect(&pro, &[]).len(), 1);
        assert!(unknown_keyboards(&pro).is_empty());
    }

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    // The owner's AK980 PRO (fw 0x0106), as in shape.rs.
    const FF13: &str = "06 13 ff 09 01 a1 01 09 02 15 00 26 ff 00 75 08 95 40 81 02 09 03 15 00 26 ff 00 75 08 \
                        95 40 91 02 0a 00 ff 15 00 26 ff 00 75 08 95 40 b1 02 c0";
    const FF68: &str = "06 68 ff 09 61 a1 01 09 62 15 00 26 ff 00 75 08 95 40 81 02 09 63 15 00 26 ff 00 75 08 \
                        96 00 10 91 02 c0";
    // The "AA" family's FF68: 64-byte commands, no 4096-byte pages.
    const AA_FF68: &str = "06 68 ff 09 61 a1 01 15 00 26 ff 00 75 08 95 40 09 62 81 02 95 40 09 63 91 02 c0";

    fn ak980_pro() -> [HidInterface; 3] {
        [
            collection(0x8009, 0x0001, 0x0006, "AK980 PRO"),
            collection(0x8009, 0xFF13, 0x0001, "AK980 PRO"),
            collection(0x8009, 0xFF68, 0x0061, "AK980 PRO"),
        ]
    }

    /// Descriptors by usage page; the keyboard collection is never read.
    fn descriptors(ff13: &'static str, ff68: &'static str) -> impl FnMut(&HidInterface) -> Option<Vec<u8>> {
        move |i| match i.usage_page {
            0xFF13 => Some(hex(ff13)),
            0xFF68 => Some(hex(ff68)),
            page => panic!("collection {page:04X} has no vendor layout to check"),
        }
    }

    #[test]
    fn the_right_layout_is_the_model() {
        let found = classify(&ak980_pro(), &[], descriptors(FF13, FF68));
        assert_eq!(found.keyboards.len(), 1);
        assert_eq!(found.keyboards[0].connection, ConnectionKind::Usb);
        assert!(found.unknown.is_empty());
    }

    #[test]
    fn a_name_match_with_another_layout_asks_for_support() {
        // Another command family behind the same ids and name: its screen
        // channel takes 64-byte commands. Nothing may be sent to it, and the
        // user gets "Request support" with every collection's details.
        let found = classify(&ak980_pro(), &[], descriptors(FF13, AA_FF68));
        assert!(found.keyboards.is_empty(), "not driven as the AK980 PRO");
        assert_eq!(found.unknown.len(), 1);
        let k = &found.unknown[0];
        assert_eq!(k.key(), "0C45:8009:usb");
        assert_eq!(k.name(), Some("AK980 PRO"));
        assert_eq!(k.collections.len(), 3);
        assert_eq!(k.paths.len(), 3);

        // The command channel alone is enough to tell.
        let found = classify(&ak980_pro(), &[], descriptors(FF68, FF68));
        assert!(found.keyboards.is_empty());
        assert_eq!(found.unknown.len(), 1);

        // Listed even when the OS shows no keyboard collection for it: its
        // ids and name already say it is a keyboard.
        let vendor_only = [collection(0x8009, 0xFF13, 0x0001, "AK980 PRO")];
        let found = classify(&vendor_only, &[], descriptors(FF68, FF68));
        assert!(found.keyboards.is_empty());
        assert_eq!(found.unknown.len(), 1);
    }

    #[test]
    fn an_unreadable_layout_leaves_the_keyboard_as_found() {
        // Opening the channel checks again before anything is sent.
        let found = classify(&ak980_pro(), &[], |_| None);
        assert_eq!(found.keyboards.len(), 1);
        assert!(found.unknown.is_empty());
    }

    #[test]
    fn only_the_cable_channels_are_checked() {
        // The receiver and Bluetooth have no layout to check (nothing is sent
        // there), so their descriptors are never read.
        let list = [iface(0xFEFE, 0xFF60, hid::Bus::Usb, ""), iface(0xFEFE, 0x0001, hid::Bus::Bluetooth, "")];
        let found = classify(&list, &[], |i| panic!("read {:04X}", i.usage_page));
        assert_eq!(found.keyboards.len(), 2);
        assert!(found.unknown.is_empty());
    }
}
