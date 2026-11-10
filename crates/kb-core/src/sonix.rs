//! Driver for the SONiX FF13 command channel (wired AK980 PRO).
//!
//! Every packet goes through [`kb_protocol::sonix::guard`] before it is
//! written. Only flows documented in docs/research/protokol-ff13.md §2.3
//! are implemented, and each one follows the official driver's sequence.

use kb_protocol::sonix::keymap::{keymap_table, KeyAction, Layer};
use kb_protocol::sonix::macros::{macro_image, MacroStep};
use kb_protocol::sonix::screen::{self, ScreenAnimation};
use kb_protocol::sonix::{
    self as proto, guard, guard_screen_page, header, parse_ack, payload_of, report, Ack, Blocked, ClockTime,
    EffectSettings, Invalid, Op, PacketKind, Rgb, PAYLOAD, REPORT_LEN,
};
use kb_transport::hid::HidChannel;
use std::thread::sleep;
use std::time::{Duration, Instant};

/// Pause the official driver keeps before and after configuration packets.
const CMD_DELAY: Duration = Duration::from_millis(35);
/// Gap between real-time data packets (≥1 ms or the pipe stalls).
const REALTIME_GAP: Duration = Duration::from_millis(2);
/// How long to look for a header's echo, and how often.
const ECHO_TIMEOUT: Duration = Duration::from_millis(120);
const ECHO_POLL: Duration = Duration::from_millis(4);
/// `04 F5`: time for the echo to turn 01, then for the 9 data reports.
const READ_ACCEPT_TIMEOUT: Duration = Duration::from_millis(150);
const READ_DATA_TIMEOUT: Duration = Duration::from_millis(400);
/// Pause before and after the macro pages (official driver, S1 :322-335).
const MACRO_GAP: Duration = Duration::from_millis(30);
/// How long to wait for the display channel's ack after one page (the
/// keyboard answers once the page is in flash; ~145 ms expected).
const PAGE_ACK_TIMEOUT: Duration = Duration::from_millis(3000);
/// START retries while a previous session is still closing.
const BEGIN_ATTEMPTS: usize = 10;
const BEGIN_RETRY: Duration = Duration::from_millis(300);

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error(transparent)]
    Transport(#[from] kb_transport::TransportError),
    #[error("blocked by safety guard: {0}")]
    Blocked(#[from] Blocked),
    #[error("invalid value: {0}")]
    Invalid(#[from] Invalid),
    #[error("keyboard refused 0x{op:02X} (status 0x{status:02X})")]
    Refused { op: u8, status: u8 },
    #[error("no valid echo for 0x{0:02X}")]
    NoEcho(u8),
    #[error("keyboard stayed busy; no configuration session could be opened")]
    Busy,
    #[error("keyboard received different data (checksum 0x{got:04X}, expected 0x{expected:04X})")]
    Checksum { expected: u16, got: u16 },
    #[error("keyboard echoed different data for 0x{0:02X}")]
    DataEcho(u8),
    #[error("screen upload stopped at page {page} of {total}: {reason}")]
    ScreenPage { page: usize, total: usize, reason: String },
}

/// LE16 byte sum the keyboard reports in the `04 02` echo (wrapping).
fn byte_sum(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0u16, |s, &b| s.wrapping_add(b as u16))
}

pub type Result<T> = std::result::Result<T, DriverError>;

/// Feature-report port; implemented by the real HID channel and by test fakes.
pub trait FeaturePort {
    fn set_feature(&mut self, report: &[u8; REPORT_LEN]) -> Result<()>;
    fn get_feature(&mut self) -> Result<Vec<u8>>;
}

/// Back-off between GET_FEATURE attempts. The keyboard occasionally
/// STALLs the control request (Windows: ERROR_GEN_FAILURE 0x1F, seen in the
/// app on 2026-11-10); a short retry succeeds. Same policy as S2 hid.rs.
const GET_RETRY: [Duration; 3] = [Duration::from_millis(5), Duration::from_millis(10), Duration::from_millis(15)];

impl FeaturePort for HidChannel {
    fn set_feature(&mut self, report: &[u8; REPORT_LEN]) -> Result<()> {
        Ok(self.send_feature(report)?)
    }
    fn get_feature(&mut self) -> Result<Vec<u8>> {
        let mut last = None;
        for (attempt, wait) in std::iter::once(Duration::ZERO).chain(GET_RETRY).enumerate() {
            sleep(wait);
            match HidChannel::get_feature(self, 0, REPORT_LEN) {
                Ok(r) => {
                    if attempt > 0 {
                        tracing::debug!(attempt, "GET_FEATURE recovered after stall");
                    }
                    return Ok(r);
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.expect("at least one attempt").into())
    }
}

/// Display channel (usage page FF68): 4096-byte output pages, each answered
/// by an input report. Implemented by the real HID channel and test fakes.
/// There is deliberately no feature-report access here (see kb-transport).
pub trait DisplayPort {
    /// Drop input reports already queued; returns how many.
    fn drain(&mut self) -> usize;
    /// Send one page as an output report (WriteFile), never as SET_REPORT.
    fn write_page(&mut self, page: &[u8; screen::PAGE]) -> Result<()>;
    /// Wait for the page's input report.
    fn read_ack(&mut self, timeout: Duration) -> Result<Vec<u8>>;
}

impl DisplayPort for HidChannel {
    fn drain(&mut self) -> usize {
        let mut n = 0;
        while n < 64 && self.read_input(PAYLOAD + 1, 0).is_ok() {
            n += 1;
        }
        n
    }
    fn write_page(&mut self, page: &[u8; screen::PAGE]) -> Result<()> {
        let mut r = vec![0u8; screen::PAGE + 1];
        r[1..].copy_from_slice(page);
        let n = self.write_output(&r)?;
        if n == 0 {
            return Err(DriverError::ScreenPage { page: 0, total: 0, reason: "nothing written".into() });
        }
        Ok(())
    }
    fn read_ack(&mut self, timeout: Duration) -> Result<Vec<u8>> {
        Ok(self.read_input(PAYLOAD + 1, timeout.as_millis().min(i32::MAX as u128) as i32)?)
    }
}

/// What a screen upload measured (the first uploads are experiments; see
/// docs/research/ekran-derin-arastirma.md §8).
#[derive(Debug, Clone)]
pub struct ScreenUploadReport {
    pub pages: usize,
    /// Input reports that were already queued before the upload.
    pub drained: usize,
    /// Status byte of the `04 72` echo.
    pub announce_status: u8,
    /// Write-to-ack time of every page.
    pub page_times: Vec<Duration>,
    /// The first page's ack, as read.
    pub first_ack: Vec<u8>,
    /// First 8 payload bytes of the `04 02` echo.
    pub commit_echo: Vec<u8>,
    /// Byte sums of everything sent on the display channel.
    pub sum16: u16,
    pub sum32: u32,
}

pub struct SonixFf13<P: FeaturePort> {
    port: P,
}

impl<P: FeaturePort> SonixFf13<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }

    /// Read whatever the feature report currently holds without sending
    /// anything. Purely passive.
    pub fn peek(&mut self) -> Result<Vec<u8>> {
        self.port.get_feature()
    }

    fn write(&mut self, packet: &[u8; PAYLOAD], kind: PacketKind) -> Result<()> {
        guard(packet, kind)?;
        self.port.set_feature(&report(packet))
    }

    /// Send a header and wait for its echo, skipping unrelated reports
    /// (e.g. data left over from an earlier read).
    fn command(&mut self, op: Op, arg: u8, count: u16, delay: Duration) -> Result<Ack> {
        let reply = self.command_reply(op, arg, count, delay)?;
        parse_ack(&reply).ok_or(DriverError::NoEcho(op as u8))
    }

    /// Like [`Self::command`], returning the raw echo.
    fn command_reply(&mut self, op: Op, arg: u8, count: u16, delay: Duration) -> Result<Vec<u8>> {
        sleep(delay);
        self.write(&header(op, arg, count), PacketKind::Header)?;
        sleep(delay);
        let deadline = Instant::now() + ECHO_TIMEOUT;
        loop {
            match self.port.get_feature() {
                Ok(r) => {
                    if parse_ack(&r).is_some_and(|a| a.op == op as u8) {
                        return Ok(r);
                    }
                }
                Err(e) if Instant::now() >= deadline => return Err(e),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(DriverError::NoEcho(op as u8));
            }
            sleep(ECHO_POLL);
        }
    }

    /// Colours currently shown on the keys (`04 F5`), as `(led, rgb)`.
    ///
    /// Read-only; the official driver polls this routinely. Measured on fw
    /// 0x0106: the echo first reads back with status 00 and turns 01 once
    /// the keyboard has the table ready, then 9 data reports follow. Ends
    /// with `04 02` exactly like the driver.
    pub fn read_colors(&mut self) -> Result<Vec<(u8, Rgb)>> {
        sleep(Duration::from_millis(10));
        // The header must announce the 9 packets the host will read
        // (S4/S8). Measured 2026-11-10: with count 00 (S1/S2) only the first
        // read on a connection works; the session never closes, every later
        // F5 is ignored and following commands are swallowed. With 09 every
        // read succeeds and the commit echoes status 01.
        self.write(&header(Op::ReadColors, 0, proto::COLOR_TABLE_PACKETS as u16), PacketKind::Header)?;

        // 1. Wait for the echo to report 01 (accepted) or FF (refused).
        let mut table = Vec::with_capacity(proto::COLOR_TABLE_PACKETS * PAYLOAD);
        let mut status = 0u8;
        let deadline = Instant::now() + READ_ACCEPT_TIMEOUT;
        while Instant::now() < deadline {
            sleep(ECHO_POLL);
            let Ok(r) = self.port.get_feature() else { continue };
            if let Some(a) = parse_ack(&r).filter(|a| a.op == Op::ReadColors as u8) {
                status = a.status;
                if a.status == proto::STATUS_OK || a.status == proto::STATUS_REFUSED {
                    break;
                }
            }
        }

        // 2. Collect the 9 data reports, skipping repeated echoes.
        if status == proto::STATUS_OK {
            let deadline = Instant::now() + READ_DATA_TIMEOUT;
            while table.len() < proto::COLOR_TABLE_PACKETS * PAYLOAD && Instant::now() < deadline {
                sleep(Duration::from_millis(3));
                let Ok(r) = self.port.get_feature() else { continue };
                if parse_ack(&r).is_some() {
                    continue;
                }
                table.extend_from_slice(payload_of(&r));
            }
        }

        // 3. Close like the official driver, whatever happened above. The
        //    35 ms pacing matters: measured 2026-11-10, a commit sent right
        //    after the last data read is not processed (echo status 00), the
        //    session stays open and the next F5 is ignored.
        let commit = self.command(Op::Commit, 0, 0, CMD_DELAY);
        tracing::debug!(?commit, status, bytes = table.len(), "read_colors");
        if status != proto::STATUS_OK {
            return Err(DriverError::Refused { op: Op::ReadColors as u8, status });
        }
        commit?;
        Ok(proto::parse_color_table(&table))
    }

    /// Show one real-time frame (`04 20`). RAM only: nothing is written to
    /// flash and the saved effect is untouched.
    pub fn realtime_frame(&mut self, leds: impl IntoIterator<Item = (u8, Rgb)>) -> Result<()> {
        let table = proto::realtime_table(leds);
        self.write(&header(Op::Realtime, 0, proto::REALTIME_PACKETS), PacketKind::Header)?;
        // Measured on the AK980 PRO (fw 0x0106): the echo comes back with
        // status 00, not 01. Only an explicit FF is a refusal. Once the header
        // is sent the frame must always be completed, otherwise the keyboard
        // keeps waiting for 8 data packets and swallows later commands, so a
        // failed echo read is logged, not returned.
        match self.port.get_feature() {
            Ok(echo) => {
                if let Some(a) = parse_ack(&echo) {
                    if a.op == Op::Realtime as u8 && a.status == proto::STATUS_REFUSED {
                        return Err(DriverError::Refused { op: a.op, status: a.status });
                    }
                }
            }
            Err(e) => tracing::warn!("realtime echo unreadable, completing the frame anyway: {e}"),
        }
        for chunk in table.chunks_exact(PAYLOAD) {
            sleep(REALTIME_GAP);
            let mut p = [0u8; PAYLOAD];
            p.copy_from_slice(chunk);
            self.write(&p, PacketKind::Data)?;
        }
        sleep(REALTIME_GAP);
        self.write(&header(Op::Commit, 0, 0), PacketKind::Header)?;
        if let Err(e) = self.port.get_feature() {
            tracing::debug!("realtime commit echo unreadable: {e}");
        }
        Ok(())
    }

    // ------------------------------------------------------------ configuration (flash writes)

    /// Header with the official driver's pacing; only an explicit `FF`
    /// counts as a refusal (status 00 and 01 both occur on fw 0x0106).
    fn acked(&mut self, op: Op, arg: u8, count: u16) -> Result<Ack> {
        let ack = self.command(op, arg, count, CMD_DELAY)?;
        if ack.status == proto::STATUS_REFUSED {
            return Err(DriverError::Refused { op: ack.op, status: ack.status });
        }
        Ok(ack)
    }

    /// Open a configuration session, retrying while the keyboard is busy.
    fn begin(&mut self) -> Result<()> {
        for attempt in 0..BEGIN_ATTEMPTS {
            match self.acked(Op::Begin, 0, 0) {
                Ok(_) => return Ok(()),
                Err(DriverError::Refused { .. } | DriverError::NoEcho(_)) => {
                    tracing::debug!(attempt, "START refused, retrying");
                    sleep(BEGIN_RETRY);
                }
                Err(e) => return Err(e),
            }
        }
        Err(DriverError::Busy)
    }

    fn data(&mut self, packet: &[u8; PAYLOAD]) -> Result<()> {
        sleep(CMD_DELAY);
        self.write(packet, PacketKind::Data)
    }

    /// `04 F0`: apply. Sent without reading the echo, like the official driver.
    fn apply(&mut self) -> Result<()> {
        sleep(CMD_DELAY);
        self.write(&header(Op::Apply, 0, 0), PacketKind::Header)
    }

    /// `04 02` after a data phase. Measured on fw 0x0106: the echo is
    /// `04 02 00 01 SUM_LO SUM_HI`, SUM being the byte sum of the data
    /// received since the header. A mismatch means the keyboard got
    /// different bytes than we sent.
    fn commit_verified(&mut self, sent: &[u8]) -> Result<()> {
        let ack = self.command(Op::Commit, 0, 0, CMD_DELAY)?;
        if ack.status == proto::STATUS_REFUSED {
            return Err(DriverError::Refused { op: ack.op, status: ack.status });
        }
        if let Some(got) = ack.sum {
            let expected = byte_sum(sent);
            if got != expected {
                return Err(DriverError::Checksum { expected, got });
            }
        }
        Ok(())
    }

    /// `04 18` → `04 13` (1) → data → `04 02` → `04 F0`.
    fn write_lighting(&mut self, data: &[u8; PAYLOAD]) -> Result<()> {
        self.begin()?;
        self.acked(Op::Lighting, 0, 1)?;
        self.data(data)?;
        self.commit_verified(data)?;
        self.apply()
    }

    /// Store a firmware effect. **Writes the keyboard's flash**: call only on
    /// an explicit user action, never while a slider is being dragged.
    pub fn apply_effect(&mut self, settings: &EffectSettings) -> Result<()> {
        let data = proto::lighting_data(settings)?;
        self.write_lighting(&data)
    }

    /// Store per-key colours and switch to per-key mode. **Writes flash.**
    /// LEDs missing from `colors` turn off.
    pub fn apply_per_key(&mut self, colors: impl IntoIterator<Item = (u8, Rgb)>, brightness: u8) -> Result<()> {
        let mode = proto::per_key_mode_data(brightness)?;
        let table = proto::per_key_table(colors)?;
        self.write_lighting(&mode)?;
        self.write_table(Op::PerKeyColors, &table)?;
        self.apply()
    }

    /// Store one keymap layer: `04 18` → `04 11` (base) or `04 27` (Fn)
    /// announcing 9 packets → the table → `04 02` → `04 F0`. **Writes
    /// flash** and replaces the whole layer: keys not in `entries` go back
    /// to the firmware default. The keyboard cannot report its keymap, so
    /// the caller has to remember what it wrote.
    pub fn apply_keymap(&mut self, layer: Layer, entries: impl IntoIterator<Item = (u8, KeyAction)>) -> Result<()> {
        let table = keymap_table(entries)?;
        self.write_table(layer.op(), &table)?;
        // The official driver reads the APPLY echo in this flow (S1 :300-315).
        self.acked(Op::Apply, 0, 0).map(|_| ())
    }

    /// Store every macro: `04 19` → `04 15` announcing the pages → 30 ms →
    /// the pages → 30 ms → `04 02`. No `04 18` and no `04 F0`, like the
    /// official driver. **Writes flash** and replaces all macros: `04 19`
    /// clears the stored ones first, and an empty list leaves none.
    pub fn write_macros(&mut self, slots: &[Vec<MacroStep>]) -> Result<()> {
        let image = macro_image(slots)?;
        self.acked(Op::MacroBegin, 0, 0)?;
        self.acked(Op::MacroData, 0, (image.len() / PAYLOAD) as u16)?;
        sleep(MACRO_GAP);
        for chunk in image.chunks_exact(PAYLOAD) {
            let mut p = [0u8; PAYLOAD];
            p.copy_from_slice(chunk);
            self.data(&p)?;
        }
        sleep(MACRO_GAP);
        self.commit_verified(&image)
    }

    /// `04 18` → `op` announcing the table's packets → the table → `04 02`
    /// with its byte sum checked.
    fn write_table(&mut self, op: Op, table: &[u8]) -> Result<()> {
        self.begin()?;
        self.acked(op, 0, (table.len() / PAYLOAD) as u16)?;
        for chunk in table.chunks_exact(PAYLOAD) {
            let mut p = [0u8; PAYLOAD];
            p.copy_from_slice(chunk);
            self.data(&p)?;
        }
        self.commit_verified(table)
    }

    /// Set the keyboard clock: `04 18` → `04 28` (1) → data → `04 02`, no
    /// `04 F0`. Sets the clock (RTC); the screen switches to its clock page.
    ///
    /// Measured on fw 0x0106 (2026-11-10): the keyboard echoes the data
    /// packet byte for byte (the official driver reads it too), and the
    /// `04 02` echo carries status 01 but a byte sum of 0 in this flow. So
    /// the data echo is what proves delivery here, not the sum.
    pub fn set_clock(&mut self, time: &ClockTime) -> Result<()> {
        let data = proto::clock_data(time)?;
        self.begin()?;
        self.acked(Op::Clock, 0, 1)?;
        self.data(&data)?;
        let echo = self.port.get_feature()?;
        if payload_of(&echo).get(..PAYLOAD) != Some(&data[..]) {
            tracing::debug!(echo = ?payload_of(&echo).get(..11), "clock data echo differs");
            return Err(DriverError::DataEcho(Op::Clock as u8));
        }
        self.acked(Op::Commit, 0, 0).map(|_| ())
    }

    /// Upload an image or animation to the screen's GIF page: `04 18` →
    /// `04 72 01 … PAGES` → the pages on the display channel, each followed
    /// by its ack → `04 02`. Never `04 F0` (it restarts the player and
    /// breaks persistence). **Writes the keyboard's external flash**: every
    /// page erases one 4 KiB sector, and the previous animation (including
    /// the factory one) is gone. Call only on an explicit user action.
    ///
    /// Every page is checked before the first byte is sent. If a page fails,
    /// the session is closed with `04 02` and the GIF page stays incomplete
    /// until the next upload; the home page is not affected.
    pub fn upload_screen<D: DisplayPort>(
        &mut self,
        display: &mut D,
        anim: &ScreenAnimation,
        mut progress: impl FnMut(usize, usize),
    ) -> Result<ScreenUploadReport> {
        let pages = screen::upload_pages(anim)?;
        for p in &pages {
            guard_screen_page(p)?;
        }
        let total = pages.len();
        let sum16 = pages.iter().flatten().fold(0u16, |s, &b| s.wrapping_add(b as u16));
        let sum32 = pages.iter().flatten().fold(0u32, |s, &b| s.wrapping_add(b as u32));

        let drained = display.drain();
        self.begin()?;
        let announce = self.acked(Op::ScreenUpload, screen::SLOT, total as u16)?;
        let mut page_times = Vec::with_capacity(total);
        let mut first_ack = Vec::new();
        for (i, page) in pages.iter().enumerate() {
            let t = Instant::now();
            let sent = display.write_page(page).and_then(|_| display.read_ack(PAGE_ACK_TIMEOUT));
            match sent {
                Ok(ack) => {
                    if i == 0 {
                        first_ack = ack;
                    }
                    page_times.push(t.elapsed());
                }
                Err(e) => {
                    let close = self.command(Op::Commit, 0, 0, CMD_DELAY);
                    tracing::warn!(page = i, ?close, "screen upload aborted");
                    return Err(DriverError::ScreenPage { page: i + 1, total, reason: e.to_string() });
                }
            }
            progress(i + 1, total);
        }
        let reply = self.command_reply(Op::Commit, 0, 0, CMD_DELAY)?;
        let ack = parse_ack(&reply).ok_or(DriverError::NoEcho(Op::Commit as u8))?;
        if ack.status == proto::STATUS_REFUSED {
            return Err(DriverError::Refused { op: ack.op, status: ack.status });
        }
        Ok(ScreenUploadReport {
            pages: total,
            drained,
            announce_status: announce.status,
            page_times,
            first_ack,
            commit_echo: payload_of(&reply).iter().take(8).copied().collect(),
            sum16,
            sum32,
        })
    }

    pub fn into_inner(self) -> P {
        self.port
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake keyboard: like the real one, the feature report holds only the
    /// latest echo (headers echoed with status 01). Records every write.
    #[derive(Default)]
    struct Fake {
        writes: Vec<[u8; REPORT_LEN]>,
        last: Option<Vec<u8>>,
        /// Data packets still expected after the last header, and their sum.
        pending: usize,
        sum: u16,
        /// Corrupt the reported checksum (simulates a transfer error).
        corrupt: bool,
        /// Echo data packets with one byte changed.
        garble_echo: bool,
    }

    impl FeaturePort for Fake {
        fn set_feature(&mut self, r: &[u8; REPORT_LEN]) -> Result<()> {
            self.writes.push(*r);
            if self.pending > 0 {
                self.pending -= 1;
                self.sum = self.sum.wrapping_add(byte_sum(&r[1..]));
                // Data packets are echoed as they are.
                let mut echo = r.to_vec();
                if self.garble_echo {
                    echo[4] ^= 1;
                }
                self.last = Some(echo);
                return Ok(());
            }
            if r[1] == 0x04 {
                let mut echo = r.to_vec();
                echo[4] = 0x01;
                if r[2] == 0x02 {
                    let s = if self.corrupt { self.sum ^ 1 } else { self.sum };
                    echo[5..7].copy_from_slice(&s.to_le_bytes());
                }
                self.pending = r[9] as usize | (r[10] as usize) << 8;
                if r[2] != 0x02 {
                    self.sum = 0;
                }
                self.last = Some(echo);
            }
            Ok(())
        }
        fn get_feature(&mut self) -> Result<Vec<u8>> {
            Ok(self.last.clone().unwrap_or_else(|| vec![0; REPORT_LEN]))
        }
    }

    /// Fake display channel: records pages, answers each with an ack.
    #[derive(Default)]
    struct FakeDisplay {
        pages: Vec<Vec<u8>>,
        queued: usize,
        fail_at: Option<usize>,
    }

    impl DisplayPort for FakeDisplay {
        fn drain(&mut self) -> usize {
            std::mem::take(&mut self.queued)
        }
        fn write_page(&mut self, page: &[u8; screen::PAGE]) -> Result<()> {
            self.pages.push(page.to_vec());
            Ok(())
        }
        fn read_ack(&mut self, _: Duration) -> Result<Vec<u8>> {
            if self.fail_at == Some(self.pages.len() - 1) {
                return Err(DriverError::Transport(kb_transport::TransportError::Timeout(3000)));
            }
            Ok(vec![0x01, 0x5A, 0x02, 0x00])
        }
    }

    #[test]
    fn screen_upload_sequence_matches_official_flow() {
        let anim = ScreenAnimation::still(screen::test_pattern());
        let mut d = SonixFf13::new(Fake::default());
        let mut display = FakeDisplay { queued: 2, ..FakeDisplay::default() };
        let mut seen = Vec::new();
        let r = d.upload_screen(&mut display, &anim, |done, total| seen.push((done, total))).unwrap();
        let w = d.into_inner().writes;
        // START, 04 72, COMMIT on the command channel; no 04 F0.
        assert_eq!(w.len(), 3);
        assert_eq!(w[0][2], 0x18);
        assert_eq!(&w[1][1..11], &[0x04, 0x72, 0x01, 0, 0, 0, 0, 0, 16, 0], "slot 1, 16 pages");
        assert_eq!(w[2][2], 0x02);
        assert!(w.iter().all(|r| r[2] != 0xF0));
        assert_eq!(display.pages.len(), 16);
        assert_eq!(&display.pages[0][..3], &[1, screen::STILL_DELAY, 0xFF]);
        assert_eq!((r.pages, r.drained, r.page_times.len()), (16, 2, 16));
        assert_eq!(r.first_ack, vec![0x01, 0x5A, 0x02, 0x00]);
        assert_eq!(seen.last(), Some(&(16, 16)));
    }

    #[test]
    fn screen_upload_closes_the_session_when_a_page_fails() {
        let anim = ScreenAnimation::still(screen::test_pattern());
        let mut d = SonixFf13::new(Fake::default());
        let mut display = FakeDisplay { fail_at: Some(3), ..FakeDisplay::default() };
        let err = d.upload_screen(&mut display, &anim, |_, _| {}).unwrap_err();
        assert!(matches!(err, DriverError::ScreenPage { page: 4, total: 16, .. }), "{err:?}");
        assert_eq!(display.pages.len(), 4, "no page after the failed one");
        let w = d.into_inner().writes;
        assert_eq!(w.last().unwrap()[2], 0x02, "session closed with 04 02");
        assert!(w.iter().all(|r| r[2] != 0xF0));
    }

    #[test]
    fn realtime_frame_sequence() {
        let mut d = SonixFf13::new(Fake::default());
        d.realtime_frame([(1, [255, 0, 0])]).unwrap();
        let w = d.into_inner().writes;
        assert_eq!(w.len(), 1 + 8 + 1, "header + 8 data + commit");
        assert_eq!(&w[0][..10], &[0, 4, 0x20, 0, 0, 0, 0, 0, 0, 8]);
        assert_eq!(&w[1][5..9], &[1, 255, 0, 0], "LED 1 at offset 4 of the table");
        assert_eq!(&w[9][..3], &[0, 4, 0x02]);
        assert!(w.iter().all(|r| r[0] == 0), "report ID 0 everywhere");
    }

    #[test]
    fn clock_sequence_matches_official_flow() {
        let mut d = SonixFf13::new(Fake::default());
        let t = ClockTime { year: 2026, month: 10, day: 29, hour: 15, minute: 7, second: 32, weekday: 4 };
        d.set_clock(&t).unwrap();
        let w = d.into_inner().writes;
        let ops: Vec<(u8, u8)> = w.iter().map(|r| (r[1], r[2])).collect();
        assert_eq!(ops.len(), 4, "START, header, data, COMMIT; no APPLY");
        assert_eq!(ops[0], (0x04, 0x18));
        assert_eq!((ops[1], w[1][9]), ((0x04, 0x28), 1), "clock header announces 1 packet");
        assert_eq!(&w[2][1..12], &[0x00, 0x01, 0x5A, 0x1A, 0x0A, 0x1D, 0x0F, 0x07, 0x20, 0x00, 0x04]);
        assert_eq!(ops[3], (0x04, 0x02));
    }

    #[test]
    fn clock_fails_when_the_echo_differs() {
        let mut d = SonixFf13::new(Fake { garble_echo: true, ..Fake::default() });
        let t = ClockTime { year: 2026, month: 11, day: 10, hour: 5, minute: 9, second: 0, weekday: 2 };
        assert!(matches!(d.set_clock(&t), Err(DriverError::DataEcho(0x28))));
        let w = d.into_inner().writes;
        assert_eq!(w.len(), 3, "no COMMIT after a bad echo");
    }

    #[test]
    fn effect_sequence_matches_official_flow() {
        let mut d = SonixFf13::new(Fake::default());
        d.apply_effect(&EffectSettings::factory()).unwrap();
        let w = d.into_inner().writes;
        let ops: Vec<(u8, u8)> = w.iter().map(|r| (r[1], r[2])).collect();
        assert_eq!(ops.len(), 5);
        assert_eq!(ops[0], (0x04, 0x18), "START");
        assert_eq!((ops[1], w[1][9]), ((0x04, 0x13), 1), "lighting header announces 1 packet");
        assert_eq!(&w[2][1..17], &[11, 0xFF, 0, 0, 0, 0, 0, 0, 1, 5, 3, 0, 0, 0, 0xAA, 0x55]);
        assert_eq!(ops[3], (0x04, 0x02), "COMMIT");
        assert_eq!(ops[4], (0x04, 0xF0), "APPLY");
    }

    #[test]
    fn per_key_sequence() {
        let mut d = SonixFf13::new(Fake::default());
        d.apply_per_key([(1, [0, 255, 0])], 4).unwrap();
        let w = d.into_inner().writes;
        // lighting(5) + START + header + 9 data + COMMIT + APPLY
        assert_eq!(w.len(), 5 + 1 + 1 + 9 + 1 + 1);
        assert_eq!(&w[2][1..2], &[0x80]);
        assert_eq!((w[6][2], w[6][9]), (0x23, 9));
        assert_eq!(&w[7][5..9], &[1, 0, 255, 0]);
        assert_eq!(&w[15][63..65], &[0xAA, 0x55]);
        assert_eq!(w.last().unwrap()[2], 0xF0);
    }

    #[test]
    fn keymap_sequence_matches_official_flow() {
        let mut d = SonixFf13::new(Fake::default());
        d.apply_keymap(Layer::Fn, [(33, KeyAction::key(0x04))]).unwrap();
        let w = d.into_inner().writes;
        // START + header + 9 data + COMMIT + APPLY
        assert_eq!(w.len(), 1 + 1 + 9 + 1 + 1);
        assert_eq!(w[0][2], 0x18);
        assert_eq!((w[1][2], w[1][9]), (0x27, 9), "Fn layer header announces 9 packets");
        // Entry 33 at byte 132: packet 2 (bytes 128..192), offset 4.
        assert_eq!(&w[4][5..9], &[0x02, 0x00, 0x04, 0x00]);
        assert_eq!(&w[10][63..65], &[0xAA, 0x55]);
        assert_eq!(w[11][2], 0x02);
        assert_eq!(w[12][2], 0xF0);

        let mut d = SonixFf13::new(Fake::default());
        d.apply_keymap(Layer::Base, []).unwrap();
        let w = d.into_inner().writes;
        assert_eq!(w[1][2], 0x11, "base layer");
        assert!(w[2..10].iter().all(|r| r[1..].iter().all(|&b| b == 0)), "reset = all default");
    }

    #[test]
    fn macro_sequence_matches_official_flow() {
        use kb_protocol::sonix::macros::text_steps;
        let mut d = SonixFf13::new(Fake::default());
        d.write_macros(&[text_steps("ok", 25)]).unwrap();
        let w = d.into_inner().writes;
        // "ok" is 4 steps: 400 + 8 + 32 = 440 bytes → 8 pages.
        // 04 19 + 04 15 + 8 pages + 04 02; no 04 18, no 04 F0.
        assert_eq!(w.len(), 1 + 1 + 8 + 1);
        assert_eq!(w[0][1..3], [0x04, 0x19]);
        assert_eq!((w[1][2], w[1][9]), (0x15, 8), "04 15 announces 8 pages");
        assert_eq!(&w[2][1..5], &[0x90, 0x01, 0, 0], "directory: slot 0 at 400");
        assert_eq!(&w[9][63..65], &[0xAA, 0x55]);
        assert_eq!(w[10][2], 0x02);
        assert!(w.iter().all(|r| r[2] != 0x18 && r[2] != 0xF0));

        let mut d = SonixFf13::new(Fake::default());
        d.write_macros(&[]).unwrap();
        let w = d.into_inner().writes;
        assert_eq!(w[1][9], 8, "no macros: 8 empty pages");
    }

    #[test]
    fn keymap_checksum_mismatch_skips_apply() {
        let mut d = SonixFf13::new(Fake { corrupt: true, ..Fake::default() });
        assert!(matches!(d.apply_keymap(Layer::Base, []), Err(DriverError::Checksum { .. })));
        assert!(d.into_inner().writes.iter().all(|r| r[2] != 0xF0));
    }

    #[test]
    fn checksum_mismatch_is_reported() {
        let mut d = SonixFf13::new(Fake { corrupt: true, ..Fake::default() });
        let err = d.apply_effect(&EffectSettings::factory()).unwrap_err();
        assert!(matches!(err, DriverError::Checksum { expected: 0x0212, got: 0x0213 }), "{err:?}");
        // APPLY must not follow a failed commit.
        assert!(d.into_inner().writes.iter().all(|r| r[2] != 0xF0));
    }

    #[test]
    fn busy_start_gives_up() {
        struct Busy;
        impl FeaturePort for Busy {
            fn set_feature(&mut self, _: &[u8; REPORT_LEN]) -> Result<()> {
                Ok(())
            }
            fn get_feature(&mut self) -> Result<Vec<u8>> {
                let mut r = vec![0u8; REPORT_LEN];
                r[1..5].copy_from_slice(&[4, 0x18, 0, 0xFF]);
                Ok(r)
            }
        }
        let err = SonixFf13::new(Busy).apply_effect(&EffectSettings::factory()).unwrap_err();
        assert!(matches!(err, DriverError::Busy));
    }

    #[test]
    fn realtime_echo_with_status_zero_completes_the_frame() {
        struct EchoZero(usize);
        impl FeaturePort for EchoZero {
            fn set_feature(&mut self, _: &[u8; REPORT_LEN]) -> Result<()> {
                self.0 += 1;
                Ok(())
            }
            fn get_feature(&mut self) -> Result<Vec<u8>> {
                let mut r = vec![0u8; REPORT_LEN];
                r[1..3].copy_from_slice(&[4, 0x20]);
                r[9] = 8;
                Ok(r)
            }
        }
        let mut d = SonixFf13::new(EchoZero(0));
        d.realtime_frame([(1, [1, 2, 3])]).unwrap();
        assert_eq!(d.into_inner().0, 10, "header + 8 data + commit");
    }

    /// Scripted keyboard: GET_FEATURE pops replies in order; once empty it
    /// echoes the last header written (status 01).
    struct Script {
        replies: std::collections::VecDeque<Vec<u8>>,
        last_header: Vec<u8>,
        headers: Vec<Vec<u8>>,
    }

    impl FeaturePort for Script {
        fn set_feature(&mut self, r: &[u8; REPORT_LEN]) -> Result<()> {
            if r[1] == 0x04 {
                self.headers.push(r.to_vec());
                let mut e = r.to_vec();
                e[4] = 1;
                self.last_header = e;
            }
            Ok(())
        }
        fn get_feature(&mut self) -> Result<Vec<u8>> {
            Ok(self.replies.pop_front().unwrap_or_else(|| self.last_header.clone()))
        }
    }

    fn wire(payload: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; REPORT_LEN];
        r[1..1 + payload.len()].copy_from_slice(payload);
        r
    }

    #[test]
    fn read_colors_waits_for_status_01_and_collects_nine_packets() {
        let mut replies = std::collections::VecDeque::new();
        replies.push_back(wire(&[4, 0xF5, 0, 0])); // not ready yet
        replies.push_back(wire(&[4, 0xF5, 0, 0]));
        replies.push_back(wire(&[4, 0xF5, 0, 1])); // accepted
        replies.push_back(wire(&[4, 0xF5, 0, 1])); // repeated echo, skipped
        for i in 0..9u8 {
            let mut p = [0u8; PAYLOAD];
            let idx = i * 16 + 1;
            p[4..8].copy_from_slice(&[idx, 0xFA, 0x88, 0x3C]);
            replies.push_back(wire(&p));
        }
        let mut d = SonixFf13::new(Script { replies, last_header: vec![0; REPORT_LEN], headers: Vec::new() });
        let colors = d.read_colors().unwrap();
        // The F5 header must announce 9 packets (see read_colors); then COMMIT.
        let h = &d.port.headers;
        assert_eq!((h[0][2], h[0][9]), (0xF5, 9));
        assert_eq!(h.last().unwrap()[2], 0x02);
        let lit: Vec<u8> = colors.iter().filter(|(_, c)| c != &[0, 0, 0]).map(|(l, _)| *l).collect();
        assert_eq!(lit, vec![1, 17, 33, 49, 65, 81, 97, 113, 129]);
    }

    #[test]
    fn realtime_frame_completes_even_if_echo_read_fails() {
        struct Stall(usize);
        impl FeaturePort for Stall {
            fn set_feature(&mut self, _: &[u8; REPORT_LEN]) -> Result<()> {
                self.0 += 1;
                Ok(())
            }
            fn get_feature(&mut self) -> Result<Vec<u8>> {
                Err(DriverError::Transport(kb_transport::TransportError::Timeout(0)))
            }
        }
        let mut d = SonixFf13::new(Stall(0));
        d.realtime_frame([(1, [1, 2, 3])]).unwrap();
        assert_eq!(d.into_inner().0, 10, "header + 8 data + commit even though every read failed");
    }

    #[test]
    fn refused_realtime_is_an_error() {
        struct Refuse;
        impl FeaturePort for Refuse {
            fn set_feature(&mut self, _: &[u8; REPORT_LEN]) -> Result<()> {
                Ok(())
            }
            fn get_feature(&mut self) -> Result<Vec<u8>> {
                let mut r = vec![0u8; REPORT_LEN];
                r[1..5].copy_from_slice(&[4, 0x20, 0, 0xFF]);
                Ok(r)
            }
        }
        let err = SonixFf13::new(Refuse).realtime_frame([]).unwrap_err();
        assert!(matches!(err, DriverError::Refused { op: 0x20, status: 0xFF }));
    }
}
