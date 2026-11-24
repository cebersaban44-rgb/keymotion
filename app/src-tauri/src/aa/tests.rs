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

fn yes() -> Result<(), String> {
    Ok(())
}

fn several() -> Result<(), String> {
    Err("more than one keyboard on the USB cable".into())
}

fn untested() -> Result<(), String> {
    Err("untested keyboard firmware 3.01".into())
}

/// The Memory gate says yes.
fn go() -> LiveContext<'static> {
    LiveContext { gate: Some(&yes), app_effect: None, arming: None }
}

/// The gate says no, for this reason.
fn no(gate: Gate<'static>) -> LiveContext<'static> {
    LiveContext { gate: Some(gate), app_effect: None, arming: None }
}

/// A gate that says yes and counts how often it was asked.
struct Counting(std::cell::Cell<u32>);

impl Counting {
    fn new() -> Self {
        Self(std::cell::Cell::new(0))
    }
    fn ask(&self) -> Result<(), String> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
    fn asked(&self) -> u32 {
        self.0.get()
    }
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
    assert_eq!(report.screen_max_frames, Some(140));
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

#[test]
fn live_lights_switch_to_per_key_mode_and_back_after_the_hold() {
    let dir = TempDir::new("live");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let gate = Counting::new();
    let ask = || gate.ask();
    let counted = LiveContext { gate: Some(&ask), app_effect: None, arming: None };
    d.live_starting(&counted).unwrap();
    assert_eq!(gate.asked(), 1, "the switch is a memory write");
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    let note = armed_note(&dir.0).expect("noted before the switch");
    assert_eq!(AaEffect::decode(&note.effect.0).unwrap().mode, 1, "the effect to put back");
    assert!(d.status().live_armed);
    assert!(d.status().live_restores, "its own effect comes back after the hold");
    for _ in 0..3 {
        d.live_frame(vec![(led("esc"), [9, 9, 9])], &counted).unwrap();
    }
    assert_eq!(gate.asked(), 1, "frames write nothing to memory: no gate");
    assert!(sim.state().frames.last().expect("a frame").1, "shown");

    // The stream ends: the hold runs, nothing changes yet.
    d.live_stopped();
    let due = d.next_due().expect("the effect goes back later");
    assert!(due > Instant::now() + LIVE_HOLD - Duration::from_secs(2));
    d.run_due(Instant::now(), &go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);

    // A stream within the hold calls it off, without a write.
    let effect_writes = sim.state().sent(cmd::SET_EFFECT).len();
    d.live_starting(&LiveContext::default()).unwrap();
    assert_eq!(d.next_due(), None);
    assert_eq!(sim.state().sent(cmd::SET_EFFECT).len(), effect_writes);
    d.live_stopped();

    // After the hold, a refusal waits and tries again later.
    let later = Instant::now() + LIVE_HOLD + Duration::from_secs(1);
    d.run_due(later, &no(&several)).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    assert!(d.next_due().unwrap() > later);

    // The gate says yes: the user's effect, colour and all, and the note goes.
    let much_later = later + Duration::from_secs(11);
    d.run_due(much_later, &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());
    assert!(!d.status().live_armed);
    assert_eq!(d.next_due(), None);
}

#[test]
fn a_crash_leaves_a_note_and_the_next_connect_puts_the_effect_back() {
    let dir = TempDir::new("crash");
    let sim = Sim::ak980();
    {
        let mut d = driver(&sim, Some(&dir.0));
        d.live_starting(&go()).unwrap();
        // The app ends here (a crash, a pulled cable): no live_stopped.
    }
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    assert!(armed_note(&dir.0).is_some());

    let mut d = driver(&sim, Some(&dir.0));
    assert!(d.next_due().is_some_and(|t| t <= Instant::now()));
    // A refused gate (the firmware question unanswered) waits.
    d.run_due(Instant::now(), &no(&untested)).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    assert!(armed_note(&dir.0).is_some());
    // The user said yes.
    d.retry_live();
    d.run_due(Instant::now(), &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());
    assert_eq!(d.next_due(), None);
}

#[test]
fn live_lights_that_run_at_connect_keep_the_noted_effect_for_later() {
    let dir = TempDir::new("crash-live");
    let sim = Sim::ak980();
    {
        let mut d = driver(&sim, Some(&dir.0));
        d.live_starting(&go()).unwrap();
    }
    let mut d = driver(&sim, Some(&dir.0));
    let effect_writes = sim.state().sent(cmd::SET_EFFECT).len();
    d.live_starting(&go()).unwrap();
    assert_eq!(sim.state().sent(cmd::SET_EFFECT).len(), effect_writes, "in per-key mode already");
    d.live_stopped();
    d.run_due(Instant::now() + LIVE_HOLD + Duration::from_secs(1), &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12], "the noted effect goes back");
    assert!(armed_note(&dir.0).is_none());
}

#[test]
fn closing_puts_the_effect_back_without_the_hold() {
    let dir = TempDir::new("closing");
    let sim = Sim::ak980();
    let mut d = Driver::<kb_transport::hid::HidChannel>::Aa(Box::new(driver(&sim, Some(&dir.0))));
    d.live_starting(&go()).unwrap();
    d.live_stopped().unwrap();
    d.closing(&go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());
    // Refused: the note stays for the next connect.
    let dir = TempDir::new("closing-refused");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&go()).unwrap();
    d.closing(&no(&several)).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    assert!(armed_note(&dir.0).is_some());
}

#[test]
fn an_unknown_colour_is_never_armed() {
    let dir = TempDir::new("colour");
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(FACTORY_EFFECT);
    let mut d = driver(&sim, Some(&dir.0));
    let gate = Counting::new();
    let ask = || gate.ask();
    let counted = LiveContext { gate: Some(&ask), app_effect: None, arming: None };
    d.live_starting(&counted).unwrap();
    assert_eq!(sim.state().effect[0], 1, "not switched");
    assert!(sim.state().sent(cmd::SET_EFFECT).is_empty());
    assert_eq!(d.status().live_lights_off, Some(LiveLightsOff::ColorUnknown));
    assert!(armed_note(&dir.0).is_none());
    // Frames are dropped, not sent, and nobody is asked anything (no
    // firmware question for a switch that cannot happen).
    d.live_frame(vec![(led("esc"), [9, 9, 9])], &counted).unwrap();
    assert!(sim.state().sent(cmd::REALTIME).is_empty());
    assert_eq!(gate.asked(), 0);

    // The app's record of another effect does not help.
    let app = AaEffect::decode(&FACTORY_EFFECT).unwrap();
    let other = move || Some(AaEffect { brightness: 2, ..app });
    d.live_stopped();
    d.live_starting(&LiveContext { gate: Some(&yes), app_effect: Some(&other), arming: None }).unwrap();
    assert_eq!(sim.state().effect[0], 1);

    // Its record of this very effect does: the colour it stored.
    let same = move || Some(app);
    d.live_stopped();
    d.live_starting(&LiveContext { gate: Some(&yes), app_effect: Some(&same), arming: None }).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    let note = armed_note(&dir.0).unwrap();
    assert_eq!(AaEffect::decode(&note.effect.0).unwrap().color_part(), app.color_part());
    assert_eq!(d.status().live_lights_off, None);

    // A colour Keymotion wrote itself is known.
    let dir = TempDir::new("colour-own");
    let sim = Sim::ak980();
    sim.state().set_effect_by_another_tool(FACTORY_EFFECT);
    let mut d = driver(&sim, Some(&dir.0));
    d.apply_effect(&effect(2, [10, 20, 30])).unwrap();
    d.live_starting(&go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
}

#[test]
fn a_refused_gate_holds_live_lights_back() {
    let dir = TempDir::new("gate");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&no(&untested)).unwrap();
    assert_eq!(
        d.status().live_lights_off,
        Some(LiveLightsOff::Blocked { reason: "untested keyboard firmware 3.01".into() })
    );
    assert!(sim.state().sent(cmd::SET_EFFECT).is_empty());
    assert!(armed_note(&dir.0).is_none());
    // Not asked again right away (the question is not put again and again).
    let gate = Counting::new();
    let ask = || gate.ask();
    d.live_frame(vec![], &LiveContext { gate: Some(&ask), app_effect: None, arming: None }).unwrap();
    assert_eq!(gate.asked(), 0);
    assert!(sim.state().sent(cmd::SET_EFFECT).is_empty());
    // The user said yes: the refusal goes at once (the UI stops saying live
    // lights wait for the answer), and the next frame switches.
    d.retry_live();
    assert_eq!(d.status().live_lights_off, None);
    d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    assert!(sim.state().frames.last().unwrap().1);

    // No gate at all: nothing is written, and the next frame tries again.
    let sim = Sim::ak980();
    let dir = TempDir::new("gate-unasked");
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&LiveContext::default()).unwrap();
    assert!(sim.state().sent(cmd::SET_EFFECT).is_empty());
    d.live_frame(vec![], &go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
}

#[test]
fn the_users_own_write_is_what_comes_back() {
    let dir = TempDir::new("own-write");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&go()).unwrap();
    // A new effect saved while live lights run: written now, and no longer
    // the old one is put back.
    d.apply_effect(&effect(2, [255, 0, 0])).unwrap();
    assert_eq!(sim.state().effect[0], 2);
    assert!(armed_note(&dir.0).is_none());
    // Frames that keep coming switch again, with the new effect to go back.
    d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    d.live_stopped();
    d.run_due(Instant::now() + LIVE_HOLD + Duration::from_secs(1), &go()).unwrap();
    let e = sim.state().effect;
    assert_eq!((e[0], [e[1], e[2], e[3]]), (2, [255, 0, 0]));
    // A per-key design saved: it is the effect, nothing to put back.
    d.live_starting(&go()).unwrap();
    d.apply_per_key(vec![(led("esc"), [5, 5, 5])], 5).unwrap();
    d.live_stopped();
    d.run_due(Instant::now() + LIVE_HOLD + Duration::from_secs(1), &go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
}

/// Each way a keyboard does not look as expected, made in the simulator.
fn misbehaving() -> Vec<(&'static str, Sim)> {
    let insane = Sim::ak980();
    insane.state().effect[0] = 99;
    let pending = Sim::ak980();
    pending.state().info[32] = 1;
    let silent = Sim::ak980();
    silent.state().faults.silent_info = true;
    vec![("insane", insane), ("update pending", pending), ("silent", silent)]
}

#[test]
fn a_keyboard_that_does_not_look_right_is_read_only() {
    for (name, sim) in misbehaving() {
        let dir = TempDir::new(&format!("read-only-{}", name.replace(' ', "-")));
        let mut d = driver(&sim, Some(&dir.0));
        let ro = d.read_only().unwrap_or_else(|| panic!("{name}: writable"));
        match name {
            "update pending" => assert_eq!(ro, ReadOnly::UpdatePending),
            _ => assert!(matches!(&ro, ReadOnly::NotSane(why) if !why.is_empty()), "{name}: {ro:?}"),
        }
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
        d.live_starting(&go()).unwrap();
        assert_eq!(d.status().live_lights_off, Some(LiveLightsOff::ReadOnly), "{name}");
        assert_eq!(writes(&sim), 0, "{name}: nothing written");
        assert!(backups_in(&dir.0).is_empty(), "{name}: no backup of what does not look right");
    }

    // Waiting for an update: the device information says so, not an error.
    let sim = Sim::ak980();
    sim.state().info[32] = 1;
    let mut d = driver(&sim, None);
    assert!(d.device_info().unwrap().update_pending);
    assert_eq!(
        d.read_only().unwrap().to_string(),
        "the keyboard is waiting for a firmware update; nothing is read or written"
    );
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

#[test]
fn live_lights_on_a_keyboard_in_per_key_mode_put_nothing_back() {
    let dir = TempDir::new("live-per-key");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    // Per-key lighting saved from Keymotion: the keyboard's own effect.
    d.apply_effect(&effect(aa_effect::MODE_PER_KEY, [0, 0, 0])).unwrap();
    d.live_starting(&go()).unwrap();
    let status = d.status();
    assert!(status.live_armed);
    assert!(!status.live_restores, "nothing comes back: no \"about 30 seconds\" notice");
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
    let img = aa_macros::encode(slots).unwrap();
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
    assert!(device_report(&DeviceInfo { firmware_status: 1, ..i }).update_pending);
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

fn later() -> Instant {
    Instant::now() + LIVE_HOLD + Duration::from_secs(1)
}

fn effect_writes(sim: &Sim) -> usize {
    sim.state().sent(cmd::SET_EFFECT).len()
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

/// A keyboard whose per-key mode reads back with another speed than was
/// written (a firmware that normalises it): the switch lands, and its
/// read-back differs.
struct Normalising(kb_core::aa::sim::SimPort);

impl OutputPort for Normalising {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        self.0.write(report)
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        use kb_protocol::aa::frame;
        Ok(self.0.read(timeout_ms)?.map(|mut r| {
            let at = usize::from(r.len() == frame::WIRE_LEN && r[0] == 0);
            let data = at + frame::HEADER_LEN;
            if r[at] == frame::REPLY && r[at + 1] == cmd::EFFECT && r[data] == aa_effect::MODE_PER_KEY {
                r[data + 10] = r[data + 10] % 5 + 1;
            }
            r
        }))
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
fn a_live_note_is_only_for_the_keyboard_that_wrote_it() {
    let dir = TempDir::new("note-identity");
    let a = Sim::ak980();
    {
        let mut d = driver(&a, Some(&dir.0));
        d.live_starting(&go()).unwrap();
        // The app ends here.
    }
    assert!(armed_note(&dir.0).is_some());

    // Another AK980 PRO on the same firmware, kept in per-key mode by its
    // user: nothing is written to it, and the note stays for the first.
    let b = Sim::ak980();
    {
        let mut s = b.state();
        aa_keymap::set_entry(&mut s.keymap, aa("a"), [aa_keymap::page::KEYBOARD, 0, 0x05, 0]);
        s.effect[0] = aa_effect::MODE_PER_KEY;
        s.effect[8] = 0;
    }
    let shown = b.state().effect;
    let mut d = driver(&b, Some(&dir.0));
    assert_eq!(d.next_due(), None, "nothing to put back on it");
    d.run_due(Instant::now(), &go()).unwrap();
    assert!(b.state().sent(cmd::SET_EFFECT).is_empty());
    assert_eq!(b.state().effect, shown);
    assert!(armed_note(&dir.0).is_some(), "not its note");
    // Nor does one in an effect of its own drop it.
    let c = Sim::ak980();
    aa_keymap::set_entry(&mut c.state().keymap, aa("b"), [aa_keymap::page::KEYBOARD, 0, 0x06, 0]);
    let _ = driver(&c, Some(&dir.0));
    assert!(armed_note(&dir.0).is_some(), "not its note either");

    // The first keyboard comes back: its effect goes back.
    let mut d = driver(&a, Some(&dir.0));
    d.run_due(Instant::now(), &go()).unwrap();
    assert_eq!(a.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());

    // After a crash the user chose a live mode on the keyboard itself: it
    // stays, and the note goes.
    let dir = TempDir::new("note-own-choice");
    let a = Sim::ak980();
    {
        let mut d = driver(&a, Some(&dir.0));
        d.live_starting(&go()).unwrap();
    }
    a.state().effect[0] = 20;
    let writes_before = effect_writes(&a);
    let mut d = driver(&a, Some(&dir.0));
    d.run_due(Instant::now(), &go()).unwrap();
    assert_eq!(a.state().effect[0], 20);
    assert_eq!(effect_writes(&a), writes_before);
    assert!(armed_note(&dir.0).is_none());
}

#[test]
fn nothing_is_switched_once_the_app_is_closing() {
    let dir = TempDir::new("closed");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&go()).unwrap();
    assert!(d.holds_keyboard());
    d.closing(&go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(!d.holds_keyboard());
    // Frames that still come do not switch it again.
    let writes_before = effect_writes(&sim);
    d.live_starting(&go()).unwrap();
    d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    assert_eq!(effect_writes(&sim), writes_before);
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());
}

#[test]
fn a_read_again_during_live_lights_puts_nothing_back_meanwhile() {
    let dir = TempDir::new("reread-armed");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&go()).unwrap();
    d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    let writes_before = effect_writes(&sim);
    // The keyboard reports a reset with the next reply: what was known is
    // stale, and the background reads it again.
    let next = sim.state().packets.len() + 1;
    sim.state().faults.reset_before = Some(next);
    let _ = d.live_frame(vec![(led("esc"), [2, 2, 2])], &go());
    assert!(d.stale(), "the reset closed the gate");
    d.read_at = None;
    d.run_due(Instant::now(), &go()).unwrap();
    assert!(!d.stale(), "read again");
    assert_eq!(effect_writes(&sim), writes_before, "nothing put back while frames come");
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    assert!(d.status().live_armed);
    // The stream ends: after the hold the effect goes back, once.
    d.live_frame(vec![(led("esc"), [3, 3, 3])], &go()).unwrap();
    d.live_stopped();
    d.run_due(later(), &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert_eq!(effect_writes(&sim), writes_before + 1);
}

#[test]
fn levels_a_keyboard_reads_out_of_range_go_back_in_range() {
    let dir = TempDir::new("levels");
    let sim = Sim::ak980();
    {
        // What sibling boards and AJAZZ's hub leave: brightness 6, speed 0.
        let mut s = sim.state();
        s.effect[9] = 6;
        s.effect[10] = 0;
    }
    let mut d = driver(&sim, Some(&dir.0));
    assert_eq!(d.read_only(), None);
    d.live_starting(&go()).unwrap();
    assert!(d.status().live_armed, "{:?}", d.status().live_lights_off);
    let back = AaEffect::decode(&armed_note(&dir.0).unwrap().effect.0).unwrap();
    assert_eq!((back.mode, back.brightness, back.speed), (1, 5, 1));
    d.live_stopped();
    d.run_due(later(), &go()).unwrap();
    let e = sim.state().effect;
    assert_eq!((e[0], e[9], e[10]), (1, 5, 1));
}

#[test]
fn a_backup_before_a_restore_during_live_lights_holds_the_users_effect() {
    let dir = TempDir::new("restore-armed");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let (_, first) = backups_in(&dir.0).remove(0);
    d.live_starting(&go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
    d.restore(&first.snapshot, None, None).unwrap();
    let (_, before) = backups_in(&dir.0).into_iter().find(|(_, b)| b.reason == backups::Reason::BeforeRestore).unwrap();
    let factory = AaEffect::decode(&FACTORY_EFFECT).unwrap();
    assert_eq!(before.snapshot.effect().unwrap().mode, 1, "not the per-key mode live lights hold");
    assert_eq!(before.snapshot.effect_color, Some(factory.color_part().into()));
}

#[test]
fn a_failed_switch_is_not_tried_again_with_every_frame() {
    let dir = TempDir::new("arm-retry");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    // The keyboard echoes the switch but keeps its effect.
    sim.state().faults.ignore_writes = Some(cmd::SET_EFFECT);
    let e = d.live_starting(&go()).unwrap_err();
    assert!(e.to_string().contains("differs"), "{e}");
    assert_eq!(effect_writes(&sim), 1);
    for _ in 0..5 {
        d.live_starting(&go()).unwrap();
        d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    }
    assert_eq!(effect_writes(&sim), 1, "the next try waits");
    assert!(matches!(d.status().live_lights_off, Some(LiveLightsOff::Failed { .. })));
    assert!(armed_note(&dir.0).is_none(), "the keyboard did not change");
    // Each failure in a row waits longer.
    if let Live::Off { retry_at, .. } = &mut d.live {
        *retry_at = Some(Instant::now());
    }
    d.live_frame(vec![], &go()).unwrap_err();
    assert_eq!(effect_writes(&sim), 2);
    match &d.live {
        Live::Off { retry_at: Some(t), .. } => assert!(*t > Instant::now() + ARM_RETRY + Duration::from_secs(20)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_switch_that_lands_but_reads_back_otherwise_still_puts_the_effect_back() {
    let open = |sim: &Sim, dir: &Path| {
        let mut d = AaDriver::new(AaPort::new(Normalising(sim.command())), Sim::model(), target(Some(dir))).unwrap();
        d.connect(None);
        d
    };
    let dir = TempDir::new("arm-landed");
    let sim = Sim::ak980();
    let mut d = open(&sim, &dir.0);
    d.live_starting(&go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY, "the switch landed");
    assert!(d.status().live_armed);
    d.live_frame(vec![(led("esc"), [3, 3, 3])], &go()).unwrap();
    assert!(sim.state().frames.last().unwrap().1, "frames show");
    d.live_stopped();
    d.run_due(later(), &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());

    // A crash after such a switch: the note holds what the keyboard shows.
    let dir = TempDir::new("arm-landed-crash");
    let sim = Sim::ak980();
    {
        let mut d = open(&sim, &dir.0);
        d.live_starting(&go()).unwrap();
    }
    let mut d = open(&sim, &dir.0);
    assert!(d.next_due().is_some());
    d.run_due(Instant::now(), &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
}

#[test]
fn an_effect_chosen_on_the_keyboard_during_the_hold_stays() {
    let mut chosen = FACTORY_EFFECT;
    chosen[0] = 4;
    let dir = TempDir::new("hold-chosen");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&go()).unwrap();
    d.live_stopped();
    sim.state().set_effect_by_another_tool(chosen);
    let writes_before = effect_writes(&sim);
    d.run_due(later(), &go()).unwrap();
    assert_eq!(sim.state().effect[0], 4);
    assert_eq!(effect_writes(&sim), writes_before);
    assert!(armed_note(&dir.0).is_none());
    assert!(!d.status().live_armed);
    assert_eq!(d.next_due(), None);

    // Closing the app, too.
    let dir = TempDir::new("closing-chosen");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    d.live_starting(&go()).unwrap();
    sim.state().set_effect_by_another_tool(chosen);
    let writes_before = effect_writes(&sim);
    d.closing(&go()).unwrap();
    assert_eq!(sim.state().effect[0], 4);
    assert_eq!(effect_writes(&sim), writes_before);
}

#[test]
fn live_lights_wait_out_the_quiet_time_after_a_cut_upload() {
    let dir = TempDir::new("quiet-arm");
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
    d.live_starting(&go()).unwrap();
    for _ in 0..5 {
        d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
        assert_eq!(d.status(), status, "nothing changes for the UI meanwhile");
    }
    assert!(!d.diagnostics().iter().any(|l| l.contains("no commands for another")), "no error noted");
    assert!(sim.state().sent(cmd::SET_EFFECT).is_empty());
    // Once it is over (not 30 s later), the next frame switches.
    std::thread::sleep(Duration::from_millis(300));
    d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY);
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

/// A keyboard that, once switched to per-key mode, stops answering reads of
/// its effect while `deaf` is set: the switch goes out and is echoed, its
/// read-back never comes.
struct LosesReadBack {
    port: kb_core::aa::sim::SimPort,
    deaf: std::sync::Arc<AtomicBool>,
}

impl OutputPort for LosesReadBack {
    fn write(&mut self, report: &[u8]) -> kb_core::aa::Result<usize> {
        use kb_protocol::aa::frame;
        let data = 1 + frame::HEADER_LEN;
        if report.len() > data && report[2] == cmd::SET_EFFECT && report[data] == aa_effect::MODE_PER_KEY {
            self.deaf.store(true, Ordering::SeqCst);
        }
        self.port.write(report)
    }
    fn read(&mut self, timeout_ms: u32) -> kb_core::aa::Result<Option<Vec<u8>>> {
        use kb_protocol::aa::frame;
        let r = self.port.read(timeout_ms)?;
        let effect_reply = r.as_ref().is_some_and(|r| {
            let at = usize::from(r.len() == frame::WIRE_LEN && r[0] == 0);
            r[at] == frame::REPLY && r[at + 1] == cmd::EFFECT
        });
        if effect_reply && self.deaf.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(r)
    }
}

#[test]
fn a_switch_whose_read_back_is_lost_still_gets_its_effect_back() {
    let dir = TempDir::new("arm-lost-readback");
    let sim = Sim::ak980();
    let deaf = std::sync::Arc::new(AtomicBool::new(false));
    let port = LosesReadBack { port: sim.command(), deaf: deaf.clone() };
    let mut d = AaDriver::new(AaPort::new(port), Sim::model(), target(Some(&dir.0))).unwrap();
    d.connect(None);
    d.live_starting(&go()).unwrap_err();
    assert_eq!(sim.state().effect[0], aa_effect::MODE_PER_KEY, "the switch landed");
    assert!(armed_note(&dir.0).is_some(), "the note stays: the switch may have landed");
    // The keyboard answers again: the background looks, sees per-key mode
    // and puts the effect back.
    deaf.store(false, Ordering::SeqCst);
    d.read_at = None;
    d.run_due(Instant::now() + DISARM_RETRY + Duration::from_secs(1), &go()).unwrap();
    assert_eq!(sim.state().effect[..12], FACTORY_EFFECT[..12]);
    assert!(armed_note(&dir.0).is_none());
}

#[test]
fn a_failed_switch_waits_across_streams_too() {
    let dir = TempDir::new("arm-retry-streams");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    sim.state().faults.ignore_writes = Some(cmd::SET_EFFECT);
    d.live_starting(&go()).unwrap_err();
    assert_eq!(effect_writes(&sim), 1);
    // Agents' lights start and stop often.
    for _ in 0..5 {
        d.live_stopped();
        d.live_starting(&go()).unwrap();
        d.live_frame(vec![(led("esc"), [1, 1, 1])], &go()).unwrap();
    }
    assert_eq!(effect_writes(&sim), 1, "each new stream waits too");
    assert!(matches!(d.status().live_lights_off, Some(LiveLightsOff::Failed { .. })));
}

#[test]
fn whoever_waits_for_closing_hears_of_a_switch_before_it_is_written() {
    let dir = TempDir::new("arming");
    let sim = Sim::ak980();
    let mut d = driver(&sim, Some(&dir.0));
    let told = std::cell::Cell::new(None);
    let arming = || told.set(Some(effect_writes(&sim)));
    let ctx = LiveContext { gate: Some(&yes), app_effect: None, arming: Some(&arming) };
    d.live_starting(&ctx).unwrap();
    assert_eq!(told.get(), Some(0), "told before the switch was written");
    assert_eq!(effect_writes(&sim), 1);
    // Frames on an armed keyboard switch nothing: nobody is told.
    told.set(None);
    d.live_frame(vec![(led("esc"), [1, 1, 1])], &ctx).unwrap();
    assert_eq!(told.get(), None);
}

#[test]
fn notes_of_keyboards_never_seen_again_do_not_pile_up() {
    let dir = TempDir::new("notes-cap");
    let keyboards: Vec<Sim> = (0..NOTES_KEPT + 3)
        .map(|i| {
            let sim = Sim::ak980();
            let usage = 0x05 + i as u8;
            aa_keymap::set_entry(&mut sim.state().keymap, aa("a"), [aa_keymap::page::KEYBOARD, 0, usage, 0]);
            {
                let mut d = driver(&sim, Some(&dir.0));
                d.live_starting(&go()).unwrap();
                // The app ends here; this keyboard is never seen again.
            }
            sim
        })
        .collect();
    assert_eq!(armed_notes(&dir.0).len(), NOTES_KEPT);
    // The newest stay: the last keyboard still gets its effect back.
    let last = keyboards.last().unwrap();
    let mut d = driver(last, Some(&dir.0));
    d.run_due(Instant::now(), &go()).unwrap();
    assert_eq!(last.state().effect[..12], FACTORY_EFFECT[..12]);
    assert_eq!(armed_notes(&dir.0).len(), NOTES_KEPT - 1);
}

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
