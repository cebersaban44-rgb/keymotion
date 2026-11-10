//! `kb` — developer and QA tool.
//!
//! Only `effect` and `per-key` write the keyboard's flash, and both need
//! `--yes`. `live` and `live-probe` stream RAM-only real-time LED frames;
//! `clock` sets the keyboard clock; everything else only reads. Flash-writing commands are added only after
//! they are verified on the hardware.

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

/// Open the wired command channel of the first supported keyboard.
fn open_command() -> Result<(kb_core::sonix::SonixFf13<kb_transport::hid::HidChannel>, &'static kb_devices::Model)> {
    let models = catalogue().map_err(|e| anyhow::anyhow!("{e}"))?;
    let hid = Hid::new()?;
    for iface in hid.interfaces() {
        let Some(m) = match_interface(models, &probe(&iface)) else { continue };
        if m.role == InterfaceRole::Command && m.mode.kind == kb_devices::ConnectionKind::Usb {
            let ch = hid.open(&iface).context("komut kanalı açılamadı")?;
            return Ok((kb_core::sonix::SonixFf13::new(ch), m.model));
        }
    }
    anyhow::bail!("USB modunda klavye yok. Kablo takılı ve arkadaki anahtar ortada (USB) olmalı.")
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
