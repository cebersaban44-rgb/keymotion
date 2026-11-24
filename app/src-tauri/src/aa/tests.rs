//! The "AA" arm against the simulated keyboard (`kb_core::aa::sim`), which
//! also checks the driver: a packet the guard refuses, a keymap the table
//! guard refuses or command traffic during an upload make it panic.

use super::*;
use crate::device::MirrorExpect;
use crate::driver::{Display, Driver};
use kb_core::aa::sim::{Sim, FACTORY_EFFECT};
use kb_core::color::LedCalibration;
use kb_protocol::aa::{cmd, effect as aa_effect, perkey};
use kb_protocol::sonix::macros::StepKind;
use kb_protocol::sonix::screen;
use std::sync::atomic::Ordering;

/// A data folder of its own, removed at the end.
pub(crate) struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("keymotion-aa-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn target(dir: Option<&Path>) -> AaTarget {
    AaTarget { record_id: "ajazz-ak980-pro.aa".into(), usb: Sim::usb(), data_dir: dir.map(Path::to_path_buf) }
}

/// A driver on `sim`, connected (read whole).
fn driver(sim: &Sim, dir: Option<&Path>) -> AaDriver {
    let mut d = AaDriver::new(AaPort::new(sim.command()), Sim::model(), target(dir)).unwrap();
    d.connect(None);
    d
}

fn led(id: &str) -> u8 {
    Sim::model().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap().led
}

fn aa(id: &str) -> u8 {
    Sim::model().geometry.as_ref().unwrap().keys.iter().find(|k| k.id == id).unwrap().aa.unwrap()
}

/// Commands that change the keyboard (flash or RAM).
const WRITES: [u8; 7] = [
    cmd::SET_KEYMAP,
    cmd::SET_EFFECT,
    cmd::SET_PER_KEY,
    cmd::SET_MACROS,
    cmd::SET_FN_KEYMAP,
    cmd::REALTIME,
    cmd::CLOCK,
];

fn writes(sim: &Sim) -> usize {
    sim.state().packets.iter().filter(|p| WRITES.contains(&p[2])).count()
}

fn backups_in(dir: &Path) -> Vec<(String, backups::KeyboardBackup)> {
    backups::list(&backups::dir(dir))
}

fn effect(mode: u8, color: Rgb) -> EffectSettings {
    EffectSettings { mode, color, rainbow: false, brightness: 3, speed: 2, direction: 0 }
}

#[test]
fn a_sane_keyboard_is_backed_up_before_anything_is_written() {
    let dir = TempDir::new("backup");
    let sim = Sim::ak980();
    let d = driver(&sim, Some(&dir.0));
    assert_eq!(d.read_only(), None);
    assert_eq!(writes(&sim), 0, "reading only");
    let files = backups_in(&dir.0);
    assert_eq!(files.len(), 1);
    let (name, b) = &files[0];
    assert!(name.starts_with("aa-8039-0301-") && name.ends_with(".json"), "{name}");
    assert_eq!((b.reason, b.pid, b.release), (backups::Reason::FirstConnect, 0x8039, 0x0301));
    assert_eq!(b.record_id, "ajazz-ak980-pro.aa");
    assert_eq!(b.snapshot.keymap.0, sim.state().keymap, "the keyboard as it was");
    assert!(b.snapshot.effect_color.is_some(), "the factory colour reads back");
    assert_eq!(d.status().backup.as_deref(), Some(name.as_str()));

    // The next connect finds it: one backup per keyboard.
    drop(d);
    let mut d = driver(&sim, Some(&dir.0));
    assert_eq!(backups_in(&dir.0).len(), 1);
    assert_eq!(d.status().backup.as_deref(), Some(name.as_str()));
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();

    // Without a data folder there is no backup, so nothing is written.
    let sim = Sim::ak980();
    let mut d = driver(&sim, None);
    assert!(matches!(d.read_only(), Some(ReadOnly::NoBackup(_))), "{:?}", d.read_only());
    let e = d.apply_effect(&effect(2, [255, 0, 0])).unwrap_err();
    assert_eq!(
        e.to_string(),
        "no backup of the keyboard could be saved, so nothing is written: the app has no data folder"
    );
    assert!(!e.is_transport());
    assert_eq!(writes(&sim), 0);
}

#[test]
fn a_backup_takes_the_colour_the_app_knows() {
    // Another tool wrote the effect: its colour reads back white.
    let dir = TempDir::new("backup-colour");
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(FACTORY_EFFECT);
    let app = AaEffect::decode(&FACTORY_EFFECT).unwrap();
    let mut d = AaDriver::new(AaPort::new(sim.command()), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(Some(app));
    let (_, b) = &backups_in(&dir.0)[0];
    assert_eq!(b.snapshot.effect_color, Some(app.color_part().into()), "from the app's record");
    assert!(!d.status().effect_color_known, "the keyboard itself does not tell");

    // A record of another effect tells nothing.
    let dir = TempDir::new("backup-colour-other");
    let other = AaEffect { mode: 4, ..app };
    let mut d = AaDriver::new(AaPort::new(sim.command()), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(Some(other));
    assert_eq!(backups_in(&dir.0)[0].1.snapshot.effect_color, None);
}

#[test]
fn every_write_goes_through_the_arm() {
    let dir = TempDir::new("writes");
    let sim = Sim::ak980();
    let mut d = Driver::<kb_transport::hid::HidChannel>::Aa(Box::new(driver(&sim, Some(&dir.0))));
    assert_eq!(d.protocol(), kb_devices::Protocol::AjazzAa);

    // Effect: levels mapped to the keyboard's 1–5.
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();
    let e = sim.state().effect;
    assert_eq!((e[0], [e[1], e[2], e[3]], e[9], e[10]), (2, [255, 0, 0], aa_effect::LEVELS[3], aa_effect::LEVELS[2]));

    // Per-key colours: the keys by LED, translated; the others black.
    d.apply_per_key(vec![(led("esc"), [1, 2, 3]), (led("enter"), [4, 5, 6])], 4).unwrap();
    let s = sim.state().clone();
    assert_eq!(s.effect[0], aa_effect::MODE_PER_KEY);
    assert_eq!(perkey::get(&s.per_key, aa("esc")), [1, 2, 3]);
    assert_eq!(perkey::get(&s.per_key, aa("enter")), [4, 5, 6]);
    assert_eq!(perkey::get(&s.per_key, aa("w")), [0, 0, 0]);

    // The mirror: the colours the keys show, by LED.
    let shown = d.mirror_colors(&MirrorExpect::default(), LedCalibration::RAW, &[]).unwrap();
    assert_eq!(shown.len(), 97);
    assert!(shown.contains(&(led("esc"), [1, 2, 3])) && shown.contains(&(led("enter"), [4, 5, 6])));

    // Macros and the clock.
    let steps = vec![MacroStep::new(StepKind::KeyDown, 4, 20), MacroStep::new(StepKind::KeyUp, 4, 20)];
    d.write_macros(&[steps]).unwrap();
    assert!(!sim.state().sent(cmd::SET_MACROS).is_empty());
    let t = ClockTime { year: 2026, month: 11, day: 23, hour: 21, minute: 5, second: 0, weekday: 1 };
    d.set_clock(&t).unwrap();
    assert!(sim.state().clock.is_some());

    // The device information: battery on the cable, the screen's frames.
    let report = d.device_info().unwrap().expect("a report");
    assert_eq!(report.battery, Some(100));
    assert_eq!(report.charge, None, "charge status 2 means nothing sure");
    assert_eq!(report.screen_max_frames, Some(146), "firmware 769's own frame limit");
    assert!(!report.update_pending);

    // The screen, over its own channel, with progress.
    let anim = ScreenAnimation::still(screen::test_pattern());
    let mut display = Display::<kb_transport::hid::HidChannel>::Aa(AaPort::new(sim.display()));
    let mut seen = Vec::new();
    let done = d.upload_screen(&mut display, &anim, |n, total| seen.push((n, total)), &AtomicBool::new(false)).unwrap();
    assert_eq!(done.pages, sim.state().screen.len());
    assert_eq!(seen.last(), Some(&(done.pages, done.pages)));
}

#[test]
fn only_the_keys_that_differ_are_written() {
    let dir = TempDir::new("keymap");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let before = sim.state().keymap.clone();

    // The keyboard's own layer: nothing to write.
    d.apply_keymap(Layer::Base, &LayerMap::new()).unwrap();
    assert!(sim.state().sent(cmd::SET_KEYMAP).is_empty());

    // One key: only its slot changes, every other slot stays as read.
    d.apply_keymap(Layer::Base, &LayerMap::from([(led("a"), KeyAction::key(0x05))])).unwrap();
    let after = sim.state().keymap.clone();
    let changed: Vec<u8> =
        (0..128u8).filter(|&s| aa_keymap::entry(&before, s) != aa_keymap::entry(&after, s)).collect();
    assert_eq!(changed, [aa("a")]);
    assert_eq!(aa_keymap::entry(&after, aa("a")), [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);

    // The same map again: nothing written.
    let writes_before = sim.state().sent(cmd::SET_KEYMAP).len();
    d.apply_keymap(Layer::Base, &LayerMap::from([(led("a"), KeyAction::key(0x05))])).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_KEYMAP).len(), writes_before);

    // Back to its default: the key's own usage again.
    d.apply_keymap(Layer::Base, &LayerMap::new()).unwrap();
    assert_eq!(sim.state().keymap, before);

    // The Fn layer keeps its firmware functions (a factory reset on
    // Fn+Space): a map that names none of them sends nothing, where a
    // whole layer of defaults would be refused.
    d.apply_keymap(Layer::Fn, &LayerMap::new()).unwrap();
    assert!(sim.state().sent(cmd::SET_FN_KEYMAP).is_empty());
    let fn_before = sim.state().fn_keymap.clone();
    d.apply_keymap(Layer::Fn, &LayerMap::from([(led("q"), KeyAction::Consumer(0xE9))])).unwrap();
    let fn_after = sim.state().fn_keymap.clone();
    let changed: Vec<u8> =
        (0..128u8).filter(|&s| aa_keymap::entry(&fn_before, s) != aa_keymap::entry(&fn_after, s)).collect();
    assert_eq!(changed, [aa("q")]);
    assert!(aa_keymap::is_func(aa_keymap::entry(&fn_after, aa("space"))), "the factory reset stays");

    // What the keyboard cannot take is refused with its reason.
    let e = d.apply_keymap(
        Layer::Base,
        &LayerMap::from([(
            led("b"),
            KeyAction::Macro { slot: 1, play: kb_protocol::sonix::keymap::MacroPlay::WhileHeld },
        )]),
    );
    assert_eq!(
        e.unwrap_err().to_string(),
        "not supported on this keyboard: a macro cannot repeat while the key is held"
    );
}

/// Each way a keyboard does not look as expected, made in the simulator.
fn misbehaving() -> Vec<(&'static str, Sim)> {
    let insane = Sim::ak980();
    insane.state().effect[0] = 99;
    let silent = Sim::ak980();
    silent.state().faults.silent_info = true;
    vec![("insane", insane), ("silent", silent)]
}

#[test]
fn a_keyboard_that_does_not_look_right_is_read_only() {
    for (name, sim) in misbehaving() {
        let dir = TempDir::new(&format!("read-only-{}", name.replace(' ', "-")));
        let mut d = driver(&sim, Some(&dir.0));
        let ro = d.read_only().unwrap_or_else(|| panic!("{name}: writable"));
        assert!(matches!(&ro, ReadOnly::NotSane(why) if !why.is_empty()), "{name}: {ro:?}");
        let shown = d.status().read_only.unwrap();
        match (&ro, &shown) {
            (ReadOnly::NotSane(why), ReadOnlyReason::NotSane { reasons, read_failed }) => {
                assert_eq!(reasons, &crate::diagnostics::summarize_reasons(why), "{name}");
                assert_eq!(*read_failed, name == "silent", "{name}: a read that did not finish, not a failed check");
            }
            _ => assert_eq!(shown, (&ro).into(), "{name}"),
        }
        let refusals = [
            d.apply_effect(&effect(2, [255, 0, 0])).unwrap_err(),
            d.apply_per_key(vec![], 5).unwrap_err(),
            d.apply_keymap(Layer::Base, &LayerMap::from([(led("a"), KeyAction::key(5))])).unwrap_err(),
            d.write_macros(&[]).unwrap_err(),
            d.set_clock(&ClockTime { year: 2026, month: 1, day: 1, hour: 0, minute: 0, second: 0, weekday: 4 })
                .unwrap_err(),
        ];
        for e in refusals {
            assert_eq!(e.to_string(), d.read_only().unwrap().to_string(), "{name}");
        }
        assert_eq!(writes(&sim), 0, "{name}: nothing written");
        assert!(backups_in(&dir.0).is_empty(), "{name}: no backup of what does not look right");
    }

    // What firmware 769 really holds is no reason: a per-key table of all
    // 00 (issue #154, after AJAZZ's updater) or all FF, a 1 in byte 32.
    for (name, prepare) in [
        ("zeros", (|s: &mut kb_core::aa::sim::SimState| s.per_key.fill(0)) as fn(&mut kb_core::aa::sim::SimState)),
        ("erased", |s| s.erased_per_key()),
        ("byte 32", |s| s.info[32] = 1),
    ] {
        let sim = Sim::ak980();
        prepare(&mut sim.state());
        let dir = TempDir::new(&format!("writable-{}", name.replace(' ', "-")));
        let mut d = driver(&sim, Some(&dir.0));
        assert_eq!(d.read_only(), None, "{name}");
        assert!(!d.device_info().unwrap().update_pending, "{name}");
        d.apply_per_key(vec![(led("esc"), [1, 2, 3])], 5).unwrap();
        assert_eq!(perkey::shown(&sim.state().per_key, aa("esc")), [1, 2, 3], "{name}");
    }
}

#[test]
fn a_keyboard_that_answers_again_is_read_again_before_a_write() {
    let dir = TempDir::new("silent");
    let sim = Sim::ak980();
    sim.state().faults.silent_info = true;
    let mut d = driver(&sim, Some(&dir.0));
    assert!(matches!(d.read_only(), Some(ReadOnly::NotSane(why)) if why[0].contains("0x10")));
    // In the background, at most every half minute.
    assert!(d.next_due().is_some_and(|t| t > Instant::now() + Duration::from_secs(20)));
    sim.state().faults.silent_info = false;
    // A write reads it first.
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();
    assert_eq!(d.read_only(), None);
    assert_eq!(backups_in(&dir.0).len(), 1);
    assert_eq!(sim.state().effect[0], 2);
}

/// A wheel turn and a 2 ms pause, as AJAZZ's tool writes them: no app
/// step holds either.
fn wheel() -> Vec<AaMacroEntry> {
    use kb_protocol::aa::macros::flag;
    vec![
        AaMacroEntry { delay_ms: 2, code: 0xFF, flag: flag::MOUSE_DOWN },
        AaMacroEntry { delay_ms: 0, code: 0xFF, flag: flag::MOUSE_UP },
    ]
}

/// The keyboard's macro memory holding `slots`, as another tool left it.
fn macros_written_elsewhere(sim: &Sim, slots: &[Vec<AaMacroEntry>]) {
    let img = aa_macros::encode(slots, aa_macros::CAPACITY).unwrap();
    let mut st = sim.state();
    st.macros.fill(0);
    st.macros[..img.table.len()].copy_from_slice(&img.table);
    let at = usize::from(aa_macros::BLOCKS_AT);
    st.macros[at..at + img.blocks.len()].copy_from_slice(&img.blocks);
}

#[test]
fn keys_and_macros_set_elsewhere_are_shown_and_kept() {
    let dir = TempDir::new("elsewhere");
    let sim = Sim::ak980();
    // Set in AJAZZ's tool: B runs a firmware function, D types "b", macro
    // slot 1 turns the wheel, slot 2 types "a".
    let func = [aa_keymap::page::FUNC, 0, 0, 2];
    {
        let mut st = sim.state();
        aa_keymap::set_entry(&mut st.keymap, aa("b"), func);
        aa_keymap::set_entry(&mut st.keymap, aa("d"), [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);
    }
    let typed = vec![MacroStep::new(StepKind::KeyDown, 4, 20), MacroStep::new(StepKind::KeyUp, 4, 20)];
    let typed_entries = aa_macros::from_steps(&typed).unwrap();
    macros_written_elsewhere(&sim, &[Vec::new(), wheel(), typed_entries.clone()]);
    let mut d = driver(&sim, Some(&dir.0));

    // What the Key mapping and Macros pages start from.
    let c = d.contents().expect("read and sane");
    assert_eq!(c.base.actions, LayerMap::from([(led("d"), KeyAction::key(0x05))]));
    assert_eq!(c.base.fixed, [led("b")]);
    assert!(c.fn_layer.actions.is_empty() && c.fn_layer.fixed.is_empty(), "the factory Fn layer");
    assert_eq!(c.macros[..3], [MacroContents::Empty, MacroContents::Fixed(2), MacroContents::Steps(typed.clone())]);
    assert!(c.macros[3..].iter().all(|m| *m == MacroContents::Empty));

    // A save of the page as shown (D kept, A added): B's firmware function
    // stays; only A's slot changes.
    let before = sim.state().keymap.clone();
    let map = LayerMap::from([(led("d"), KeyAction::key(0x05)), (led("a"), KeyAction::key(0x06))]);
    d.apply_keymap(Layer::Base, &map).unwrap();
    let after = sim.state().keymap.clone();
    let changed: Vec<u8> =
        (0..128u8).filter(|&s| aa_keymap::entry(&before, s) != aa_keymap::entry(&after, s)).collect();
    assert_eq!(changed, [aa("a")]);
    assert_eq!(aa_keymap::entry(&after, aa("b")), func);
    // Even a map that names no key at all leaves it.
    d.apply_keymap(Layer::Base, &LayerMap::new()).unwrap();
    assert_eq!(aa_keymap::entry(&sim.state().keymap, aa("b")), func);
    assert_eq!(d.contents().unwrap().base.fixed, [led("b")]);

    // A macro save that leaves slot 1 empty keeps the wheel, byte for byte;
    // the others are as the app asked.
    let mine = vec![MacroStep::new(StepKind::KeyDown, 5, 20), MacroStep::new(StepKind::KeyUp, 5, 20)];
    d.write_macros(std::slice::from_ref(&mine)).unwrap();
    let back = d.keyboard_mut().read_macros().unwrap();
    assert_eq!(back[0], aa_macros::from_steps(&mine).unwrap());
    assert_eq!(back[1], wheel());
    assert!(back[2].is_empty(), "a macro the app shows goes when the app leaves it out");
    let c = d.contents().unwrap();
    assert_eq!(c.macros[1], MacroContents::Fixed(2));
    let before = c.fingerprint;
    // Not even a save that fills its slot replaces it: refused, nothing
    // written.
    let sent = sim.state().sent(cmd::SET_MACROS).len();
    let e = d.write_macros(&[Vec::new(), mine.clone()]).unwrap_err();
    assert_eq!(e.to_string(), macro_elsewhere(1));
    assert_eq!(sim.state().sent(cmd::SET_MACROS).len(), sent);
    assert_eq!(d.keyboard_mut().read_macros().unwrap()[1], wheel());
    // Another slot is the app's.
    d.write_macros(&[mine.clone(), Vec::new(), mine.clone()]).unwrap();
    assert_eq!(d.keyboard_mut().read_macros().unwrap()[2], aa_macros::from_steps(&mine).unwrap());
    assert_ne!(d.contents().unwrap().fingerprint, before, "a change shows in the fingerprint");
}

#[test]
fn a_save_made_against_keys_or_macros_changed_elsewhere_is_refused() {
    let dir = TempDir::new("changed-elsewhere");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let (keys, macros) = {
        let c = d.contents().unwrap();
        (c.keys_source, c.macros_source)
    };
    // The page's own source: saved.
    let a = LayerMap::from([(led("a"), KeyAction::key(0x05))]);
    d.apply_keymap_from(Layer::Base, &a, Some(&keys)).unwrap();
    let keys = d.contents().unwrap().keys_source;
    // Meanwhile AJAZZ's web tool sets B; the page still shows the old keys.
    aa_keymap::set_entry(&mut sim.state().keymap, aa("b"), [aa_keymap::page::KEYBOARD, 0, 0x06, 0]);
    let before = sim.state().keymap.clone();
    let e = d.apply_keymap_from(Layer::Base, &LayerMap::from([(led("c"), KeyAction::key(0x07))]), Some(&keys));
    assert_eq!(e.unwrap_err().to_string(), ERR_KEYS_CHANGED);
    assert_eq!(sim.state().keymap, before, "nothing written: B stays as AJAZZ's tool set it");
    // The fresh read went into the snapshot: the page can show B now.
    let c = d.contents().unwrap();
    assert_eq!(c.base.actions.get(&led("b")), Some(&KeyAction::key(0x06)));
    assert_ne!(c.keys_source, keys);
    // Against the new source the save goes.
    let both = LayerMap::from([
        (led("a"), KeyAction::key(0x05)),
        (led("b"), KeyAction::key(0x06)),
        (led("c"), KeyAction::key(0x07)),
    ]);
    d.apply_keymap_from(Layer::Base, &both, Some(&c.keys_source)).unwrap();
    assert_eq!(aa_keymap::entry(&sim.state().keymap, aa("c")), [aa_keymap::page::KEYBOARD, 0, 0x07, 0]);

    // The same for macros.
    let mine = vec![MacroStep::new(StepKind::KeyDown, 5, 20), MacroStep::new(StepKind::KeyUp, 5, 20)];
    macros_written_elsewhere(&sim, &[aa_macros::from_steps(&mine).unwrap()]);
    let e = d.write_macros_from(&[], Some(&macros)).unwrap_err();
    assert_eq!(e.to_string(), ERR_MACROS_CHANGED);
    assert_eq!(d.keyboard_mut().read_macros().unwrap()[0], aa_macros::from_steps(&mine).unwrap(), "not deleted");
    let c = d.contents().unwrap();
    assert_eq!(c.macros[0], MacroContents::Steps(mine.clone()));
    d.write_macros_from(&[mine.clone(), mine.clone()], Some(&c.macros_source)).unwrap();

    // A change made elsewhere on the Fn layer refuses a save of the base
    // layer too, before anything is written (a page saves both, base first).
    let keys = d.contents().unwrap().keys_source;
    aa_keymap::set_entry(&mut sim.state().fn_keymap, aa("q"), [aa_keymap::page::KEYBOARD, 0, 0x08, 0]);
    let base_before = sim.state().keymap.clone();
    let e = d.apply_keymap_from(Layer::Base, &LayerMap::from([(led("e"), KeyAction::key(0x09))]), Some(&keys));
    assert_eq!(e.unwrap_err().to_string(), ERR_KEYS_CHANGED);
    assert_eq!(sim.state().keymap, base_before, "nothing written, as the message says");
}

#[test]
fn the_keyboards_own_fn_functions_are_kept() {
    let dir = TempDir::new("builtin");
    let sim = Sim::ak980();
    // A firmware that keeps zeros for a factory entry (N5), and a key whose
    // factory firmware function was replaced in another program.
    let default_fn = sim.state().default_fn.clone();
    let func_keys: Vec<(u8, u8)> = Sim::model()
        .geometry
        .as_ref()
        .unwrap()
        .keys
        .iter()
        .filter(|k| k.id != "space" && k.id != "fn")
        .filter_map(|k| Some((k.led, k.aa?)))
        .filter(|&(_, slot)| aa_keymap::is_func(aa_keymap::entry(&default_fn, slot)))
        .collect();
    assert!(func_keys.len() >= 2, "the simulated Fn layer has firmware functions: {func_keys:?}");
    let ((zeroed_led, zeroed), (moved_led, moved)) = (func_keys[0], func_keys[1]);
    {
        let mut st = sim.state();
        aa_keymap::set_entry(&mut st.fn_keymap, zeroed, [0; 4]);
        aa_keymap::set_entry(&mut st.fn_keymap, moved, [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);
    }
    let mut d = driver(&sim, Some(&dir.0));
    let c = d.contents().unwrap();
    assert!(c.fn_layer.builtin.contains(&led("space")), "the factory reset is the keyboard's own function");
    assert!(c.fn_layer.builtin.contains(&zeroed_led), "zeros are the factory function");
    assert_eq!(c.fn_layer.actions.get(&moved_led), Some(&KeyAction::key(0x05)));
    assert_eq!(c.fn_layer.pinned, [moved_led], "its default never comes back");
    assert!(c.fn_layer.fixed.is_empty());
    // A map that names the factory reset, and one that names nothing (the
    // Default profile): neither writes over a firmware function, neither
    // fails on the key whose factory function can't come back.
    let before = sim.state().fn_keymap.clone();
    let kept = d.apply_keymap_from(Layer::Fn, &LayerMap::from([(led("space"), KeyAction::key(0x04))]), None).unwrap();
    assert!(kept >= 1);
    assert_eq!(sim.state().fn_keymap, before);
    d.apply_keymap_from(Layer::Fn, &LayerMap::new(), None).unwrap();
    assert_eq!(sim.state().fn_keymap, before, "nothing written");
    assert!(sim.state().sent(cmd::SET_FN_KEYMAP).is_empty());
}

#[test]
fn nothing_is_taken_from_a_keyboard_that_does_not_look_right() {
    for (name, sim) in misbehaving() {
        let d = driver(&sim, None);
        assert!(d.contents().is_none(), "{name}");
    }
}

#[test]
fn restore_writes_the_backup_back_and_says_what_it_did() {
    let dir = TempDir::new("restore");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let (_, first) = backups_in(&dir.0).remove(0);
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();
    d.apply_keymap(Layer::Base, &LayerMap::from([(led("a"), KeyAction::key(0x05))])).unwrap();
    d.write_macros(&[vec![MacroStep::new(StepKind::KeyDown, 4, 20), MacroStep::new(StepKind::KeyUp, 4, 20)]]).unwrap();

    let mut d = Driver::<kb_transport::hid::HidChannel>::Aa(Box::new(d));
    let report = d.restore(&first.snapshot, None, None).unwrap();
    for section in ["base layer", "effect", "macros"] {
        assert!(report.written.contains(&section), "{section}: {:?}", report.written);
    }
    assert!(!report.written.contains(&"Fn layer"), "unchanged sections are not written");
    assert_eq!(sim.state().keymap, first.snapshot.keymap.0);
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    let result = crate::dto::KeyboardRestoreResult::from(report);
    assert!(result.written.iter().any(|s| s == "base layer"));

    // The state before was backed up first.
    let all = backups_in(&dir.0);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].1.reason, backups::Reason::BeforeRestore);
    assert_eq!(aa_keymap::entry(&all[1].1.snapshot.keymap.0, aa("a")), [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);
}

#[test]
fn a_restore_without_the_effects_colour_needs_one() {
    let dir = TempDir::new("restore-colour");
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(FACTORY_EFFECT);
    let mut d = driver(&sim, Some(&dir.0));
    let (_, first) = backups_in(&dir.0).remove(0);
    assert_eq!(first.snapshot.effect_color, None);
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();
    let writes_before = writes(&sim);
    let e = d.restore(&first.snapshot, None, None).unwrap_err();
    assert!(e.to_string().contains("does not report its effect colour"), "{e}");
    assert_eq!(writes(&sim), writes_before, "refused before anything was written");
    assert_eq!(backups_in(&dir.0).len(), 1, "no backup kept for a restore that did not happen");
    // With the colour given, it goes.
    let color = EffectColor { color: [0x00, 0x80, 0xFF], color_mode: 0 };
    let report = d.restore(&first.snapshot, Some(color), None).unwrap();
    assert!(report.written.contains(&"effect"));
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
}

#[test]
fn a_cut_upload_keeps_the_command_channel_quiet_for_a_while() {
    let dir = TempDir::new("cut");
    let sim = Sim::ak980();
    sim.state().upload_quiet = Duration::from_millis(100);
    let mut d = driver(&sim, Some(&dir.0));
    d.keyboard_mut().set_upload_cooldown(Duration::from_millis(250));
    let anim = ScreenAnimation::still(screen::test_pattern());
    let cancel = AtomicBool::new(false);
    let mut display = AaPort::new(sim.display());
    let e = d
        .upload_screen(
            &mut display,
            &anim,
            |done, _| {
                if done == 2 {
                    cancel.store(true, Ordering::SeqCst);
                }
            },
            &cancel,
        )
        .unwrap_err();
    assert!(e.to_string().starts_with("screen upload cancelled after 2 of"), "{e}");
    assert!(!e.is_transport(), "the command channel stays");
    // Nothing reaches the keyboard meanwhile, and that is no warning.
    let sent = sim.state().packets.len();
    let e = d.mirror_colors().unwrap_err();
    assert!(e.is_noise(), "{e}");
    assert!(e.to_string().starts_with("a screen upload stopped part-way; no commands for another"), "{e}");
    assert!(d.apply_effect(&effect(2, [1, 2, 3])).is_err());
    assert_eq!(sim.state().packets.len(), sent);
    assert!(d.in_quiet_time(), "refusals now are on purpose (no live-frame failure)");
    // Then the device information first, and on.
    std::thread::sleep(Duration::from_millis(300));
    assert!(!d.in_quiet_time());
    d.device_info().unwrap();
    assert_eq!(sim.state().packets[sent][2], cmd::DEVICE_INFO);
}

/// A screen channel whose writes fail like a stalled one.
struct Broken;

impl OutputPort for Broken {
    fn write(&mut self, _: &[u8]) -> kb_core::aa::Result<usize> {
        Err(AaError::Transport(kb_transport::TransportError::Refused("unplugged")))
    }
    fn read(&mut self, _: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

#[test]
fn a_broken_screen_channel_leaves_the_command_channel_open() {
    let dir = TempDir::new("broken");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let anim = ScreenAnimation::still(screen::test_pattern());
    let e = d.upload_screen(&mut AaPort::new(Broken), &anim, |_, _| {}, &AtomicBool::new(false)).unwrap_err();
    assert!(matches!(e, Error::AaScreen(_)), "{e:?}");
    assert!(!e.is_transport(), "the screen channel goes, the command channel and its quiet time stay");
}

/// A port that slips a report nobody asked for in before every reply, as
/// another program on the same channel would: the reply to its own read of
/// the factory Fn layer.
struct Chatty(kb_core::aa::sim::SimPort, bool);

impl OutputPort for Chatty {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        self.1 = true;
        self.0.write(report)
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        if std::mem::take(&mut self.1) {
            let mut r = vec![0u8; 64];
            r[..5].copy_from_slice(&[0x55, cmd::DEFAULT_FN, 0x38, 0x38, 0x01]);
            return Ok(Some(r));
        }
        self.0.read(timeout_ms)
    }
}

#[test]
fn replies_nobody_asked_for_raise_the_conflict_hint() {
    let mut w = ConflictWatch::default();
    let t = Instant::now();
    assert!(!w.note(t, 0));
    assert!(!w.note(t + Duration::from_secs(1), CONFLICT_THRESHOLD - 1) && !w.suspected);
    assert!(w.note(t + Duration::from_secs(2), CONFLICT_THRESHOLD) && w.suspected, "changed");
    // A quiet minute later the hint goes.
    assert!(w.note(t + CONFLICT_WINDOW * 2, CONFLICT_THRESHOLD) && !w.suspected);
    // Spread thinly over a long time: no hint.
    let mut w = ConflictWatch::default();
    for i in 0..30u64 {
        w.note(t + CONFLICT_WINDOW * i as u32 / 2, i);
        assert!(!w.suspected, "{i}");
    }

    // On a channel another program talks on: the reads count them.
    let dir = TempDir::new("chatty");
    let sim = Sim::ak980();
    let mut d = AaDriver::new(AaPort::new(Chatty(sim.command(), false)), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(None);
    assert!(d.status().other_program_suspected);
    assert_eq!(d.read_only(), None, "a hint, nothing is blocked");
    assert!(d.diagnostics().iter().any(|l| l.contains("another program suspected: true")));
}

#[test]
fn the_device_information_reports_what_is_known() {
    let mut raw = [0u8; kb_protocol::aa::info::LEN];
    raw[..kb_protocol::aa::info::FIRMWARE_769_CABLE.len()].copy_from_slice(&kb_protocol::aa::info::FIRMWARE_769_CABLE);
    let i = DeviceInfo::parse(&raw).unwrap();
    let r = device_report(&i);
    assert_eq!((r.battery, r.charge, r.screen_max_frames, r.update_pending), (Some(100), None, None, false));
    let charging = DeviceInfo { charge_status: 1, battery_level: 40, tft_max_frames: 120, ..i };
    let r = device_report(&charging);
    assert_eq!((r.battery, r.charge, r.screen_max_frames), (Some(40), Some(ChargeState::Charging), Some(120)));
    assert_eq!(device_report(&DeviceInfo { battery_level: 0xFF, ..i }).battery, None);
    // No such field on firmware 769: never "waiting for an update".
    assert!(!device_report(&DeviceInfo { firmware_status: 1, ..i }).update_pending);
}

#[test]
fn backups_keep_the_first_and_the_newest() {
    let dir = TempDir::new("prune");
    let sim = Sim::ak980();
    let d = driver(&sim, Some(&dir.0));
    let first = backups_in(&dir.0)[0].0.clone();
    let snap = d.kb.snapshot().unwrap().clone();
    let t = target(Some(&dir.0));
    let mut saved = Vec::new();
    for _ in 0..7 {
        let b = backups::KeyboardBackup::new(&t, backups::Reason::BeforeRestore, snap.clone());
        saved.push(backups::save(&backups::dir(&dir.0), &b).unwrap());
        backups::prune(&backups::dir(&dir.0), t.usb, &[]);
    }
    let names: Vec<String> = backups_in(&dir.0).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names.len(), backups::KEPT);
    assert_eq!(names[0], first, "the one from before the first write stays");
    assert_eq!(names[1..], saved[saved.len() - (backups::KEPT - 1)..], "then the newest");
    // Another keyboard's backups are its own.
    let other = AaTarget { usb: UsbIdentity { pid: 0x8009, release: 0x0301 }, ..t };
    let b = backups::KeyboardBackup::new(&other, backups::Reason::FirstConnect, snap);
    backups::save(&backups::dir(&dir.0), &b).unwrap();
    backups::prune(&backups::dir(&dir.0), other.usb, &[]);
    assert_eq!(backups_in(&dir.0).len(), backups::KEPT + 1);
    // Only backup names are opened.
    for bad in ["../keymap.json", "aa-8039-0301-x.json", "aa-8039-0301-1.json/..", "keymap.json"] {
        assert!(backups::load(&backups::dir(&dir.0), bad).is_err(), "{bad}");
    }
}

#[test]
fn the_diagnostics_hold_no_keymap_macro_or_per_key_content() {
    let dir = TempDir::new("diagnostics");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.apply_per_key(vec![(led("esc"), [0x12, 0x34, 0x56])], 5).unwrap();
    let text = d.diagnostics().join("\n");
    for part in [
        "ajazz-ak980-pro.aa on USB 8039, firmware 3.01",
        "device information (0x10, 48 bytes, full",
        "per-key table (0x14, 512 bytes, full",
        "writes: allowed",
        "backup: aa-8039-0301-",
    ] {
        assert!(text.contains(part), "{part}:\n{text}");
    }
    assert!(!text.contains("12 34 56") && !text.contains("123456"), "{text}");
    assert!(!text.contains("failed a check") && !text.contains("failed read"), "{text}");
}

/// A read that fails the check: the report says why in a few lines, shows
/// the effect whole, and names the read kept for the developer, which is
/// no backup.
#[test]
fn a_failed_read_is_kept_and_named_in_the_report() {
    let dir = TempDir::new("failed-read");
    let sim = Sim::ak980();
    sim.state().effect[0] = 99;
    let d = driver(&sim, Some(&dir.0));
    let lines = d.diagnostics();
    let text = lines.join("\n");
    let kept = lines.iter().find_map(|l| l.strip_prefix("failed read saved in the app's data folder: ")).expect(&text);
    let name = kept.rsplit(['\\', '/']).next().unwrap();
    assert!(dir.0.join("keyboard-reads").join(name).is_file(), "{kept}");
    assert!(backups_in(&dir.0).is_empty(), "not a backup");
    for part in ["did not pass the sanity check (", "\n  unknown effect mode 99", "effect (0x13, 16 bytes, full"] {
        assert!(text.contains(part), "{part}:\n{text}");
    }
    assert!(text.contains("writes: refused, the keyboard does not look as expected (above)"), "{text}");
    // The same read on the next connect keeps the one file.
    let again = driver(&sim, Some(&dir.0));
    assert!(again.diagnostics().iter().any(|l| l.ends_with(name)));
    assert_eq!(std::fs::read_dir(dir.0.join("keyboard-reads")).unwrap().count(), 1);
}

// ---------------------------------------------------------------- review of #195

fn names(dir: &Path) -> Vec<String> {
    backups_in(dir).into_iter().map(|(n, _)| n).collect()
}

/// A channel whose USB handle dies after `left` more writes: every write
/// from then on fails as a stalled one does.
pub(crate) struct Dying {
    pub port: kb_core::aa::sim::SimPort,
    pub left: usize,
}

impl OutputPort for Dying {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        if self.left == 0 {
            return Err(AaError::Transport(kb_transport::TransportError::Refused("the handle died")));
        }
        self.left -= 1;
        self.port.write(report)
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        self.port.read(timeout_ms)
    }
}

/// A keyboard that sends a status notice (`55 FA`) before every reply.
struct Noticing(kb_core::aa::sim::SimPort, bool);

impl OutputPort for Noticing {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        self.1 = true;
        self.0.write(report)
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        if std::mem::take(&mut self.1) {
            let mut r = vec![0u8; 64];
            r[..3].copy_from_slice(&[0x55, 0xFA, 0x01]);
            return Ok(Some(r));
        }
        self.0.read(timeout_ms)
    }
}

#[test]
fn a_refused_restore_removes_no_backup_and_the_first_stays_first() {
    let dir = TempDir::new("restore-prune");
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(FACTORY_EFFECT);
    let mut d = driver(&sim, Some(&dir.0));
    let bdir = backups::dir(&dir.0);
    let (first, first_backup) = backups_in(&dir.0).remove(0);
    let t = target(Some(&dir.0));
    let snap = d.kb.snapshot().unwrap().clone();
    // One saved while the PC clock said earlier than at the first connect.
    let mut early = backups::KeyboardBackup::new(&t, backups::Reason::BeforeRestore, snap.clone());
    early.saved_at_ms = first_backup.saved_at_ms - 60_000;
    let early = backups::save(&bdir, &early).unwrap();
    let others: Vec<String> = (0..3)
        .map(|_| {
            let b = backups::KeyboardBackup::new(&t, backups::Reason::BeforeRestore, snap.clone());
            backups::save(&bdir, &b).unwrap()
        })
        .collect();
    let all = names(&dir.0);
    assert_eq!(all.len(), backups::KEPT);
    assert_eq!(all[0], early, "it sorts before the first");
    assert_eq!(backups::first_of(&bdir, Sim::usb()), Some(first.clone()), "the first by why it was taken");

    // The backup's effect colour is unknown and the keyboard's effect
    // differs: refused before anything is written, and nothing is pruned.
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();
    let source = others[0].clone();
    let restoring = backups::load(&bdir, &source).unwrap();
    let e = d.restore(&restoring.snapshot, None, Some(&source)).unwrap_err();
    assert!(e.to_string().contains("does not report its effect colour"), "{e}");
    assert_eq!(names(&dir.0), all, "a refused restore removes no backup");

    // The try again with a colour goes: the file restored and the first stay.
    let color = EffectColor { color: [0x00, 0x80, 0xFF], color_mode: 0 };
    d.restore(&restoring.snapshot, Some(color), Some(&source)).unwrap();
    let after = names(&dir.0);
    assert_eq!(after.len(), backups::KEPT, "{after:?}");
    assert!(after.contains(&first) && after.contains(&source), "{after:?}");
    assert!(!after.contains(&early), "the oldest other one went: {after:?}");
}

#[test]
fn a_channel_the_full_read_left_dead_is_handed_back_to_be_opened_again() {
    let dir = TempDir::new("dead-channel");
    let sim = Sim::ak980();
    let port = Dying { port: sim.command(), left: 3 };
    let mut d = AaDriver::new(AaPort::new(port), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(None);
    assert!(d.take_reopen().is_some_and(|e| e.needs_reopen()));
    // A write: the read again fails the same way, and the actor hears it
    // as a transport error (it drops the handle), not as "does not look
    // as expected".
    let e = d.apply_effect(&effect(2, [1, 2, 3])).unwrap_err();
    assert!(e.is_transport(), "{e}");
}

#[test]
fn replies_that_queue_up_on_an_idle_keyboard_still_count() {
    let mut w = ConflictWatch::default();
    let t = Instant::now();
    w.note(t, 0);
    // Asked again only after a minute, with 25 replies nobody asked for.
    assert!(w.note(t + CONFLICT_WINDOW + Duration::from_secs(1), 25) && w.suspected);

    // The keyboard's own notices are not another program.
    let dir = TempDir::new("notices");
    let sim = Sim::ak980();
    let mut d = AaDriver::new(AaPort::new(Noticing(sim.command(), false)), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(None);
    assert!(d.kb.notices_reported() >= CONFLICT_THRESHOLD, "{}", d.kb.notices_reported());
    assert!(!d.status().other_program_suspected);
}

// ---------------------------------------------------------------- second check of #195

#[test]
fn restores_that_stop_part_way_do_not_pile_up_backups() {
    let dir = TempDir::new("restore-incomplete");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let bdir = backups::dir(&dir.0);
    let first = names(&dir.0).remove(0);
    let t = target(Some(&dir.0));
    let snap = d.kb.snapshot().unwrap().clone();
    let others: Vec<String> = (0..backups::KEPT - 1)
        .map(|_| {
            let b = backups::KeyboardBackup::new(&t, backups::Reason::BeforeRestore, snap.clone());
            backups::save(&bdir, &b).unwrap()
        })
        .collect();
    assert_eq!(names(&dir.0).len(), backups::KEPT);
    // The keyboard's keymap differs from the backups', and it does not keep
    // keymap writes: every try stops in the base layer.
    d.apply_keymap(Layer::Base, &LayerMap::from([(led("a"), KeyAction::key(0x05))])).unwrap();
    sim.state().faults.ignore_writes = Some(cmd::SET_KEYMAP);
    let source = others[0].clone();
    let restoring = backups::load(&bdir, &source).unwrap();
    for i in 0..3 {
        let e = d.restore(&restoring.snapshot, None, Some(&source)).unwrap_err();
        assert!(e.to_string().starts_with("the restore stopped while writing the base layer"), "{e}");
        let now = names(&dir.0);
        assert_eq!(now.len(), backups::KEPT, "try {i}: {now:?}");
        assert!(now.contains(&first) && now.contains(&source), "try {i}: {now:?}");
    }
}

// ---------------------------------------------------------------- firmware 769 (beta.2)

#[test]
fn live_lights_switch_nothing_and_write_nothing() {
    // Firmware 769 shows frames in any effect mode: no switch to per-key
    // mode, no note, no hold, nothing put back.
    let dir = TempDir::new("live");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting();
    for _ in 0..3 {
        d.live_frame(vec![(led("esc"), [9, 9, 9])]).unwrap();
    }
    let s = sim.state().clone();
    assert!(s.frames.last().expect("a frame").1, "shown");
    assert!(s.sent(cmd::SET_EFFECT).is_empty(), "the effect stays as it is");
    assert_eq!(s.effect, FACTORY_EFFECT);
    assert_eq!(writes(&sim), s.sent(cmd::REALTIME).len(), "frames only");
    let status = d.status();
    assert!(!status.live_armed && !status.live_restores && status.live_lights_off.is_none(), "{status:?}");
    assert!(!dir.0.join("aa-live-armed.json").exists(), "no note on disk");
    // The stream ends: nothing is due, nothing goes back.
    d.live_stopped();
    assert_eq!(d.next_due(), None);
    d.run_due(&|| None).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_EFFECT).len(), 0);
}

#[test]
fn live_lights_show_on_a_read_only_keyboard_too() {
    // Frames store nothing: the sanity check, which guards flash, does not
    // hold them back.
    let dir = TempDir::new("live-read-only");
    let sim = Sim::ak980();
    sim.state().effect[0] = 99;
    let mut d = driver(&sim, Some(&dir.0));
    assert!(d.read_only().is_some());
    d.live_starting();
    d.live_frame(vec![(led("esc"), [9, 9, 9])]).unwrap();
    assert!(sim.state().frames.last().is_some_and(|f| f.1));
    assert_eq!(d.status().live_lights_off, None);
    assert!(sim.state().sent(cmd::SET_EFFECT).is_empty());
}

#[test]
fn live_lights_wait_while_the_keyboard_ignores_writes() {
    // Just switched on (or reset): frames are held back, and the UI hears
    // why, until the keyboard takes them.
    let dir = TempDir::new("live-settling");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.keyboard_mut().set_settle(Duration::from_millis(150));
    d.live_starting();
    d.live_frame(vec![(led("esc"), [9, 9, 9])]).unwrap();
    assert!(sim.state().sent(cmd::REALTIME).is_empty());
    assert_eq!(d.status().live_lights_off, Some(LiveLightsOff::Blocked { reason: LIVE_SETTLING.into() }));
    std::thread::sleep(Duration::from_millis(200));
    d.live_frame(vec![(led("esc"), [9, 9, 9])]).unwrap();
    assert!(sim.state().frames.last().is_some_and(|f| f.1));
    assert_eq!(d.status().live_lights_off, None);
}

#[test]
fn live_lights_wait_out_the_quiet_time_after_a_cut_upload() {
    let dir = TempDir::new("quiet-live");
    let sim = Sim::ak980();
    sim.state().upload_quiet = Duration::from_millis(100);
    let mut d = driver(&sim, Some(&dir.0));
    d.keyboard_mut().set_upload_cooldown(Duration::from_millis(250));
    let anim = ScreenAnimation::still(screen::test_pattern());
    let cancel = AtomicBool::new(false);
    let mut display = AaPort::new(sim.display());
    let progress = |done, _| {
        if done == 2 {
            cancel.store(true, Ordering::SeqCst);
        }
    };
    d.upload_screen(&mut display, &anim, progress, &cancel).unwrap_err();
    let status = d.status();
    d.live_starting();
    for _ in 0..5 {
        d.live_frame(vec![(led("esc"), [1, 1, 1])]).unwrap();
        assert_eq!(d.status(), status, "nothing changes for the UI meanwhile");
    }
    assert!(!d.diagnostics().iter().any(|l| l.contains("no commands for another")), "no error noted");
    assert!(sim.state().sent(cmd::REALTIME).is_empty());
    // Once it is over (not 30 s later), the next frame shows.
    std::thread::sleep(Duration::from_millis(300));
    d.live_frame(vec![(led("esc"), [1, 1, 1])]).unwrap();
    assert!(sim.state().frames.last().is_some_and(|f| f.1));
}

#[test]
fn a_backup_keeps_the_effect_mode_the_app_knows() {
    // Per-key colours saved from Keymotion in an earlier run: the keyboard
    // reads mode 0, which off would read too. The app's lighting record
    // says 128, and the first backup keeps that, not "off".
    let dir = TempDir::new("backup-mode");
    let sim = Sim::ak980();
    let pk = [128, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 4, 3, 0, 0, 0, 0xAA, 0x55];
    sim.state().set_effect_by_another_tool(pk);
    let app = AaEffect::decode(&pk).unwrap();
    let mut d = AaDriver::new(AaPort::new(sim.command()), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(Some(app));
    let (_, b) = &backups_in(&dir.0)[0];
    assert_eq!((b.snapshot.effect.0[0], b.snapshot.effect_mode), (0, Some(128)));
    assert_eq!(b.snapshot.effect().unwrap().mode, 128);
    // A record of another mode (or other levels) tells nothing.
    let dir = TempDir::new("backup-mode-other");
    let mut d = AaDriver::new(AaPort::new(sim.command()), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(Some(AaEffect { mode: 1, ..app }));
    assert_eq!(backups_in(&dir.0)[0].1.snapshot.effect_mode, None);
}
