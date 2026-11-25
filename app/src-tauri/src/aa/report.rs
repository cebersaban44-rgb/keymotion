//! What the diagnostics report says about a full read of an "AA" keyboard
//! ([`super::AaDriver::diagnostics`]), and the failed reads kept for the
//! developer ([`reads`]).
//!
//! * One line per section read: its command, its length, whether it came
//!   in whole, how many bytes are not zero ("all 00" / "all FF"), and for
//!   the tables of 4-byte entries how many entries are all zero and what
//!   their first byte holds. The small blocks (options, effect) are shown
//!   whole; the effect decoded too.
//! * A table the sanity check found fault with gets a hex excerpt: its
//!   first [`EXCERPT_HEAD`] and last [`EXCERPT_TAIL`] bytes. Keymaps show
//!   only their entry pages otherwise.
//! * Macro contents never: they can hold typed text. Only the table's
//!   addresses and how many blocks there are.

use crate::diagnostics::{hex, hex_excerpt, number_ranges};
use kb_core::aa::{AaEffect, AaSnapshot};
use kb_protocol::aa::{cmd, effect, info, keymap, macros, perkey};

/// Bytes of a table that failed a check shown from its start...
pub const EXCERPT_HEAD: usize = 128;
/// ... and from its end.
pub const EXCERPT_TAIL: usize = 16;
/// Values of the first entry byte named in a keymap line, at most.
const PAGES_SHOWN: usize = 5;

/// The tables a hex excerpt can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    PerKey,
    Base,
    Fn,
    FactoryFn,
    Macros,
}

impl Table {
    /// In the order excerpts are shown (the report cuts from the end).
    const ALL: [Table; 5] = [Table::PerKey, Table::Base, Table::Fn, Table::FactoryFn, Table::Macros];

    fn name(self) -> &'static str {
        match self {
            Self::PerKey => "per-key table",
            Self::Base => "base layer",
            Self::Fn => "Fn layer",
            Self::FactoryFn => "factory Fn layer",
            Self::Macros => "macro table",
        }
    }

    fn command(self) -> u8 {
        match self {
            Self::PerKey => cmd::PER_KEY,
            Self::Base => cmd::KEYMAP,
            Self::Fn => cmd::FN_KEYMAP,
            Self::FactoryFn => cmd::DEFAULT_FN,
            Self::Macros => cmd::MACROS,
        }
    }

    fn bytes(self, s: &AaSnapshot) -> &[u8] {
        match self {
            Self::PerKey => &s.per_key.0,
            Self::Base => &s.keymap.0,
            Self::Fn => &s.fn_keymap.0,
            Self::FactoryFn => &s.default_fn.0,
            Self::Macros => &s.macro_table.0,
        }
    }

    /// Whether a reason of the sanity check is about this table: by the
    /// table's name in it ("per-key entry 3 …", "Fn layer slot 9 …",
    /// "macro 4 at address …").
    fn named_in(self, reason: &str) -> bool {
        let r = reason.to_ascii_lowercase();
        let without_factory = r.replace("factory fn layer", "");
        match self {
            Self::PerKey => r.contains("per-key"),
            Self::Base => r.contains("base layer"),
            Self::Fn => without_factory.contains("fn layer"),
            Self::FactoryFn => r.contains("factory fn layer"),
            Self::Macros => r.contains("macro"),
        }
    }
}

/// The tables `reasons` find fault with, in [`Table::ALL`] order.
pub fn failed_tables(reasons: &[String]) -> Vec<Table> {
    Table::ALL.into_iter().filter(|t| reasons.iter().any(|r| t.named_in(r))).collect()
}

/// `(0x14, 512 bytes, full, all 00)`: command, length, whole or not, and
/// how many bytes are not zero.
fn head(command: u8, bytes: &[u8], want: usize) -> String {
    let len = match bytes.len() {
        n if n == want => format!("{want} bytes, full"),
        n if n < want => format!("{n} of {want} bytes"),
        n => format!("{n} bytes, expected {want}"),
    };
    let content = if bytes.is_empty() {
        "empty".to_string()
    } else if bytes.iter().all(|&b| b == 0) {
        "all 00".to_string()
    } else if bytes.iter().all(|&b| b == 0xFF) {
        "all FF".to_string()
    } else {
        format!("{} non-zero", bytes.iter().filter(|&&b| b != 0).count())
    };
    format!("(0x{command:02X}, {len}, {content})")
}

/// The 4-byte entries of a table.
fn entries(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.chunks_exact(4)
}

/// `128 entries, 18 all zero`.
fn entry_count(bytes: &[u8]) -> String {
    let n = entries(bytes).count();
    let zero = entries(bytes).filter(|e| e.iter().all(|&b| b == 0)).count();
    format!("{n} entries, {zero} all zero")
}

/// A keymap layer: its entry pages (byte 0) by how often they occur.
fn keymap_line(name: &str, command: u8, bytes: &[u8]) -> String {
    let mut counts = [0usize; 256];
    for e in entries(bytes) {
        counts[usize::from(e[0])] += 1;
    }
    let mut pages: Vec<(u8, usize)> =
        (0..=255u8).map(|p| (p, counts[usize::from(p)])).filter(|(_, n)| *n > 0).collect();
    pages.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut shown: Vec<String> = pages.iter().take(PAGES_SHOWN).map(|(p, n)| format!("{p:02X} in {n}")).collect();
    let others: usize = pages.iter().skip(PAGES_SHOWN).map(|(_, n)| n).sum();
    if others > 0 {
        shown.push(format!("{} other values in {others}", pages.len() - PAGES_SHOWN));
    }
    let pages = if shown.is_empty() { String::new() } else { format!("; byte 0 (page): {}", shown.join(", ")) };
    format!("{name} {}: {}{pages}", head(command, bytes, keymap::LEN), entry_count(bytes))
}

/// The per-key table: whether each entry's first byte is its own index
/// (as the keyboard's software writes it), 00, FF or something else.
fn per_key_line(bytes: &[u8]) -> String {
    let (mut index, mut zero, mut ff, mut other) = (0, 0, 0, 0);
    for (i, e) in entries(bytes).enumerate() {
        match e[0] {
            b if usize::from(b) == i => index += 1,
            0 => zero += 1,
            0xFF => ff += 1,
            _ => other += 1,
        }
    }
    let n = entries(bytes).count();
    let mut first = vec![format!("its index in {index} of {n} entries")];
    for (count, what) in [(zero, "00"), (ff, "FF"), (other, "something else")] {
        if count > 0 {
            first.push(format!("{what} in {count}"));
        }
    }
    format!(
        "per-key table {}: {}; byte 0 = {}",
        head(cmd::PER_KEY, bytes, perkey::LEN),
        entry_count(bytes),
        first.join(", ")
    )
}

/// The macro table: how many slots point to a block (addresses only).
fn macro_table_line(bytes: &[u8]) -> String {
    let pointers = macros::pointers(bytes);
    let zero = pointers.iter().filter(|&&p| p == 0).count();
    let erased = pointers.iter().filter(|&&p| p == u32::MAX).count();
    let mut parts = vec![format!("{} slots, {} point to a block", pointers.len(), pointers.len() - zero - erased)];
    if zero > 0 {
        parts.push(format!("00 00 00 00 in {zero}"));
    }
    if erased > 0 {
        parts.push(format!("FF FF FF FF in {erased}"));
    }
    format!("macro table {}: {}", head(cmd::MACROS, bytes, macros::TABLE_LEN), parts.join(", "))
}

/// The effect block whole, and what it says.
fn effect_line(bytes: &[u8]) -> String {
    let decoded = AaEffect::decode(bytes)
        .map(|e| {
            format!(
                " = mode {}, colour {:02X} {:02X} {:02X}, colour mode {}, brightness {}, speed {}, direction {}",
                e.mode, e.color[0], e.color[1], e.color[2], e.color_mode, e.brightness, e.speed, e.direction
            )
        })
        .unwrap_or_default();
    format!("effect {}: {}{decoded}", head(cmd::EFFECT, bytes, effect::LEN), hex(bytes))
}

/// One line per section of a full read (device information parsed too).
pub fn section_lines(s: &AaSnapshot) -> Vec<String> {
    let d = &s.device;
    let blocks: usize = s.macro_blocks.iter().map(|b| b.bytes.0.len()).sum();
    let slots: Vec<u64> = s.macro_blocks.iter().map(|b| u64::from(b.slot)).collect();
    vec![
        format!("device information {}: {}", head(cmd::DEVICE_INFO, &s.info.0, info::LEN), hex(&s.info.0)),
        format!(
            "parsed: vid {:04X} pid {:04X} version {:04X} product {:04X} work mode {} battery {} charge {} \
             macro space {} tft frames {} direction {} frame version {} lighting version {} firmware status {} \
             profile {}/{}",
            d.vid,
            d.pid,
            d.version,
            d.product,
            d.work_mode,
            d.battery_level,
            d.charge_status,
            d.macro_space_size,
            d.tft_max_frames,
            d.tft_direction,
            d.frame_version,
            d.lighting_version,
            d.firmware_status,
            d.current_profile,
            d.max_profile
        ),
        format!("options {}: {}", head(cmd::OPTIONS, &s.options.0, 56), hex(&s.options.0)),
        keymap_line("base layer", cmd::KEYMAP, &s.keymap.0),
        keymap_line("Fn layer", cmd::FN_KEYMAP, &s.fn_keymap.0),
        keymap_line("factory Fn layer", cmd::DEFAULT_FN, &s.default_fn.0),
        effect_line(&s.effect.0),
        per_key_line(&s.per_key.0),
        macro_table_line(&s.macro_table.0),
        if s.macro_blocks.is_empty() {
            "macro blocks: none read".to_string()
        } else {
            format!(
                "macro blocks (0x{:02X}): {} read, {blocks} bytes in all, for slots {} (contents left out)",
                cmd::MACROS,
                s.macro_blocks.len(),
                number_ranges(&slots)
            )
        },
    ]
}

/// A hex excerpt of every table `reasons` find fault with.
pub fn excerpts(s: &AaSnapshot, reasons: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for t in failed_tables(reasons) {
        let bytes = t.bytes(s);
        let what = if bytes.len() > EXCERPT_HEAD + EXCERPT_TAIL {
            format!("first {EXCERPT_HEAD} and last {EXCERPT_TAIL} of {} bytes", bytes.len())
        } else {
            format!("all {} bytes", bytes.len())
        };
        out.push(format!("{} (0x{:02X}) failed a check, {what}:", t.name(), t.command()));
        out.extend(hex_excerpt(bytes, EXCERPT_HEAD, EXCERPT_TAIL).into_iter().map(|l| format!("  {l}")));
    }
    out
}

/// Full reads that did not pass the sanity check, in the app's data folder
/// (`keyboard-reads/`): for the developer, when the report is not enough.
/// Never in `keyboard-backups/`, so never offered for a restore; macro
/// entries are zeroed (they can hold typed text). The newest [`KEPT`] stay;
/// a read the same as the newest of its keyboard is not saved again.
pub mod reads {
    use crate::aa::AaTarget;
    use kb_core::aa::AaSnapshot;
    use serde::{Deserialize, Serialize};
    use std::io;
    use std::path::{Path, PathBuf};

    pub const DIR: &str = "keyboard-reads";
    /// Files kept.
    pub const KEPT: usize = 5;
    const VERSION: u32 = 1;
    const NOTE: &str = "A full read that did not pass Keymotion's sanity check, for the developer. \
                        Macro entries are left out (zeros): they can hold typed text. Not a backup: \
                        Keymotion never restores it.";

    pub fn dir(data_dir: &Path) -> PathBuf {
        data_dir.join(DIR)
    }

    /// One file.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct FailedRead {
        pub note: String,
        pub saved_at_ms: u64,
        pub record_id: String,
        pub pid: u16,
        /// Firmware version (USB `bcdDevice`).
        pub release: u16,
        /// Every reason the check gave.
        pub reasons: Vec<String>,
        pub snapshot: AaSnapshot,
    }

    #[derive(Deserialize)]
    struct Stored {
        version: u32,
        #[serde(flatten)]
        value: FailedRead,
    }

    /// `read-<ms>-<pid>-<release>.json`: its time and USB ids; anything else
    /// is not one of these files.
    fn parse_name(name: &str) -> Option<(u64, u16, u16)> {
        let stem = name.strip_prefix("read-")?.strip_suffix(".json")?;
        let mut parts = stem.split('-');
        let (at, pid, release) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() || pid.len() != 4 || release.len() != 4 || !at.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((at.parse().ok()?, u16::from_str_radix(pid, 16).ok()?, u16::from_str_radix(release, 16).ok()?))
    }

    /// The files, oldest first.
    fn names(dir: &Path) -> Vec<(u64, String)> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut out: Vec<(u64, String)> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| parse_name(&n).map(|(at, _, _)| (at, n)))
            .collect();
        out.sort();
        out
    }

    pub fn load(dir: &Path, name: &str) -> Option<FailedRead> {
        parse_name(name)?;
        let text = std::fs::read_to_string(dir.join(name)).ok()?;
        serde_json::from_str::<Stored>(&text).ok().filter(|s| s.version == VERSION).map(|s| s.value)
    }

    /// The snapshot as saved: macro entries zeroed, their headers kept.
    fn without_macro_entries(s: &AaSnapshot) -> AaSnapshot {
        let mut s = s.clone();
        for b in &mut s.macro_blocks {
            for byte in b.bytes.0.iter_mut().skip(4) {
                *byte = 0;
            }
        }
        s
    }

    /// Save a failed read of `target` in `data_dir`; returns the file as
    /// the report names it (`keyboard-reads\read-….json`).
    pub fn keep(data_dir: &Path, target: &AaTarget, s: &AaSnapshot, reasons: &[String]) -> io::Result<String> {
        let dir = dir(data_dir);
        let read = FailedRead {
            note: NOTE.into(),
            saved_at_ms: super::super::now_ms(),
            record_id: target.record_id.clone(),
            pid: target.usb.pid,
            release: target.usb.release,
            reasons: reasons.to_vec(),
            snapshot: without_macro_entries(s),
        };
        let shown = |name: &str| format!("{DIR}{}{name}", std::path::MAIN_SEPARATOR);
        let newest_of_this = names(&dir)
            .into_iter()
            .rev()
            .find(|(_, n)| parse_name(n).is_some_and(|(_, pid, release)| (pid, release) == (read.pid, read.release)));
        if let Some((_, name)) = newest_of_this {
            if load(&dir, &name).is_some_and(|old| old.snapshot == read.snapshot && old.reasons == read.reasons) {
                return Ok(shown(&name));
            }
        }
        let mut at = read.saved_at_ms;
        let name = loop {
            let name = format!("read-{at}-{:04x}-{:04x}.json", read.pid, read.release);
            if !dir.join(&name).exists() {
                break name;
            }
            at += 1;
        };
        crate::settings::save(&dir, &name, VERSION, &read)?;
        prune(&dir, &name);
        Ok(shown(&name))
    }

    /// Remove the oldest files (by the time in their names) beyond
    /// [`KEPT`], never `just_saved`: after the clock went back it is the
    /// "oldest", and the report names it.
    fn prune(dir: &Path, just_saved: &str) {
        let names = names(dir);
        let mut left = names.len();
        for (_, name) in &names {
            if left <= KEPT {
                break;
            }
            if name == just_saved {
                continue;
            }
            match std::fs::remove_file(dir.join(name)) {
                Ok(()) => left -= 1,
                Err(e) => tracing::warn!(file = %name, "old keyboard read not removed: {e}"),
            }
        }
    }

    #[cfg(test)]
    pub fn names_in(data_dir: &Path) -> Vec<String> {
        names(&dir(data_dir)).into_iter().map(|(_, n)| n).collect()
    }

    #[cfg(test)]
    pub fn name_ok(name: &str) -> bool {
        parse_name(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aa::AaTarget;
    use crate::diagnostics::{self, summarize_reasons, Facts};
    use kb_core::aa::{check_sanity, DeviceRecord, Hex, MacroBlock, UsbIdentity};
    use kb_protocol::aa::info::DeviceInfo;

    const USB: UsbIdentity = UsbIdentity { pid: 0x8039, release: 0x0302 };

    /// The device information j0p3s's keyboard gave (1.3.0-beta.1 report).
    fn tester_info() -> Vec<u8> {
        let mut b = vec![0u8; info::LEN];
        b[..23].copy_from_slice(&[
            0x40, 0x00, 0x00, 0x0C, 0x45, 0x0C, 0x39, 0x80, 0x02, 0x03, 0xFF, 0xFF, 0x01, 0x00, 0x33, 0x00, 0x00, 0x64,
            0x02, 0x00, 0x00, 0x00, 0x92,
        ]);
        b
    }

    fn keymap_table() -> Vec<u8> {
        let mut t = vec![0u8; keymap::LEN];
        for slot in 0..104u8 {
            keymap::set_entry(&mut t, slot, [0x02, 0x00, slot.wrapping_add(4), 0x00]);
        }
        t
    }

    /// A read like j0p3s's: everything plausible, the per-key table all 00.
    fn tester_snapshot() -> AaSnapshot {
        let raw = tester_info();
        let device: DeviceRecord = DeviceInfo::parse(&raw).unwrap().into();
        AaSnapshot {
            format: AaSnapshot::FORMAT,
            info: Hex(raw),
            device,
            options: Hex(vec![0; 56]),
            keymap: Hex(keymap_table()),
            fn_keymap: Hex(keymap_table()),
            default_fn: Hex(keymap_table()),
            effect: Hex(vec![5, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 1, 5, 3, 0, 0, 0, 0xAA, 0x55]),
            effect_color: None,
            effect_mode: None,
            per_key: Hex(vec![0; perkey::LEN]),
            macro_table: Hex(vec![0; macros::TABLE_LEN]),
            macro_blocks: vec![],
        }
    }

    fn tester_reasons() -> Vec<String> {
        (1..128).map(|i| format!("per-key entry {i} starts with 0")).collect()
    }

    #[test]
    fn the_tester_case_reads_in_a_few_lines() {
        let s = tester_snapshot();
        let lines = section_lines(&s);
        let text = lines.join("\n");
        for part in [
            "device information (0x10, 48 bytes, full, 15 non-zero): 40 00 00 0C 45 0C 39 80 02 03 FF FF 01",
            "parsed: vid 0C45 pid 8039 version 0302",
            "options (0x11, 56 bytes, full, all 00): 00 00",
            "base layer (0x12, 512 bytes, full, 208 non-zero): 128 entries, 24 all zero; byte 0 (page): 02 in 104, 00 in 24",
            "effect (0x13, 16 bytes, full, 9 non-zero): 05 FF FF FF 00 00 00 00 01 05 03 00 00 00 AA 55 = mode 5, colour FF FF FF, colour mode 1, brightness 5, speed 3, direction 0",
            "per-key table (0x14, 512 bytes, full, all 00): 128 entries, 128 all zero; byte 0 = its index in 1 of 128 entries, 00 in 127",
            "macro table (0x15, 400 bytes, full, all 00): 100 slots, 0 point to a block, 00 00 00 00 in 100",
            "macro blocks: none read",
        ] {
            assert!(text.contains(part), "{part:?} missing:\n{text}");
        }
        assert_eq!(lines.len(), 10, "{text}");

        let reasons = tester_reasons();
        assert_eq!(summarize_reasons(&reasons), ["per-key entry 1-127 starts with 0"]);
        assert_eq!(failed_tables(&reasons), [Table::PerKey]);
        let ex = excerpts(&s, &reasons);
        assert_eq!(ex[0], "per-key table (0x14) failed a check, first 128 and last 16 of 512 bytes:");
        assert_eq!(ex[1], "  000: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(ex[9], "  … 368 bytes left out");
        assert_eq!(ex[10], "  1F0: 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00");
        assert_eq!(ex.len(), 11);
    }

    #[test]
    fn reasons_point_at_their_tables() {
        let r = |s: &str| failed_tables(&[s.to_string()]);
        assert_eq!(r("per-key entry 3 starts with 0"), [Table::PerKey]);
        assert_eq!(r("base layer slot 4: unknown entry page 0x0E"), [Table::Base]);
        assert_eq!(r("Fn layer slot 4: unknown entry page 0x0E"), [Table::Fn]);
        assert_eq!(r("factory Fn layer slot 4: unknown entry page 0x0E"), [Table::FactoryFn]);
        assert_eq!(r("macro 3 at address 9000 is outside the macro memory or unreadable"), [Table::Macros]);
        assert_eq!(r("the per-key table read is 500 bytes, expected 512"), [Table::PerKey]);
        assert!(r("unknown effect mode 99").is_empty());
        assert!(r("vendor id 1234, expected 0C45").is_empty());
    }

    #[test]
    fn macro_contents_stay_out_of_the_report() {
        let mut s = tester_snapshot();
        let mut table = vec![0u8; macros::TABLE_LEN];
        table[..4].copy_from_slice(&400u32.to_le_bytes());
        s.macro_table = Hex(table);
        // "secret" as key codes, and as ASCII in case it ever gets there.
        let block = [vec![4, 0, 0, 0], b"SECRET".repeat(2)].concat();
        s.macro_blocks = vec![MacroBlock { slot: 0, addr: 400, bytes: Hex(block) }];
        let reasons = vec!["macro 0 at address 400 is outside the macro memory or unreadable".to_string()];
        let text = [section_lines(&s), excerpts(&s, &reasons)].concat().join("\n");
        assert!(
            text.contains("macro blocks (0x15): 1 read, 16 bytes in all, for slots 0 (contents left out)"),
            "{text}"
        );
        assert!(text.contains("macro table (0x15) failed a check"), "{text}");
        assert!(!text.contains("53 45 43") && !text.contains("SECRET"), "{text}");
    }

    #[test]
    fn a_short_table_says_so() {
        assert_eq!(head(cmd::PER_KEY, &[1, 0, 0], 512), "(0x14, 3 of 512 bytes, 1 non-zero)");
        assert_eq!(head(cmd::PER_KEY, &[0xFF; 4], 2), "(0x14, 4 bytes, expected 2, all FF)");
        assert_eq!(head(cmd::PER_KEY, &[], 512), "(0x14, 0 of 512 bytes, empty)");
        let mut good = vec![0u8; perkey::LEN];
        for i in 0..128 {
            good[i * 4] = i as u8;
            good[i * 4 + 1] = 0x20;
        }
        good[4 * 127] = 0xFF;
        good[4 * 126] = 7;
        assert!(per_key_line(&good).ends_with("byte 0 = its index in 126 of 128 entries, FF in 1, something else in 1"));
    }

    /// A read where everything is wrong at once, and as many reasons as the
    /// check could ever give: the report still fits a GitHub issue.
    #[test]
    fn the_worst_case_report_fits_a_github_issue() {
        let mut seed = 0x1234_5678u32;
        let mut noise = |n: usize| -> Vec<u8> {
            (0..n)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    (seed >> 24) as u8
                })
                .collect()
        };
        let mut s = tester_snapshot();
        s.keymap = Hex(noise(keymap::LEN));
        s.fn_keymap = Hex(noise(keymap::LEN));
        s.default_fn = Hex(noise(keymap::LEN));
        s.per_key = Hex(noise(perkey::LEN));
        s.macro_table = Hex(noise(macros::TABLE_LEN));
        s.effect = Hex(noise(effect::LEN));
        s.options = Hex(noise(56));
        s.macro_blocks = (0..100u8).map(|slot| MacroBlock { slot, addr: 400, bytes: Hex(noise(400)) }).collect();
        let mut reasons = check_sanity(&s, USB).err().unwrap_or_default();
        // Every reason the check knows, each table at its worst.
        for name in ["base layer", "Fn layer", "factory Fn layer"] {
            reasons.extend((0..128).map(|i| format!("{name} slot {i}: unknown entry page 0x{:02X}", i * 7 % 256)));
        }
        reasons.extend((0..128).map(|i| format!("per-key entry {i} starts with {}", (i * 37) % 256)));
        reasons.extend((0..100).map(|i| format!("macro {i} at address {} is outside the macro memory", 9000 + i * 13)));
        reasons.push(format!("a reason with a very long text {}", "word ".repeat(400)));
        let summary = summarize_reasons(&reasons);
        assert!(summary.len() <= 9, "{summary:#?}");
        assert!(summary.iter().all(|l| l.chars().count() <= 240), "{summary:#?}");

        let mut aa = vec!["ajazz-ak980-pro.aa on USB 8039, firmware 3.02".to_string()];
        aa.extend(section_lines(&s));
        aa.push(format!("full read: done, did not pass the sanity check ({} reasons):", reasons.len()));
        aa.extend(summary.iter().map(|l| format!("  {l}")));
        aa.extend(excerpts(&s, &reasons));
        aa.extend((0..10).map(|i| format!("error at {i}: {}", "x ".repeat(300))));
        let what = format!("The keyboard does not look as expected: {}", reasons.join("; "));
        let f = Facts {
            app_name: "Keymotion".into(),
            app_version: "1.3.0-beta.2 (GitHub)".into(),
            time: "t".into(),
            windows: "Windows 11".into(),
            webview: "142".into(),
            ui_language: "en".into(),
            context: Some(what),
            keyboards: vec!["AJAZZ AK980 PRO (ajazz-ak980-pro), usb, firmware 3.02".into(); 3],
            aa,
            data_files: (0..60).map(|i| (format!("file-{i}.json"), 1000)).collect(),
            log_lines: (0..diagnostics::LOG_LINES)
                .map(|i| format!("2026-11-24T03:42:{i:02}Z  WARN kb_core::aa: gate {i} why={reasons:?}"))
                .collect(),
        };
        let r = diagnostics::report(&f);
        let n = r.chars().count();
        assert!(n <= diagnostics::REPORT_MAX + 1, "{n} characters");
        assert!(n < 65_536);
        assert!(r.contains("characters left out]"), "markers show what was cut");
        assert!(r.contains("\"AA\" keyboard:\n  ajazz-ak980-pro.aa on USB 8039"), "{r}");
        assert!(r.contains("per-key table (0x14) failed a check"), "the first excerpt is in:\n{r}");
        let aa_part = &r[r.find("\"AA\" keyboard:").unwrap()..r.find("\nData files:").unwrap()];
        assert!(aa_part.chars().count() <= 6_100, "{}", aa_part.chars().count());
        let log = &r[r.find("log lines (").unwrap()..];
        assert!(log.lines().all(|l| l.chars().count() <= 400), "log lines are cut");
        assert!(log.contains("older lines left out"), "{log}");
    }

    #[test]
    fn failed_reads_have_their_own_folder() {
        let dir = std::env::temp_dir().join(format!("klavye-aa-reads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let target = AaTarget { record_id: "ajazz-ak980-pro.aa".into(), usb: USB, data_dir: Some(dir.clone()) };
        let mut s = tester_snapshot();
        let mut table = vec![0u8; macros::TABLE_LEN];
        table[..4].copy_from_slice(&400u32.to_le_bytes());
        s.macro_table = Hex(table);
        s.macro_blocks = vec![MacroBlock { slot: 0, addr: 400, bytes: Hex(vec![4, 0, 0, 0, 0x80, 0x16, 5, 0]) }];
        let reasons = tester_reasons();

        let first = reads::keep(&dir, &target, &s, &reasons).unwrap();
        assert!(first.starts_with(&format!("keyboard-reads{}read-", std::path::MAIN_SEPARATOR)), "{first}");
        // The same read again: no second file.
        assert_eq!(reads::keep(&dir, &target, &s, &reasons).unwrap(), first);
        let names = reads::names_in(&dir);
        assert_eq!(names.len(), 1);
        let saved = reads::load(&reads::dir(&dir), &names[0]).unwrap();
        assert_eq!(saved.reasons, reasons, "every reason, not the summary");
        assert_eq!(saved.snapshot.per_key, s.per_key);
        assert_eq!(saved.snapshot.macro_blocks[0].bytes.0, [4, 0, 0, 0, 0, 0, 0, 0], "macro entries zeroed");
        assert!(saved.note.contains("Not a backup"));
        // Never where backups are looked for, never a backup's name.
        assert!(super::super::backups::list(&super::super::backups::dir(&dir)).is_empty());
        assert!(!dir.join(super::super::backups::DIR).exists());
        assert!(names.iter().all(|n| !n.starts_with("aa-") && reads::name_ok(n)));

        // Different reads: the newest five stay.
        for i in 0..7u8 {
            s.per_key.0[4] = i + 1;
            reads::keep(&dir, &target, &s, &reasons).unwrap();
        }
        let names = reads::names_in(&dir);
        assert_eq!(names.len(), reads::KEPT);
        assert_eq!(reads::load(&reads::dir(&dir), names.last().unwrap()).unwrap().snapshot.per_key.0[4], 7);
        assert!(!reads::name_ok("read-1-8039-0302.json.tmp") && !reads::name_ok("aa-8039-0302-1.json"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// After the clock went back, the read just saved has the oldest name:
    /// it stays (the report names it), the oldest other one goes.
    #[test]
    fn the_read_just_saved_is_never_pruned() {
        let dir = std::env::temp_dir().join(format!("klavye-aa-reads-clock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let reads_dir = reads::dir(&dir);
        std::fs::create_dir_all(&reads_dir).unwrap();
        let later: Vec<String> = (0..reads::KEPT).map(|i| format!("read-9999999999990{i}-8039-0302.json")).collect();
        for name in &later {
            std::fs::write(reads_dir.join(name), "{}").unwrap();
        }
        let target = AaTarget { record_id: "ajazz-ak980-pro.aa".into(), usb: USB, data_dir: Some(dir.clone()) };
        let kept = reads::keep(&dir, &target, &tester_snapshot(), &tester_reasons()).unwrap();
        let name = kept.rsplit(std::path::MAIN_SEPARATOR).next().unwrap();
        let now = reads::names_in(&dir);
        assert_eq!(now.len(), reads::KEPT, "{now:?}");
        assert!(now.iter().any(|n| n == name), "the new read stays: {now:?}");
        assert!(!now.contains(&later[0]) && now.contains(&later[1]), "the oldest other one went: {now:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
