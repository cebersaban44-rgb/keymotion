//! The driver against the simulated keyboard.

use super::sim::{Faults, Sim, SimPort, SimState, FACTORY_EFFECT};
use super::*;
use kb_protocol::aa::keymap::{func_ids, page};
use kb_protocol::sonix::keymap::{mods, MacroPlay, MouseButton};
use kb_protocol::sonix::macros::StepKind;
use kb_protocol::sonix::screen::{rgb565, test_pattern, Frame};

// LEDs ("04 xx" indices) of a few AK980 PRO keys, and their "AA" slots.
const ESC: u8 = 1;
const W: u8 = 39;
const A: u8 = 56;
const ENTER: u8 = 85;
const FN: u8 = 96;
const SPACE: u8 = 94;
const UP: u8 = 101;
const BACKSPACE: u8 = 103;
const SPACE_AA: u8 = 83;
const ENTER_AA: u8 = 76;
const FN_AA: u8 = 85;

fn layout() -> AaLayout {
    AaLayout::from_model(Sim::model()).unwrap()
}

/// A driver on the simulated keyboard that does not wait for it to settle
/// (tests that do set their own time).
fn keyboard(sim: &Sim) -> AaKeyboard<SimPort> {
    let mut k = AaKeyboard::new(sim.command(), layout(), Sim::usb());
    k.set_settle(Duration::ZERO);
    k
}

/// A keyboard that passed the sanity gate.
fn ready(sim: &Sim) -> AaKeyboard<SimPort> {
    let mut k = keyboard(sim);
    k.read_all().unwrap();
    assert!(k.is_sane(), "{:?}", k.sanity_reasons());
    k
}

fn slot(table: &[u8], s: u8) -> [u8; 4] {
    keymap::entry(table, s)
}

/// Write packets the keyboard has received.
fn writes(sim: &Sim) -> usize {
    sim.state().packets.iter().filter(|p| guard::WRITES.contains(&p[2])).count()
}

// ------------------------------------------------------------------ reading

#[test]
fn the_first_request_is_the_device_information() {
    let sim = Sim::ak980();
    let mut k = keyboard(&sim);
    let i = k.device_info().unwrap();
    assert_eq!((i.vid, i.pid, i.version, i.product, i.battery_level), (0x0C45, 0x8039, 0x0301, 0x33, 100));
    let s = sim.state();
    assert_eq!(s.packets.len(), 1, "one packet, nothing else");
    assert_eq!(&s.packets[0][..9], &[0, 0xAA, 0x10, 0x30, 0, 0, 0, 1, 0]);
}

#[test]
fn a_full_read_passes_the_sanity_gate() {
    let sim = Sim::ak980();
    let mut k = keyboard(&sim);
    assert!(!k.is_sane());
    let snap = k.read_all().unwrap();
    assert!(k.is_sane());
    assert_eq!(snap.device.pid, 0x8039);
    // Firmware 769 fills in its frame limit, 146, on every read.
    assert_eq!(snap.device.tft_max_frames, 146);
    assert_eq!((snap.keymap.0.len(), snap.default_fn.0.len(), snap.per_key.0.len()), (512, 512, 512));
    // The per-key table of a freshly flashed keyboard: all 00, and sane.
    assert!(snap.per_key.0.iter().all(|&b| b == 0));
    assert_eq!(snap.effect.0, FACTORY_EFFECT);
    assert!(snap.macro_blocks.is_empty());
    // Every read the WebHID driver makes on connect, and nothing else.
    let s = sim.state();
    let cmds: std::collections::BTreeSet<u8> = s.packets.iter().map(|p| p[2]).collect();
    assert_eq!(cmds.into_iter().collect::<Vec<_>>(), [0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x1C]);
    // A 512-byte read is the capture's 10 chunks.
    let keymap_reads = s.sent(cmd::KEYMAP);
    assert_eq!(keymap_reads.len(), 10);
    assert_eq!(&keymap_reads[9][1..9], &[0xAA, 0x12, 0x08, 0xF8, 0x01, 0x00, 0x01, 0x00]);
    assert_eq!(k.unexpected_reports(), 0);
}

#[test]
fn the_firmware_status_byte_is_only_noted() {
    // Byte 32 is no field of firmware 769 (§F.7): a 1 there stops nothing.
    let sim = Sim::ak980();
    sim.state().info[32] = 1;
    let mut k = keyboard(&sim);
    let snap = k.read_all().unwrap();
    assert_eq!(snap.device.firmware_status, 1);
    assert!(k.is_sane(), "{:?}", k.sanity_reasons());
    k.apply_effect(&EffectSettings::factory()).unwrap();
    k.set_clock(&clock_time()).unwrap();
}

#[test]
fn no_read_goes_out_before_the_device_information() {
    // A single read on a fresh driver asks the device information first.
    let sim = Sim::ak980();
    let mut k = keyboard(&sim);
    k.read_keymap(Layer::Base).unwrap();
    let sent: Vec<u8> = sim.state().packets.iter().map(|p| p[2]).take(2).collect();
    assert_eq!(sent, [cmd::DEVICE_INFO, cmd::KEYMAP]);

    // A device information that went unanswered is asked again first.
    let sim = Sim::ak980();
    sim.state().faults.silent_info = true;
    let mut k = keyboard(&sim);
    assert!(matches!(k.read_effect(), Err(AaError::Timeout { cmd: 0x10 })));
    assert!(k.info().is_none());
    sim.state().faults.silent_info = false;
    k.read_effect().unwrap();
    let sent: Vec<u8> = sim.state().packets.iter().map(|p| p[2]).collect();
    assert_eq!(sent, [0x10, 0x10, 0x10, 0x10, 0x13]);
}

#[test]
fn no_answer_gives_up_after_three_attempts() {
    let sim = Sim::ak980().with_faults(Faults { silent_info: true, ..Faults::default() });
    let mut k = keyboard(&sim);
    assert!(matches!(k.read_all(), Err(AaError::Timeout { cmd: 0x10 })));
    assert_eq!(sim.state().packets.len(), ATTEMPTS);
}

#[test]
fn a_lost_reply_is_retried() {
    let sim = Sim::ak980().with_faults(Faults { drop_reply: Some(3), ..Faults::default() });
    let mut k = keyboard(&sim);
    k.read_all().unwrap();
    assert!(k.is_sane());
    let s = sim.state();
    assert_eq!(s.packets[2], s.packets[3], "the third request went out again");
}

#[test]
fn stray_reports_are_skipped_and_counted() {
    let sim = Sim::ak980().with_faults(Faults { stray_before: Some(4), ..Faults::default() });
    let mut k = keyboard(&sim);
    k.read_all().unwrap();
    assert!(k.is_sane(), "a reply for another address is not taken for this one");
    assert_eq!(k.unexpected_reports(), 1);
    assert_eq!(k.resets_reported(), 0);
}

#[test]
fn a_reset_makes_everything_stale() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 1);
    k.read_effect().unwrap();
    assert_eq!((k.unexpected_reports(), k.resets_reported()), (1, 1));
    assert!(!k.is_sane());
    assert!(
        matches!(k.apply_effect(&EffectSettings::factory()), Err(AaError::NotSane(why)) if why[0].contains("reset"))
    );
    // A frame stores nothing: the closed gate does not hold it back (the
    // reset blink does, when there is one: see the settle tests).
    k.realtime_frame([]).unwrap();
    // Reading everything again clears it.
    k.read_all().unwrap();
    assert!(k.is_sane());
    // A reset during the full read leaves the gate closed.
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 5);
    k.read_all().unwrap();
    assert!(!k.is_sane());
    assert!(k.sanity_reasons().iter().any(|r| r.contains("reset")));
}

#[test]
fn a_reset_stops_the_operation_it_arrives_in() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // During the fresh read of a keymap change: nothing is written.
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 3);
    let r = k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]);
    assert!(matches!(r, Err(AaError::NotSane(why)) if why[0].contains("reset")));
    assert!(sim.state().sent(cmd::SET_KEYMAP).is_empty());

    // Between two chunks of a write: the rest is not sent.
    k.read_all().unwrap();
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 12);
    assert!(matches!(k.write_per_key([(ENTER, [0xFF, 0, 0])]), Err(AaError::NotSane(_))));
    assert_eq!(sim.state().sent(cmd::SET_PER_KEY).len(), 2);

    // During the reads a restore plans with: nothing is written.
    k.read_all().unwrap();
    let backup = k.snapshot().unwrap().clone();
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    let before = writes(&sim);
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 2);
    assert!(matches!(k.restore(&backup, None), Err(AaError::NotSane(_))));
    assert_eq!(writes(&sim), before);
}

#[test]
fn nothing_is_written_before_a_sane_read() {
    let sim = Sim::ak980();
    let mut k = keyboard(&sim);
    assert!(matches!(k.apply_effect(&EffectSettings::factory()), Err(AaError::NotSane(_))));
    assert!(matches!(k.write_per_key([]), Err(AaError::NotSane(_))));
    assert!(matches!(k.apply_keymap(Layer::Base, []), Err(AaError::NotSane(_))));
    assert!(matches!(k.write_macros(&[]), Err(AaError::NotSane(_))));
    assert!(matches!(k.set_clock(&clock_time()), Err(AaError::NotSane(_))));
    assert!(sim.state().packets.is_empty());
}

#[test]
fn a_corrupted_read_closes_the_gate() {
    // The effect mode reads 0xFE, which no effect is.
    let sim = Sim::ak980().with_faults(Faults { corrupt: Some(cmd::EFFECT), ..Faults::default() });
    let mut k = keyboard(&sim);
    let snap = k.read_all().unwrap();
    assert!(!k.is_sane());
    assert!(k.sanity_reasons().iter().any(|r| r.contains("effect mode 254")), "{:?}", k.sanity_reasons());
    assert_eq!(snap.effect.0[0], 0xFE, "the snapshot is returned anyway");
    let before = sim.state().packets.len();
    assert!(matches!(k.apply_effect(&EffectSettings::factory()), Err(AaError::NotSane(_))));
    assert_eq!(sim.state().packets.len(), before);
}

#[test]
fn an_unanswered_device_information_closes_the_gate() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.silent_info = true;
    assert!(matches!(k.device_info(), Err(AaError::Timeout { cmd: 0x10 })));
    assert!(k.info().is_none());
    // The verdict and the writes agree, and say why.
    assert!(!k.is_sane());
    let why = k.sanity_reasons();
    assert!(why.len() == 1 && why[0].contains("went unanswered"), "{why:?}");
    let n = sim.state().packets.len();
    let r = k.apply_effect(&red_breath());
    assert!(matches!(&r, Err(AaError::NotSane(w)) if *w == why), "{r:?}");
    assert!(matches!(k.set_clock(&clock_time()), Err(AaError::NotSane(_))));
    assert_eq!(sim.state().packets.len(), n, "nothing sent");
    // Answered again, the gate opens again.
    sim.state().faults.silent_info = false;
    k.device_info().unwrap();
    assert!(k.is_sane(), "{:?}", k.sanity_reasons());
    k.apply_effect(&red_breath()).unwrap();
}

#[test]
fn the_sanity_gate() {
    let sim = Sim::ak980();
    let good = keyboard(&sim).read_all().unwrap();
    let usb = Sim::usb();
    assert_eq!(check_sanity(&good, usb), Ok(()));
    let fails = |change: &dyn Fn(&mut AaSnapshot), usb: UsbIdentity, want: &str| {
        let mut s = good.clone();
        change(&mut s);
        let why = check_sanity(&s, usb).expect_err(want);
        assert!(why.iter().any(|w| w.contains(want)), "{want}: {why:?}");
    };
    fails(&|_| {}, UsbIdentity { pid: 0x8009, ..usb }, "product id");
    fails(&|_| {}, UsbIdentity { release: 0x0106, ..usb }, "firmware version");
    let info = |i: usize, v: u8| {
        move |s: &mut AaSnapshot| {
            s.info.0[i] = v;
            s.device = DeviceInfo::parse(&s.info.0).unwrap().into();
        }
    };
    fails(&info(4, 0x46), usb, "vendor id");
    fails(&|s| s.info.0[17] = 99, usb, "does not match");
    fails(&|s| s.keymap.0[4 * 20] = 0x04, usb, "unknown entry page 0x04");
    fails(&|s| s.default_fn.0[4 * 7] = 0x0E, usb, "factory Fn layer slot 7");
    fails(&|s| s.effect.0[0] = 30, usb, "effect mode");
    fails(&|s| s.effect.0[9] = 7, usb, "brightness 7");
    fails(&|s| s.effect.0[10] = 7, usb, "speed 7");
    fails(&|s| s.effect.0[11] = 4, usb, "direction 4");
    fails(&|s| s.effect_color = Some(ColorRecord { color: [1, 2, 3], color_mode: 2 }), usb, "colour mode");
    fails(&|s| s.effect_mode = Some(7), usb, "remembered effect mode 7");
    fails(
        &|s| {
            s.effect.0[0] = 1;
            s.effect_mode = Some(128);
        },
        usb,
        "remembered effect mode 128 does not read back as 1",
    );
    fails(&|s| s.keymap.0.truncate(504), usb, "base layer read is 504 bytes");
    fails(&|s| s.macro_table.0[0..4].copy_from_slice(&100u32.to_le_bytes()), usb, "macro 0");
    fails(&|s| s.macro_table.0[8..12].copy_from_slice(&5000u32.to_le_bytes()), usb, "macro 2");
    // Levels other tools leave (0–6), any levels when off, per-key mode and
    // FUNC_V2 pages are all plausible.
    for (mode, brightness, speed) in [(1, 0, 0), (1, 6, 6), (0, 0, 9), (128, 1, 5)] {
        let mut s = good.clone();
        s.effect.0[0] = mode;
        s.effect.0[9] = brightness;
        s.effect.0[10] = speed;
        s.keymap.0[4 * 20] = 0x90;
        assert_eq!(check_sanity(&s, usb), Ok(()), "{mode} {brightness} {speed}");
    }
    // What firmware 769 really holds passes: a per-key table of all 00
    // (flashed, HID reset), all FF (never programmed), any entry without
    // its index; a 1 in byte 32 of the device information; a read-back
    // of 0 known to be per-key mode.
    for fill in [0x00u8, 0xFF] {
        let mut s = good.clone();
        s.per_key.0.fill(fill);
        assert_eq!(check_sanity(&s, usb), Ok(()), "per-key all {fill:02X}");
    }
    let mut s = good.clone();
    s.per_key.0[40] = 0;
    s.info.0[32] = 1;
    s.device = DeviceInfo::parse(&s.info.0).unwrap().into();
    s.effect.0[0] = 0;
    s.effect_mode = Some(128);
    assert_eq!(check_sanity(&s, usb), Ok(()));
    // The reasons come by kind, slot and value too.
    let mut s = good.clone();
    s.keymap.0[4 * 20] = 0x04;
    s.keymap.0[4 * 21] = 0x04;
    let kinds: Vec<(&str, String)> = check_reasons(&s, usb).iter().map(|r| (r.kind(), r.to_string())).collect();
    assert_eq!(
        kinds,
        [
            ("unknown-page", "base layer slot 20: unknown entry page 0x04".to_string()),
            ("unknown-page", "base layer slot 21: unknown entry page 0x04".to_string())
        ]
    );
    assert_eq!(check_reasons(&s, usb)[1], SanityReason::UnknownPage { layer: "base layer", slot: 21, page: 4 });
}

#[test]
fn a_flashed_keyboard_takes_per_key_colours() {
    // Issue #154: after AJAZZ's updater the per-key table is all 00, and
    // beta.1 refused every write. All FF (never programmed) is the same.
    for erased in [false, true] {
        let sim = Sim::ak980();
        if erased {
            sim.state().erased_per_key();
        }
        let mut k = ready(&sim);
        k.apply_per_key([(ENTER, [0xFF, 0, 0])], 5).unwrap();
        let s = sim.state();
        assert!(perkey::is_well_formed(&s.per_key), "every entry carries its index");
        assert_eq!(perkey::shown(&s.per_key, ENTER_AA), [0xFF, 0, 0]);
        assert_eq!(s.effect[0], 128);
        drop(s);
        assert!(k.is_sane(), "{:?}", k.sanity_reasons());
        // What the keys show now: Enter red, every other key black.
        let shown = k.read_live_colors().unwrap();
        let lit: Vec<u8> = shown.iter().filter(|(_, c)| *c != [0, 0, 0]).map(|(l, _)| *l).collect();
        assert_eq!(lit, [ENTER]);
    }
}

#[test]
fn levels_other_tools_leave_are_read_but_never_written() {
    // Brightness 6 and speed 0, as sibling boards and AJAZZ's hub leave them.
    let sim = Sim::ak980();
    sim.state().effect[9..11].copy_from_slice(&[6, 0]);
    let mut k = ready(&sim);
    assert_eq!(k.read_effect().unwrap().effect.brightness, 6);
    // Per-key mode at level 5 keeps the speed it read: 0 goes as 1.
    k.apply_per_key([], 5).unwrap();
    assert_eq!(sim.state().effect[9..11], [5, 1], "only 1–5 is ever sent");
}

#[test]
fn erased_or_impossible_macro_pointers_do_not_overflow() {
    // Erased flash: every table entry FF FF FF FF, which names no macro.
    let sim = Sim::ak980();
    sim.state().macros[..400].fill(0xFF);
    let mut k = ready(&sim);
    assert!(k.snapshot().unwrap().macro_blocks.is_empty());
    assert!(k.read_macros().unwrap().iter().all(Vec::is_empty));
    let addrs: Vec<u16> = sim.state().sent(cmd::MACROS).iter().map(|p| u16::from_le_bytes([p[4], p[5]])).collect();
    assert!(addrs.iter().all(|&a| a < 400), "only the table was read: {addrs:?}");
    // A pointer near the top of the address range is outside the memory,
    // without wrapping around to a small address.
    sim.state().macros[..4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    k.read_all().unwrap();
    assert!(!k.is_sane());
    assert!(k.sanity_reasons().iter().any(|r| r.contains("macro 0")), "{:?}", k.sanity_reasons());
}

#[test]
fn live_colours_map_back_to_leds() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // Static red, a custom colour: every key shows the effect colour.
    sim.state().effect[1..4].copy_from_slice(&[0xFF, 0, 0]);
    sim.state().custom_color = true;
    let c = k.read_live_colors().unwrap();
    assert_eq!(c.len(), 97);
    assert!(c.iter().all(|(_, rgb)| *rgb == [0xFF, 0, 0]));
    // Per-key mode with the custom colour: Enter's entry (slot 76) shows
    // on Enter's LED (85); entries without their index stay dark.
    {
        let mut s = sim.state();
        s.effect[0] = 128;
        s.custom_color = true;
        perkey::set(&mut s.per_key, ENTER_AA, [1, 2, 3]);
        perkey::set(&mut s.per_key, 34, [4, 5, 6]);
        s.per_key[34 * 4] = 35;
    }
    let c = k.read_live_colors().unwrap();
    assert_eq!(c.iter().find(|(led, _)| *led == ENTER).unwrap().1, [1, 2, 3]);
    let lit: Vec<u8> = c.iter().filter(|(_, rgb)| *rgb != [0, 0, 0]).map(|(l, _)| *l).collect();
    assert_eq!(lit, [ENTER]);
    let reqs = sim.state().sent(cmd::LIVE_COLORS).len();
    assert_eq!(reqs, 20, "two reads of 10 chunks");
}

#[test]
fn frame_version_1_lengthens_only_the_default_wait() {
    for (c, plain, v1) in [
        (cmd::EFFECT, 500, 2000),
        (cmd::DEVICE_INFO, 500, 2000),
        (cmd::SET_KEYMAP, 1000, 1000),
        (cmd::SET_PER_KEY, 2000, 2000),
        (cmd::REALTIME, 50, 50),
    ] {
        assert_eq!((timeout_ms_for(c, 0), timeout_ms_for(c, 1)), (plain, v1), "{c:02X}");
    }
}

// ------------------------------------------------------------------ effect

fn red_breath() -> EffectSettings {
    EffectSettings { mode: 7, color: [0xFF, 0x20, 0x00], rainbow: false, brightness: 4, speed: 0, direction: 1 }
}

const PINK: EffectColor = EffectColor { color: [0xFF, 0x69, 0xB4], color_mode: 0 };

/// Static pink, one colour, stored the WebHID driver's way.
fn pink_static() -> [u8; 16] {
    [1, 0xFF, 0x69, 0xB4, 0xFF, 0, 0, 0, 0, 5, 3, 0, 0, 0, 0xAA, 0x55]
}

#[test]
fn effects_are_written_in_the_keyboards_ranges() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_effect(&red_breath()).unwrap();
    assert_eq!(sim.state().effect, [7, 0xFF, 0x20, 0x00, 0xFF, 0, 0, 0, 0, 4, 1, 1, 0, 0, 0xAA, 0x55]);
    let w = sim.state().sent(cmd::SET_EFFECT)[0].clone();
    assert_eq!(&w[1..9], &[0xAA, 0x23, 0x10, 0, 0, 0, 1, 0]);
    // The colour reads back FF FF FF; the driver remembers what it wrote.
    let raw = k.snapshot().unwrap().effect.0.clone();
    assert_eq!(raw[1..4], [0xFF; 3]);
    let (back, known) = k.effect_settings().unwrap();
    assert!(known);
    assert_eq!(back, EffectSettings { speed: 1, ..red_breath() }, "speed 0 is stored as 1");
    // Out of range or unknown: refused before anything is sent.
    let n = sim.state().packets.len();
    assert!(matches!(k.apply_effect(&EffectSettings { mode: 22, ..red_breath() }), Err(AaError::Invalid(_))));
    assert!(matches!(k.apply_effect(&EffectSettings { brightness: 6, ..red_breath() }), Err(AaError::Invalid(_))));
    assert!(matches!(k.apply_effect(&EffectSettings { direction: 4, ..red_breath() }), Err(AaError::Invalid(_))));
    let raw_bad = AaEffect { brightness: 0, ..AaEffect::from_settings(&red_breath()).unwrap() };
    assert!(matches!(k.write_effect(&raw_bad), Err(AaError::Invalid(_))));
    assert_eq!(sim.state().packets.len(), n);
}

#[test]
fn another_tools_colour_is_never_overwritten_with_the_readback() {
    // Static pink set with AJAZZ's tool: every read now says white rainbow.
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(pink_static());
    let mut k = ready(&sim);
    let snap = k.snapshot().unwrap();
    assert_eq!((snap.effect.0[1..4].to_vec(), snap.effect.0[8]), (vec![0xFF; 3], 1));
    assert_eq!(snap.effect_color, None, "unknown, not white");
    let (settings, known) = k.effect_settings().unwrap();
    assert!(!known);
    assert_eq!((settings.mode, settings.brightness), (1, 5));

    // Per-key mode keeps the effect's other fields; its colour, unknown,
    // goes in as the neutral one, not the read-back's white rainbow.
    k.apply_per_key([(ENTER, [1, 2, 3])], 5).unwrap();
    let s = sim.state().effect;
    assert_eq!((s[..4].to_vec(), s[8]), (vec![128, 0xFF, 0xFF, 0xFF], 0), "the neutral colour");
    assert_eq!(k.current_effect().unwrap().color(), Some(NEUTRAL_COLOR), "written, so known");
    // A colour the caller gives goes as given.
    k.write_effect(&AaEffect::decode(&pink_static()).unwrap().with_color(PINK)).unwrap();
    assert_eq!(sim.state().effect, pink_static());
}

#[test]
fn a_colour_picked_on_the_keyboard_reads_back_as_itself() {
    // A palette colour (the keyboard's Fn keys) is no custom one: it reads
    // back, and takes the place of the one this driver wrote.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_effect(&red_breath()).unwrap();
    assert_eq!(k.read_effect().unwrap().color(), Some(EffectColor { color: [0xFF, 0x20, 0], color_mode: 0 }));
    {
        let mut s = sim.state();
        s.custom_color = false;
        s.palette_color = [0, 0xFF, 0];
    }
    let e = k.read_effect().unwrap();
    assert_eq!(
        (e.color(), e.effect.mode, e.mode_known),
        (Some(EffectColor { color: [0, 0xFF, 0], color_mode: 0 }), 7, true)
    );
}

#[test]
fn a_snapshot_keeps_the_colour_the_driver_knew() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // Before any write the read-back is the keyboard's own colour.
    let factory = ColorRecord { color: [0x00, 0x80, 0xFF], color_mode: 0 };
    assert_eq!(k.snapshot().unwrap().effect_color, Some(factory));
    k.apply_effect(&red_breath()).unwrap();
    let snap = k.snapshot().unwrap().clone();
    assert_eq!(snap.effect.0[1..4], [0xFF; 3]);
    let red = ColorRecord { color: [0xFF, 0x20, 0x00], color_mode: 0 };
    assert_eq!(snap.effect_color, Some(red));
    assert_eq!(k.read_all().unwrap().effect_color, Some(red), "still known in the same session");

    // A new session cannot know it, but the backup can put it back.
    let mut later = ready(&sim);
    assert_eq!(later.snapshot().unwrap().effect_color, None);
    later.apply_effect(&EffectSettings { mode: 1, ..red_breath() }).unwrap();
    let r = later.restore(&snap, None).unwrap();
    assert_eq!(r.written, ["effect"]);
    assert_eq!(sim.state().effect[..12], [7, 0xFF, 0x20, 0x00, 0xFF, 0, 0, 0, 0, 4, 1, 1]);
}

#[test]
fn a_reset_forgets_the_colour_the_driver_wrote() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_effect(&red_breath()).unwrap();
    assert!(k.read_effect().unwrap().color_known);
    // The keyboard resets while the effect is read: whatever it holds now,
    // the colour written before is no longer vouched for.
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 1);
    let e = k.read_effect().unwrap();
    assert_eq!(k.resets_reported(), 1);
    assert!(!e.color_known);
    assert_eq!(k.current_effect().unwrap().color(), None);
    k.read_all().unwrap();
    assert_eq!(k.snapshot().unwrap().effect_color, None);
    let (_, known) = k.effect_settings().unwrap();
    assert!(!known);
}

#[test]
fn an_effect_the_keyboard_did_not_keep_is_an_error() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.ignore_writes = Some(cmd::SET_EFFECT);
    let n = sim.state().packets.len();
    assert!(matches!(k.apply_effect(&red_breath()), Err(AaError::Readback { what: "effect" })));
    // The gate closed and the keyboard was read again at once: the read
    // after the write starts with the device information, and passed.
    let s = sim.state();
    let after: Vec<u8> = s.packets[n..].iter().map(|p| p[2]).collect();
    assert_eq!(&after[..3], &[cmd::SET_EFFECT, cmd::EFFECT, cmd::DEVICE_INFO]);
    drop(s);
    assert!(k.is_sane(), "{:?}", k.sanity_reasons());
}

#[test]
fn mode_128_reads_back_as_0_and_that_is_success() {
    // Firmware 769 reports modes 0, 20, 21 and 128 as 0 (§F.6): a per-key
    // save is no failure, and the driver knows the mode it wrote.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_per_key([(W, [0, 0, 0xFF])], 3).unwrap();
    assert_eq!(sim.state().effect_read()[0], 0, "what the keyboard reports");
    let e = k.read_effect().unwrap();
    assert!(e.mode_known && e.color_known);
    assert_eq!((e.effect.mode, e.effect.brightness), (128, 3));
    // The snapshot (and every backup taken from it) keeps the mode.
    let snap = k.read_all().unwrap();
    assert_eq!((snap.effect.0[0], snap.effect_mode), (0, Some(128)));
    assert_eq!(snap.effect().unwrap().mode, 128);
    assert!(snap.effect_mode_known());
    // A new session cannot tell 128 from off: it says it does not know.
    let mut later = ready(&sim);
    let e = later.read_effect().unwrap();
    assert!(!e.mode_known);
    assert_eq!(later.snapshot().unwrap().effect_mode, None);
    assert!(!later.snapshot().unwrap().effect_mode_known());
    // Off and the other per-key modes are no failure either.
    for mode in [0u8, 20, 21] {
        k.apply_effect(&EffectSettings { mode, ..red_breath() }).unwrap();
        assert_eq!(k.read_effect().unwrap().effect.mode, mode);
    }
}

#[test]
fn a_different_echo_is_an_error() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.garble_echo = true;
    assert!(matches!(k.apply_effect(&red_breath()), Err(AaError::Mismatch { cmd: 0x23 })));
    assert!(!k.is_sane(), "an unfinished write closes the gate");
}

#[test]
fn a_failed_write_asks_for_a_new_channel() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let n = sim.state().packets.len();
    sim.state().faults.fail_write = Some(n + 1);
    let err = k.apply_effect(&red_breath()).unwrap_err();
    assert!(err.needs_reopen(), "{err:?}");
    assert_eq!(sim.state().packets.len(), n, "not retried on the same channel");
}

// ------------------------------------------------------------------ lost echoes

/// Run `op` on a fresh keyboard, losing the echo of the `offset`-th request
/// it sends (counted from 1).
fn with_lost_echo(op: fn(&mut AaKeyboard<SimPort>), offset: Option<usize>) -> SimState {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    if let Some(o) = offset {
        let n = sim.state().packets.len();
        sim.state().faults.drop_reply = Some(n + o);
    }
    op(&mut k);
    let s = sim.state().clone();
    s
}

#[test]
fn lost_write_echoes_are_resent_where_they_belong() {
    // The write chunks: first, a middle one, the last (counted from the
    // operation's first request: fresh reads come first). The chunk goes
    // again where it was: on its page's first byte it erases the page
    // again before anything else is on it, elsewhere it programs the same
    // bytes over themselves.
    type Op = fn(&mut AaKeyboard<SimPort>);
    let cases: [(&str, Op, [usize; 3]); 4] = [
        ("keymap", |k| k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap(), [11, 15, 20]),
        ("per-key", |k| k.write_per_key([(ENTER, [0xFF, 0, 0])]).unwrap(), [11, 15, 20]),
        ("macros", |k| k.write_macros(&[a_macro()]).unwrap(), [1, 4, 8]),
        ("frame", |k| k.realtime_frame([(ENTER, [1, 2, 3])]).unwrap(), [1, 4, 7]),
    ];
    for (what, op, offsets) in cases {
        let clean = with_lost_echo(op, None);
        for o in offsets {
            let s = with_lost_echo(op, Some(o));
            assert_eq!(s.packets.len(), clean.packets.len() + 1, "{what} {o}: one resend");
            assert_eq!(s.keymap, clean.keymap, "{what} {o}");
            assert_eq!(s.per_key, clean.per_key, "{what} {o}");
            assert_eq!(s.macros, clean.macros, "{what} {o}");
            assert_eq!(s.frames, clean.frames, "{what} {o}");
        }
    }
}

#[test]
fn a_page_start_chunk_that_times_out_is_sent_again_at_once_and_the_page_survives() {
    // A retry repeats the chunk whose echo is missing, right away, never an
    // earlier one: a page-start chunk sent again after later chunks of its
    // page would erase them (beta.1's macro write did that).
    let step = MacroStep::new(StepKind::KeyDown, 0x04, 10);
    let big = [vec![step; 300], vec![step; 300]];
    let run = |lose: Option<u16>| {
        let sim = Sim::ak980();
        sim.state().macros.fill(0);
        let mut k = ready(&sim);
        if let Some(addr) = lose {
            // The n-th request is the macro chunk at `addr`.
            let n = sim.state().packets.len();
            let at = frame::paged_chunks(400 + 4 * 602).iter().position(|c| c.addr == addr).unwrap();
            sim.state().faults.drop_reply = Some(n + 1 + at);
        }
        k.write_macros(&big).unwrap();
        let s = sim.state().clone();
        s
    };
    let clean = run(None);
    for page_start in [0u16, 1024, 2048] {
        let s = run(Some(page_start));
        let sent: Vec<u16> = s.sent(cmd::SET_MACROS).iter().map(|w| u16::from_le_bytes([w[4], w[5]])).collect();
        let i = sent.iter().position(|&a| a == page_start).unwrap();
        assert_eq!(sent[i + 1], page_start, "{page_start}: the same chunk again, at once");
        assert_eq!(sent.iter().filter(|&&a| a == page_start).count(), 2, "{page_start}: once more, no other");
        assert_eq!(sent.len(), clean.sent(cmd::SET_MACROS).len() + 1);
        assert_eq!(s.macros, clean.macros, "{page_start}: the page is whole");
    }
}

// ------------------------------------------------------------------ per-key

#[test]
fn no_key_of_the_layout_sits_on_a_dead_per_key_slot() {
    // Firmware 769 maps 29 per-key slots to no key (LED 20, which nothing
    // else lights; slot 127 is the last to set it). Those slots are written
    // black: that must darken no key of the model.
    const DEAD: [u8; 29] = [
        13, 14, 15, 84, 86, 96, 99, 100, 101, 102, 103, 107, 111, 112, 113, 114, 115, 116, 117, 118, 119, 120, 121,
        122, 123, 124, 125, 126, 127,
    ];
    let l = layout();
    let on_dead: Vec<(u8, u8)> = l.keys().iter().filter(|k| DEAD.contains(&k.aa)).map(|k| (k.led, k.aa)).collect();
    assert_eq!(on_dead, [], "keys (LED, slot) on a dead slot");
    // And every one of them goes out black, carrying its index.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let all: Vec<(u8, Rgb)> = l.keys().iter().map(|k| (k.led, [0x10, 0x20, 0x30])).collect();
    k.write_per_key(all).unwrap();
    let s = sim.state();
    for slot in DEAD {
        assert_eq!(&s.per_key[usize::from(slot) * 4..usize::from(slot) * 4 + 4], &[slot, 0, 0, 0]);
    }
}

#[test]
fn per_key_colours_land_on_their_aa_slots() {
    let sim = Sim::ak980();
    // Slots that are no key of the layout (99, the knob's 13) and the old
    // `AA 55` tail: the whole table is written, those black with their
    // index in front.
    perkey::set(&mut sim.state().per_key, 99, [9, 9, 9]);
    perkey::set(&mut sim.state().per_key, 13, [7, 7, 7]);
    sim.state().per_key[510..].copy_from_slice(&[0xAA, 0x55]);
    let mut k = ready(&sim);
    k.write_per_key([(ENTER, [0xFF, 0, 0]), (ESC, [0, 0xFF, 0])]).unwrap();
    let s = sim.state();
    assert_eq!(&s.per_key[76 * 4..76 * 4 + 4], &[76, 0xFF, 0, 0], "Enter is slot 76 ...");
    assert_eq!(&s.per_key[85 * 4..85 * 4 + 4], &[85, 0, 0, 0], "... never 85 (Fn)");
    assert_eq!(&s.per_key[..4], &[0, 0, 0xFF, 0], "Esc is slot 0");
    assert_eq!(&s.per_key[99 * 4..99 * 4 + 4], &[99, 0, 0, 0]);
    assert_eq!(&s.per_key[13 * 4..13 * 4 + 4], &[13, 0, 0, 0]);
    assert_eq!(&s.per_key[504..], &[126, 0, 0, 0, 127, 0, 0, 0], "slots 126–127 like any other");
    assert!(perkey::is_well_formed(&s.per_key));
    let writes = s.sent(cmd::SET_PER_KEY);
    assert_eq!(writes.len(), 10);
    assert_eq!(&writes[9][1..9], &[0xAA, 0x24, 0x08, 0xF8, 0x01, 0x00, 0x01, 0x00]);
    drop(s);
    // Read back by LED.
    let back = k.read_per_key().unwrap();
    assert_eq!(back.iter().find(|(l, _)| *l == ENTER).unwrap().1, [0xFF, 0, 0]);
    assert!(matches!(k.write_per_key([(0, [1, 1, 1])]), Err(AaError::UnknownLed(0))));
}

#[test]
fn per_key_mode_keeps_the_rest_of_the_effect() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_per_key([(W, [0, 0, 0xFF])], 2).unwrap();
    let s = sim.state();
    assert_eq!(s.effect, [128, 0x00, 0x80, 0xFF, 0xFF, 0, 0, 0, 0, 2, 3, 0, 0, 0, 0xAA, 0x55]);
    assert_eq!(perkey::get(&s.per_key, 34), [0, 0, 0xFF], "W is slot 34");
    // The table went first, then the effect (its write makes the keyboard
    // show the table).
    let order: Vec<u8> = s.packets.iter().map(|p| p[2]).filter(|c| [0x23, 0x24].contains(c)).collect();
    assert_eq!(order.first(), Some(&0x24));
    assert_eq!(order.last(), Some(&0x23));
    drop(s);
    // Again: per-key mode reads back as 0, so the effect goes every time.
    k.apply_per_key([(W, [0, 0xFF, 0])], 2).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_EFFECT).len(), 2);
    assert_eq!(perkey::get(&sim.state().per_key, 34), [0, 0xFF, 0]);
    assert!(sim.state().custom_color);
}

#[test]
fn unchanged_tables_are_not_written() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // W's own factory entry: nothing to write.
    k.apply_keymap(Layer::Base, [(W, KeyAction::key(0x1A))]).unwrap();
    assert!(sim.state().sent(cmd::SET_KEYMAP).is_empty());
    // Every key black: the factory table (all 00) gets its indices once,
    // and then holds that table.
    k.write_per_key([]).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_PER_KEY).len(), 10);
    assert_eq!(sim.state().per_key, perkey::blank());
    k.write_per_key([]).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_PER_KEY).len(), 10);
}

#[test]
fn a_failed_full_read_closes_the_gate() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.silent_info = true;
    assert!(matches!(k.read_all(), Err(AaError::Timeout { cmd: 0x10 })));
    assert!(!k.is_sane());
    assert!(matches!(k.apply_effect(&red_breath()), Err(AaError::NotSane(_))));
}

// ------------------------------------------------------------------ keymap

#[test]
fn keymap_changes_touch_only_the_slots_asked_for() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let before = sim.state().keymap.clone();
    let shortcut = KeyAction::Key { mods: mods::LEFT_CTRL, code: 0x06 };
    k.apply_keymap(Layer::Base, [(A, shortcut), (ENTER, KeyAction::Disabled), (ESC, KeyAction::Default)]).unwrap();
    let after = sim.state().keymap.clone();
    assert_eq!(slot(&after, 49), [0x02, 0x01, 0x06, 0x00], "A is slot 49");
    assert_eq!(slot(&after, ENTER_AA), [0x02, 0x00, 0x00, 0x00]);
    assert_eq!(slot(&after, 0), [0x02, 0x00, 0x29, 0x00], "Esc was at its factory function already");
    for s in (0..128u8).filter(|s| ![49, ENTER_AA].contains(s)) {
        assert_eq!(slot(&after, s), slot(&before, s), "slot {s}");
    }
    assert_eq!(slot(&after, 13), [0x03, 0xE9, 0, 0], "the knob");
    // Read back as actions, by LED.
    let map = k.read_keymap(Layer::Base).unwrap();
    let of = |led| map.iter().find(|(l, _)| *l == led).unwrap().1;
    assert_eq!(of(A), Binding::Action(shortcut));
    assert_eq!(of(ENTER), Binding::Action(KeyAction::Disabled));
    assert_eq!(of(W), Binding::Action(KeyAction::key(0x1A)));
    assert_eq!(of(FN), Binding::Raw([0x02, 0x00, 0xAF, 0x00]), "the Fn key's own entry");
}

#[test]
fn default_gives_a_key_its_factory_function_back() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_keymap(Layer::Base, [(ESC, KeyAction::key(0x3A))]).unwrap();
    // Written as 00 00 00 00, read back as the factory Esc: that is success.
    k.apply_keymap(Layer::Base, [(ESC, KeyAction::Default)]).unwrap();
    let s = sim.state();
    let first_chunks: Vec<&Vec<u8>> = s.sent(cmd::SET_KEYMAP).into_iter().filter(|p| p[4..6] == [0, 0]).collect();
    assert_eq!(&first_chunks.last().unwrap()[9..13], &[0, 0, 0, 0], "sent as the factory function");
    assert_eq!(slot(&s.keymap, 0), [0x02, 0x00, 0x29, 0x00], "the keyboard fills in its own");
    drop(s);
    assert_eq!(slot(&k.snapshot().unwrap().keymap.0, 0), [0x02, 0x00, 0x29, 0x00]);
    assert!(k.is_sane());
}

#[test]
fn default_never_brings_back_a_factory_firmware_function() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // Fn+Space is factory reset from the factory; the user put Space there.
    k.apply_keymap(Layer::Fn, [(SPACE, KeyAction::key(0x2C))]).unwrap();
    let n = writes(&sim);
    let err = k.apply_keymap(Layer::Fn, [(SPACE, KeyAction::Default)]).unwrap_err();
    assert!(matches!(err, AaError::Unsupported(why) if why.contains("factory function")), "{err}");
    assert_eq!(writes(&sim), n);
    assert_eq!(slot(&sim.state().fn_keymap, SPACE_AA), [0x02, 0x00, 0x2C, 0x00]);
    // Where the key still is its factory function, Default changes nothing.
    k.apply_keymap(Layer::Fn, [(BACKSPACE, KeyAction::Default)]).unwrap();
    assert_eq!(writes(&sim), n);
}

#[test]
fn the_fn_layer_keeps_its_firmware_functions() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let before = sim.state().fn_keymap.clone();
    k.apply_keymap(Layer::Fn, [(W, KeyAction::Consumer(0xE9)), (A, KeyAction::Wheel { up: true })]).unwrap();
    let s = sim.state();
    assert_eq!(slot(&s.fn_keymap, 34), [0x03, 0xE9, 0, 0]);
    assert_eq!(slot(&s.fn_keymap, SPACE_AA), [0x0D, 0, 0, 1], "factory reset stays on Space, where it was");
    assert_eq!(func_ids(slot(&s.fn_keymap, 92)), [7]);
    assert_eq!(&s.fn_keymap[510..], &before[510..]);
    assert_eq!(s.sent(cmd::SET_FN_KEYMAP).len(), 10);
    assert!(s.sent(cmd::SET_KEYMAP).is_empty(), "the base layer is untouched");
    drop(s);
    let map = k.read_default_fn().unwrap();
    let space = map.iter().find(|(l, _)| *l == SPACE).unwrap().1;
    assert_eq!(space, Binding::Raw([page::FUNC, 0, 0, 1]));
}

#[test]
fn what_the_keyboard_cannot_do_is_refused_before_sending() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let n = sim.state().packets.len();
    let refused = [
        (FN, KeyAction::key(0x04)),
        (A, KeyAction::Mouse(MouseButton::DoubleClick)),
        (A, KeyAction::Macro { slot: 0, play: MacroPlay::WhileHeld }),
        (A, KeyAction::Key { mods: mods::LEFT_CTRL | mods::LEFT_ALT, code: 0 }),
    ];
    assert!(matches!(k.apply_keymap(Layer::Base, [refused[1]]), Err(AaError::Unsupported(_))));
    assert!(matches!(k.apply_keymap(Layer::Base, [refused[2]]), Err(AaError::Unsupported(_))));
    assert!(matches!(k.apply_keymap(Layer::Fn, [refused[3]]), Err(AaError::Unsupported(_))));
    assert!(matches!(k.apply_keymap(Layer::Base, [(0, KeyAction::Default)]), Err(AaError::UnknownLed(0))));
    assert!(matches!(k.apply_keymap(Layer::Base, [(A, KeyAction::key(0xAF))]), Err(AaError::Invalid(_))));
    assert_eq!(sim.state().packets.len(), n, "nothing sent");
    // The Fn key: a fresh read, then refused (not written).
    assert!(matches!(k.apply_keymap(Layer::Base, [refused[0]]), Err(AaError::Unsupported(_))));
    assert!(sim.state().sent(cmd::SET_KEYMAP).is_empty());
    // The knob has no LED, so it cannot even be named.
    assert!(layout().led(13).is_none());
}

#[test]
fn a_keymap_the_keyboard_did_not_keep_is_an_error() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.ignore_writes = Some(cmd::SET_KEYMAP);
    let r = k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]);
    assert!(matches!(r, Err(AaError::Readback { what: "keymap" })));
}

// ------------------------------------------------------------------ macros

fn a_macro() -> Vec<MacroStep> {
    vec![MacroStep::new(StepKind::KeyDown, 0x04, 10), MacroStep::new(StepKind::KeyUp, 0x04, 0)]
}

#[test]
fn the_a_macro() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.write_macros(&[a_macro()]).unwrap();
    let s = sim.state();
    assert_eq!(&s.macros[..4], &[0x90, 0x01, 0x00, 0x00], "pointer 400");
    assert_eq!(&s.macros[400..412], &[0x04, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x04, 0xB0, 0x00, 0x00, 0x04, 0x30]);
    // One image from address 0, each byte once: the table and the block,
    // 412 bytes in 8 chunks, LAST on the final one only.
    let writes = s.sent(cmd::SET_MACROS);
    assert_eq!(writes.len(), 8);
    let addrs: Vec<u16> = writes.iter().map(|w| u16::from_le_bytes([w[4], w[5]])).collect();
    assert_eq!(addrs, [0, 56, 112, 168, 224, 280, 336, 392]);
    assert_eq!(&writes[0][9..13], &[0x90, 0x01, 0x00, 0x00], "the table first");
    assert_eq!(&writes[7][1..9], &[0xAA, 0x25, 20, 0x88, 0x01, 0, 1, 0]);
    assert!(writes[..7].iter().all(|w| w[7] == 0));
    drop(s);
    let back = k.read_macros().unwrap();
    assert_eq!(macros::to_steps(&back[0]).unwrap(), a_macro());
    assert!(back[1..].iter().all(Vec::is_empty));
    let snap = k.snapshot().unwrap();
    assert_eq!(snap.macro_blocks.len(), 1);
    assert_eq!(check_sanity(snap, Sim::usb()), Ok(()));
    // A full read finds it too.
    let snap = k.read_all().unwrap();
    assert_eq!(snap.macro_blocks[0].addr, 400);
    assert_eq!(snap.macros()[0].len(), 2);
}

#[test]
fn fresh_reads_of_keys_and_macros_update_the_snapshot() {
    // Changed in another program (AJAZZ's web tool) after the full read.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let elsewhere = {
        let mut st = sim.state();
        keymap::set_entry(&mut st.keymap, 20, [page::KEYBOARD, 0, 0x06, 0]);
        st.keymap.clone()
    };
    let img = macros::encode(&[macros::from_steps(&a_macro()).unwrap()], macros::CAPACITY).unwrap();
    {
        let mut st = sim.state();
        st.macros[..img.table.len()].copy_from_slice(&img.table);
        let at = usize::from(macros::BLOCKS_AT);
        st.macros[at..at + img.blocks.len()].copy_from_slice(&img.blocks);
    }
    assert_ne!(k.snapshot().unwrap().keymap.0, elsewhere);
    k.read_keymap(Layer::Base).unwrap();
    assert_eq!(k.snapshot().unwrap().keymap.0, elsewhere, "the snapshot shows the keyboard as it is now");
    assert!(k.snapshot().unwrap().macros()[0].is_empty());
    k.read_macros().unwrap();
    assert_eq!(k.snapshot().unwrap().macros()[0], macros::from_steps(&a_macro()).unwrap());
}

#[test]
fn macro_entries_the_steps_cannot_express_go_back_byte_for_byte() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // A wheel turn (button 255, as AJAZZ writes it) and a 2 ms pause: no
    // app step holds either.
    let wheel = vec![
        AaMacroEntry { delay_ms: 2, code: 0xFF, flag: macros::flag::MOUSE_DOWN },
        AaMacroEntry { delay_ms: 0, code: 0xFF, flag: macros::flag::MOUSE_UP },
    ];
    assert!(macros::to_steps(&wheel).is_none());
    let a = macros::from_steps(&a_macro()).unwrap();
    k.write_macro_entries(&[a.clone(), Vec::new(), wheel.clone()]).unwrap();
    let back = k.read_macros().unwrap();
    assert_eq!(back[0], a);
    assert!(back[1].is_empty());
    assert_eq!(back[2], wheel);
    assert_eq!(k.snapshot().unwrap().macros()[2], wheel);
}

#[test]
fn a_macro_write_that_fails_part_way_closes_the_gate() {
    // The table comes first in the one image a write sends: one that stops
    // part-way can leave a pointer to a block that is not there. The gate
    // closes until a read clears it.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.write_macros(&[a_macro()]).unwrap();
    let n = sim.state().packets.len();
    sim.state().faults.fail_write = Some(n + 8);
    let err = k.write_macros(&[a_macro(), a_macro()]).unwrap_err();
    assert!(err.needs_reopen(), "{err}");
    assert!(!k.is_sane());
    assert!(k.sanity_reasons()[0].contains("did not finish"));

    // The keyboard drops the writes from the third chunk on (it echoes
    // them all the same): the read-back tells, the gate closes and the
    // keyboard is read again. The page was erased by the first chunk, so
    // the second macro's pointer leads to erased flash: the read finds it
    // outside the macro memory, and the keyboard stays read-only.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let n = sim.state().packets.len();
    sim.state().faults.drop_writes_from = Some(n + 3);
    let long: Vec<MacroStep> = (0..40)
        .map(|i| MacroStep::new(if i % 2 == 0 { StepKind::KeyDown } else { StepKind::KeyUp }, 0x05, 10))
        .collect();
    let err = k.write_macros(&[a_macro(), long]).unwrap_err();
    assert!(matches!(err, AaError::Readback { what: "macros" }), "{err}");
    assert!(!k.is_sane());
    assert!(k.sanity_reasons().iter().any(|r| r.contains("macro 1 at address 412")), "{:?}", k.sanity_reasons());
}

#[test]
fn macros_fill_all_three_pages_of_macro_memory() {
    // After a HID reset the macro pages hold 00, which no write can turn
    // into anything else unless the page is erased first: every page needs
    // a chunk on its first byte.
    let sim = Sim::ak980();
    sim.state().macros.fill(0);
    let mut k = ready(&sim);
    assert_eq!(k.macro_capacity(), 2672);
    let step = MacroStep::new(StepKind::KeyDown, 0x04, 10);
    k.write_macros(&[vec![step; 300], vec![step; 300], vec![step; 60]]).unwrap();
    let s = sim.state();
    let starts: Vec<u16> = s.sent(cmd::SET_MACROS).iter().map(|w| u16::from_le_bytes([w[4], w[5]])).collect();
    assert!(starts.contains(&1024) && starts.contains(&2048), "{starts:?}");
    let ends: Vec<usize> = s
        .sent(cmd::SET_MACROS)
        .iter()
        .map(|w| usize::from(u16::from_le_bytes([w[4], w[5]])) + usize::from(w[3]))
        .collect();
    assert!(ends.iter().all(|&e| e <= macros::AREA_LEN));
    assert!(s.dropped.is_empty());
    drop(s);
    let back = k.read_macros().unwrap();
    assert_eq!((back[0].len(), back[1].len(), back[2].len()), (300, 300, 60));
    assert!(k.is_sane());
}

#[test]
fn macro_capacity_edges() {
    // Firmware 769 reports 3072 bytes: 2672 after the table.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let step = MacroStep::new(StepKind::KeyDown, 0x04, 10);
    k.write_macros(&[vec![step; 667]]).unwrap();
    let n = sim.state().packets.len();
    let too_big = k.write_macros(&[vec![step; 668]]).unwrap_err();
    assert!(matches!(too_big, AaError::TooLarge { macros: 1, entries: 668, bytes: 2676, limit: 2672 }), "{too_big}");
    assert!(too_big.to_string().contains("2676 bytes"));
    assert!(matches!(k.write_macros(&vec![vec![step]; 100]), Err(AaError::TooLarge { macros: 100, .. })));
    assert_eq!(sim.state().packets.len(), n);
    // A keyboard reporting less gets the WebHID driver's 512.
    let small = Sim::ak980();
    small.state().info[2..4].copy_from_slice(&0u16.to_le_bytes());
    let mut k2 = ready(&small);
    assert_eq!(k2.macro_capacity(), 512);
    assert!(matches!(k2.write_macros(&[vec![step; 128]]), Err(AaError::TooLarge { limit: 512, .. })));
    // Clearing every macro: the empty table only.
    k.write_macros(&[]).unwrap();
    assert!(sim.state().macros[..400].iter().all(|&b| b == 0));
    assert_eq!(sim.state().packets.len(), n + 8 + 8, "8 table chunks written, 8 read back");
}

// ------------------------------------------------------------------ clock

fn clock_time() -> ClockTime {
    ClockTime { year: 2026, month: 10, day: 19, hour: 11, minute: 24, second: 34, weekday: 1 }
}

#[test]
fn the_clock_matches_the_capture() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.set_clock(&clock_time()).unwrap();
    let s = sim.state();
    let w = s.sent(cmd::CLOCK);
    assert_eq!(
        &w[0][1..19],
        &[0xAA, 0x34, 0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x5A, 0x01, 0x5A, 0x1A, 0x0A, 0x13, 0x0B, 0x18, 0x22, 0x01]
    );
    assert_eq!(s.clock.as_deref().map(|c| c.len()), Some(24));
    drop(s);
    // Nothing reads the clock back: a reset reported meanwhile means it
    // did not count.
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 1);
    assert!(matches!(k.set_clock(&clock_time()), Err(AaError::NotSane(why)) if why[0].contains("reset")));
}

// ------------------------------------------------------------------ live lighting

#[test]
fn live_frames_show_in_any_mode_and_write_nothing() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // The factory effect (static) stays as it is: no switch, no note.
    let n = sim.state().packets.len();
    k.realtime_frame([(ENTER, [1, 2, 3]), (0, [9, 9, 9])]).unwrap();
    let s = sim.state();
    assert!(s.packets[n..].iter().all(|p| p[2] == cmd::REALTIME), "the effect is neither read nor written");
    assert_eq!(s.effect, FACTORY_EFFECT);
    let (frame, visible) = s.frames.last().unwrap().clone();
    assert!(visible);
    assert_eq!(frame.len(), 97 * 4, "every key of the layout, in layout order");
    assert_eq!(&frame[..4], &[0, 0, 0, 0], "Esc first, black");
    assert!(frame.chunks(4).any(|e| e == [ENTER_AA, 1, 2, 3]));
    assert!(!frame.chunks(4).any(|e| e[0] == FN_AA && e[1..] != [0, 0, 0]));
    let chunks = s.sent(cmd::REALTIME);
    assert_eq!(chunks.len(), 7);
    assert_eq!(&chunks[0][1..9], &[0xAA, 0x32, 0x38, 0x00, 0x00, 0x00, 0x00, 0x00]);
    assert_eq!(&chunks[6][1..9], &[0xAA, 0x32, 0x34, 0x50, 0x01, 0x00, 0x01, 0x00], "52 bytes at 336, LAST");
    drop(s);
    // The keys show the frame; the effect again once frames stop.
    let shown = k.read_live_colors().unwrap();
    assert_eq!(shown.iter().find(|(l, _)| *l == ENTER).unwrap().1, [1, 2, 3]);
    assert_eq!(shown.iter().find(|(l, _)| *l == ESC).unwrap().1, [0, 0, 0]);
    sim.state().frame_hold = Duration::ZERO;
    let shown = k.read_live_colors().unwrap();
    assert_eq!(shown.iter().find(|(l, _)| *l == ENTER).unwrap().1, [0x00, 0x80, 0xFF]);
    assert_eq!(writes(&sim), 7, "only the frame went");
}

#[test]
fn live_frames_need_no_sanity_gate_but_this_keyboard() {
    // A keyboard the gate keeps read-only (an effect mode the driver does
    // not know) still shows frames: they store nothing.
    let sim = Sim::ak980();
    sim.state().effect[0] = 99;
    let mut k = keyboard(&sim);
    k.read_all().unwrap();
    assert!(!k.is_sane());
    k.realtime_frame([(ENTER, [1, 2, 3])]).unwrap();
    assert_eq!(sim.state().sent(cmd::REALTIME).len(), 7);
    // A fresh driver asks the device information first; one that names
    // another USB device gets no frame.
    let sim = Sim::ak980();
    let mut k = keyboard(&sim);
    k.realtime_frame([]).unwrap();
    assert_eq!(sim.state().packets[0][2], cmd::DEVICE_INFO);
    let other = Sim::ak980();
    let mut k = AaKeyboard::new(other.command(), layout(), UsbIdentity { pid: 0x8039, release: 0x0302 });
    k.set_settle(Duration::ZERO);
    assert!(matches!(k.realtime_frame([]), Err(AaError::NotSane(why)) if why[0].contains("not this USB device")));
    assert!(other.state().sent(cmd::REALTIME).is_empty());
}

// ------------------------------------------------------------------ the write-drop window

#[test]
fn nothing_is_written_while_the_keyboard_may_drop_it() {
    // Just switched on: the power-on animation drops writes (and echoes
    // them). The driver waits its settle time before the first write.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().power_on(Duration::from_millis(60));
    k.set_settle(Duration::from_millis(120));
    // Frames are refused meanwhile, nothing sent.
    assert!(matches!(k.realtime_frame([]), Err(AaError::Settling { .. })));
    assert!(sim.state().sent(cmd::REALTIME).is_empty());
    let t = Instant::now();
    k.apply_effect(&red_breath()).unwrap();
    assert!(t.elapsed() >= Duration::from_millis(100), "the write waited");
    assert!(sim.state().dropped.is_empty());
    assert_eq!(sim.state().effect[0], 7);
    k.realtime_frame([]).unwrap();
}

#[test]
fn a_write_the_keyboard_dropped_closes_the_gate_and_reads_it_again() {
    // Too short a settle time: the write is dropped, the echo comes all
    // the same. The read-back tells; the keyboard is read again at once.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().power_on(Duration::from_millis(200));
    let before = sim.state().keymap.clone();
    let r = k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]);
    assert!(matches!(r, Err(AaError::Readback { what: "keymap" })), "{r:?}");
    assert_eq!(sim.state().dropped.len(), 10);
    assert_eq!(sim.state().keymap, before, "nothing changed");
    // Read again (sane: nothing changed), and the next write waits.
    assert!(k.is_sane(), "{:?}", k.sanity_reasons());
    assert_eq!(k.snapshot().unwrap().keymap.0, before);
}

#[test]
fn after_a_reset_frames_and_writes_wait_out_the_blink() {
    let sim = Sim::ak980().with_faults(Faults { reset_blink: Some(Duration::from_millis(80)), ..Faults::default() });
    let mut k = ready(&sim);
    k.set_settle(Duration::from_millis(100));
    std::thread::sleep(Duration::from_millis(110));
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 1);
    k.read_effect().unwrap();
    assert_eq!(k.resets_reported(), 1);
    // The keyboard blinks: frames are refused, not sent into it.
    assert!(matches!(k.realtime_frame([(ENTER, [1, 2, 3])]), Err(AaError::Settling { .. })));
    assert!(sim.state().sent(cmd::REALTIME).is_empty());
    std::thread::sleep(Duration::from_millis(110));
    k.realtime_frame([(ENTER, [1, 2, 3])]).unwrap();
    assert!(sim.state().frames.last().unwrap().1, "shown");
    // A reset during a frame: the rest of it waits too.
    let n = sim.state().packets.len();
    sim.state().faults.reset_before = Some(n + 2);
    assert!(matches!(k.realtime_frame([]), Err(AaError::Settling { .. })));
    assert_eq!(sim.state().packets.len(), n + 2);
}

// ------------------------------------------------------------------ screen

fn upload(sim: &Sim, k: &mut AaKeyboard<SimPort>) -> Result<AaScreenReport> {
    k.upload_screen(&mut sim.display(), &ScreenAnimation::still(test_pattern()), |_, _| {}, &AtomicBool::new(false))
}

#[test]
fn a_still_image_is_sixteen_packets() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let mut display = sim.display();
    let mut seen = Vec::new();
    let r = k
        .upload_screen(
            &mut display,
            &ScreenAnimation::still(test_pattern()),
            |d, t| seen.push((d, t)),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!((r.packets, r.packet_times.len(), r.stale_acks), (16, 16, 0));
    assert_eq!(seen.last(), Some(&(16, 16)));
    let s = sim.state();
    assert_eq!(s.screen.len(), 16);
    assert_eq!(&s.screen[0][..10], &[0xAA, 0x50, 0, 0, 16, 0, 0x50, 0x06, 1, 0]);
    assert!(s.screen.iter().all(|p| p.len() == 4104));
    assert_eq!(&s.screen[15][2..4], &[15, 0]);
}

#[test]
fn one_hundred_forty_frames() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let anim = ScreenAnimation::new(vec![(Frame::filled(rgb565(0, 0, 255)), 50); 140]).unwrap();
    let r = k.upload_screen(&mut sim.display(), &anim, |_, _| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(r.packets, 2215);
    let s = sim.state();
    assert_eq!(s.screen.len(), 2215);
    assert_eq!(&s.screen[0][8..11], &[140, 50, 50]);
    assert_eq!(s.screen[0][8 + 140], 0, "no delay for the last frame");
}

#[test]
fn screen_answers_are_counted_one_per_packet_sent() {
    // The simulator refuses a packet written while an answer is unread, so
    // each of these uploads only finishes if no answer is taken for the
    // wrong packet.
    let late = Sim::ak980().with_faults(Faults { late_ack: Some(3), ..Faults::default() });
    let r = upload(&late, &mut ready(&late)).unwrap();
    assert_eq!(r.packets, 16);
    let s = late.state();
    assert_eq!(s.screen.len(), 17, "packet 2 went out twice");
    assert_eq!(s.screen[2], s.screen[3]);
    drop(s);

    let double = Sim::ak980().with_faults(Faults { double_ack: Some(2), ..Faults::default() });
    let r = upload(&double, &mut ready(&double)).unwrap();
    assert_eq!((r.packets, r.stale_acks), (16, 1));
    assert_eq!(double.state().screen.len(), 16);

    let lost = Sim::ak980().with_faults(Faults { drop_ack: Some(5), ..Faults::default() });
    upload(&lost, &mut ready(&lost)).unwrap();
    assert_eq!(lost.state().screen.len(), 17);
}

#[test]
fn after_a_cancelled_upload_the_command_channel_waits() {
    let sim = Sim::ak980();
    sim.state().upload_quiet = Duration::from_millis(20);
    let mut k = ready(&sim);
    k.set_upload_cooldown(Duration::from_millis(60));
    let cancel = AtomicBool::new(false);
    let still = ScreenAnimation::still(test_pattern());
    let r = k.upload_screen(
        &mut sim.display(),
        &still,
        |done, _| {
            if done == 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    assert!(matches!(r, Err(AaError::Cancelled { done: 3, total: 16 })));
    let n = sim.state().packets.len();
    assert!(matches!(k.read_effect(), Err(AaError::UploadInterrupted { .. })));
    assert!(matches!(k.apply_effect(&red_breath()), Err(AaError::UploadInterrupted { .. })));
    assert!(matches!(k.device_info(), Err(AaError::UploadInterrupted { .. })));
    assert_eq!(sim.state().packets.len(), n, "nothing on the command channel");
    // After the cooldown: one device-information request, then the rest.
    std::thread::sleep(Duration::from_millis(80));
    k.read_effect().unwrap();
    let s = sim.state();
    assert_eq!((s.packets[n][2], s.packets[n + 1][2]), (cmd::DEVICE_INFO, cmd::EFFECT));
}

#[test]
fn an_upload_that_stops_answering_silences_the_command_channel() {
    let sim = Sim::ak980().with_faults(Faults { mute_display_from: Some(5), ..Faults::default() });
    sim.state().upload_quiet = Duration::from_millis(20);
    let mut k = ready(&sim);
    k.set_upload_cooldown(Duration::from_millis(60));
    assert!(matches!(upload(&sim, &mut k), Err(AaError::Timeout { cmd: 0x50 })));
    assert_eq!(sim.state().screen.len(), 4 + ATTEMPTS);
    let n = sim.state().packets.len();
    assert!(matches!(k.set_clock(&clock_time()), Err(AaError::UploadInterrupted { .. })));
    assert!(matches!(upload(&sim, &mut k), Err(AaError::UploadInterrupted { .. })), "nor a new upload yet");
    assert_eq!(sim.state().packets.len(), n);
    std::thread::sleep(Duration::from_millis(80));
    k.set_clock(&clock_time()).unwrap();
}

#[test]
fn screen_limits() {
    let sim = Sim::ak980();
    sim.state().tft_frames = 24; // the keyboard takes 24 frames
    let mut k = ready(&sim);
    let anim = ScreenAnimation::new(vec![(Frame::filled(0), 50); 25]).unwrap();
    let r = k.upload_screen(&mut sim.display(), &anim, |_, _| {}, &AtomicBool::new(false));
    assert!(matches!(r, Err(AaError::Invalid(CodecError::Range { max: 24, .. }))));
    // Cancelled before the first packet: nothing sent, nothing to wait for.
    let still = ScreenAnimation::still(test_pattern());
    let r = k.upload_screen(&mut sim.display(), &still, |_, _| {}, &AtomicBool::new(true));
    assert!(matches!(r, Err(AaError::Cancelled { done: 0, total: 16 })));
    assert!(sim.state().screen.is_empty());
    k.read_effect().unwrap();

    let sim = Sim::ak980();
    sim.state().info[28] = 255; // no screen
    let mut k = ready(&sim);
    let r = k.upload_screen(&mut sim.display(), &still, |_, _| {}, &AtomicBool::new(false));
    assert!(matches!(r, Err(AaError::Unsupported(_))));
}

// ------------------------------------------------------------------ backup

#[test]
fn a_backup_restores_what_was_changed() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    let original = sim.state().clone();

    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    k.apply_keymap(Layer::Fn, [(W, KeyAction::Consumer(0xE9))]).unwrap();
    k.apply_effect(&red_breath()).unwrap();
    k.write_per_key([(ENTER, [0xFF, 0, 0])]).unwrap();
    k.write_macros(&[a_macro()]).unwrap();

    let r = k.restore(&backup, None).unwrap();
    assert!(r.kept.is_empty());
    assert!(!r.effect_mode_guessed);
    assert_eq!(r.written, ["base layer", "Fn layer", "per-key table", "effect", "macros"]);
    let s = sim.state();
    assert_eq!(s.keymap, original.keymap);
    assert_eq!(s.fn_keymap, original.fn_keymap);
    // The factory table (all 00) shows every key dark: it goes back as
    // every key black, each entry with its index.
    assert!(original.per_key.iter().all(|&b| b == 0));
    assert_eq!(s.per_key, perkey::blank());
    assert_eq!(&s.macros[..400], &original.macros[..400]);
    assert_eq!(s.effect[..12], original.effect[..12]);
    assert_eq!(s.effect[14..], [0xAA, 0x55], "written the WebHID driver's way");
    assert!(s.sent(0x21).is_empty(), "options are never written");
    drop(s);
    // Restoring again changes nothing.
    assert!(k.restore(&backup, None).unwrap().written.is_empty());
}

#[test]
fn a_restore_keeps_firmware_functions_the_keyboard_no_longer_has() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    let original = sim.state().keymap.clone();
    // The user changed the base layer, and put ordinary keys on Fn+Space
    // (factory reset) and Fn+↑ (brightness up).
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    k.apply_keymap(Layer::Fn, [(SPACE, KeyAction::key(0x2C)), (UP, KeyAction::Consumer(0xE9))]).unwrap();
    let r = k.restore(&backup, None).unwrap();
    assert_eq!(r.written, ["base layer"], "the base layer is restored all the same");
    assert_eq!(sim.state().keymap, original);
    let kept: Vec<(Layer, u8, Option<u8>)> = r.kept.iter().map(|s| (s.layer, s.slot, s.led)).collect();
    assert_eq!(kept, [(Layer::Fn, SPACE_AA, Some(SPACE)), (Layer::Fn, 90, Some(UP))]);
    assert_eq!(r.kept[0].backup, [0x0D, 0, 0, 1]);
    assert!(r.kept[0].reason.contains("firmware function 1"), "{}", r.kept[0].reason);
    assert_eq!(slot(&sim.state().fn_keymap, SPACE_AA), [0x02, 0x00, 0x2C, 0x00]);
}

#[test]
fn a_restore_keeps_entries_the_encoder_would_refuse() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let mut backup = k.snapshot().unwrap().clone();
    // Macro 200 in play mode 7: page 6, but nothing this driver writes.
    keymap::set_entry(&mut backup.keymap.0, 49, [0x06, 200, 7, 0]);
    let r = k.restore(&backup, None).unwrap();
    assert_eq!(r.kept.iter().map(|s| (s.layer, s.slot)).collect::<Vec<_>>(), [(Layer::Base, 49)]);
    assert!(r.written.is_empty());
    assert_eq!(slot(&sim.state().keymap, 49), [0x02, 0x00, 0x04, 0x00]);
}

#[test]
fn a_restore_reports_the_fn_key_knob_and_tail_it_keeps() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let original = sim.state().clone();
    let mut backup = k.snapshot().unwrap().clone();
    let other = [0x02, 0x00, 0x05, 0x00];
    for (fn_layer, s) in [(false, FN_AA), (false, 13), (true, 126)] {
        let table = if fn_layer { &mut backup.fn_keymap.0 } else { &mut backup.keymap.0 };
        assert_ne!(slot(table, s), other);
        keymap::set_entry(table, s, other);
    }
    let r = k.restore(&backup, None).unwrap();
    assert!(r.written.is_empty(), "nothing else differs");
    let kept: Vec<(Layer, u8, [u8; 4])> = r.kept.iter().map(|s| (s.layer, s.slot, s.backup)).collect();
    assert_eq!(kept, [(Layer::Base, FN_AA, other), (Layer::Base, 13, other), (Layer::Fn, 126, other)]);
    assert_eq!(r.kept[0].led, Some(FN));
    assert!(r.kept[0].reason.contains("Fn key"), "{}", r.kept[0].reason);
    assert!(r.kept[1].reason.contains("knob"), "{}", r.kept[1].reason);
    assert!(r.kept[2].reason.contains("126"), "{}", r.kept[2].reason);
    let s = sim.state();
    assert_eq!((&s.keymap, &s.fn_keymap), (&original.keymap, &original.fn_keymap));
}

#[test]
fn a_restore_that_fails_part_way_says_what_it_wrote() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    k.apply_keymap(Layer::Fn, [(W, KeyAction::Consumer(0xE9))]).unwrap();
    k.apply_effect(&red_breath()).unwrap();
    sim.state().faults.ignore_writes = Some(cmd::SET_EFFECT);
    let err = k.restore(&backup, None).unwrap_err();
    let AaError::RestoreIncomplete { section, report, error } = &err else { panic!("{err:?}") };
    assert_eq!((*section, report.written.as_slice()), ("effect", &["base layer", "Fn layer"][..]));
    assert!(matches!(**error, AaError::Readback { what: "effect" }));
    assert!(!err.needs_reopen());
    assert!(err.to_string().contains("already written: base layer, Fn layer"), "{err}");
    assert_eq!(sim.state().keymap, backup.keymap.0, "the base layer is back");

    // A stalled first write: nothing written, and the channel must go.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    let n = sim.state().packets.len();
    k.restore(&backup, None).unwrap();
    let reads = sim.state().packets.len() - n;
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    let n = sim.state().packets.len();
    sim.state().faults.fail_write = Some(n + reads + 1);
    let err = k.restore(&backup, None).unwrap_err();
    assert!(
        matches!(&err, AaError::RestoreIncomplete { section: "base layer", report, .. } if report.written.is_empty()),
        "{err:?}"
    );
    assert!(err.needs_reopen(), "{err}");
}

#[test]
fn a_per_key_backup_without_its_colour_gets_the_neutral_one() {
    // Per-key colours and mode set with AJAZZ's tool: the colour reads back
    // as the quirk, and the mode as 0, which off would read too.
    let sim = Sim::ak980();
    {
        let mut s = sim.state();
        s.set_effect_by_another_tool([128, 0xFF, 0x69, 0xB4, 0xFF, 0, 0, 0, 0, 3, 3, 0, 0, 0, 0xAA, 0x55]);
        s.per_key = perkey::blank().to_vec();
        perkey::set(&mut s.per_key, ENTER_AA, [0xFF, 0, 0]);
    }
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    assert_eq!((backup.effect_color, backup.effect_mode), (None, None));
    assert!(!backup.effect_mode_known());
    // Reading the same now: left alone.
    assert!(k.restore(&backup, None).unwrap().written.is_empty());
    k.apply_effect(&red_breath()).unwrap();
    let r = k.restore(&backup, None).unwrap();
    assert_eq!(r.written, ["effect"]);
    // The backup's table lights a key: per-key mode, never off.
    assert!(r.effect_mode_guessed);
    let s = sim.state().effect;
    assert_eq!((s[..4].to_vec(), s[8], s[9]), (vec![128, 0xFF, 0xFF, 0xFF], 0, 3), "per-key mode, white, level 3");

    // A backup whose table lights nothing goes back as off (the same look).
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool([0, 0, 0, 0, 0xFF, 0, 0, 0, 0, 3, 3, 0, 0, 0, 0xAA, 0x55]);
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    k.apply_effect(&red_breath()).unwrap();
    let r = k.restore(&backup, None).unwrap();
    assert!(r.effect_mode_guessed);
    assert_eq!(sim.state().effect[0], 0);
}

#[test]
fn a_backup_keeps_the_mode_written_not_off() {
    // Per-key colours saved from Keymotion: the backup taken after says
    // 128, though the keyboard reads 0, and a restore writes 128 back.
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_per_key([(ENTER, [0xFF, 0, 0])], 4).unwrap();
    let backup = k.read_all().unwrap();
    assert_eq!((backup.effect.0[0], backup.effect_mode), (0, Some(128)));
    let json = serde_json::to_string(&backup).unwrap();
    assert!(json.contains("\"effect_mode\":128"), "{json}");
    k.apply_effect(&red_breath()).unwrap();
    let r = k.restore(&backup, None).unwrap();
    assert!(!r.effect_mode_guessed);
    assert_eq!(r.written, ["effect"]);
    assert_eq!(sim.state().effect[..4], [128, 0x00, 0x80, 0xFF][..], "per-key, the colour it had");
    assert_eq!(sim.state().effect[9], 4);
}

#[test]
fn a_restore_that_cannot_finish_writes_nothing() {
    // 100 one-step macros stored with AJAZZ's hub: inside the keyboard's
    // memory, so the gate takes them, but one more than this driver ever
    // writes (99, as the WebHID driver).
    let sim = Sim::ak980();
    {
        let mut s = sim.state();
        for i in 0..100usize {
            let at = 400 + i * 8;
            s.macros[i * 4..i * 4 + 4].copy_from_slice(&(at as u32).to_le_bytes());
            s.macros[at..at + 8].copy_from_slice(&[2, 0, 0, 0, 10, 0, 4, 0xB0]);
        }
    }
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    assert_eq!(backup.macros().iter().filter(|m| !m.is_empty()).count(), 100);
    // Kept as they are, they are no reason to fail.
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    assert_eq!(k.restore(&backup, None).unwrap().written, ["base layer"]);
    // Changed, they cannot be written back: nothing is, the keymap included.
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    k.write_macros(&[]).unwrap();
    let n = writes(&sim);
    let err = k.restore(&backup, None).unwrap_err();
    assert!(matches!(err, AaError::TooLarge { macros: 100, .. }), "{err}");
    assert_eq!(writes(&sim), n, "planned before anything was sent");
}

#[test]
fn a_backup_without_its_colour_asks_for_it_before_writing() {
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(pink_static());
    let mut k = ready(&sim);
    let backup = k.snapshot().unwrap().clone();
    assert_eq!(backup.effect_color, None);
    k.apply_keymap(Layer::Base, [(A, KeyAction::key(0x05))]).unwrap();
    k.apply_effect(&red_breath()).unwrap();
    let n = writes(&sim);
    assert!(matches!(k.restore(&backup, None), Err(AaError::ColorUnknown)));
    assert_eq!(writes(&sim), n, "nothing written, the keymap included");
    let r = k.restore(&backup, Some(PINK)).unwrap();
    assert_eq!(r.written, ["base layer", "effect"]);
    assert_eq!(sim.state().effect, pink_static());
    // Mode and levels already match: the effect is left alone.
    assert!(k.restore(&backup, None).unwrap().written.is_empty());
}

#[test]
fn a_backup_from_another_keyboard_or_broken_is_refused() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let mut other = k.snapshot().unwrap().clone();
    other.info.0[14] = 0x2C;
    other.device = DeviceInfo::parse(&other.info.0).unwrap().into();
    assert!(matches!(k.restore(&other, None), Err(AaError::NotSane(_))));
    let mut broken = k.snapshot().unwrap().clone();
    broken.per_key.0.pop();
    assert!(matches!(k.restore(&broken, None), Err(AaError::NotSane(_))));
    assert_eq!(writes(&sim), 0);
}

#[test]
fn backups_are_json_with_hex_strings() {
    let sim = Sim::ak980();
    let snap = keyboard(&sim).read_all().unwrap();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(json.contains("\"info\":\"4000000c450c398001"), "{}", &json[..80]);
    assert!(json.contains("\"effect_color\":{\"color\":[0,128,255],\"color_mode\":0}"), "{json}");
    let back: AaSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
    assert!(serde_json::from_str::<AaSnapshot>(&json.replace("\"info\":\"40", "\"info\":\"4")).is_err());
    assert!(serde_json::from_str::<AaSnapshot>(&json.replace("\"info\":\"40", "\"info\":\"zz")).is_err());
    // A backup without the colour and mode fields (beta.1) reads as
    // "unknown".
    let old = json
        .replace(",\"effect_color\":{\"color\":[0,128,255],\"color_mode\":0}", "")
        .replace(",\"effect_mode\":null", "");
    assert!(!old.contains("effect_mode"));
    let old: AaSnapshot = serde_json::from_str(&old).unwrap();
    assert_eq!((old.effect_color, old.effect_mode), (None, None));
}

// ------------------------------------------------------------------ the simulator's own checks

#[test]
#[should_panic(expected = "guard refuses")]
fn the_simulator_catches_a_packet_past_the_guard() {
    let sim = Sim::ak980();
    let mut port = sim.command();
    let mut factory_reset = [0u8; 65];
    factory_reset[1..4].copy_from_slice(&[0xAA, 0x0F, 0xFF]);
    let _ = port.write(&factory_reset);
}

#[test]
#[should_panic(expected = "table guard refuses")]
fn the_simulator_catches_a_new_firmware_function() {
    let sim = Sim::ak980();
    let mut port = sim.command();
    let mut table = sim.state().keymap.clone();
    keymap::set_entry(&mut table, 49, [0x0D, 0, 0, 1]);
    for ch in frame::chunks(512, 0, true) {
        let w = frame::wire(&frame::request(cmd::SET_KEYMAP, &ch, 0, &table[ch.offset..]));
        let _ = port.write(&w);
    }
}

#[test]
fn the_simulator_ands_a_page_that_was_not_erased() {
    // AJAZZ's WebHID driver's 56-byte chunks never start one on 1024: the
    // second page is programmed over what it held. After a HID reset that
    // is 00, and 00 AND anything stays 00.
    let sim = Sim::ak980();
    sim.state().macros.fill(0);
    let mut port = sim.command();
    let image: Vec<u8> = (0..1200).map(|i| (i % 251) as u8 | 1).collect();
    for ch in frame::chunks(image.len(), 0, true) {
        let w = frame::wire(&frame::request(cmd::SET_MACROS, &ch, 0, &image[ch.offset..]));
        port.write(&w).unwrap();
    }
    let s = sim.state();
    assert_eq!(&s.macros[..1008], &image[..1008], "the first page was erased");
    assert!(s.macros[1024..1200].iter().all(|&b| b == 0), "the second was not");
    drop(s);
    // The same image cut on the page boundary lands whole.
    let sim = Sim::ak980();
    sim.state().macros.fill(0);
    let mut port = sim.command();
    for ch in frame::paged_chunks(image.len()) {
        let w = frame::wire(&frame::request(cmd::SET_MACROS, &ch, 0, &image[ch.offset..]));
        port.write(&w).unwrap();
    }
    assert_eq!(&sim.state().macros[..1200], &image[..]);
}

#[test]
#[should_panic(expected = "does not continue a table write")]
fn the_simulator_catches_a_patch_in_the_middle_of_a_page() {
    // What beta.1 did with macros: the table again after the blocks, then
    // the last block chunk again.
    let sim = Sim::ak980();
    let mut port = sim.command();
    let ch = frame::Chunk { offset: 0, addr: 400, len: 12, last: true };
    let w = frame::wire(&frame::request(cmd::SET_MACROS, &ch, 0, &[4, 0, 0, 0, 10, 0, 4, 0xB0, 0, 0, 4, 0x30]));
    let _ = port.write(&w);
}

#[test]
fn the_simulator_reads_like_firmware_769() {
    let sim = Sim::ak980();
    let mut s = sim.state();
    // The device information: a fixed flash block, four bytes filled in.
    s.battery = 77;
    let i = DeviceInfo::parse(&s.info_read()).unwrap();
    assert_eq!((i.battery_level, i.charge_status, i.tft_max_frames, i.version), (77, 2, 146, 0x0301));
    // Modes 0, 20, 21, 128 read 0; an unknown mode written is ignored.
    for (m, read) in [(0u8, 0u8), (1, 1), (19, 19), (20, 0), (21, 0), (128, 0)] {
        s.set_effect_by_another_tool([m, 1, 2, 3, 0xFF, 0, 0, 0, 0, 5, 3, 0, 0, 0, 0xAA, 0x55]);
        let e = s.effect_read();
        assert_eq!((e[0], &e[1..4], e[8]), (read, &[0xFF, 0xFF, 0xFF][..], 1), "{m}");
    }
    s.set_effect_by_another_tool([99, 1, 2, 3, 0xFF, 0, 0, 0, 0, 5, 3, 0, 0, 0, 0xAA, 0x55]);
    assert_eq!(s.effect[0], 128, "the last valid one stays");
}

#[test]
#[should_panic(expected = "before the previous answer was read")]
fn the_simulator_catches_a_screen_packet_sent_too_early() {
    let sim = Sim::ak980();
    let mut port = sim.display();
    let blob = kb_protocol::aa::screen::blob(&ScreenAnimation::still(test_pattern()));
    for i in 0..2 {
        let mut w = vec![0u8; screen::WIRE_LEN];
        w[1..].copy_from_slice(&screen::packet(&blob, i, 16));
        let _ = port.write(&w);
    }
}
