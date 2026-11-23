//! Turn raw HID collections and paired BLE devices into a list of keyboards.

use kb_devices::{
    catalogue, match_ble_name, match_interface, match_interfaces, Bus, Capability, ConnectionKind, InterfaceMatch,
    InterfaceProbe, InterfaceRole, ModeSpec, Model, Protocol,
};
use kb_transport::hid::{self, HidInterface};
use serde::Serialize;
use std::cmp::Reverse;

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
    /// The model file's mode it matched: one of `model.modes`, of kind
    /// `connection`. It names the command family ([`Self::protocol`]).
    pub mode: &'static ModeSpec,
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
/// keyboard offers there: [`mode_capabilities`] of the first mode of `kind`.
pub fn app_capabilities(model: &Model, kind: ConnectionKind) -> Vec<Capability> {
    model.mode(kind).map(|mode| mode_capabilities(model, mode)).unwrap_or_default()
}

/// Features this app drives in `mode` (one of `model`'s), out of what the
/// model file says the keyboard offers there.
///
/// - USB: the command and screen channels. Keyboard options are not built
///   yet. The "04 xx" firmware has no battery reading over USB; the "AA"
///   family reports the battery in its device information.
/// - Bluetooth: no vendor channel; only the standard GATT battery service.
/// - 2.4 GHz receiver: no driver yet (the other modes come after USB), so
///   nothing is offered even though the model file lists what the receiver
///   is said to carry.
pub fn mode_capabilities(model: &Model, mode: &ModeSpec) -> Vec<Capability> {
    let protocol = model.protocol_of(mode);
    mode.capabilities
        .iter()
        .copied()
        .filter(|c| match mode.kind {
            ConnectionKind::Usb => match c {
                Capability::Options => false,
                Capability::Battery => protocol == Protocol::AjazzAa,
                _ => true,
            },
            ConnectionKind::Bluetooth => *c == Capability::Battery,
            ConnectionKind::Dongle => false,
        })
        .collect()
}

impl DetectedKeyboard {
    /// Features usable right now: what the app drives in the detected mode,
    /// given the interfaces the OS actually exposed.
    pub fn capabilities(&self) -> Vec<Capability> {
        mode_capabilities(self.model, self.mode)
            .into_iter()
            .filter(|c| match c {
                Capability::Battery if self.connection == ConnectionKind::Bluetooth => self.ble.is_some(),
                Capability::Screen => self.display.is_some(),
                _ => self.command.is_some(),
            })
            .collect()
    }

    /// The command family of the detected mode.
    pub fn protocol(&self) -> Protocol {
        self.model.protocol_of(self.mode)
    }

    /// What the app keys this keyboard's stored records by: the model id in
    /// the "04 xx" family, `"<model id>.aa"` in the "AA" family, whose key
    /// indices, tables and limits differ.
    pub fn record_id(&self) -> String {
        match self.protocol() {
            Protocol::SonixFf13 => self.model.id.clone(),
            Protocol::AjazzAa => format!("{}.aa", self.model.id),
        }
    }

    /// Stable identity for UI keys and change detection: the record id and
    /// the connection.
    pub fn key(&self) -> String {
        format!("{}:{:?}", self.record_id(), self.connection)
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

/// Group matching HID collections per (model, connection, command family)
/// and attach BLE peers.
///
/// Each collection goes to the first mode it matches: without the report
/// descriptors, two families behind the same IDs and name cannot be told
/// apart ([`classify`] reads them).
///
/// Placeholder product strings that some Bluetooth stacks report
/// ("Model_Number") are dropped.
pub fn detect(interfaces: &[HidInterface], ble: &[BlePeer]) -> Vec<DetectedKeyboard> {
    let Ok(models) = catalogue() else {
        tracing::error!("embedded model catalogue failed to parse");
        return Vec::new();
    };
    detect_among(interfaces, ble, models)
}

fn detect_among(interfaces: &[HidInterface], ble: &[BlePeer], models: &'static [Model]) -> Vec<DetectedKeyboard> {
    let chosen: Vec<_> = interfaces.iter().map(|i| match_interface(models, &probe(i))).collect();
    group(interfaces, &chosen, ble, models)
}

/// One keyboard family: model id, connection and command family. Detected
/// keyboards are grouped by it; [`classify`] judges layouts per family and
/// USB IDs.
type Family<'m> = (&'m str, ConnectionKind, Protocol);

fn family<'m>(m: &InterfaceMatch<'m>) -> Family<'m> {
    (m.model.id.as_str(), m.mode.kind, m.protocol)
}

/// Build the keyboards from the mode chosen for each collection (`chosen`,
/// same order as `interfaces`; `None` leaves the collection out) and attach
/// BLE peers.
fn group(
    interfaces: &[HidInterface],
    chosen: &[Option<InterfaceMatch<'static>>],
    ble: &[BlePeer],
    models: &'static [Model],
) -> Vec<DetectedKeyboard> {
    let mut out: Vec<DetectedKeyboard> = Vec::new();

    for (iface, m) in interfaces.iter().zip(chosen) {
        let Some(m) = m else { continue };
        let idx = match out
            .iter()
            .position(|k| k.model.id == m.model.id && k.connection == m.mode.kind && k.protocol() == m.protocol)
        {
            Some(i) => i,
            None => {
                out.push(DetectedKeyboard {
                    model: m.model,
                    mode: m.mode,
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
        let Some((model, mode)) = match_ble_name(models, &peer.name) else { continue };
        if let Some(k) = out.iter_mut().find(|k| k.model.id == model.id && k.connection == ConnectionKind::Bluetooth) {
            k.ble = Some(peer.clone());
        } else if peer.connected {
            out.push(DetectedKeyboard {
                model,
                mode,
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

    out.sort_by_key(|k| (k.model.id.clone(), k.connection as u8, k.protocol()));
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
    let status: Vec<Status> = interfaces
        .iter()
        .map(|i| match match_interface(models, &probe(i)) {
            Some(_) => Status::Driven,
            None => Status::Unmatched,
        })
        .collect();
    unknown_among(interfaces, &status)
}

/// How one HID collection relates to the model catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    /// Part of a keyboard the app drives.
    Driven,
    /// Its IDs and name match a model, but every family that could own it
    /// was rejected by a report layout.
    Rejected,
    /// No model matches it.
    Unmatched,
}

/// The keyboards among `interfaces` the app does not drive; `status` says
/// how each collection (same order) matched the catalogue.
fn unknown_among(interfaces: &[HidInterface], status: &[Status]) -> Vec<UnknownKeyboard> {
    let named = |s: &Option<String>| s.as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("ajazz"));
    // Each keyboard, and whether it matched a model whose shape it lacks:
    // that one is a keyboard for sure, keyboard collection or not.
    let mut out: Vec<(UnknownKeyboard, bool)> = Vec::new();
    for (iface, status) in interfaces.iter().zip(status) {
        let rejected_model = match status {
            Status::Rejected => true,
            Status::Driven => continue,
            Status::Unmatched => false,
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
/// of every keyboard on the cable checked against its command family's
/// report layout ([`crate::shape`]).
///
/// Other boards share the AK980 PRO's IDs, and some could share its name
/// too (another firmware, another command family). One whose command or
/// screen channel has another layout is not that mode of the model: nothing
/// may be sent to it. When the model lists a mode of another family behind
/// the same IDs and name, and the channels have that family's layout, it is
/// that mode; otherwise it is listed with the unknown keyboards, where the
/// user can ask for support, instead of as a keyboard every action fails on.
///
/// Families are judged one by one (model, connection, command family, USB
/// IDs): a channel whose layout does not fit, or a channel of another of
/// the model's families with that family's layout, rejects that family
/// behind those IDs only, never another family of the same model. Each
/// collection then goes to a family that could own it: a cable family when
/// its IDs and name match one (never a receiver mode with the same IDs),
/// not rejected, with the most channels whose layout fits, the first in
/// the model files on a tie.
///
/// `descriptor` gives a collection's report descriptor, as the OS keeps it
/// (reading it sends nothing to the keyboard). `None` (it could not be
/// read) leaves the keyboard as found, in the first family that matches:
/// opening the channel checks the layout again before anything is sent.
pub fn classify(
    interfaces: &[HidInterface],
    ble: &[BlePeer],
    descriptor: impl FnMut(&HidInterface) -> Option<Vec<u8>>,
) -> Found {
    let Ok(models) = catalogue() else {
        tracing::error!("embedded model catalogue failed to parse");
        return Found::default();
    };
    classify_among(interfaces, ble, models, descriptor)
}

fn classify_among(
    interfaces: &[HidInterface],
    ble: &[BlePeer],
    models: &'static [Model],
    mut descriptor: impl FnMut(&HidInterface) -> Option<Vec<u8>>,
) -> Found {
    /// What the cable's vendor channels say about one family.
    #[derive(Default)]
    struct Verdict {
        rejected: bool,
        /// Channels whose layout was read and fits.
        fitting: usize,
    }
    // A family is judged per USB ID: boards with other IDs never share a
    // collection with it.
    type Judged = (Family<'static>, u16, u16);
    let judged = |m: &InterfaceMatch<'static>| -> Judged { (family(m), m.mode.vid, m.mode.pid) };
    let candidates: Vec<Vec<InterfaceMatch<'static>>> =
        interfaces.iter().map(|i| match_interfaces(models, &probe(i))).collect();
    let mut verdicts: Vec<(Judged, Verdict)> = Vec::new();
    for m in candidates.iter().flatten() {
        if !verdicts.iter().any(|(k, _)| *k == judged(m)) {
            verdicts.push((judged(m), Verdict::default()));
        }
    }
    let at = |verdicts: &[(Judged, Verdict)], m: &InterfaceMatch<'static>| {
        verdicts.iter().position(|(k, _)| *k == judged(m)).expect("every candidate has a verdict")
    };

    // Collections whose layout was read: a vendor channel of some family.
    let mut judged_channel = vec![false; interfaces.len()];
    for (i, (iface, candidates)) in interfaces.iter().zip(&candidates).enumerate() {
        // Only the cable's vendor channels have a layout to check: nothing
        // is sent to the receiver or over Bluetooth yet.
        let usb: Vec<&InterfaceMatch<'static>> =
            candidates.iter().filter(|m| m.mode.kind == ConnectionKind::Usb).collect();
        let is_channel = usb.iter().any(|m| m.role != InterfaceRole::Standard);
        if !is_channel || usb.iter().all(|m| verdicts[at(&verdicts, m)].1.rejected) {
            continue;
        }
        let Some(desc) = descriptor(iface) else { continue };
        judged_channel[i] = true;
        for m in &usb {
            let fits = if m.role == InterfaceRole::Standard {
                // Not a channel of this family, but maybe one of another
                // family of the same model: a board that carries it, with
                // that family's layout, is not purely this family.
                !usb.iter().any(|o| {
                    o.model.id == m.model.id
                        && o.protocol != m.protocol
                        && o.role != InterfaceRole::Standard
                        && crate::shape::fits(o.protocol, o.role, &desc)
                })
            } else {
                crate::shape::fits(m.protocol, m.role, &desc)
            };
            let i = at(&verdicts, m);
            let v = &mut verdicts[i].1;
            if fits {
                v.fitting += usize::from(m.role != InterfaceRole::Standard);
            } else if !v.rejected {
                // Every enumeration finds it again; the app logs the change.
                tracing::debug!(
                    model = %m.model.id,
                    protocol = %m.protocol,
                    usage_page = format_args!("{:04X}", iface.usage_page),
                    shape = ?crate::shape::report_shape(&desc),
                    "name matches, report layout does not: this family is ruled out"
                );
                v.rejected = true;
            }
        }
    }

    let verdict = |m: &InterfaceMatch<'static>| &verdicts[at(&verdicts, m)].1;
    let mut chosen: Vec<Option<InterfaceMatch<'static>>> = Vec::with_capacity(interfaces.len());
    let mut status: Vec<Status> = Vec::with_capacity(interfaces.len());
    for (c, &judged) in candidates.iter().zip(&judged_channel) {
        // A collection whose IDs and name match a cable mode belongs to a
        // cable mode or to none: a receiver mode behind the same IDs is
        // never the fallback for a rejected cable family.
        let cable = c.iter().any(|m| m.mode.kind == ConnectionKind::Usb);
        // `min_by_key` keeps the first of equals: the model file's order.
        let pick = c
            .iter()
            .filter(|m| !cable || m.mode.kind == ConnectionKind::Usb)
            .filter(|m| !verdict(m).rejected)
            // A vendor channel whose layout was read is not another family's
            // plain collection unless that family has a channel that fits:
            // a "04 xx" command channel with the wrong layout does not make
            // an "AA" keyboard without channels appear.
            .filter(|m| !judged || m.role != InterfaceRole::Standard || verdict(m).fitting > 0)
            .min_by_key(|m| Reverse(verdict(m).fitting))
            .copied();
        status.push(match (&pick, c.is_empty()) {
            (Some(_), _) => Status::Driven,
            (None, false) => Status::Rejected,
            (None, true) => Status::Unmatched,
        });
        chosen.push(pick);
    }
    Found { keyboards: group(interfaces, &chosen, ble, models), unknown: unknown_among(interfaces, &status) }
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
        let k = &found.keyboards[0];
        assert_eq!(k.connection, ConnectionKind::Usb);
        assert_eq!(k.protocol(), Protocol::SonixFf13);
        assert_eq!(k.mode.command_usage_page, Some(0xFF13));
        assert_eq!(k.command.as_ref().unwrap().usage_page, 0xFF13);
        assert_eq!(k.display.as_ref().unwrap().usage_page, 0xFF68);
        assert_eq!(k.record_id(), "ajazz-ak980-pro");
        assert_eq!(k.key(), "ajazz-ak980-pro:Usb", "the app's key for this keyboard is unchanged");
        assert!(found.unknown.is_empty());
    }

    // Another family's layout is not the model: neither of its families
    // fits these channels.
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

    // ---- Two command families behind the same IDs and name -------------

    /// The AK980 PRO with an "AA" mode next to its "04 xx" one, on its old
    /// PID (AJAZZ's tools know "AA" firmware there too) and on firmware
    /// 769's, with other capabilities than the shipped model file's.
    const TWO_FAMILIES: &str = r#"
        id = "test-ak980"
        name = "AK980 PRO (test)"
        vendor = "AJAZZ"
        protocol = "sonix-ff13"
        layout = "98"
        key_count = 0
        has_knob = true
        has_side_light = false

        [[modes]]
        kind = "usb"
        vid = 0x0C45
        pid = 0x8009
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF13
        display_usage_page = 0xFF68
        capabilities = ["lighting", "per-key-rgb", "options", "screen", "clock", "battery"]
        verified = true

        [[modes]]
        kind = "usb"
        protocol = "ajazz-aa"
        vid = 0x0C45
        pid = 0x8009
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF68
        display_usage_page = 0xFF67
        capabilities = ["lighting", "keymap", "options", "screen", "battery"]
        verified = false

        [[modes]]
        kind = "usb"
        protocol = "ajazz-aa"
        vid = 0x0C45
        pid = 0x8039
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF68
        display_usage_page = 0xFF67
        capabilities = ["lighting", "keymap", "options", "screen", "battery"]
        verified = false
    "#;

    fn two_families() -> &'static [Model] {
        static MODELS: std::sync::OnceLock<Vec<Model>> = std::sync::OnceLock::new();
        MODELS.get_or_init(|| vec![toml::from_str(TWO_FAMILIES).expect("test model parses")])
    }

    // An AK980 PRO on firmware 769, from its "Request support" report
    // (issue #154): the same descriptors as in shape.rs.
    const AA_769_FF68: &str = "06 68 FF 09 61 A1 01 09 62 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
                               95 40 91 02 C0";
    const AA_769_FF67: &str = "06 67 FF 09 61 A1 01 09 64 15 00 26 FF 00 75 08 95 40 81 02 09 63 15 00 26 FF 00 75 08 \
                               96 08 10 91 02 09 62 15 00 26 FF 00 75 08 95 40 B1 02 C0";

    /// The collections of an "AA" AK980 PRO with USB product id `pid`, as
    /// in #154 (the keyboard, consumer, system, NKRO and mouse collections
    /// folded into two), at release 0x0301.
    fn aa_board(pid: u16) -> Vec<HidInterface> {
        [(0x0001, 0x0006), (0x000C, 0x0001), (0xFFFF, 0x0001), (0xFF68, 0x0061), (0xFF67, 0x0061)]
            .into_iter()
            .map(|(page, usage)| HidInterface { release: 0x0301, ..collection(pid, page, usage, "AK980 PRO") })
            .collect()
    }

    /// Descriptors of both families' vendor channels, told apart by
    /// release; a standard collection is never read.
    fn either_family(i: &HidInterface) -> Option<Vec<u8>> {
        let aa = i.release == 0x0301;
        Some(hex(match (i.usage_page, aa) {
            (0xFF13, false) => FF13,
            (0xFF68, false) => FF68,
            (0xFF68, true) => AA_769_FF68,
            (0xFF67, true) => AA_769_FF67,
            (page, _) => panic!("collection {page:04X} has no vendor layout to check"),
        }))
    }

    #[test]
    fn an_ff13_board_is_the_ff13_mode() {
        let found = classify_among(&ak980_pro(), &[], two_families(), either_family);
        assert!(found.unknown.is_empty());
        assert_eq!(found.keyboards.len(), 1, "the \"AA\" mode of the same ids is ruled out");
        let k = &found.keyboards[0];
        assert_eq!(k.protocol(), Protocol::SonixFf13);
        assert_eq!(k.command.as_ref().unwrap().usage_page, 0xFF13);
        assert_eq!(k.display.as_ref().unwrap().usage_page, 0xFF68);
        assert_eq!(k.standard_collections, 1);
        assert_eq!(k.record_id(), "test-ak980");
        // Capabilities come from the detected mode: no battery on the cable
        // in this family, though its mode lists one.
        let caps = k.capabilities();
        assert!(caps.contains(&Capability::Clock) && caps.contains(&Capability::PerKeyRgb));
        assert!(!caps.contains(&Capability::Battery) && !caps.contains(&Capability::Options));
    }

    #[test]
    fn an_aa_board_is_the_aa_mode_when_the_model_has_one() {
        for pid in [0x8009, 0x8039] {
            let found = classify_among(&aa_board(pid), &[], two_families(), either_family);
            assert!(found.unknown.is_empty(), "{pid:04X}: the \"04 xx\" family's rejection hides nothing");
            assert_eq!(found.keyboards.len(), 1, "{pid:04X}");
            let k = &found.keyboards[0];
            assert_eq!(k.protocol(), Protocol::AjazzAa);
            assert_eq!(k.mode.pid, pid);
            assert_eq!(k.command.as_ref().unwrap().usage_page, 0xFF68);
            assert_eq!(k.display.as_ref().unwrap().usage_page, 0xFF67);
            assert_eq!(k.standard_collections, 3);
            assert_eq!(k.firmware, Some(0x0301));
            assert_eq!(k.record_id(), "test-ak980.aa");
            assert_eq!(k.key(), "test-ak980.aa:Usb");
            // The "AA" mode's capabilities, battery included, not the first
            // USB mode's (no clock, no per-key table listed there).
            let caps = k.capabilities();
            assert!(caps.contains(&Capability::Battery) && caps.contains(&Capability::Keymap));
            assert!(caps.contains(&Capability::Screen));
            assert!(!caps.contains(&Capability::Clock) && !caps.contains(&Capability::PerKeyRgb));
            assert!(!caps.contains(&Capability::Options), "options are not built");
        }
    }

    #[test]
    fn an_aa_battery_needs_the_command_channel() {
        let bare: Vec<HidInterface> = aa_board(0x8039).into_iter().filter(|i| i.usage_page != 0xFF68).collect();
        let found = classify_among(&bare, &[], two_families(), either_family);
        let k = &found.keyboards[0];
        assert_eq!(k.protocol(), Protocol::AjazzAa, "the screen channel alone tells the family");
        assert_eq!(k.capabilities(), vec![Capability::Screen]);
    }

    /// The shipped AK980 PRO without its "AA" modes: the model file as it
    /// was before the "AA" driver.
    fn ff13_only() -> &'static [Model] {
        let mut m = shipped_ak980();
        let ff13: Vec<ModeSpec> =
            m.modes.iter().filter(|mode| m.protocol_of(mode) == Protocol::SonixFf13).cloned().collect();
        m.modes = ff13;
        catalogue_of(vec![m])
    }

    #[test]
    fn an_aa_board_on_the_old_pid_asks_for_support_without_an_aa_mode() {
        // Only the "04 xx" mode behind 0x8009.
        let found = classify_among(&aa_board(0x8009), &[], ff13_only(), either_family);
        assert!(found.keyboards.is_empty());
        assert_eq!(found.unknown.len(), 1);
        let k = &found.unknown[0];
        assert_eq!(k.key(), "0C45:8009:usb");
        assert_eq!(k.release, 0x0301);
        assert_eq!(k.collections.len(), 5, "every collection's details for the request");
        // On firmware 769's own id it matches no model at all.
        let found = classify_among(&aa_board(0x8039), &[], ff13_only(), |i| panic!("read {:04X}", i.usage_page));
        assert!(found.keyboards.is_empty());
        assert_eq!(found.unknown.len(), 1);
        assert_eq!(found.unknown[0].key(), "0C45:8039:usb");
    }

    #[test]
    fn the_shipped_model_drives_both_families_by_their_layouts() {
        // Firmware 769 on its own id, and "AA" firmware on the old one.
        for pid in [0x8039, 0x8009] {
            let found = classify(&aa_board(pid), &[], either_family);
            assert!(found.unknown.is_empty(), "{pid:04X}");
            assert_eq!(found.keyboards.len(), 1, "{pid:04X}");
            let k = &found.keyboards[0];
            assert_eq!((k.protocol(), k.mode.pid), (Protocol::AjazzAa, pid));
            assert_eq!(k.command.as_ref().unwrap().usage_page, 0xFF68);
            assert_eq!(k.display.as_ref().unwrap().usage_page, 0xFF67);
            assert_eq!(k.record_id(), "ajazz-ak980-pro.aa");
            assert_eq!(k.firmware, Some(0x0301));
            assert!(!k.mode.verified && k.mode.verified_firmware.is_empty(), "nothing tried on a keyboard");
            let caps = k.capabilities();
            for c in [
                Capability::Lighting,
                Capability::PerKeyRgb,
                Capability::Keymap,
                Capability::Macros,
                Capability::Screen,
                Capability::Clock,
                Capability::Battery,
            ] {
                assert!(caps.contains(&c), "{pid:04X}: {c:?}");
            }
            assert!(!caps.contains(&Capability::Options));
        }
        // The owner's "04 xx" board on 0x8009 still wins on its layouts,
        // with today's record id and capabilities.
        let found = classify(&ak980_pro(), &[], either_family);
        assert!(found.unknown.is_empty());
        assert_eq!(found.keyboards.len(), 1);
        let k = &found.keyboards[0];
        assert_eq!(k.protocol(), Protocol::SonixFf13);
        assert_eq!((k.record_id(), k.key()), ("ajazz-ak980-pro".to_string(), "ajazz-ak980-pro:Usb".to_string()));
        assert_eq!(k.command.as_ref().unwrap().usage_page, 0xFF13);
        assert_eq!(k.display.as_ref().unwrap().usage_page, 0xFF68);
        assert!(!k.capabilities().contains(&Capability::Battery));
        // Unreadable layouts: the first mode in the model file, the
        // "04 xx" one, as before (opening checks the layout again).
        let found = classify(&ak980_pro(), &[], |_| None);
        assert_eq!(found.keyboards.len(), 1);
        assert_eq!(found.keyboards[0].protocol(), Protocol::SonixFf13);
        // Both on the cable: two keyboards, so nothing is written.
        let both: Vec<HidInterface> = ak980_pro().into_iter().chain(aa_board(0x8039)).collect();
        let found = classify(&both, &[], either_family);
        assert_eq!(found.keyboards.len(), 2);
        assert_eq!(wired_count(&found.keyboards), 2);
    }

    #[test]
    fn an_unreadable_layout_keeps_the_first_family() {
        // As with one family: the first mode in the model file, and opening
        // the channel checks the layout before anything is sent.
        for board in [ak980_pro().to_vec(), aa_board(0x8009)] {
            let found = classify_among(&board, &[], two_families(), |_| None);
            assert!(found.unknown.is_empty());
            assert_eq!(found.keyboards.len(), 1);
            assert_eq!(found.keyboards[0].protocol(), Protocol::SonixFf13);
        }
        // `detect` reads no layout: the same choice.
        let found = detect_among(&aa_board(0x8009), &[], two_families());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].protocol(), Protocol::SonixFf13);
        // Firmware 769's id has only the "AA" mode.
        let found = classify_among(&aa_board(0x8039), &[], two_families(), |_| None);
        assert_eq!(found.keyboards[0].protocol(), Protocol::AjazzAa);
    }

    #[test]
    fn rejecting_one_family_hides_no_other_board() {
        // A "04 xx" board on 0x8009 rules out the "AA" mode behind 0x8009,
        // not the "AA" board on 0x8039 next to it.
        let both: Vec<HidInterface> = ak980_pro().into_iter().chain(aa_board(0x8039)).collect();
        let found = classify_among(&both, &[], two_families(), either_family);
        assert!(found.unknown.is_empty());
        let families: Vec<_> = found.keyboards.iter().map(|k| (k.protocol(), k.mode.pid, k.key())).collect();
        assert_eq!(
            families,
            [
                (Protocol::SonixFf13, 0x8009, "test-ak980:Usb".to_string()),
                (Protocol::AjazzAa, 0x8039, "test-ak980.aa:Usb".to_string())
            ]
        );
        assert_eq!(wired_count(&found.keyboards), 2, "two keyboards on the cable: no writes");
    }

    #[test]
    fn a_board_without_a_name_is_never_driven() {
        // A bootloader (ISP mode) can keep the keyboard's PID and report no
        // product string: neither family's mode matches, no layout is read,
        // and nothing is offered.
        for product in [None, Some(String::new())] {
            let nameless = |board: Vec<HidInterface>| -> Vec<HidInterface> {
                board.into_iter().map(|i| HidInterface { product: product.clone(), ..i }).collect()
            };
            let boards = [nameless(ak980_pro().to_vec()), nameless(aa_board(0x8009)), nameless(aa_board(0x8039))];
            for board in &boards {
                for models in [catalogue().unwrap(), two_families()] {
                    let found = classify_among(board, &[], models, |i| panic!("read {:04X}", i.usage_page));
                    assert!(found.keyboards.is_empty(), "{product:?}");
                    assert!(detect_among(board, &[], models).is_empty(), "{product:?}");
                }
            }
        }
    }

    #[test]
    fn a_mix_of_both_families_asks_for_support() {
        // The "04 xx" command channel next to an "AA" command channel on
        // FF68: the "04 xx" family lacks its screen layout, and the "AA"
        // family would leave another family's channel on the board. Neither
        // is driven.
        let mixed = [
            collection(0x8009, 0x0001, 0x0006, "AK980 PRO"),
            collection(0x8009, 0xFF13, 0x0001, "AK980 PRO"),
            collection(0x8009, 0xFF68, 0x0061, "AK980 PRO"),
        ];
        let found = classify_among(&mixed, &[], two_families(), descriptors(FF13, AA_769_FF68));
        assert!(found.keyboards.is_empty());
        assert_eq!(found.unknown.len(), 1);
        assert_eq!(found.unknown[0].collections.len(), 3);

        // An "AA" board on 0x8009 whose FF68 did not show up: its FF67, an
        // "AA" screen, rules the "04 xx" family out on its own.
        let mut aa = aa_board(0x8009);
        aa.retain(|i| i.usage_page != 0xFF68);
        let found = classify_among(&aa, &[], two_families(), either_family);
        assert_eq!(found.keyboards.len(), 1);
        assert_eq!(found.keyboards[0].protocol(), Protocol::AjazzAa);
        assert!(found.unknown.is_empty());
    }

    // ---- Several modes or model files behind the same IDs and name -------

    /// A test-only catalogue (leaked: tests only).
    fn catalogue_of(models: Vec<Model>) -> &'static [Model] {
        Box::leak(models.into_boxed_slice())
    }

    fn shipped_ak980() -> Model {
        catalogue().unwrap().iter().find(|m| m.id == "ajazz-ak980-pro").unwrap().clone()
    }

    /// A model whose receiver mode reports the same IDs as its cable mode
    /// and names no product (no model file does this yet).
    const RECEIVER_SAME_IDS: &str = r#"
        id = "test-same-ids"
        name = "Same IDs (test)"
        vendor = "AJAZZ"
        protocol = "sonix-ff13"
        layout = "98"
        key_count = 0
        has_knob = false
        has_side_light = false

        [[modes]]
        kind = "usb"
        vid = 0x0C45
        pid = 0x8009
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF13
        display_usage_page = 0xFF68
        capabilities = ["lighting", "screen"]
        verified = false

        [[modes]]
        kind = "dongle"
        vid = 0x0C45
        pid = 0x8009
        command_usage_page = 0xFF68
        capabilities = ["lighting"]
        verified = false
    "#;

    #[test]
    fn a_rejected_cable_family_never_falls_back_to_the_receiver() {
        let models = catalogue_of(vec![toml::from_str(RECEIVER_SAME_IDS).unwrap()]);
        // An "AA" board on 0x8009: the cable mode's layout does not fit, and
        // the receiver mode behind the same IDs must not take it instead.
        let found = classify_among(&aa_board(0x8009), &[], models, either_family);
        assert!(found.keyboards.is_empty(), "driven as {:?}", found.keyboards.first().map(|k| k.connection));
        assert_eq!(found.unknown.len(), 1, "\"Request support\" instead");
        assert_eq!(found.unknown[0].key(), "0C45:8009:usb");
        assert_eq!(found.unknown[0].collections.len(), 5);
        // The right layout is the cable mode, as before.
        let found = classify_among(&ak980_pro(), &[], models, either_family);
        assert_eq!(found.keyboards.len(), 1);
        assert_eq!(found.keyboards[0].connection, ConnectionKind::Usb);
        assert!(found.unknown.is_empty());
    }

    /// Another model file with the AK980 PRO's IDs and name, whose vendor
    /// channel sits on another usage page: none of the board's channels is
    /// its.
    const OTHER_PAGES: &str = r#"
        id = "test-other-pages"
        name = "Other pages (test)"
        vendor = "AJAZZ"
        protocol = "sonix-ff13"
        layout = "98"
        key_count = 0
        has_knob = false
        has_side_light = false

        [[modes]]
        kind = "usb"
        vid = 0x0C45
        pid = 0x8009
        product_contains = "AK980 PRO"
        command_usage_page = 0xFF99
        capabilities = ["lighting"]
        verified = false
    "#;

    #[test]
    fn the_model_file_whose_channels_fit_wins() {
        // Listed first, the other model file would own every collection; the
        // AK980 PRO's has two fitting channels on this board, it has none.
        let models = catalogue_of(vec![toml::from_str(OTHER_PAGES).unwrap(), shipped_ak980()]);
        let found = classify_among(&ak980_pro(), &[], models, descriptors(FF13, FF68));
        assert_eq!(found.keyboards.len(), 1);
        let k = &found.keyboards[0];
        assert_eq!(k.model.id, "ajazz-ak980-pro");
        assert!(k.command.is_some() && k.display.is_some());
        assert_eq!(k.standard_collections, 1);
        assert!(found.unknown.is_empty());
        // Without layouts the first model file keeps them, like `detect`.
        let found = classify_among(&ak980_pro(), &[], models, |_| None);
        assert_eq!(found.keyboards.len(), 1);
        assert_eq!(found.keyboards[0].model.id, "test-other-pages");
        assert_eq!(detect_among(&ak980_pro(), &[], models)[0].model.id, "test-other-pages");
    }

    #[test]
    fn identical_model_files_tie_to_the_first() {
        let twin = Model { id: "test-twin".into(), ..shipped_ak980() };
        for (models, first) in [
            (catalogue_of(vec![twin.clone(), shipped_ak980()]), "test-twin"),
            (catalogue_of(vec![shipped_ak980(), twin]), "ajazz-ak980-pro"),
        ] {
            let found = classify_among(&ak980_pro(), &[], models, descriptors(FF13, FF68));
            assert_eq!(found.keyboards.len(), 1, "one keyboard, not one per model file");
            assert_eq!(found.keyboards[0].model.id, first);
            assert!(found.keyboards[0].command.is_some() && found.keyboards[0].display.is_some());
        }
    }
}
