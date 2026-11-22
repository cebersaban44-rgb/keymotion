//! Prints how loud the PC's sound is, once a second, for a few seconds:
//! `cargo run -p kb-winsys --example loopback [seconds]` (10 by default).
//! Only levels are printed; nothing is recorded or kept.

use kb_winsys::loopback::{Capture, State};
use std::time::{Duration, Instant};

/// Poll rate while the stream is open, as the music lights will.
const TICK: Duration = Duration::from_millis(50);

fn db(v: f32) -> String {
    if v <= 0.0 {
        "-inf dBFS".into()
    } else {
        format!("{:.1} dBFS", 20.0 * v.log10())
    }
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let t = Instant::now();
    let mut capture = Capture::new();
    println!("opened in {:?}", t.elapsed());
    let mut samples = Vec::new();
    let end = Instant::now() + Duration::from_secs(seconds);
    let mut next_line = Instant::now() + Duration::from_secs(1);
    // Per second: mono samples, sum of squares, loudest sample, loudest
    // meter reading, polls, resets and the slowest poll.
    let (mut frames, mut squares, mut peak, mut meter, mut polls, mut resets) = (0usize, 0f64, 0f32, 0f32, 0u32, 0u32);
    let mut slowest = Duration::ZERO;
    while Instant::now() < end {
        samples.clear();
        let t = Instant::now();
        let poll = capture.poll(&mut samples);
        slowest = slowest.max(t.elapsed());
        polls += 1;
        let state = poll.state;
        resets += u32::from(poll.reset);
        frames += poll.frames;
        squares += samples.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>();
        if poll.state == State::Open {
            peak = peak.max(poll.peak);
        } else {
            meter = meter.max(poll.peak);
        }
        if Instant::now() >= next_line {
            let rms = if frames > 0 { (squares / frames as f64).sqrt() as f32 } else { 0.0 };
            let format = capture.format().map_or("closed".to_string(), |f| f.to_string());
            println!(
                "{state:?}: {format}; {frames} samples, rms {}, peak {}; meter {}; {polls} polls (slowest {slowest:?}), {resets} resets",
                db(rms),
                db(peak),
                db(meter),
            );
            (frames, squares, peak, meter, polls, resets) = (0, 0.0, 0.0, 0.0, 0, 0);
            slowest = Duration::ZERO;
            next_line += Duration::from_secs(1);
        }
        std::thread::sleep(if state == State::Open { TICK } else { capture.wait() });
    }
}
