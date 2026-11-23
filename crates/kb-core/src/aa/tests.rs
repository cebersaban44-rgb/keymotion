//! The driver against the simulated keyboard.

use super::sim::{Faults, Sim, SimPort};
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
const SPACE_AA: u8 = 83;
const ENTER_AA: u8 = 76;
const FN_AA: u8 = 85;

fn layout() -> AaLayout {
    AaLayout::from_model(Sim::model()).unwrap()
}

fn keyboard(sim: &Sim) -> AaKeyboard<SimPort> {
    AaKeyboard::new(sim.command(), layout(), Sim::usb())
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
    assert_eq!(snap.device.tft_max_frames, 140);
    assert_eq!((snap.keymap.0.len(), snap.default_fn.0.len(), snap.per_key.0.len()), (512, 512, 512));
    assert_eq!(snap.effect.0[..4], [1, 0xFF, 0xFF, 0xFF]);
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
fn a_keyboard_waiting_for_an_update_gets_nothing_more() {
    let sim = Sim::ak980();
    sim.state().info[32] = 1;
    let mut k = keyboard(&sim);
    assert!(matches!(k.read_all(), Err(AaError::UpdatePending)));
    assert!(k.info().unwrap().update_pending());
    assert!(matches!(k.read_effect(), Err(AaError::UpdatePending)));
    assert!(matches!(k.apply_effect(&EffectSettings::factory()), Err(AaError::UpdatePending)));
    assert!(matches!(k.set_clock(&clock_time()), Err(AaError::UpdatePending)));
    assert!(!k.is_sane());
    assert_eq!(sim.state().packets.len(), 1, "only the device information was asked for");
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
    assert!(matches!(k.realtime_frame([]), Err(AaError::NotSane(_))));
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
fn nothing_is_written_before_a_sane_read() {
    let sim = Sim::ak980();
    let mut k = keyboard(&sim);
    assert!(matches!(k.apply_effect(&EffectSettings::factory()), Err(AaError::NotSane(_))));
    assert!(matches!(k.write_per_key([]), Err(AaError::NotSane(_))));
    assert!(matches!(k.apply_keymap(Layer::Base, []), Err(AaError::NotSane(_))));
    assert!(matches!(k.write_macros(&[]), Err(AaError::NotSane(_))));
    assert!(matches!(k.set_clock(&clock_time()), Err(AaError::NotSane(_))));
    assert!(matches!(k.arm_live(), Err(AaError::NotSane(_))));
    assert!(sim.state().packets.is_empty());
}

#[test]
fn a_corrupted_read_closes_the_gate() {
    // Entry 0 of the per-key table no longer starts with 0.
    let sim = Sim::ak980().with_faults(Faults { corrupt: Some(cmd::PER_KEY), ..Faults::default() });
    let mut k = keyboard(&sim);
    let snap = k.read_all().unwrap();
    assert!(!k.is_sane());
    assert!(k.sanity_reasons().iter().any(|r| r.contains("per-key entry 0")), "{:?}", k.sanity_reasons());
    assert_eq!(snap.per_key.0[0], 0xFF, "the snapshot is returned anyway");
    let before = sim.state().packets.len();
    assert!(matches!(k.apply_effect(&EffectSettings::factory()), Err(AaError::NotSane(_))));
    assert_eq!(sim.state().packets.len(), before);
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
    fails(&info(32, 1), usb, "firmware status");
    fails(&|s| s.info.0[17] = 99, usb, "does not match");
    fails(&|s| s.keymap.0[4 * 20] = 0x04, usb, "unknown entry page 0x04");
    fails(&|s| s.default_fn.0[4 * 7] = 0x0E, usb, "factory Fn layer slot 7");
    fails(&|s| s.effect.0[0] = 30, usb, "effect mode");
    fails(&|s| s.effect.0[9] = 6, usb, "brightness 6");
    fails(&|s| s.effect.0[10] = 0, usb, "speed 0");
    fails(&|s| s.effect.0[11] = 4, usb, "direction 4");
    fails(&|s| s.per_key.0[40] = 0, usb, "per-key entry 10");
    fails(&|s| s.keymap.0.truncate(504), usb, "base layer read is 504 bytes");
    fails(&|s| s.macro_table.0[0..4].copy_from_slice(&100u32.to_le_bytes()), usb, "macro 0");
    fails(&|s| s.macro_table.0[8..12].copy_from_slice(&5000u32.to_le_bytes()), usb, "macro 2");
    // Off may hold levels outside 1–5; per-key mode and FUNC_V2 pages are known.
    let mut off = good.clone();
    off.effect.0[..1].copy_from_slice(&[0]);
    off.effect.0[9] = 0;
    off.effect.0[10] = 0;
    off.keymap.0[4 * 20] = 0x90;
    assert_eq!(check_sanity(&off, usb), Ok(()));
}

#[test]
fn live_colours_map_back_to_leds() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // Static red: every key shows the effect colour.
    sim.state().effect[1..4].copy_from_slice(&[0xFF, 0, 0]);
    let c = k.read_live_colors().unwrap();
    assert_eq!(c.len(), 97);
    assert!(c.iter().all(|(_, rgb)| *rgb == [0xFF, 0, 0]));
    // Per-key mode: Enter's entry (slot 76) shows on Enter's LED (85).
    {
        let mut s = sim.state();
        s.effect[0] = 128;
        perkey::set(&mut s.per_key, ENTER_AA, [1, 2, 3]);
    }
    let c = k.read_live_colors().unwrap();
    assert_eq!(c.iter().find(|(led, _)| *led == ENTER).unwrap().1, [1, 2, 3]);
    let lit: Vec<u8> = c.iter().filter(|(_, rgb)| *rgb != [0, 0, 0]).map(|(l, _)| *l).collect();
    assert_eq!(lit, [ENTER]);
    let reqs = sim.state().sent(cmd::LIVE_COLORS).len();
    assert_eq!(reqs, 20, "two reads of 10 chunks");
}

// ------------------------------------------------------------------ effect

fn red_breath() -> EffectSettings {
    EffectSettings { mode: 7, color: [0xFF, 0x20, 0x00], rainbow: false, brightness: 4, speed: 0, direction: 1 }
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
    let back = k.effect_settings().unwrap();
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
fn an_effect_the_keyboard_did_not_keep_is_an_error() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.ignore_writes = Some(cmd::SET_EFFECT);
    assert!(matches!(k.apply_effect(&red_breath()), Err(AaError::Readback { what: "effect" })));
}

#[test]
fn a_different_echo_is_an_error() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    sim.state().faults.garble_echo = true;
    assert!(matches!(k.apply_effect(&red_breath()), Err(AaError::Mismatch { cmd: 0x23 })));
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

// ------------------------------------------------------------------ per-key

#[test]
fn per_key_colours_land_on_their_aa_slots() {
    let sim = Sim::ak980();
    // Slots that are no key of the layout keep what they hold.
    perkey::set(&mut sim.state().per_key, 99, [9, 9, 9]);
    perkey::set(&mut sim.state().per_key, 13, [7, 7, 7]);
    let mut k = ready(&sim);
    k.write_per_key([(ENTER, [0xFF, 0, 0]), (ESC, [0, 0xFF, 0])]).unwrap();
    let s = sim.state();
    assert_eq!(&s.per_key[76 * 4..76 * 4 + 4], &[76, 0xFF, 0, 0], "Enter is slot 76 ...");
    assert_eq!(&s.per_key[85 * 4..85 * 4 + 4], &[85, 0, 0, 0], "... never 85 (Fn)");
    assert_eq!(&s.per_key[..4], &[0, 0, 0xFF, 0], "Esc is slot 0");
    assert_eq!(&s.per_key[99 * 4..99 * 4 + 4], &[99, 9, 9, 9]);
    assert_eq!(&s.per_key[13 * 4..13 * 4 + 4], &[13, 7, 7, 7]);
    assert_eq!(&s.per_key[510..], &[0xAA, 0x55], "slots 126–127 as read");
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
    assert_eq!(s.effect, [128, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 2, 3, 0, 0, 0, 0xAA, 0x55]);
    assert_eq!(perkey::get(&s.per_key, 34), [0, 0, 0xFF], "W is slot 34");
    // The effect went first, then the table.
    let order: Vec<u8> = s.packets.iter().map(|p| p[2]).filter(|c| [0x23, 0x24].contains(c)).collect();
    assert_eq!(order.first(), Some(&0x23));
    assert_eq!(order.last(), Some(&0x24));
    drop(s);
    // Already in per-key mode at that level: only the table is written.
    k.apply_per_key([(W, [0, 0xFF, 0])], 2).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_EFFECT).len(), 1);
    assert_eq!(perkey::get(&sim.state().per_key, 34), [0, 0xFF, 0]);
}

#[test]
fn unchanged_tables_are_not_written() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    // W's own factory entry, and every key black as the factory table has it.
    k.apply_keymap(Layer::Base, [(W, KeyAction::key(0x1A))]).unwrap();
    k.write_per_key([]).unwrap();
    let s = sim.state();
    assert!(s.sent(cmd::SET_KEYMAP).is_empty());
    assert!(s.sent(cmd::SET_PER_KEY).is_empty());
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
    assert_eq!(slot(&after, 0), [0, 0, 0, 0]);
    for s in (0..128u8).filter(|s| ![49, ENTER_AA, 0].contains(s)) {
        assert_eq!(slot(&after, s), slot(&before, s), "slot {s}");
    }
    assert_eq!(slot(&after, 13), [0x03, 0xE9, 0, 0], "the knob");
    assert_eq!(&after[510..], &[0xAA, 0x55]);
    // Read back as actions, by LED.
    let map = k.read_keymap(Layer::Base).unwrap();
    let of = |led| map.iter().find(|(l, _)| *l == led).unwrap().1;
    assert_eq!(of(A), Binding::Action(shortcut));
    assert_eq!(of(ENTER), Binding::Action(KeyAction::Disabled));
    assert_eq!(of(W), Binding::Action(KeyAction::key(0x1A)));
    assert_eq!(of(FN), Binding::Raw([0x02, 0x00, 0xAF, 0x00]), "the Fn key's own entry");
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
    let space = map.iter().find(|(l, _)| *l == 94).unwrap().1;
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
    // The table: 8 chunks, none with LAST; the block: one chunk with LAST.
    let writes = s.sent(cmd::SET_MACROS);
    assert_eq!(writes.len(), 9);
    assert!(writes[..8].iter().all(|w| w[7] == 0));
    assert_eq!(&writes[8][1..9], &[0xAA, 0x25, 12, 0x90, 0x01, 0, 1, 0]);
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
fn macro_capacity_edges() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let step = MacroStep::new(StepKind::KeyDown, 0x04, 10);
    k.write_macros(&[vec![step; 127]]).unwrap();
    let n = sim.state().packets.len();
    let too_big = k.write_macros(&[vec![step; 128]]).unwrap_err();
    assert!(matches!(too_big, AaError::TooLarge { macros: 1, entries: 128, bytes: 516, limit: 512 }), "{too_big}");
    assert!(too_big.to_string().contains("516 bytes"));
    assert!(matches!(k.write_macros(&vec![vec![step]; 65]), Err(AaError::TooLarge { macros: 65, .. })));
    assert_eq!(sim.state().packets.len(), n);
    // Clearing every macro.
    k.write_macros(&[]).unwrap();
    assert!(sim.state().macros[..400].iter().all(|&b| b == 0));
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
}

// ------------------------------------------------------------------ live lighting

#[test]
fn live_frames_need_an_armed_effect() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    assert!(matches!(k.realtime_frame([(ENTER, [1, 2, 3])]), Err(AaError::NotArmed)));
    assert!(sim.state().sent(cmd::REALTIME).is_empty());

    let previous = k.arm_live().unwrap().expect("static white was replaced");
    assert_eq!((previous.mode, previous.brightness, previous.speed), (1, 5, 3));
    assert_eq!(sim.state().effect[..1], [128], "per-key mode");
    assert_eq!(sim.state().effect[9..11], [5, 3], "levels kept");
    assert!(k.arm_live().unwrap().is_none(), "already armed: nothing replaced");

    k.realtime_frame([(ENTER, [1, 2, 3]), (0, [9, 9, 9])]).unwrap();
    let s = sim.state();
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

    k.disarm_live(previous).unwrap();
    let s = sim.state();
    assert_eq!(&s.effect[..4], &[1, 0xFF, 0xFF, 0xFF]);
    assert_eq!(s.effect[8..11], [1, 5, 3]);
    drop(s);
    assert!(matches!(k.realtime_frame([]), Err(AaError::NotArmed)));
}

#[test]
fn disarming_puts_back_the_colour_the_driver_wrote() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    k.apply_effect(&red_breath()).unwrap();
    let previous = k.arm_live().unwrap().unwrap();
    assert_eq!(previous.color, [0xFF, 0x20, 0x00], "not the FF FF FF the keyboard reads back");
    k.disarm_live(previous).unwrap();
    assert_eq!(&sim.state().effect[..4], &[7, 0xFF, 0x20, 0x00]);
}

// ------------------------------------------------------------------ screen

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
    assert_eq!((r.packets, r.packet_times.len()), (16, 16));
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
fn screen_limits() {
    let sim = Sim::ak980();
    sim.state().info[22] = 24; // the keyboard takes 24 frames
    let mut k = ready(&sim);
    let anim = ScreenAnimation::new(vec![(Frame::filled(0), 50); 25]).unwrap();
    let r = k.upload_screen(&mut sim.display(), &anim, |_, _| {}, &AtomicBool::new(false));
    assert!(matches!(r, Err(AaError::Invalid(CodecError::Range { max: 24, .. }))));
    // Cancelled before the first packet.
    let still = ScreenAnimation::still(test_pattern());
    let r = k.upload_screen(&mut sim.display(), &still, |_, _| {}, &AtomicBool::new(true));
    assert!(matches!(r, Err(AaError::Cancelled { done: 0, total: 16 })));
    assert!(sim.state().screen.is_empty());

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

    k.restore(&backup).unwrap();
    let s = sim.state();
    assert_eq!(s.keymap, original.keymap);
    assert_eq!(s.fn_keymap, original.fn_keymap);
    assert_eq!(s.per_key, original.per_key);
    assert_eq!(&s.macros[..400], &original.macros[..400]);
    assert_eq!(s.effect[..12], original.effect[..12]);
    assert_eq!(s.effect[14..], [0xAA, 0x55], "written the WebHID driver's way");
    assert!(s.sent(0x21).is_empty(), "options are never written");
}

#[test]
fn a_backup_never_moves_a_firmware_function() {
    let sim = Sim::ak980();
    let mut k = ready(&sim);
    let mut backup = k.snapshot().unwrap().clone();
    // Factory reset on A instead of Space.
    keymap::set_entry(&mut backup.fn_keymap.0, 49, [0x0D, 0, 0, 1]);
    let err = k.restore(&backup).unwrap_err();
    assert!(matches!(err, AaError::Blocked(Blocked::DangerousFunc { slot: 49, id: 1 })), "{err}");
    assert!(sim.state().sent(cmd::SET_FN_KEYMAP).is_empty());
    // Another keyboard's backup, or a broken one.
    let mut other = k.snapshot().unwrap().clone();
    other.info.0[14] = 0x2C;
    other.device = DeviceInfo::parse(&other.info.0).unwrap().into();
    assert!(matches!(k.restore(&other), Err(AaError::NotSane(_))));
    let mut broken = k.snapshot().unwrap().clone();
    broken.per_key.0.pop();
    assert!(matches!(k.restore(&broken), Err(AaError::NotSane(_))));
}

#[test]
fn backups_are_json_with_hex_strings() {
    let sim = Sim::ak980();
    let snap = keyboard(&sim).read_all().unwrap();
    let json = serde_json::to_string(&snap).unwrap();
    assert!(json.contains("\"info\":\"4000000c450c398001"), "{}", &json[..80]);
    let back: AaSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snap);
    assert!(serde_json::from_str::<AaSnapshot>(&json.replace("\"info\":\"40", "\"info\":\"4")).is_err());
    assert!(serde_json::from_str::<AaSnapshot>(&json.replace("\"info\":\"40", "\"info\":\"zz")).is_err());
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
