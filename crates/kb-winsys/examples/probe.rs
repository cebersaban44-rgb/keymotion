//! Prints the gauge readings of this PC for a few seconds:
//! `cargo run -p kb-winsys --example probe`.

use kb_winsys::{memory_load, Audio, CpuMeter, GpuMeter};
use std::time::Duration;

fn main() {
    let mut cpu = CpuMeter::new();
    let t = std::time::Instant::now();
    let mut gpu = GpuMeter::new();
    println!("graphics counters opened in {:?}", t.elapsed());
    let t = std::time::Instant::now();
    let mut audio = Audio::new();
    println!("audio opened in {:?}", t.elapsed());
    println!("graphics counters: {}, audio: {}", gpu.is_some(), audio.is_some());
    cpu.sample();
    for _ in 0..5 {
        std::thread::sleep(Duration::from_secs(1));
        let t = std::time::Instant::now();
        let g = gpu.as_mut().and_then(GpuMeter::sample);
        let took = t.elapsed();
        let (v, m) = audio.as_mut().map_or((None, None), |a| (a.speakers(), a.mic_muted()));
        println!(
            "processor {:?}  graphics {:?} ({took:?})  memory {:?}  speakers {:?}  mic muted {:?}",
            cpu.sample().map(|v| v.round()),
            g.map(|v| v.round()),
            memory_load().map(|v| v.round()),
            v.map(|v| (format!("{:.0}%", v.level * 100.0), v.muted)),
            m,
        );
    }
}
