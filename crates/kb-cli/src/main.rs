//! `kb` — developer and QA tool.
//!
//! `effect`, `per-key`, `keymap`, `macros`, `screen-test`, `screen-gif` and
//! `direction-probe` write the keyboard's flash, and all need `--yes`. `live` and `live-probe` stream RAM-only real-time LED frames;
//! `clock` sets the keyboard clock; everything else only reads. Flash-writing commands are added only after
//! they are verified on the hardware.
//!
//! Like the app, every packet passes `kb_protocol::sonix::guard`, a channel
//! is used only when its report layout is the "04 xx" family's, and nothing
//! is sent while another program talks to the keyboard (the app itself
//! included). Unlike the app, it does not ask about firmware versions the
//! model file has not listed: measuring a new firmware is what it is for,
//! and `--yes` is the confirmation for flash writes.

mod motion;
mod png;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use kb_devices::{catalogue, match_ble_name, match_interface, Bus, InterfaceProbe, InterfaceRole};
use kb_transport::hid::{self, Hid, HidInterface};

#[derive(Parser)]
#[command(name = "kb", version, about = "Klavye geliştirme aracı (salt-okunur)")]
struct Cli {
    /// Log level filter, e.g. `debug` or `kb_transport=trace`.
    #[arg(long, global = true, default_value = "warn")]
    log: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Detect supported keyboards in every connection mode.
    List,
    /// List every HID collection on the system (debugging).
    HidAll,
    /// Dump report descriptors of a detected keyboard's vendor interfaces.
    Descriptors,
    /// Read the battery level of paired Bluetooth keyboards.
    Battery,
    /// Show the embedded model catalogue.
    Models,
    /// Passively read the command channel's feature report (sends nothing).
    Peek,
    /// Print input reports the wired keyboard sends on its own, on every
    /// vendor collection except the screen's (sends nothing).
    Listen {
        /// How long to listen, in seconds.
        #[arg(long, default_value_t = 20)]
        seconds: u64,
    },
    /// Read the colours currently shown on the keys (`04 F5`, read-only).
    ReadColors {
        /// Read N times on the same handle (diagnostics).
        #[arg(long, default_value_t = 1)]
        repeat: u32,
        /// Pause between repeated reads, in ms.
        #[arg(long, default_value_t = 250)]
        interval: u64,
    },
    /// Stream real-time LED frames (`04 20`, RAM only, no flash write).
    Live {
        /// `wave` (rainbow across the board) or `solid`.
        #[arg(long, default_value = "wave")]
        pattern: String,
        /// Colour for `solid`, as RRGGBB.
        #[arg(long, default_value = "FF8A3D")]
        color: String,
        /// How long to stream, in seconds.
        #[arg(long, default_value_t = 5.0)]
        seconds: f32,
        /// Frames per second.
        #[arg(long, default_value_t = 30)]
        fps: u32,
    },
    /// Diagnose real-time frames (RAM only): does `04 F5` see the live
    /// overlay, and do LEDs left out of a frame keep their last colour?
    LiveProbe,
    /// List the firmware lighting effects.
    Effects,
    /// Measure which way directional effects move: stores each effect in
    /// rainbow mode with each direction code, reads frames back with
    /// `04 F5` and estimates the motion. WRITES FLASH (one effect write per
    /// mode and direction); needs --yes. Restore your own lighting after.
    DirectionProbe {
        /// Effect ids to test.
        #[arg(long, value_delimiter = ',', default_value = "10,11,12,16,18")]
        modes: Vec<u8>,
        /// Direction codes to test.
        #[arg(long, value_delimiter = ',', default_value = "0,1,2,3")]
        dirs: Vec<u8>,
        /// Effect speed (0–5); slow keeps shifts between reads small.
        #[arg(long, default_value_t = 1)]
        speed: u8,
        /// Frames read per setting.
        #[arg(long, default_value_t = 6)]
        frames: usize,
        #[arg(long)]
        yes: bool,
    },
    /// Screen test E1: upload one still test pattern (border, coloured
    /// corners, an arrow pointing up, colour ramps) to the screen's GIF page.
    /// WRITES FLASH and replaces the animation there (the factory one too);
    /// needs --yes. `--preview` only writes the exact image as PNG.
    ScreenTest {
        /// Write a PNG preview of what would be sent (4× scale) and stop.
        #[arg(long)]
        preview: Option<std::path::PathBuf>,
        #[arg(long)]
        yes: bool,
    },
    /// Upload a 240×135 GIF (at most 140 frames) to the screen's GIF page,
    /// e.g. the factory animation backup. WRITES FLASH and replaces the
    /// animation there; needs --yes. Without it, prints the summary.
    ScreenGif {
        file: std::path::PathBuf,
        /// Use this delay for every frame (ms, 2–508) instead of the file's.
        #[arg(long)]
        delay_ms: Option<u32>,
        /// Write a PNG contact sheet of what would be sent and stop.
        #[arg(long)]
        preview: Option<std::path::PathBuf>,
        #[arg(long)]
        yes: bool,
    },
    /// Set the keyboard clock to the PC's local time (`04 28`). The screen
    /// switches to its clock page.
    Clock {
        /// Send this time instead, as "YYYY-MM-DD HH:MM:SS" (for testing).
        #[arg(long)]
        at: Option<String>,
    },
    /// Store per-key colours: KEY=RRGGBB pairs (key ids from the model).
    /// Unlisted keys turn off. WRITES FLASH; needs --yes.
    PerKey {
        /// e.g. w=FF8A3D a=FF8A3D up=00AAFF
        #[arg(required = true)]
        keys: Vec<String>,
        #[arg(long, default_value_t = 5)]
        brightness: u8,
        #[arg(long)]
        yes: bool,
    },
    /// Store one keymap layer from KEY=ACTION pairs (key ids from the
    /// model). The whole layer is replaced: unlisted keys go back to the
    /// firmware default, and no pairs resets the layer. WRITES FLASH; needs
    /// --yes. The app does not see this write: it keeps its own copy.
    ///
    /// ACTION: default | off | keys joined with + (a, f13, ctrl+shift+a,
    /// lctrl) | media:ID | mouse:left|right|middle|back|forward|double |
    /// wheel:up|down | macro:SLOT[:times:N|:toggle|:hold]. `kb keymap-targets`
    /// lists the ids.
    Keymap {
        /// `base` or `fn`.
        #[arg(long, default_value = "base")]
        layer: String,
        /// e.g. kp-slash=a kp-star=media:volume-up caps=esc
        keys: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
    /// List what a key can be assigned (key, media and mouse ids).
    KeymapTargets,
    /// Store every macro: the Nth SPEC goes to slot N-1, `-` leaves a slot
    /// empty, no SPEC clears all macros. WRITES FLASH; needs --yes. Bind a
    /// macro to a key with `kb keymap KEY=macro:SLOT`. The app does not see
    /// this write.
    ///
    /// SPEC: text:ok (US layout) | combo:ctrl+shift+esc |
    /// steps:down:a,wait:50,up:a,click:left (wait sets the previous step's pause)
    Macros {
        specs: Vec<String>,
        /// Pause after each step, in ms (at least 10).
        #[arg(long, default_value_t = 20)]
        delay: u16,
        #[arg(long)]
        yes: bool,
    },
    /// Store a firmware lighting effect. WRITES FLASH; needs --yes.
    Effect {
        /// Effect id (see `kb effects`). Default: factory "Rolling".
        #[arg(long, default_value_t = 11)]
        mode: u8,
        #[arg(long, default_value = "FF0000")]
        color: String,
        /// Rainbow/colourful instead of a single colour.
        #[arg(long)]
        rainbow: bool,
        #[arg(long, default_value_t = 5)]
        brightness: u8,
        #[arg(long, default_value_t = 3)]
        speed: u8,
        #[arg(long, default_value_t = 0)]
        direction: u8,
        /// Confirm the flash write.
        #[arg(long)]
        yes: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(&cli.log))
        .with_writer(std::io::stderr)
        .init();

    match cli.cmd {
        Cmd::List => list(),
        Cmd::HidAll => hid_all(),
        Cmd::Listen { seconds } => listen(seconds),
        Cmd::Descriptors => descriptors(),
        Cmd::Battery => battery(),
        Cmd::Models => models(),
        Cmd::Peek => peek(),
        Cmd::ReadColors { repeat, interval } if repeat > 1 => {
            let (mut drv, _) = open_command()?;
            for i in 0..repeat {
                let t0 = std::time::Instant::now();
                match drv.read_colors() {
                    Ok(c) => println!("#{i}: ok, {} LED, {} ms", c.len(), t0.elapsed().as_millis()),
                    Err(e) => println!("#{i}: HATA {e}, {} ms", t0.elapsed().as_millis()),
                }
                std::thread::sleep(std::time::Duration::from_millis(interval));
            }
            Ok(())
        }
        Cmd::ReadColors { .. } => read_colors(),
        Cmd::Live { pattern, color, seconds, fps } => live(&pattern, &color, seconds, fps),
        Cmd::LiveProbe => live_probe(),
        Cmd::Effects => {
            for e in kb_protocol::sonix::EFFECTS {
                println!("{:>3}  {:<12} caps=0x{:02X}", e.id, e.name, e.caps);
            }
            Ok(())
        }
        Cmd::DirectionProbe { modes, dirs, speed, frames, yes } => {
            anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");
            direction_probe(&modes, &dirs, speed, frames)
        }
        Cmd::ScreenTest { preview, yes } => screen_test(preview.as_deref(), yes),
        Cmd::ScreenGif { file, delay_ms, preview, yes } => screen_gif(&file, delay_ms, preview.as_deref(), yes),
        Cmd::Clock { at } => {
            let time = match at {
                Some(s) => {
                    let t = chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                        .context("tarih \"YYYY-MM-DD HH:MM:SS\" biçiminde olmalı")?;
                    kb_core::clock::from_datetime(&t.and_utc())
                }
                None => kb_core::clock::now(),
            };
            let (mut drv, model) = open_command()?;
            let started = std::time::Instant::now();
            drv.set_clock(&time)?;
            let t = time;
            println!(
                "{}: saat {:04}-{:02}-{:02} {:02}:{:02}:{:02} (haftanın günü {}) gönderildi, {} ms",
                model.name,
                t.year,
                t.month,
                t.day,
                t.hour,
                t.minute,
                t.second,
                t.weekday,
                started.elapsed().as_millis()
            );
            Ok(())
        }
        Cmd::PerKey { keys, brightness, yes } => {
            anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");
            let (mut drv, model) = open_command()?;
            let geo = model.geometry.as_ref().context("modelde geometri yok")?;
            let mut colors = Vec::new();
            for pair in &keys {
                let (id, hex) = pair.split_once('=').context("KEY=RRGGBB biçimi bekleniyor")?;
                let key = geo.keys.iter().find(|k| k.id == id).with_context(|| format!("bilinmeyen tuş `{id}`"))?;
                colors.push((key.led, parse_hex(hex)?));
            }
            let started = std::time::Instant::now();
            drv.apply_per_key(colors.iter().copied(), brightness)?;
            println!(
                "{}: {} tuşun rengi kaydedildi (sağlama toplamı doğrulandı), {} ms",
                model.name,
                colors.len(),
                started.elapsed().as_millis()
            );
            Ok(())
        }
        Cmd::Keymap { layer, keys, yes } => {
            anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");
            keymap(&layer, &keys)
        }
        Cmd::KeymapTargets => {
            keymap_targets();
            Ok(())
        }
        Cmd::Macros { specs, delay, yes } => {
            anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");
            macros(&specs, delay)
        }
        Cmd::Effect { mode, color, rainbow, brightness, speed, direction, yes } => {
            anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");
            let settings = kb_protocol::sonix::EffectSettings {
                mode,
                color: parse_hex(&color)?,
                rainbow,
                brightness,
                speed,
                direction,
            };
            let (mut drv, model) = open_command()?;
            let started = std::time::Instant::now();
            drv.apply_effect(&settings)?;
            let name = kb_protocol::sonix::effect(mode).map_or("?", |e| e.name);
            println!("{}: efekt {mode} ({name}) kaydedildi, {} ms", model.name, started.elapsed().as_millis());
            Ok(())
        }
    }
}

fn keymap(layer: &str, pairs: &[String]) -> Result<()> {
    use kb_protocol::sonix::keymap::Layer;
    let layer = match layer {
        "base" => Layer::Base,
        "fn" => Layer::Fn,
        other => anyhow::bail!("katman `base` ya da `fn` olmalı, `{other}` değil"),
    };
    let (mut drv, model) = open_command()?;
    let geo = model.geometry.as_ref().context("modelde geometri yok")?;
    let mut entries = Vec::new();
    for pair in pairs {
        let (id, action) = pair.split_once('=').context("KEY=ACTION biçimi bekleniyor")?;
        let key = geo.keys.iter().find(|k| k.id == id).with_context(|| format!("bilinmeyen tuş `{id}`"))?;
        entries.push((key.led, parse_key_action(action)?));
    }
    let map = kb_core::keymap::canonical(model, layer, entries)?;
    let started = std::time::Instant::now();
    drv.apply_keymap(layer, map.iter().map(|(&l, &a)| (l, a)))?;
    let name = |led: u8| geo.keys.iter().find(|k| k.led == led).map_or("?", |k| k.id.as_str());
    println!(
        "{}: {:?} katmanı yazıldı (sağlama toplamı doğrulandı), {} ms",
        model.name,
        layer,
        started.elapsed().as_millis()
    );
    if map.is_empty() {
        println!("  bütün tuşlar varsayılan");
    }
    for (led, action) in &map {
        println!("  {:<10} → {action:?}", name(*led));
    }
    Ok(())
}

fn macros(specs: &[String], delay: u16) -> Result<()> {
    use kb_protocol::sonix::macros::{budget_used, macro_image, BUDGET};
    let slots: Vec<_> =
        specs.iter().map(|s| if s == "-" { Ok(Vec::new()) } else { parse_macro(s, delay) }).collect::<Result<_>>()?;
    let pages = macro_image(&slots)?.len() / 64;
    let (mut drv, model) = open_command()?;
    let started = std::time::Instant::now();
    drv.write_macros(&slots)?;
    println!(
        "{}: {} makro yuvası yazıldı ({} sayfa, bellek {}/{} bayt, sağlama toplamı doğrulandı), {} ms",
        model.name,
        slots.len(),
        pages,
        budget_used(&slots),
        BUDGET,
        started.elapsed().as_millis()
    );
    for (i, steps) in slots.iter().enumerate() {
        let total: u32 = steps.iter().map(|s| s.delay_ms.max(10) as u32).sum();
        println!("  {i:>2}: {} adım, {total} ms", steps.len());
    }
    Ok(())
}

/// `text:…`, `combo:keys` or `steps:down:K,up:K,tap:K,click:BUTTON,wait:MS`.
fn parse_macro(spec: &str, delay: u16) -> Result<Vec<kb_protocol::sonix::macros::MacroStep>> {
    use kb_protocol::sonix::keymap::{usage_by_id, KeyAction};
    use kb_protocol::sonix::macros::{mouse, text_steps, MacroStep, StepKind};
    let (kind, rest) = spec.split_once(':').context("makro `text:`, `combo:` ya da `steps:` ile başlamalı")?;
    let key = |id: &str| usage_by_id(id).map(|u| u.code).with_context(|| format!("bilinmeyen tuş `{id}`"));
    let step = |k, code| MacroStep::new(k, code, delay);
    match kind {
        "text" => Ok(text_steps(rest, delay)),
        "combo" => {
            let KeyAction::Key { mods, code } = parse_key_action(rest)? else {
                anyhow::bail!("combo yalnızca tuşlardan oluşur")
            };
            let held: Vec<u8> = (0..8).filter(|b| mods & (1 << b) != 0).map(|b| 0xE0 + b).collect();
            let mut steps: Vec<_> = held.iter().map(|&c| step(StepKind::KeyDown, c)).collect();
            if code != 0 {
                steps.extend([step(StepKind::KeyDown, code), step(StepKind::KeyUp, code)]);
            }
            steps.extend(held.iter().rev().map(|&c| step(StepKind::KeyUp, c)));
            Ok(steps)
        }
        "steps" => {
            let mut steps: Vec<MacroStep> = Vec::new();
            for part in rest.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                let (verb, arg) = part.split_once(':').with_context(|| format!("adım `{part}` anlaşılmadı"))?;
                match verb {
                    "down" => steps.push(step(StepKind::KeyDown, key(arg)?)),
                    "up" => steps.push(step(StepKind::KeyUp, key(arg)?)),
                    "tap" => steps.extend([step(StepKind::KeyDown, key(arg)?), step(StepKind::KeyUp, key(arg)?)]),
                    "click" => {
                        let b = match arg {
                            "left" => mouse::LEFT,
                            "right" => mouse::RIGHT,
                            "middle" => mouse::MIDDLE,
                            _ => anyhow::bail!("bilinmeyen fare tuşu `{arg}`"),
                        };
                        steps.extend([step(StepKind::MouseDown, b), step(StepKind::MouseUp, b)]);
                    }
                    "wait" => {
                        let last = steps.last_mut().context("`wait` bir adımdan sonra gelmeli")?;
                        last.delay_ms = arg.parse().context("bekleme ms olarak yazılmalı")?;
                    }
                    _ => anyhow::bail!("bilinmeyen adım `{verb}`"),
                }
            }
            Ok(steps)
        }
        _ => anyhow::bail!("makro `text:`, `combo:` ya da `steps:` ile başlamalı"),
    }
}

/// `default`, `off`, `media:ID`, `mouse:BUTTON`, `wheel:up|down`,
/// `macro:SLOT[:times:N|:toggle|:hold]`, or keys joined with `+` (at most
/// one non-modifier).
fn parse_key_action(s: &str) -> Result<kb_protocol::sonix::keymap::KeyAction> {
    use kb_protocol::sonix::keymap::{consumer_by_id, mods, usage_by_id, KeyAction, MacroPlay, MouseButton};
    let s = s.trim().to_ascii_lowercase();
    if s == "default" {
        return Ok(KeyAction::Default);
    }
    if s == "off" {
        return Ok(KeyAction::Disabled);
    }
    if let Some(id) = s.strip_prefix("media:") {
        let c = consumer_by_id(id).with_context(|| format!("bilinmeyen medya işlevi `{id}`"))?;
        return Ok(KeyAction::Consumer(c.code));
    }
    if let Some(id) = s.strip_prefix("mouse:") {
        let b = match id {
            "left" => MouseButton::Left,
            "right" => MouseButton::Right,
            "middle" => MouseButton::Middle,
            "back" => MouseButton::Back,
            "forward" => MouseButton::Forward,
            "double" => MouseButton::DoubleClick,
            _ => anyhow::bail!("bilinmeyen fare tuşu `{id}`"),
        };
        return Ok(KeyAction::Mouse(b));
    }
    if let Some(rest) = s.strip_prefix("macro:") {
        let mut parts = rest.split(':');
        let slot: u8 = parts.next().unwrap_or("").parse().context("makro yuvası sayı olmalı (0–99)")?;
        let play = match (parts.next(), parts.next()) {
            (None | Some("once"), None) => MacroPlay::Once,
            (Some("times"), Some(n)) => MacroPlay::Times(n.parse().context("tekrar sayısı 1–255 olmalı")?),
            (Some("toggle"), None) => MacroPlay::Toggle,
            (Some("hold"), None) => MacroPlay::WhileHeld,
            _ => anyhow::bail!("oynatma biçimi: once, times:N, toggle ya da hold"),
        };
        let action = KeyAction::Macro { slot, play };
        action.encode()?;
        return Ok(action);
    }
    if let Some(dir) = s.strip_prefix("wheel:") {
        return match dir {
            "up" => Ok(KeyAction::Wheel { up: true }),
            "down" => Ok(KeyAction::Wheel { up: false }),
            _ => anyhow::bail!("tekerlek yönü `up` ya da `down` olmalı"),
        };
    }
    let mut m = 0u8;
    let mut code = None;
    for token in s.split('+') {
        let bit = match token {
            "ctrl" => Some(mods::LEFT_CTRL),
            "shift" => Some(mods::LEFT_SHIFT),
            "alt" => Some(mods::LEFT_ALT),
            "win" => Some(mods::LEFT_GUI),
            _ => None,
        };
        if let Some(bit) = bit {
            m |= bit;
            continue;
        }
        let u = usage_by_id(token).with_context(|| format!("bilinmeyen tuş `{token}` (kb keymap-targets)"))?;
        match KeyAction::key(u.code) {
            KeyAction::Key { mods: bit, code: 0 } => m |= bit,
            _ => {
                anyhow::ensure!(code.is_none(), "bir atamada en fazla bir normal tuş olabilir");
                code = Some(u.code);
            }
        }
    }
    let action = KeyAction::Key { mods: m, code: code.unwrap_or(0) };
    action.encode()?;
    Ok(action)
}

fn keymap_targets() {
    use kb_protocol::sonix::keymap::{CONSUMER, USAGES};
    println!("Tuşlar (KEY ya da ctrl/shift/alt/win+KEY):");
    for u in USAGES {
        println!("  {:<14} {:<12} 0x{:02X}", u.id, u.label, u.code);
    }
    println!("Medya (media:ID):");
    for c in CONSUMER {
        println!("  {:<18} 0x{:04X}", c.id, c.code);
    }
    println!("Fare: mouse:left|right|middle|back|forward|double, wheel:up|down");
    println!("Makro: macro:SLOT (bir kez), macro:SLOT:times:N, macro:SLOT:toggle, macro:SLOT:hold");
    println!("Diğer: default (firmware varsayılanı), off (devre dışı)");
}

/// Refuse to talk to the keyboard while another program does: the ones the
/// app watches for (kb_core::conflicts) and the app itself. Two programs'
/// packets mix in one session, and a broken flash write does not recover.
fn ensure_alone() -> Result<()> {
    let programs = kb_winsys::running_programs();
    let own = std::process::id();
    let running = programs.iter().map(|p| (p.pid, p.name.as_str(), p.path.as_deref()));
    if let Some(c) = kb_core::conflicts::find_conflict(running, own) {
        anyhow::bail!("Klavyeyi başka bir program kullanıyor ({}). Önce onu kapatın.", c.id());
    }
    if programs.iter().any(|p| p.pid != own && p.name.eq_ignore_ascii_case("keymotion.exe")) {
        anyhow::bail!("Keymotion açık. Önce ondan çıkın (saatin yanındaki simge › Çık).");
    }
    Ok(())
}

/// Open the wired command channel of the first supported keyboard.
fn open_command() -> Result<(kb_core::sonix::SonixFf13<kb_transport::hid::HidChannel>, &'static kb_devices::Model)> {
    ensure_alone()?;
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let hid = Hid::new()?;
    for iface in hid.interfaces() {
        let Some(m) = match_interface(models, &probe(&iface)) else { continue };
        if m.role == InterfaceRole::Command && m.mode.kind == kb_devices::ConnectionKind::Usb {
            let ch = hid.open(&iface).context("komut kanalı açılamadı")?;
            let desc = ch.report_descriptor().context("komut kanalının tanımlayıcısı okunamadı")?;
            anyhow::ensure!(
                kb_core::shape::is_command_shape(&desc),
                "komut kanalının rapor düzeni beklenen \"04 xx\" ailesinin değil; hiçbir şey gönderilmedi"
            );
            return Ok((kb_core::sonix::SonixFf13::new(ch), m.model));
        }
    }
    anyhow::bail!("USB modunda klavye yok. Kablo takılı ve arkadaki anahtar ortada (USB) olmalı.")
}

/// Open the wired display channel (FF68) of the first supported keyboard.
fn open_display() -> Result<kb_transport::hid::HidChannel> {
    ensure_alone()?;
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let hid = Hid::new()?;
    for iface in hid.interfaces() {
        let Some(m) = match_interface(models, &probe(&iface)) else { continue };
        if m.role == InterfaceRole::Display && m.mode.kind == kb_devices::ConnectionKind::Usb {
            anyhow::ensure!(iface.usage_page == hid::DISPLAY_USAGE_PAGE, "ekran kanalı beklenmeyen usage page");
            let ch = hid.open(&iface).context("ekran kanalı açılamadı")?;
            let desc = ch.report_descriptor().context("ekran kanalının tanımlayıcısı okunamadı")?;
            anyhow::ensure!(
                kb_core::shape::is_display_shape(&desc),
                "ekran kanalının rapor düzeni beklenen gibi değil; hiçbir şey gönderilmedi"
            );
            return Ok(ch);
        }
    }
    anyhow::bail!("USB modunda ekran kanalı yok. Kablo takılı ve arkadaki anahtar ortada (USB) olmalı.")
}

/// PNG of a frame exactly as the panel gets it (RGB565 expanded), scaled.
fn frame_png(frame: &kb_protocol::sonix::screen::Frame, scale: usize) -> Vec<u8> {
    use kb_protocol::sonix::screen::{rgb888, HEIGHT, WIDTH};
    let (w, h) = (WIDTH * scale, HEIGHT * scale);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            rgb.extend_from_slice(&rgb888(frame.get(x / scale, y / scale)));
        }
    }
    png::encode_rgb(w as u32, h as u32, &rgb)
}

fn screen_test(preview: Option<&std::path::Path>, yes: bool) -> Result<()> {
    use kb_protocol::sonix::screen::{self, ScreenAnimation};
    let frame = screen::test_pattern();
    if let Some(path) = preview {
        std::fs::write(path, frame_png(&frame, 4)).context("önizleme yazılamadı")?;
        println!("Önizleme yazıldı: {}", path.display());
        return Ok(());
    }
    let anim = ScreenAnimation::still(frame);
    println!(
        "Test deseni: 1 kare, {} sayfa (~{:.1} s). Ekrandaki GIF sayfasının yerine geçer; fabrika animasyonu silinir.",
        anim.pages(),
        anim.pages() as f32 * 0.145
    );
    anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");

    upload(&anim)
}

fn screen_gif(
    file: &std::path::Path,
    delay_ms: Option<u32>,
    preview: Option<&std::path::Path>,
    yes: bool,
) -> Result<()> {
    use kb_core::screen_import::{decode_gif, exact_size_animation};
    let bytes = std::fs::read(file).context("dosya okunamadı")?;
    let decoded = decode_gif(&bytes)?;
    let anim = exact_size_animation(&decoded, delay_ms)?;
    let frames = anim.frames();
    println!(
        "{}: {}×{}, {} kare, bir tur {:.1} s, {} sayfa (~{:.0} s yükleme)",
        file.display(),
        decoded.width,
        decoded.height,
        frames.len(),
        anim.loop_ms() as f32 / 1000.0,
        anim.pages(),
        anim.pages() as f32 * 0.145
    );
    if let Some(path) = preview {
        let step = frames.len().div_ceil(16).max(1);
        let picked: Vec<_> = frames.iter().step_by(step).map(|(f, _)| f).collect();
        std::fs::write(path, sheet_png(&picked, 4)).context("önizleme yazılamadı")?;
        println!("Önizleme ({} kare) yazıldı: {}", picked.len(), path.display());
        return Ok(());
    }
    anyhow::ensure!(yes, "Bu komut klavyenin flash belleğine yazar. Onaylamak için --yes ekleyin.");
    upload(&anim)
}

/// Frames tiled `cols` per row, exactly as the panel gets them.
fn sheet_png(frames: &[&kb_protocol::sonix::screen::Frame], cols: usize) -> Vec<u8> {
    use kb_protocol::sonix::screen::{rgb888, HEIGHT, WIDTH};
    let gap = 4;
    let rows = frames.len().div_ceil(cols).max(1);
    let (w, h) = (cols * (WIDTH + gap), rows * (HEIGHT + gap));
    let mut rgb = vec![0x40u8; w * h * 3];
    for (k, f) in frames.iter().enumerate() {
        let (ox, oy) = ((k % cols) * (WIDTH + gap), (k / cols) * (HEIGHT + gap));
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let d = ((oy + y) * w + ox + x) * 3;
                rgb[d..d + 3].copy_from_slice(&rgb888(f.get(x, y)));
            }
        }
    }
    png::encode_rgb(w as u32, h as u32, &rgb)
}

/// Upload with progress, then print what was measured.
fn upload(anim: &kb_protocol::sonix::screen::ScreenAnimation) -> Result<()> {
    let (mut drv, model) = open_command()?;
    let mut display = open_display()?;
    let started = std::time::Instant::now();
    let report = drv.upload_screen(&mut display, anim, |done, total| {
        if done == 1 || done % 50 == 0 || done == total {
            eprintln!("  sayfa {done}/{total} ({} s)", started.elapsed().as_secs());
        }
    })?;
    let ms: Vec<u128> = report.page_times.iter().map(|d| d.as_millis()).collect();
    let avg = ms.iter().sum::<u128>() / ms.len().max(1) as u128;
    println!("{}: yükleme bitti, {} ms", model.name, started.elapsed().as_millis());
    println!(
        "  sayfa süresi: en kısa {} ms, ortalama {avg} ms, en uzun {} ms",
        ms.iter().min().unwrap_or(&0),
        ms.iter().max().unwrap_or(&0)
    );
    if ms.len() <= 32 {
        println!("  sayfa süreleri (ms): {ms:?}");
    }
    println!("  önceden kuyrukta: {} rapor; 04 72 durumu: {:02X}", report.drained, report.announce_status);
    println!("  ilk sayfa onayı: {}", hid::hex(&report.first_ack[..report.first_ack.len().min(16)]));
    println!("  04 02 yankısı: {}", hid::hex(&report.commit_echo));
    println!("  gönderilen toplam: sum16 {:04X}, sum32 {:08X}", report.sum16, report.sum32);
    Ok(())
}

fn peek() -> Result<()> {
    let (mut drv, model) = open_command()?;
    let r = drv.peek()?;
    println!("{}: feature raporu ({} bayt)\n  {}", model.name, r.len(), hid::hex(&r));
    Ok(())
}

fn read_colors() -> Result<()> {
    let (mut drv, model) = open_command()?;
    let colors = drv.read_colors()?;
    let names: std::collections::HashMap<u8, &str> =
        model.geometry.iter().flat_map(|g| g.keys.iter().map(|k| (k.led, k.id.as_str()))).collect();
    println!("{}: {} LED rengi okundu (ham; kırmızı bayt 2·r mod 256)", model.name, colors.len());
    for (led, [r, g, b]) in colors {
        println!("  {:>3} {:<10} #{r:02X}{g:02X}{b:02X}", led, names.get(&led).copied().unwrap_or("?"));
    }
    Ok(())
}

fn parse_hex(s: &str) -> Result<[u8; 3]> {
    let s = s.trim_start_matches('#');
    anyhow::ensure!(s.len() == 6, "renk RRGGBB biçiminde olmalı");
    let v = u32::from_str_radix(s, 16)?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

fn live(pattern: &str, color: &str, seconds: f32, fps: u32) -> Result<()> {
    let (mut drv, model) = open_command()?;
    let geo = model.geometry.as_ref().context("modelde geometri yok")?;
    let solid = parse_hex(color)?;
    let frame_time = std::time::Duration::from_secs_f32(1.0 / fps.clamp(1, 60) as f32);
    let start = std::time::Instant::now();
    let (mut frames, mut errors) = (0u32, 0u32);
    println!("{}: {pattern} akışı {seconds} sn, {fps} fps (yalnızca RAM)", model.name);
    while start.elapsed().as_secs_f32() < seconds {
        let t0 = std::time::Instant::now();
        let t = start.elapsed().as_secs_f32();
        // `test`: pure reference colours on Esc..F7, everything else off.
        const TEST: [(&str, [u8; 3]); 8] = [
            ("esc", [0xFF, 0, 0]),
            ("f1", [0, 0xFF, 0]),
            ("f2", [0, 0, 0xFF]),
            ("f3", [0xFF, 0xFF, 0xFF]),
            ("f4", [0xFF, 0xFF, 0]),
            ("f5", [0, 0xFF, 0xFF]),
            ("f6", [0xFF, 0, 0xFF]),
            ("f7", [0xFF, 0x8A, 0x3D]),
        ];
        // `gamma`: the UI palette raw on 1–6, gamma-corrected on Q–Y.
        const PALETTE: [[u8; 3]; 6] = [
            [0x8B, 0x5C, 0xF6],
            [0xFF, 0x4F, 0xA3],
            [0x38, 0xB6, 0xFF],
            [0xA8, 0xE4, 0x3A],
            [0x34, 0xC7, 0x59],
            [0xFF, 0x8A, 0x3D],
        ];
        let gamma_row = |id: &str| -> Option<[u8; 3]> {
            let raw = ["1", "2", "3", "4", "5", "6"].iter().position(|r| *r == id);
            let fixed = ["q", "w", "e", "r", "t", "y"].iter().position(|r| *r == id);
            raw.map(|i| PALETTE[i]).or_else(|| fixed.map(|i| kb_core::color::LedCalibration::DEFAULT.apply(PALETTE[i])))
        };
        let leds = geo.keys.iter().map(|k| {
            let rgb = match pattern {
                "solid" => solid,
                "test" => TEST.iter().find(|(id, _)| *id == k.id).map_or([0, 0, 0], |(_, c)| *c),
                "gamma" => gamma_row(&k.id).unwrap_or([0, 0, 0]),
                _ => hsv((k.x + k.w / 2.0) / geo.width * 360.0 - t * 140.0, 1.0, 1.0),
            };
            (k.led, rgb)
        });
        match drv.realtime_frame(leds) {
            Ok(()) => frames += 1,
            Err(e) => {
                errors += 1;
                if errors <= 3 {
                    eprintln!("kare hatası: {e}");
                }
                if errors > 20 {
                    anyhow::bail!("çok fazla hata, akış durduruldu");
                }
            }
        }
        if let Some(rest) = frame_time.checked_sub(t0.elapsed()) {
            std::thread::sleep(rest);
        }
    }
    let secs = start.elapsed().as_secs_f32();
    println!("{frames} kare gönderildi, {errors} hata, ortalama {:.1} fps", frames as f32 / secs);
    Ok(())
}

/// Five phases of 0.6 s each, with an `04 F5` read at the end of each:
/// A every key blue; B only Esc red (other keys not given); C Esc red and
/// every other key sent explicitly as black; D every key grey #808080 and
/// E violet #8B5CF6, which show how F5 reports red (`2·r mod 256`).
fn live_probe() -> Result<()> {
    let (mut drv, model) = open_command()?;
    let geo = model.geometry.as_ref().context("modelde geometri yok")?;
    let esc = geo.keys.iter().find(|k| k.id == "esc").context("esc yok")?.led;
    let keys: Vec<u8> = geo.keys.iter().map(|k| k.led).collect();
    type Frame = Vec<(u8, [u8; 3])>;
    let phases: [(&str, Frame); 5] = [
        ("A: tüm tuşlar mavi", keys.iter().map(|&l| (l, [0, 0, 255])).collect()),
        ("B: yalnızca Esc kırmızı (diğer tuşlar verilmedi)", vec![(esc, [255, 0, 0])]),
        (
            "C: Esc kırmızı, diğerleri açıkça siyah",
            keys.iter().map(|&l| (l, if l == esc { [255, 0, 0] } else { [0, 0, 0] })).collect(),
        ),
        ("D: tüm tuşlar gri #808080", keys.iter().map(|&l| (l, [0x80, 0x80, 0x80])).collect()),
        ("E: tüm tuşlar mor #8B5CF6", keys.iter().map(|&l| (l, [0x8B, 0x5C, 0xF6])).collect()),
    ];
    for (name, frame) in phases {
        let start = std::time::Instant::now();
        let mut sent = 0;
        while start.elapsed() < std::time::Duration::from_millis(600) {
            drv.realtime_frame(frame.iter().copied())?;
            sent += 1;
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let read = drv.read_colors()?;
        let shown: std::collections::HashMap<u8, [u8; 3]> = read.into_iter().collect();
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for l in &keys {
            let [r, g, b] = shown.get(l).copied().unwrap_or([0, 0, 0]);
            *counts.entry(format!("#{r:02X}{g:02X}{b:02X}")).or_default() += 1;
        }
        let mut top: Vec<_> = counts.into_iter().collect();
        top.sort_by_key(|e| std::cmp::Reverse(e.1));
        let [r, g, b] = shown.get(&esc).copied().unwrap_or([0, 0, 0]);
        println!("{name}: {sent} kare; F5 → Esc #{r:02X}{g:02X}{b:02X}, tuşlar {top:?}");
        // Keys that did not take the colour this frame gave them.
        let off: Vec<String> = geo
            .keys
            .iter()
            .filter_map(|k| {
                let want = frame.iter().find(|(l, _)| *l == k.led).map(|(_, c)| *c);
                let got = shown.get(&k.led).copied().unwrap_or([0, 0, 0]);
                // F5 reports red as 2·r mod 256: either value of the byte counts.
                let red = kb_protocol::sonix::readback_red_candidates(got[0]);
                let close = |w: [u8; 3]| {
                    red.iter().any(|r| r.abs_diff(w[0]) <= 4)
                        && w[1].abs_diff(got[1]) <= 4
                        && w[2].abs_diff(got[2]) <= 4
                };
                match want {
                    Some(w) if !close(w) => {
                        Some(format!("{}(led {}) #{:02X}{:02X}{:02X}", k.id, k.led, got[0], got[1], got[2]))
                    }
                    _ => None,
                }
            })
            .collect();
        if !off.is_empty() {
            println!("   karedeki rengi almayan tuşlar: {}", off.join(", "));
        }
        // LEDs F5 reports that are not keys of the model.
        let extra: Vec<String> = shown
            .iter()
            .filter(|(l, c)| !keys.contains(l) && **c != [0, 0, 0])
            .map(|(l, [r, g, b])| format!("led {l} #{r:02X}{g:02X}{b:02X}"))
            .collect();
        if !extra.is_empty() {
            println!("   modelde olmayan LED'ler: {}", extra.join(", "));
        }
    }
    println!("Akış bitti; klavye kayıtlı ışığına döner.");
    Ok(())
}

/// See `Cmd::DirectionProbe`.
fn direction_probe(modes: &[u8], dirs: &[u8], speed: u8, frames: usize) -> Result<()> {
    use motion::{axis_motion, Axis, Frame, KeyPos};
    let (mut drv, model) = open_command()?;
    let geo = model.geometry.as_ref().context("modelde geometri yok")?;
    let keys: Vec<KeyPos> =
        geo.keys.iter().map(|k| KeyPos { led: k.led, x: k.x + k.w / 2.0, y: k.y + k.h / 2.0 }).collect();
    let median = |mut v: Vec<f32>| {
        v.sort_by(f32::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(f32::NAN)
    };
    for &mode in modes {
        for &direction in dirs {
            let settings = kb_protocol::sonix::EffectSettings {
                mode,
                color: [0xFF, 0, 0],
                rainbow: true,
                brightness: 5,
                speed,
                direction,
            };
            drv.apply_effect(&settings)?;
            std::thread::sleep(std::time::Duration::from_millis(1200));
            let start = std::time::Instant::now();
            let mut shots: Vec<(f32, Frame)> = Vec::new();
            for _ in 0..frames {
                let t0 = start.elapsed().as_secs_f32();
                let read = drv.read_colors()?;
                let t1 = start.elapsed().as_secs_f32();
                shots.push(((t0 + t1) / 2.0, read.into_iter().collect()));
            }
            let mut line = format!("mod {mode:>2} yön {direction}:");
            for (axis, name, unit, max) in
                [(Axis::X, "x", "u/s", 3.0), (Axis::Y, "y", "u/s", 2.0), (Axis::Angle, "açı", "°/s", 60.0)]
            {
                let (mut speeds, mut confs) = (Vec::new(), Vec::new());
                for pair in shots.windows(2) {
                    let dt = pair[1].0 - pair[0].0;
                    if let Some(m) = axis_motion(&keys, &pair[0].1, &pair[1].1, dt, axis, max) {
                        speeds.push(m.speed);
                        confs.push(m.confidence);
                    }
                }
                line += &format!("  {name} {:+6.1} {unit} (güven {:4.1})", median(speeds), median(confs));
            }
            // Two-dimensional match, for diagonal and sparse patterns.
            let (mut vx, mut vy, mut c2) = (Vec::new(), Vec::new(), Vec::new());
            for pair in shots.windows(2) {
                if let Some((x, y, c)) = motion::motion_2d(&keys, &pair[0].1, &pair[1].1, pair[1].0 - pair[0].0, 2.0) {
                    vx.push(x);
                    vy.push(y);
                    c2.push(c);
                }
            }
            line += &format!("  2B ({:+5.1}, {:+5.1}) u/s (güven {:4.1})", median(vx), median(vy), median(c2));
            println!("{line}");
        }
    }
    println!("Bitti. Klavye son denenen efekte kaldı; kendi ışığınızı geri yükleyin.");
    Ok(())
}

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

fn role_label(role: InterfaceRole) -> &'static str {
    match role {
        InterfaceRole::Command => "KOMUT",
        InterfaceRole::Display => "EKRAN",
        InterfaceRole::Standard => "standart",
    }
}

fn list() -> Result<()> {
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let hid = Hid::new().context("HID alt sistemi açılamadı")?;
    let mut found = 0;
    for iface in hid.interfaces() {
        let Some(m) = match_interface(models, &probe(&iface)) else { continue };
        found += 1;
        println!(
            "{:<18} {:<11} {:<9} {:04X}:{:04X} UP=0x{:04X} U=0x{:04X} if={:<2} {}",
            m.model.name,
            m.mode.kind.label_tr(),
            role_label(m.role),
            iface.vid,
            iface.pid,
            iface.usage_page,
            iface.usage,
            iface.interface_number,
            iface.product.as_deref().unwrap_or("-"),
        );
    }
    if found == 0 {
        println!("Desteklenen bir klavye bulunamadı.");
    }

    #[cfg(windows)]
    for dev in kb_transport::ble::paired_devices().unwrap_or_default() {
        if let Some((model, _)) = match_ble_name(models, &dev.name) {
            println!(
                "{:<18} {:<11} {} ({}){}",
                model.name,
                "Bluetooth",
                dev.name,
                dev.address_string(),
                if dev.connected { " bağlı" } else { " bağlı değil" }
            );
        }
    }
    Ok(())
}

/// Listen to the wired keyboard's vendor collections. Only input reports are
/// read; nothing is written and no feature report is requested. The screen
/// collection (FF68) is left alone.
fn listen(seconds: u64) -> Result<()> {
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let hid = Hid::new()?;
    let mut channels = Vec::new();
    for iface in hid.interfaces() {
        let Some(m) = match_interface(models, &probe(&iface)) else { continue };
        if m.mode.kind != kb_devices::ConnectionKind::Usb
            || !iface.is_vendor_defined()
            || iface.usage_page == hid::DISPLAY_USAGE_PAGE
        {
            continue;
        }
        match hid.open(&iface) {
            Ok(ch) => {
                println!("dinleniyor: UP=0x{:04X} U=0x{:04X}", iface.usage_page, iface.usage);
                channels.push(ch);
            }
            Err(e) => println!("açılamadı UP=0x{:04X}: {e}", iface.usage_page),
        }
    }
    anyhow::ensure!(!channels.is_empty(), "USB'de üreticiye özel arayüz bulunamadı");
    let start = std::time::Instant::now();
    let mut count = 0;
    while start.elapsed().as_secs() < seconds {
        for ch in &channels {
            match ch.read_input(65, 20) {
                Ok(r) => {
                    count += 1;
                    println!(
                        "{:>6} ms UP=0x{:04X} {}",
                        start.elapsed().as_millis(),
                        ch.info().usage_page,
                        hid::hex(&r)
                    );
                }
                Err(kb_transport::TransportError::Timeout(_)) => {}
                Err(e) => println!("okuma hatası UP=0x{:04X}: {e}", ch.info().usage_page),
            }
        }
    }
    println!("{seconds} sn içinde {count} rapor geldi.");
    Ok(())
}

fn hid_all() -> Result<()> {
    let hid = Hid::new()?;
    let mut all = hid.interfaces();
    all.sort_by_key(|i| (i.vid, i.pid, i.usage_page, i.usage));
    for i in all {
        println!(
            "{:04X}:{:04X} rel={:04X} UP=0x{:04X} U=0x{:04X} if={:<2} bus={:<9} {:?} / {:?}",
            i.vid,
            i.pid,
            i.release,
            i.usage_page,
            i.usage,
            i.interface_number,
            format!("{:?}", i.bus),
            i.manufacturer.as_deref().unwrap_or(""),
            i.product.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

fn descriptors() -> Result<()> {
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let hid = Hid::new()?;
    let mut any = false;
    for iface in hid.interfaces() {
        let Some(m) = match_interface(models, &probe(&iface)) else { continue };
        if m.role == InterfaceRole::Standard && !iface.is_vendor_defined() {
            continue;
        }
        any = true;
        print!("{} {} UP=0x{:04X}: ", m.mode.kind.label_tr(), role_label(m.role), iface.usage_page);
        match hid.open(&iface).and_then(|ch| ch.report_descriptor()) {
            Ok(d) => println!("{} bayt\n    {}", d.len(), hid::hex(&d)),
            Err(e) => println!("okunamadı: {e}"),
        }
    }
    if !any {
        println!("Üreticiye özel arayüz bulunamadı. Klavye USB kabloyla ve mod anahtarı USB konumunda mı?");
    }
    Ok(())
}

#[cfg(windows)]
fn battery() -> Result<()> {
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut any = false;
    for dev in kb_transport::ble::paired_devices()? {
        if match_ble_name(models, &dev.name).is_none() {
            continue;
        }
        any = true;
        match kb_transport::ble::read_battery(dev.address) {
            Ok(pct) => println!("{} ({}): pil %{pct}", dev.name, dev.address_string()),
            Err(e) => println!("{} ({}): pil okunamadı: {e}", dev.name, dev.address_string()),
        }
    }
    if !any {
        println!("Eşleştirilmiş desteklenen bir Bluetooth klavye yok.");
    }
    Ok(())
}

#[cfg(not(windows))]
fn battery() -> Result<()> {
    anyhow::bail!("Bluetooth pil okuma şimdilik yalnızca Windows'ta destekleniyor")
}

fn models() -> Result<()> {
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    for m in models {
        println!("{} ({}) — protokol {}, {} tuş, düzen %{}", m.name, m.id, m.protocol, m.key_count, m.layout);
        if let Some(s) = &m.screen {
            println!(
                "  ekran {}x{} {}{}",
                s.width,
                s.height,
                s.pixel_format,
                if s.verified { "" } else { " (doğrulanmadı)" }
            );
        }
        for mode in &m.modes {
            println!(
                "  {:<11} {:04X}:{:04X} {:?}{}",
                mode.kind.label_tr(),
                mode.vid,
                mode.pid,
                mode.capabilities,
                if mode.verified { "" } else { " (doğrulanmadı)" }
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_key_action;
    use kb_protocol::sonix::keymap::{mods, KeyAction, MacroPlay, MouseButton};

    #[test]
    fn key_actions_parse() {
        assert_eq!(parse_key_action("A").unwrap(), KeyAction::key(0x04));
        assert_eq!(parse_key_action("f13").unwrap(), KeyAction::key(0x68));
        assert_eq!(
            parse_key_action("ctrl+shift+a").unwrap(),
            KeyAction::Key { mods: mods::LEFT_CTRL | mods::LEFT_SHIFT, code: 0x04 }
        );
        assert_eq!(parse_key_action("lctrl").unwrap(), KeyAction::Key { mods: mods::LEFT_CTRL, code: 0 });
        assert_eq!(parse_key_action("rshift+esc").unwrap(), KeyAction::Key { mods: mods::RIGHT_SHIFT, code: 0x29 });
        assert_eq!(parse_key_action("media:volume-up").unwrap(), KeyAction::Consumer(0xE9));
        assert_eq!(parse_key_action("mouse:back").unwrap(), KeyAction::Mouse(MouseButton::Back));
        assert_eq!(parse_key_action("wheel:down").unwrap(), KeyAction::Wheel { up: false });
        assert_eq!(parse_key_action("off").unwrap(), KeyAction::Disabled);
        assert_eq!(parse_key_action("default").unwrap(), KeyAction::Default);
        assert!(parse_key_action("a+b").is_err(), "two normal keys");
        assert!(parse_key_action("fn").is_err());
        assert!(parse_key_action("media:nope").is_err());
        assert_eq!(parse_key_action("macro:0").unwrap(), KeyAction::Macro { slot: 0, play: MacroPlay::Once });
        assert_eq!(
            parse_key_action("macro:3:times:5").unwrap(),
            KeyAction::Macro { slot: 3, play: MacroPlay::Times(5) }
        );
        assert_eq!(parse_key_action("macro:1:hold").unwrap(), KeyAction::Macro { slot: 1, play: MacroPlay::WhileHeld });
        assert!(parse_key_action("macro:100").is_err());
        assert!(parse_key_action("macro:0:times:0").is_err());
    }

    #[test]
    fn macro_specs_parse() {
        use kb_protocol::sonix::macros::StepKind::*;
        let steps =
            |s| super::parse_macro(s, 20).unwrap().iter().map(|x| (x.kind, x.code, x.delay_ms)).collect::<Vec<_>>();
        assert_eq!(
            steps("combo:ctrl+c"),
            vec![(KeyDown, 0xE0, 20), (KeyDown, 0x06, 20), (KeyUp, 0x06, 20), (KeyUp, 0xE0, 20)]
        );
        assert_eq!(
            steps("steps:tap:a,wait:100,click:left"),
            vec![(KeyDown, 0x04, 20), (KeyUp, 0x04, 100), (MouseDown, 1, 20), (MouseUp, 1, 20)]
        );
        assert_eq!(steps("text:ok").len(), 4);
        assert!(super::parse_macro("steps:wait:5", 20).is_err());
        assert!(super::parse_macro("nope", 20).is_err());
    }
}
