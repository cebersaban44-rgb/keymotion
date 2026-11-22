//! The sound the PC plays, for the music lights: Windows' own "loopback"
//! capture of the default speakers or headphones (WASAPI, shared mode). No
//! microphone, no administrator rights, nothing recorded or kept: the
//! samples go to the caller's buffer and from there only into the analysis.
//!
//! * [`Meter`]: the output's peak meter (`IAudioMeterInformation`). Reading
//!   it opens no stream and costs next to nothing, so it is what runs while
//!   nothing plays.
//! * [`Loopback`]: the capture stream itself, mixed down to mono as it is
//!   read: left and right never reach the keys apart (no direction from a
//!   game's sound).
//! * [`Capture`]: both, managed. The stream is opened when the meter shows
//!   sound and closed again after [`CLOSE_AFTER`] of silence, because an
//!   open audio stream can keep Windows from going to sleep on its own.
//!   The default output is looked up again every [`LOOK_EVERY`] (headphones
//!   plugged in, a Bluetooth headset connecting) and after any error (the
//!   device unplugged or disabled); none of it panics.
//!
//! What loopback cannot hear: a program playing in exclusive mode (some
//! music players' "WASAPI exclusive" output), and protected content a
//! driver keeps to itself. Both read as silence.
//!
//! Like the rest of this crate, the types hold COM objects and are not
//! `Send`: make them on the thread that reads them.

use std::time::{Duration, Instant};

/// Meter peak (0–1) that opens the stream: about −50 dBFS.
pub const OPEN_PEAK: f32 = 0.003_2;
/// Sample peak (0–1) that counts as sound while the stream is open: about
/// −60 dBFS.
pub const SOUND_PEAK: f32 = 0.001;
/// The stream closes after this long without sound (a pause between songs
/// is shorter).
pub const CLOSE_AFTER: Duration = Duration::from_secs(5);
/// How often the meter is read while the stream is closed…
pub const METER_EVERY: Duration = Duration::from_millis(250);
/// …and after [`LONG_QUIET`] without sound.
pub const METER_EVERY_LONG_QUIET: Duration = Duration::from_secs(1);
pub const LONG_QUIET: Duration = Duration::from_secs(10 * 60);
/// How often the default output is looked up again.
pub const LOOK_EVERY: Duration = Duration::from_secs(2);
/// Wait before trying again after the stream failed to open.
pub const RETRY_AFTER_ERROR: Duration = Duration::from_secs(5);
/// Wait before opening again after a stream that heard nothing while the
/// meter showed sound (exclusive mode, protected content): no open and
/// close every few seconds.
pub const RETRY_AFTER_DEAF: Duration = Duration::from_secs(30);
/// The stream's buffer: what can wait between two reads.
pub const BUFFER: Duration = Duration::from_millis(250);
/// More channels than this is not a speaker layout.
pub const MAX_CHANNELS: u16 = 32;

/// Why the sound cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopbackError {
    /// Windows' sound system could not be started.
    Unavailable,
    /// No speakers or headphones.
    NoOutput,
    /// The device went away (unplugged, disabled) while in use.
    DeviceLost,
    /// The output's format is not one [`mix_to_mono`] reads.
    Format(WaveFormat),
    /// Any other error code from Windows.
    Os(i32),
}

impl std::fmt::Display for LoopbackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "Windows sound system unavailable"),
            Self::NoOutput => write!(f, "no sound output device"),
            Self::DeviceLost => write!(f, "sound output device went away"),
            Self::Format(w) => write!(
                f,
                "unsupported sound format (tag {:#06x}, {} bits, {} channels, block {})",
                w.tag, w.bits, w.channels, w.block_align
            ),
            Self::Os(code) => write!(f, "Windows sound error {:#010x}", *code as u32),
        }
    }
}

impl std::error::Error for LoopbackError {}

/// `WAVE_FORMAT_PCM`: integer samples.
pub const TAG_PCM: u16 = 0x0001;
/// `WAVE_FORMAT_IEEE_FLOAT`: float samples.
pub const TAG_FLOAT: u16 = 0x0003;
/// `WAVE_FORMAT_EXTENSIBLE`: the sub-format GUID says which.
pub const TAG_EXTENSIBLE: u16 = 0xFFFE;

/// A stream format as Windows describes it (`WAVEFORMATEX`, with the
/// `WAVEFORMATEXTENSIBLE` part when there is one). Plain numbers, so the
/// decoding is tested without Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveFormat {
    pub tag: u16,
    pub channels: u16,
    pub rate: u32,
    pub block_align: u16,
    /// Bits per sample: the container size for the extensible format.
    pub bits: u16,
    pub extensible: Option<Extensible>,
}

/// The `WAVEFORMATEXTENSIBLE` part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extensible {
    /// Bits that carry the sample, at the top of the container.
    pub valid_bits: u16,
    /// The sub-format GUID: `data1`, `data2`, `data3`, `data4`.
    pub sub_format: (u32, u16, u16, [u8; 8]),
}

/// The format tag a `KSDATAFORMAT_SUBTYPE_*` GUID stands for: those for the
/// classic tags are `{0000xxxx-0000-0010-8000-00aa00389b71}`.
pub fn sub_format_tag((data1, data2, data3, data4): (u32, u16, u16, [u8; 8])) -> Option<u16> {
    const BASE: [u8; 8] = [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71];
    (data2 == 0 && data3 == 0x0010 && data4 == BASE).then_some(data1).and_then(|d| u16::try_from(d).ok())
}

/// How one sample is stored (little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sample {
    /// 8-bit unsigned integer, silence at 128.
    U8,
    I16,
    /// 24-bit integer packed in three bytes.
    I24,
    /// 32-bit integer; also a 24-bit sample at the top of a 32-bit container.
    I32,
    F32,
}

impl Sample {
    pub fn bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::I16 => 2,
            Self::I24 => 3,
            Self::I32 | Self::F32 => 4,
        }
    }

    /// One sample at the start of `b` (at least [`Sample::bytes`] long) as
    /// −1…1. A float mix can go over full scale; it is clipped like the
    /// speakers would, and a broken value reads as silence.
    fn read(self, b: &[u8]) -> f32 {
        match self {
            Self::U8 => (f32::from(b[0]) - 128.0) / 128.0,
            Self::I16 => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
            // Into the top three bytes of an i32 for the sign.
            Self::I24 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            Self::I32 => (f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])) / 2_147_483_648.0) as f32,
            Self::F32 => {
                let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                if v.is_finite() {
                    v.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            }
        }
    }
}

/// A format [`mix_to_mono`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub rate: u32,
    pub channels: u16,
    pub sample: Sample,
}

impl Format {
    /// Decode what Windows reported; an error for anything else (never a
    /// guess: wrong bytes would light the keys with noise).
    pub fn from_wave(w: &WaveFormat) -> Result<Self, LoopbackError> {
        let bad = || LoopbackError::Format(*w);
        let (tag, valid) = match (w.tag, w.extensible) {
            (TAG_EXTENSIBLE, Some(e)) => (sub_format_tag(e.sub_format).ok_or_else(bad)?, e.valid_bits),
            (TAG_EXTENSIBLE, None) => return Err(bad()),
            (tag, _) => (tag, w.bits),
        };
        if valid == 0 || valid > w.bits {
            return Err(bad());
        }
        let sample = match (tag, w.bits) {
            (TAG_PCM, 8) => Sample::U8,
            (TAG_PCM, 16) => Sample::I16,
            (TAG_PCM, 24) => Sample::I24,
            (TAG_PCM, 32) => Sample::I32,
            (TAG_FLOAT, 32) if valid == 32 => Sample::F32,
            _ => return Err(bad()),
        };
        let channels_ok = (1..=MAX_CHANNELS).contains(&w.channels);
        if !channels_ok || w.rate == 0 || usize::from(w.block_align) != usize::from(w.channels) * sample.bytes() {
            return Err(bad());
        }
        Ok(Self { rate: w.rate, channels: w.channels, sample })
    }

    /// Bytes of one frame (one sample of every channel).
    pub fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * self.sample.bytes()
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.sample {
            Sample::F32 => "float",
            _ => "integer",
        };
        let bits = match self.sample {
            Sample::U8 => 8,
            Sample::I16 => 16,
            Sample::I24 => 24,
            Sample::I32 | Sample::F32 => 32,
        };
        write!(f, "{} Hz, {} channels, {bits}-bit {kind}", self.rate, self.channels)
    }
}

/// Interleaved frames to mono: each frame's channels averaged, appended to
/// `out`. A trailing partial frame is left out. Returns the frames added.
pub fn mix_to_mono(data: &[u8], format: &Format, out: &mut Vec<f32>) -> usize {
    let size = format.sample.bytes();
    let frame = format.frame_bytes();
    if frame == 0 {
        return 0;
    }
    let frames = data.len() / frame;
    let channels = f32::from(format.channels);
    out.reserve(frames);
    for f in data.chunks_exact(frame) {
        let sum: f32 = f.chunks_exact(size).map(|s| format.sample.read(s)).sum();
        out.push(sum / channels);
    }
    frames
}

/// What one [`Loopback::read`] got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReadInfo {
    /// Frames (mono samples) added.
    pub frames: usize,
    /// Windows marked some of it as silence (added as zeros).
    pub silent: bool,
    /// A gap in the sound (the buffer ran over, or a glitch): what came
    /// before does not join up with this.
    pub discontinuity: bool,
}

/// Windows' sound output: COM and the device list, for this thread.
pub struct Output(sys::Output);

impl Output {
    /// Starts COM on this thread. `None` when Windows' sound system is not
    /// there.
    pub fn new() -> Option<Self> {
        sys::Output::new().map(Self)
    }

    /// The default speakers or headphones.
    pub fn default_device(&self) -> Result<Device, LoopbackError> {
        self.0.default_device().map(Device)
    }
}

/// One output device.
pub struct Device(sys::Device);

impl Device {
    /// Windows' id of the device, to tell a switch of device. Not for logs.
    pub fn id(&self) -> &str {
        self.0.id()
    }

    /// Its peak meter.
    pub fn meter(&self) -> Result<Meter, LoopbackError> {
        self.0.meter().map(Meter)
    }

    /// Open a loopback stream on it and start it.
    pub fn open(&self) -> Result<Loopback, LoopbackError> {
        self.0.open().map(Loopback)
    }
}

/// An output's peak meter: opens no stream.
pub struct Meter(sys::Meter);

impl Meter {
    /// The loudest sample (0–1) the output plays now.
    pub fn peak(&self) -> Result<f32, LoopbackError> {
        self.0.peak()
    }
}

/// A running loopback stream; stopped when dropped.
pub struct Loopback(sys::Loopback);

impl Loopback {
    pub fn format(&self) -> Format {
        self.0.format()
    }

    /// Everything that came in since the last read, as mono samples
    /// appended to `mono`. Nothing playing gives no frames at all.
    pub fn read(&mut self, mono: &mut Vec<f32>) -> Result<ReadInfo, LoopbackError> {
        self.0.read(mono)
    }
}

/// When the stream should be open: the policy of [`Capture`], without
/// Windows, so it is tested on its own.
#[derive(Debug, Clone, Default)]
pub struct Gate {
    /// `Some` while the stream is open: when sound was last heard, and
    /// whether any was heard since it opened.
    open: Option<(Instant, bool)>,
    /// When the stream last closed (or the gate started), for the slower
    /// meter after a long quiet.
    quiet_since: Option<Instant>,
    /// No opening before this.
    hold_until: Option<Instant>,
}

impl Gate {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The stream is closed and the meter reads `peak`: open now?
    pub fn should_open(&self, peak: f32, now: Instant) -> bool {
        self.open.is_none() && peak >= OPEN_PEAK && self.hold_until.is_none_or(|t| now >= t)
    }

    /// The stream opened.
    pub fn opened(&mut self, now: Instant) {
        self.open = Some((now, false));
        self.hold_until = None;
    }

    /// The stream failed to open: wait before the next try.
    pub fn failed(&mut self, now: Instant) {
        self.hold_until = Some(now + RETRY_AFTER_ERROR);
    }

    /// The open stream gave samples whose loudest is `peak` (0 when none
    /// came). False when it is time to close it.
    pub fn heard(&mut self, peak: f32, now: Instant) -> bool {
        let Some((last, any)) = &mut self.open else { return false };
        if peak >= SOUND_PEAK {
            *last = now;
            *any = true;
        }
        now.saturating_duration_since(*last) < CLOSE_AFTER
    }

    /// The stream closed (silence, an error or a switch of device). One
    /// that never heard anything holds the next opening back for a while.
    pub fn closed(&mut self, now: Instant) {
        if let Some((_, any)) = self.open.take() {
            if !any {
                self.hold_until = Some(now + RETRY_AFTER_DEAF);
            }
        }
        self.quiet_since = Some(now);
    }

    /// How long to wait before the next meter read while closed.
    pub fn meter_every(&self, now: Instant) -> Duration {
        match self.quiet_since {
            Some(t) if now.saturating_duration_since(t) >= LONG_QUIET => METER_EVERY_LONG_QUIET,
            _ => METER_EVERY,
        }
    }
}

/// Where [`Capture`] stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Windows' sound system could not be started.
    Unavailable,
    /// No speakers or headphones.
    NoOutput,
    /// Nothing plays: the stream is closed, only the meter is read. Poll
    /// again after [`Capture::wait`].
    Quiet,
    /// The stream is open: poll at the frame rate.
    Open,
}

/// What one [`Capture::poll`] got.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Poll {
    pub state: State,
    /// Sample rate of the samples added (0 while closed).
    pub rate: u32,
    /// Mono samples added.
    pub frames: usize,
    /// The added samples do not continue the ones before (a new stream, a
    /// switch of device, a gap): forget the history.
    pub reset: bool,
    /// Loudest of the added samples, or the meter's reading while closed
    /// (0–1).
    pub peak: f32,
}

/// The default output's sound, read only while something plays (see the
/// module notes).
pub struct Capture {
    /// Field order: the stream, meter and device go before COM.
    stream: Option<Loopback>,
    device: Option<(Device, Meter)>,
    output: Option<Output>,
    gate: Gate,
    next_look: Instant,
}

impl Default for Capture {
    fn default() -> Self {
        Self::new()
    }
}

impl Capture {
    /// Never fails: without Windows' sound system every poll says so (and
    /// it is tried again every [`LOOK_EVERY`]).
    pub fn new() -> Self {
        let now = Instant::now();
        let mut gate = Gate::default();
        gate.closed(now);
        Self { stream: None, device: None, output: Output::new(), gate, next_look: now }
    }

    /// The open stream's format.
    pub fn format(&self) -> Option<Format> {
        self.stream.as_ref().map(Loopback::format)
    }

    /// How long to wait before the next poll while not [`State::Open`].
    pub fn wait(&self) -> Duration {
        self.gate.meter_every(Instant::now())
    }

    /// Read what there is. Appends mono samples to `out` while the stream
    /// is open (the caller empties it).
    pub fn poll(&mut self, out: &mut Vec<f32>) -> Poll {
        let now = Instant::now();
        let mut poll = Poll { state: State::Quiet, rate: 0, frames: 0, reset: false, peak: 0.0 };
        if now >= self.next_look {
            self.next_look = now + LOOK_EVERY;
            poll.reset |= self.look(now);
        }
        if self.output.is_none() {
            return Poll { state: State::Unavailable, ..poll };
        }
        let Some((device, meter)) = &self.device else {
            return Poll { state: State::NoOutput, ..poll };
        };
        if self.stream.is_none() {
            let peak = match meter.peak() {
                Ok(peak) => peak,
                Err(e) => {
                    tracing::debug!("output meter failed: {e}");
                    self.lost(now);
                    return Poll { state: State::NoOutput, ..poll };
                }
            };
            poll.peak = peak;
            if !self.gate.should_open(peak, now) {
                return poll;
            }
            match device.open() {
                Ok(s) => {
                    tracing::debug!(format = %s.format(), "sound capture started");
                    self.stream = Some(s);
                    self.gate.opened(now);
                    poll.reset = true;
                }
                Err(e) => {
                    tracing::warn!("sound capture not started: {e}");
                    self.gate.failed(now);
                    return poll;
                }
            }
        }
        let Some(stream) = &mut self.stream else { return poll };
        let start = out.len();
        match stream.read(out) {
            Ok(info) => {
                poll.rate = stream.format().rate;
                poll.frames = info.frames;
                poll.reset |= info.discontinuity;
                poll.peak = out[start..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
                if self.gate.heard(poll.peak, now) {
                    poll.state = State::Open;
                } else {
                    tracing::debug!("sound capture stopped: quiet");
                    self.close(now);
                }
            }
            Err(e) => {
                out.truncate(start);
                tracing::info!("sound capture lost: {e}");
                self.lost(now);
                poll.state = State::NoOutput;
            }
        }
        poll
    }

    /// Look the default output up again (and Windows' sound system, if it
    /// was not there). True when an open stream moved to another device.
    fn look(&mut self, now: Instant) -> bool {
        if self.output.is_none() {
            self.output = Output::new();
        }
        let Some(output) = &self.output else { return false };
        let d = match output.default_device() {
            Ok(d) => d,
            Err(e) => {
                if self.device.take().is_some() {
                    tracing::info!("sound output gone: {e}");
                }
                self.close(now);
                return false;
            }
        };
        if self.device.as_ref().is_some_and(|(old, _)| old.id() == d.id()) {
            return false;
        }
        let was_open = self.stream.take().is_some();
        if self.device.take().is_some() {
            tracing::info!("sound output switched");
        }
        match d.meter() {
            Ok(m) => self.device = Some((d, m)),
            Err(e) => tracing::debug!("output meter not opened: {e}"),
        }
        if !was_open {
            return false;
        }
        // It was playing: keep listening on the new device.
        match self.device.as_ref().map(|(d, _)| d.open()) {
            Some(Ok(s)) => {
                tracing::debug!(format = %s.format(), "sound capture moved to the new output");
                self.stream = Some(s);
                return true;
            }
            Some(Err(e)) => {
                tracing::warn!("sound capture not started: {e}");
                self.gate.closed(now);
                self.gate.failed(now);
            }
            None => self.gate.closed(now),
        }
        false
    }

    fn close(&mut self, now: Instant) {
        self.stream = None;
        if self.gate.is_open() {
            self.gate.closed(now);
        }
    }

    /// The device failed: drop it and look again on the next poll.
    fn lost(&mut self, now: Instant) {
        self.close(now);
        self.device = None;
        self.next_look = now;
    }
}

#[cfg(windows)]
mod sys {
    use super::{Extensible, Format, LoopbackError, ReadInfo, WaveFormat, BUFFER, TAG_EXTENSIBLE};
    use std::rc::Rc;
    use windows::core::HRESULT;
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_DEVICE_INVALIDATED,
        AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

    /// `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)`: no default device of the kind.
    const NOT_FOUND: HRESULT = HRESULT(0x8007_0490_u32 as i32);

    fn os(e: &windows::core::Error) -> LoopbackError {
        match e.code() {
            AUDCLNT_E_DEVICE_INVALIDATED => LoopbackError::DeviceLost,
            NOT_FOUND => LoopbackError::NoOutput,
            HRESULT(code) => LoopbackError::Os(code),
        }
    }

    /// Every COM object below holds one, so COM stays started on this
    /// thread until the last of them is gone (field order: the COM object
    /// first, this last).
    type Com = Rc<crate::sys::Com>;

    pub struct Output {
        enumerator: IMMDeviceEnumerator,
        com: Com,
    }

    impl Output {
        pub fn new() -> Option<Self> {
            let com = Rc::new(crate::sys::Com::start());
            // SAFETY: COM is started on this thread.
            let enumerator = unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()?;
            Some(Self { enumerator, com })
        }

        pub fn default_device(&self) -> Result<Device, LoopbackError> {
            // SAFETY: COM calls on a live interface of this thread.
            let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }.map_err(|e| os(&e))?;
            // SAFETY: as above.
            let p = unsafe { device.GetId() }.map_err(|e| os(&e))?;
            // SAFETY: GetId returns a NUL-terminated string the caller
            // frees with CoTaskMemFree, once.
            let id = unsafe {
                let id = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                id
            };
            Ok(Device { device, id, com: self.com.clone() })
        }
    }

    pub struct Device {
        device: IMMDevice,
        id: String,
        com: Com,
    }

    impl Device {
        pub fn id(&self) -> &str {
            &self.id
        }

        pub fn meter(&self) -> Result<Meter, LoopbackError> {
            // SAFETY: COM call on a live interface of this thread; no
            // activation parameters.
            let meter = unsafe { self.device.Activate(CLSCTX_ALL, None) }.map_err(|e| os(&e))?;
            Ok(Meter { meter, _com: self.com.clone() })
        }

        pub fn open(&self) -> Result<Loopback, LoopbackError> {
            // SAFETY: COM call on a live interface of this thread; no
            // activation parameters.
            let client: IAudioClient = unsafe { self.device.Activate(CLSCTX_ALL, None) }.map_err(|e| os(&e))?;
            // SAFETY: on success the pointer is a format block Windows
            // allocated; `MixFormat` frees it once.
            let mix = MixFormat(unsafe { client.GetMixFormat() }.map_err(|e| os(&e))?);
            let format = Format::from_wave(&mix.read()?)?;
            // 100 ns units.
            let buffer = BUFFER.as_nanos() as i64 / 100;
            // SAFETY: the mix format block is valid for the call; shared
            // mode takes no periodicity.
            unsafe {
                client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, buffer, 0, mix.0, None)
            }
            .map_err(|e| os(&e))?;
            // SAFETY: the client is initialised.
            let capture: IAudioCaptureClient = unsafe { client.GetService() }.map_err(|e| os(&e))?;
            // SAFETY: as above.
            let frames = unsafe { client.GetBufferSize() }.map_err(|e| os(&e))?;
            // SAFETY: as above; stopped in Drop.
            unsafe { client.Start() }.map_err(|e| os(&e))?;
            Ok(Loopback { capture, client, format, buffer_frames: frames, _com: self.com.clone() })
        }
    }

    /// The mix format block from `GetMixFormat`, freed when dropped.
    struct MixFormat(*mut WAVEFORMATEX);

    impl MixFormat {
        fn read(&self) -> Result<WaveFormat, LoopbackError> {
            if self.0.is_null() {
                return Err(LoopbackError::Unavailable);
            }
            // SAFETY: a valid WAVEFORMATEX from GetMixFormat (packed, so
            // align 1: reading it whole is fine).
            let w = unsafe { self.0.read_unaligned() };
            let extensible = (w.wFormatTag == TAG_EXTENSIBLE && usize::from(w.cbSize) >= 22).then(|| {
                // SAFETY: the tag and the size say the block is a
                // WAVEFORMATEXTENSIBLE.
                let x = unsafe { self.0.cast::<WAVEFORMATEXTENSIBLE>().read_unaligned() };
                // SAFETY: every member of the union is a u16.
                let valid_bits = unsafe { x.Samples.wValidBitsPerSample };
                let g = x.SubFormat;
                Extensible { valid_bits, sub_format: (g.data1, g.data2, g.data3, g.data4) }
            });
            Ok(WaveFormat {
                tag: w.wFormatTag,
                channels: w.nChannels,
                rate: w.nSamplesPerSec,
                block_align: w.nBlockAlign,
                bits: w.wBitsPerSample,
                extensible,
            })
        }
    }

    impl Drop for MixFormat {
        fn drop(&mut self) {
            // SAFETY: allocated by Windows with CoTaskMemAlloc; freed once.
            unsafe { CoTaskMemFree(Some(self.0 as *const _)) };
        }
    }

    pub struct Meter {
        meter: IAudioMeterInformation,
        _com: Com,
    }

    impl Meter {
        pub fn peak(&self) -> Result<f32, LoopbackError> {
            // SAFETY: COM call on a live interface of this thread.
            let v = unsafe { self.meter.GetPeakValue() }.map_err(|e| os(&e))?;
            Ok(if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 })
        }
    }

    pub struct Loopback {
        capture: IAudioCaptureClient,
        client: IAudioClient,
        format: Format,
        buffer_frames: u32,
        _com: Com,
    }

    impl Loopback {
        pub fn format(&self) -> Format {
            self.format
        }

        pub fn read(&mut self, mono: &mut Vec<f32>) -> Result<ReadInfo, LoopbackError> {
            let mut info = ReadInfo::default();
            let frame = self.format.frame_bytes();
            // Packets come every ~10 ms; a buffer's worth twice over is
            // more than can be waiting, so the loop always ends.
            let mut budget = (self.buffer_frames as usize).max(1) * 2;
            loop {
                // SAFETY: COM call on a live interface of this thread.
                let next = unsafe { self.capture.GetNextPacketSize() }.map_err(|e| os(&e))?;
                if next == 0 || budget == 0 {
                    break;
                }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                // SAFETY: valid out pointers; the buffer is released below
                // before the next GetBuffer.
                unsafe { self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None) }
                    .map_err(|e| os(&e))?;
                if frames == 0 {
                    // Nothing was handed out ("buffer empty"): nothing to
                    // give back either.
                    // SAFETY: releasing zero frames is always allowed.
                    let _ = unsafe { self.capture.ReleaseBuffer(0) };
                    break;
                }
                let count = frames as usize;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    info.silent = true;
                    mono.resize(mono.len() + count, 0.0);
                } else {
                    // SAFETY: GetBuffer handed out `frames` frames of
                    // `frame` bytes each at `data`, valid until released.
                    let bytes = unsafe { std::slice::from_raw_parts(data, count * frame) };
                    super::mix_to_mono(bytes, &self.format, mono);
                }
                if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                    info.discontinuity = true;
                }
                info.frames += count;
                budget = budget.saturating_sub(count);
                // SAFETY: gives back exactly the packet GetBuffer handed out.
                unsafe { self.capture.ReleaseBuffer(frames) }.map_err(|e| os(&e))?;
            }
            Ok(info)
        }
    }

    impl Drop for Loopback {
        fn drop(&mut self) {
            // SAFETY: COM call on a live interface of this thread; stopping
            // a stream that already failed only returns an error.
            let _ = unsafe { self.client.Stop() };
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::{Format, LoopbackError, ReadInfo};

    pub struct Output;

    impl Output {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn default_device(&self) -> Result<Device, LoopbackError> {
            Err(LoopbackError::Unavailable)
        }
    }

    pub struct Device;

    impl Device {
        pub fn id(&self) -> &str {
            ""
        }
        pub fn meter(&self) -> Result<Meter, LoopbackError> {
            Err(LoopbackError::Unavailable)
        }
        pub fn open(&self) -> Result<Loopback, LoopbackError> {
            Err(LoopbackError::Unavailable)
        }
    }

    pub struct Meter;

    impl Meter {
        pub fn peak(&self) -> Result<f32, LoopbackError> {
            Err(LoopbackError::Unavailable)
        }
    }

    pub struct Loopback(Format);

    impl Loopback {
        pub fn format(&self) -> Format {
            self.0
        }
        pub fn read(&mut self, _mono: &mut Vec<f32>) -> Result<ReadInfo, LoopbackError> {
            Err(LoopbackError::Unavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCM_GUID: (u32, u16, u16, [u8; 8]) = (1, 0, 0x10, [0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    const FLOAT_GUID: (u32, u16, u16, [u8; 8]) = (3, 0, 0x10, [0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);

    fn plain(tag: u16, channels: u16, bits: u16) -> WaveFormat {
        WaveFormat { tag, channels, rate: 48_000, block_align: channels * bits / 8, bits, extensible: None }
    }

    fn extensible(channels: u16, bits: u16, valid_bits: u16, sub_format: (u32, u16, u16, [u8; 8])) -> WaveFormat {
        WaveFormat {
            tag: TAG_EXTENSIBLE,
            extensible: Some(Extensible { valid_bits, sub_format }),
            ..plain(TAG_EXTENSIBLE, channels, bits)
        }
    }

    fn sample(w: WaveFormat) -> Result<Sample, LoopbackError> {
        Format::from_wave(&w).map(|f| f.sample)
    }

    #[test]
    fn sub_format_guids_name_their_tag() {
        assert_eq!(sub_format_tag(PCM_GUID), Some(TAG_PCM));
        assert_eq!(sub_format_tag(FLOAT_GUID), Some(TAG_FLOAT));
        // Dolby and other GUIDs outside the classic family.
        assert_eq!(sub_format_tag((3, 0, 0x10, [0; 8])), None);
        assert_eq!(sub_format_tag((0x1_0003, 0, 0x10, PCM_GUID.3)), None);
    }

    #[test]
    fn the_shared_mix_format_is_read() {
        // What Windows nearly always reports in shared mode.
        let mix = extensible(2, 32, 32, FLOAT_GUID);
        assert_eq!(Format::from_wave(&mix), Ok(Format { rate: 48_000, channels: 2, sample: Sample::F32 }));
        assert_eq!(Format::from_wave(&mix).unwrap().to_string(), "48000 Hz, 2 channels, 32-bit float");
        for channels in 1..=8 {
            assert_eq!(sample(extensible(channels, 32, 32, FLOAT_GUID)), Ok(Sample::F32), "{channels} channels");
            assert_eq!(sample(extensible(channels, 16, 16, PCM_GUID)), Ok(Sample::I16), "{channels} channels");
            assert_eq!(sample(extensible(channels, 24, 24, PCM_GUID)), Ok(Sample::I24), "{channels} channels");
            assert_eq!(sample(extensible(channels, 32, 24, PCM_GUID)), Ok(Sample::I32), "{channels} channels");
            assert_eq!(sample(extensible(channels, 32, 32, PCM_GUID)), Ok(Sample::I32), "{channels} channels");
        }
        assert_eq!(sample(plain(TAG_FLOAT, 2, 32)), Ok(Sample::F32));
        assert_eq!(sample(plain(TAG_PCM, 2, 16)), Ok(Sample::I16));
        assert_eq!(sample(plain(TAG_PCM, 1, 8)), Ok(Sample::U8));
        assert_eq!(sample(plain(TAG_PCM, 6, 24)), Ok(Sample::I24));
    }

    #[test]
    fn unknown_or_broken_formats_are_refused() {
        let refused = |w: WaveFormat| assert_eq!(Format::from_wave(&w), Err(LoopbackError::Format(w)));
        // 64-bit float, 12-bit PCM, a compressed tag, an unknown GUID.
        refused(plain(TAG_FLOAT, 2, 64));
        refused(plain(TAG_PCM, 2, 12));
        refused(plain(0x0055, 2, 16));
        refused(extensible(2, 32, 32, (3, 0, 0x10, [0; 8])));
        // Extensible without its part, valid bits over the container.
        refused(WaveFormat { extensible: None, ..extensible(2, 32, 32, FLOAT_GUID) });
        refused(extensible(2, 16, 24, PCM_GUID));
        refused(extensible(2, 32, 0, PCM_GUID));
        // No channels, too many, no rate, a block size that does not fit.
        refused(plain(TAG_PCM, 0, 16));
        refused(plain(TAG_PCM, MAX_CHANNELS + 1, 16));
        refused(WaveFormat { rate: 0, ..plain(TAG_PCM, 2, 16) });
        refused(WaveFormat { block_align: 6, ..plain(TAG_PCM, 2, 16) });
        assert!(LoopbackError::Format(plain(0x0055, 2, 16)).to_string().contains("0x0055"));
    }

    fn mono(data: &[u8], channels: u16, sample: Sample) -> Vec<f32> {
        let mut out = vec![9.0];
        let n = mix_to_mono(data, &Format { rate: 48_000, channels, sample }, &mut out);
        assert_eq!(out.remove(0), 9.0, "appends, keeps what was there");
        assert_eq!(out.len(), n);
        out
    }

    #[test]
    fn interleaved_frames_are_averaged_to_mono() {
        let f32s = |v: &[f32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        // Stereo: left only, right only, both, opposite.
        let data = f32s(&[1.0, 0.0, 0.0, -0.5, 0.25, 0.25, 0.5, -0.5]);
        assert_eq!(mono(&data, 2, Sample::F32), vec![0.5, -0.25, 0.25, 0.0]);
        // Left and right give the same value: no direction reaches the keys.
        assert_eq!(mono(&f32s(&[0.8, 0.0]), 2, Sample::F32), mono(&f32s(&[0.0, 0.8]), 2, Sample::F32));
        // 5.1: six channels per frame.
        assert_eq!(mono(&f32s(&[0.25; 12]), 6, Sample::F32), vec![0.25, 0.25]);
        // A trailing partial frame is left out.
        assert_eq!(mono(&f32s(&[0.5, 0.5, 0.5]), 2, Sample::F32), vec![0.5]);
        // Overs are clipped, broken values are silence.
        assert_eq!(mono(&f32s(&[3.0, f32::NAN, f32::INFINITY]), 1, Sample::F32), vec![1.0, 0.0, 0.0]);
        assert_eq!(mono(&[], 2, Sample::F32), Vec::<f32>::new());
    }

    #[test]
    fn integer_samples_scale_to_full_range() {
        let i16s = |v: &[i16]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        assert_eq!(mono(&i16s(&[i16::MIN, 0, 16_384]), 1, Sample::I16), vec![-1.0, 0.0, 0.5]);
        assert_eq!(mono(&i16s(&[16_384, -16_384]), 2, Sample::I16), vec![0.0]);
        // 24-bit packed: full negative, half positive, −1 LSB.
        let i24 = [0x00, 0x00, 0x80, 0x00, 0x00, 0x40, 0xFF, 0xFF, 0xFF];
        let got = mono(&i24, 1, Sample::I24);
        assert_eq!(&got[..2], &[-1.0, 0.5]);
        assert!(got[2] < 0.0 && got[2] > -1e-6);
        let i32s = |v: &[i32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        assert_eq!(mono(&i32s(&[i32::MIN, 1 << 30, 0]), 1, Sample::I32), vec![-1.0, 0.5, 0.0]);
        assert_eq!(mono(&[0, 128, 255], 1, Sample::U8), vec![-1.0, 0.0, 127.0 / 128.0]);
    }

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn the_stream_opens_on_sound_and_closes_after_silence() {
        let t = Instant::now();
        let mut gate = Gate::default();
        gate.closed(t);
        assert!(!gate.should_open(0.0, t));
        assert!(!gate.should_open(OPEN_PEAK / 2.0, t), "faint noise keeps it closed");
        assert!(gate.should_open(0.2, t));
        gate.opened(t);
        assert!(gate.is_open());
        assert!(!gate.should_open(0.2, t), "already open");
        assert!(gate.heard(0.3, at(t, 50)));
        // A pause between songs is shorter than the close time.
        assert!(gate.heard(0.0, at(t, 3_000)));
        assert!(gate.heard(0.5, at(t, 4_000)));
        assert!(gate.heard(0.0, at(t, 4_000 + CLOSE_AFTER.as_millis() as u64 - 50)));
        assert!(!gate.heard(0.0, at(t, 4_000 + CLOSE_AFTER.as_millis() as u64)), "closes after the quiet");
        gate.closed(at(t, 9_000));
        assert!(!gate.is_open());
        // Sound again: opens at once (it heard sound, so no hold).
        assert!(gate.should_open(0.1, at(t, 9_250)));
    }

    #[test]
    fn a_stream_that_hears_nothing_is_not_reopened_at_once() {
        // Exclusive mode or protected content: the meter moves, the stream
        // stays silent.
        let t = Instant::now();
        let mut gate = Gate::default();
        gate.opened(t);
        assert!(gate.heard(SOUND_PEAK / 2.0, at(t, 1_000)));
        assert!(!gate.heard(0.0, at(t, CLOSE_AFTER.as_millis() as u64)));
        gate.closed(at(t, 5_000));
        assert!(!gate.should_open(0.5, at(t, 6_000)));
        assert!(gate.should_open(0.5, at(t, 5_000) + RETRY_AFTER_DEAF));
    }

    #[test]
    fn a_failed_open_waits_before_the_next_try() {
        let t = Instant::now();
        let mut gate = Gate::default();
        gate.failed(t);
        assert!(!gate.should_open(0.5, at(t, 1_000)));
        assert!(gate.should_open(0.5, t + RETRY_AFTER_ERROR));
    }

    #[test]
    fn the_meter_slows_down_after_a_long_quiet() {
        let t = Instant::now();
        let mut gate = Gate::default();
        assert_eq!(gate.meter_every(t), METER_EVERY);
        gate.closed(t);
        assert_eq!(gate.meter_every(at(t, 60_000)), METER_EVERY);
        assert_eq!(gate.meter_every(t + LONG_QUIET), METER_EVERY_LONG_QUIET);
        gate.opened(t + LONG_QUIET);
        gate.closed(t + LONG_QUIET);
        assert_eq!(gate.meter_every(t + LONG_QUIET), METER_EVERY, "fresh quiet");
    }

    /// By hand: `cargo test -p kb-winsys -- --ignored --nocapture loopback_stream`.
    /// Opens the stream even in silence and reads for half a second.
    #[test]
    #[ignore]
    fn print_a_loopback_stream() {
        let output = Output::new().expect("Windows sound system");
        let device = output.default_device().expect("an output device");
        println!("meter peak {:?}", device.meter().and_then(|m| m.peak()));
        let mut stream = device.open().expect("loopback stream");
        println!("format: {}", stream.format());
        let mut mono = Vec::new();
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(50));
            let info = stream.read(&mut mono).expect("read");
            let peak = mono.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            println!("{info:?}, peak {peak}");
            mono.clear();
        }
    }

    #[test]
    fn errors_read_as_text() {
        assert_eq!(LoopbackError::DeviceLost.to_string(), "sound output device went away");
        assert_eq!(LoopbackError::Os(0x8889_0010_u32 as i32).to_string(), "Windows sound error 0x88890010");
    }
}
