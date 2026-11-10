//! TFT screen upload (`04 72` + display channel FF68): the image format.
//!
//! Sources and measurements: docs/research/protokol-ff13.md §10 and
//! docs/research/ekran-derin-arastirma.md §6.
//!
//! Only a validated [`ScreenAnimation`] can be turned into upload pages, so
//! whatever the user imported, the keyboard always receives the one format
//! it was built for: 240×135 RGB565, 1–140 frames, delays the firmware
//! handles safely.

use super::Invalid;

/// Panel width in pixels.
pub const WIDTH: usize = 240;
/// Panel height in pixels.
pub const HEIGHT: usize = 135;
/// Pixels in one frame.
pub const FRAME_PIXELS: usize = WIDTH * HEIGHT;
/// Bytes in one frame (RGB565, little-endian).
pub const FRAME_BYTES: usize = FRAME_PIXELS * 2;
/// Blob header: frame count, one delay byte per frame, `FF` padding.
pub const HEADER_LEN: usize = 256;
/// One output report on the display channel.
pub const PAGE: usize = 4096;
/// Most frames the firmware takes (official driver `gif_maxframes`). A
/// sibling board's menu graphics were destroyed by going past its limit.
pub const MAX_FRAMES: usize = 140;
/// Pages of a 140-frame upload; a hard ceiling for the announce header.
pub const MAX_PAGES: usize = 2215;
/// Frames proven by the keyboard's own factory animation (135 frames).
/// 136–140 are allowed by the official driver but untested here.
pub const PROVEN_FRAMES: usize = 135;
/// Delay unit of the blob header, in milliseconds.
pub const DELAY_UNIT_MS: u32 = 2;
/// Shortest delay byte (the official driver writes `max(1, ms / 2)`).
pub const MIN_DELAY: u8 = 1;
/// Longest delay byte. `FF` is the header's padding value: the firmware
/// takes it as the end of the list and the screen tears between images.
pub const MAX_DELAY: u8 = 254;
/// Slot byte the official driver sends (`gif_count = 1`; other values
/// replace the same single animation).
pub const SLOT: u8 = 1;
/// Delay byte for a still image (100 ms). With one frame the value only
/// sets how often the firmware redraws it; 50 is what S1 sent on AK980 V2
/// hardware, and it avoids redrawing every 2 ms.
pub const STILL_DELAY: u8 = 50;

/// Pack 8-bit RGB into RGB565.
pub const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 >> 3) << 11) | ((g as u16 >> 2) << 5) | (b as u16 >> 3)
}

/// Expand RGB565 back to 8-bit RGB (for previews).
pub const fn rgb888(p: u16) -> [u8; 3] {
    let r = ((p >> 11) & 0x1F) as u8;
    let g = ((p >> 5) & 0x3F) as u8;
    let b = (p & 0x1F) as u8;
    [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
}

/// Delay byte for a frame duration in milliseconds, clamped to what the
/// firmware handles (2–508 ms).
pub fn delay_from_ms(ms: u32) -> u8 {
    let units = (ms + DELAY_UNIT_MS / 2) / DELAY_UNIT_MS;
    units.clamp(MIN_DELAY as u32, MAX_DELAY as u32) as u8
}

/// One 240×135 RGB565 frame, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame(Box<[u16]>);

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({WIDTH}x{HEIGHT})")
    }
}

impl Frame {
    /// A frame filled with one colour.
    pub fn filled(color: u16) -> Self {
        Self(vec![color; FRAME_PIXELS].into_boxed_slice())
    }

    /// Take exactly `FRAME_PIXELS` RGB565 values.
    pub fn from_pixels(pixels: Vec<u16>) -> Result<Self, Invalid> {
        if pixels.len() != FRAME_PIXELS {
            return Err(Invalid::Screen("frame must be 240×135 pixels"));
        }
        Ok(Self(pixels.into_boxed_slice()))
    }

    pub fn pixels(&self) -> &[u16] {
        &self.0
    }

    pub fn get(&self, x: usize, y: usize) -> u16 {
        self.0[y * WIDTH + x]
    }

    pub fn set(&mut self, x: usize, y: usize, color: u16) {
        if x < WIDTH && y < HEIGHT {
            self.0[y * WIDTH + x] = color;
        }
    }

    /// Fill a rectangle, clipped to the panel.
    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u16) {
        for yy in y..(y + h).min(HEIGHT) {
            for xx in x..(x + w).min(WIDTH) {
                self.0[yy * WIDTH + xx] = color;
            }
        }
    }
}

/// What the keyboard can show: 1–140 frames with safe delays. The only
/// input to [`upload_pages`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenAnimation {
    frames: Vec<(Frame, u8)>,
}

impl ScreenAnimation {
    /// Validate frames and their delay bytes.
    pub fn new(frames: Vec<(Frame, u8)>) -> Result<Self, Invalid> {
        if frames.is_empty() {
            return Err(Invalid::Screen("an animation needs at least one frame"));
        }
        if frames.len() > MAX_FRAMES {
            return Err(Invalid::Screen("at most 140 frames"));
        }
        if frames.iter().any(|(_, d)| !(MIN_DELAY..=MAX_DELAY).contains(d)) {
            return Err(Invalid::Screen("frame delay must be 1–254 (2–508 ms)"));
        }
        Ok(Self { frames })
    }

    /// One still image.
    pub fn still(frame: Frame) -> Self {
        Self { frames: vec![(frame, STILL_DELAY)] }
    }

    pub fn frames(&self) -> &[(Frame, u8)] {
        &self.frames
    }

    /// Pages the upload takes.
    pub fn pages(&self) -> usize {
        pages_for(self.frames.len())
    }

    /// One loop, in milliseconds.
    pub fn loop_ms(&self) -> u32 {
        self.frames.iter().map(|(_, d)| *d as u32 * DELAY_UNIT_MS).sum()
    }
}

/// Pages an upload of `frames` frames takes: ⌈(256 + 64 800·n) / 4096⌉.
pub const fn pages_for(frames: usize) -> usize {
    (HEADER_LEN + FRAME_BYTES * frames).div_ceil(PAGE)
}

/// The upload blob: header, frames (RGB565 LE), `FF` padding to whole pages.
pub fn blob(anim: &ScreenAnimation) -> Vec<u8> {
    let pages = anim.pages();
    let mut out = Vec::with_capacity(pages * PAGE);
    out.push(anim.frames.len() as u8);
    out.extend(anim.frames.iter().map(|(_, d)| *d));
    out.resize(HEADER_LEN, 0xFF);
    for (frame, _) in &anim.frames {
        for &p in frame.pixels() {
            out.extend_from_slice(&p.to_le_bytes());
        }
    }
    out.resize(pages * PAGE, 0xFF);
    out
}

/// The blob cut into display-channel pages. Fails only if an internal
/// invariant broke (the page ceiling is checked again here on purpose).
pub fn upload_pages(anim: &ScreenAnimation) -> Result<Vec<[u8; PAGE]>, Invalid> {
    let data = blob(anim);
    let pages = data.len() / PAGE;
    if pages == 0 || pages > MAX_PAGES || data.len() % PAGE != 0 || data[0] as usize > MAX_FRAMES {
        return Err(Invalid::Screen("upload exceeds the page limit"));
    }
    Ok(data
        .chunks_exact(PAGE)
        .map(|c| {
            let mut p = [0u8; PAGE];
            p.copy_from_slice(c);
            p
        })
        .collect())
}

/// A test frame for the first upload: border, coloured corners, an arrow
/// pointing up, primary colour bars and a grey ramp. Shows position,
/// orientation, colour order and the RGB565 gradient at a glance.
pub fn test_pattern() -> Frame {
    let white = rgb565(255, 255, 255);
    let mut f = Frame::filled(rgb565(0, 0, 0));
    // 1 px white border.
    f.fill_rect(0, 0, WIDTH, 1, white);
    f.fill_rect(0, HEIGHT - 1, WIDTH, 1, white);
    f.fill_rect(0, 0, 1, HEIGHT, white);
    f.fill_rect(WIDTH - 1, 0, 1, HEIGHT, white);
    // Corners: top-left red, top-right green, bottom-left blue, bottom-right yellow.
    let s = 24;
    f.fill_rect(2, 2, s, s, rgb565(255, 0, 0));
    f.fill_rect(WIDTH - 2 - s, 2, s, s, rgb565(0, 255, 0));
    f.fill_rect(2, HEIGHT - 2 - s, s, s, rgb565(0, 0, 255));
    f.fill_rect(WIDTH - 2 - s, HEIGHT - 2 - s, s, s, rgb565(255, 255, 0));
    // Arrow pointing up, top centre.
    let cx = WIDTH / 2;
    for row in 0..16 {
        f.fill_rect(cx - row, 6 + row, row * 2 + 1, 1, white);
    }
    f.fill_rect(cx - 4, 22, 9, 18, white);
    // Bars: red, green, blue ramps (left = dark, right = bright).
    let (x0, w) = (40, 160);
    for (i, ch) in [0usize, 1, 2].into_iter().enumerate() {
        let y = 48 + i * 18;
        for x in 0..w {
            let v = (x * 255 / (w - 1)) as u8;
            let c = match ch {
                0 => rgb565(v, 0, 0),
                1 => rgb565(0, v, 0),
                _ => rgb565(0, 0, v),
            };
            f.fill_rect(x0 + x, y, 1, 14, c);
        }
    }
    // Grey ramp.
    for x in 0..w {
        let v = (x * 255 / (w - 1)) as u8;
        f.fill_rect(x0 + x, 102, 1, 14, rgb565(v, v, v));
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_matches_the_wire_format() {
        // Red goes on the wire as `00 F8` (little-endian 0xF800).
        assert_eq!(rgb565(255, 0, 0).to_le_bytes(), [0x00, 0xF8]);
        assert_eq!(rgb565(0, 255, 0), 0x07E0);
        assert_eq!(rgb565(0, 0, 255), 0x001F);
        assert_eq!(rgb888(rgb565(255, 255, 255)), [255, 255, 255]);
        assert_eq!(rgb888(rgb565(0, 0, 0)), [0, 0, 0]);
    }

    #[test]
    fn page_counts_match_the_sources() {
        assert_eq!(pages_for(1), 16);
        assert_eq!(pages_for(24), 380);
        assert_eq!(pages_for(28), 444);
        assert_eq!(pages_for(135), 2136);
        assert_eq!(pages_for(MAX_FRAMES), MAX_PAGES);
    }

    #[test]
    fn delays_are_clamped_and_never_ff() {
        assert_eq!(delay_from_ms(0), 1);
        assert_eq!(delay_from_ms(100), 50);
        assert_eq!(delay_from_ms(101), 51);
        assert_eq!(delay_from_ms(508), 254);
        assert_eq!(delay_from_ms(510), 254, "FF is padding, never a delay");
        assert_eq!(delay_from_ms(60_000), 254);
    }

    #[test]
    fn animation_limits_are_enforced() {
        let f = Frame::filled(0);
        assert!(ScreenAnimation::new(vec![]).is_err());
        assert!(ScreenAnimation::new(vec![(f.clone(), 0)]).is_err(), "delay 0");
        assert!(ScreenAnimation::new(vec![(f.clone(), 255)]).is_err(), "delay FF");
        assert!(ScreenAnimation::new(vec![(f.clone(), 50); MAX_FRAMES]).is_ok());
        assert!(ScreenAnimation::new(vec![(f, 50); MAX_FRAMES + 1]).is_err());
        assert!(Frame::from_pixels(vec![0; FRAME_PIXELS - 1]).is_err());
    }

    #[test]
    fn blob_layout() {
        let a = Frame::filled(rgb565(255, 0, 0));
        let b = Frame::filled(rgb565(0, 0, 255));
        let anim = ScreenAnimation::new(vec![(a, 50), (b, 20)]).unwrap();
        let data = blob(&anim);
        assert_eq!(data.len(), pages_for(2) * PAGE);
        assert_eq!(&data[..4], &[2, 50, 20, 0xFF]);
        assert!(data[3..HEADER_LEN].iter().all(|&x| x == 0xFF));
        assert_eq!(&data[HEADER_LEN..HEADER_LEN + 2], &[0x00, 0xF8], "frame 0 red");
        let f1 = HEADER_LEN + FRAME_BYTES;
        assert_eq!(&data[f1..f1 + 2], &[0x1F, 0x00], "frame 1 blue");
        assert!(data[HEADER_LEN + 2 * FRAME_BYTES..].iter().all(|&x| x == 0xFF), "page padding");
    }

    #[test]
    fn still_image_takes_sixteen_pages() {
        let pages = upload_pages(&ScreenAnimation::still(test_pattern())).unwrap();
        assert_eq!(pages.len(), 16);
        assert_eq!(&pages[0][..3], &[1, STILL_DELAY, 0xFF]);
    }

    #[test]
    fn test_pattern_corners() {
        let f = test_pattern();
        assert_eq!(f.get(0, 0), rgb565(255, 255, 255), "border");
        assert_eq!(f.get(5, 5), rgb565(255, 0, 0), "top-left red");
        assert_eq!(f.get(WIDTH - 6, 5), rgb565(0, 255, 0), "top-right green");
        assert_eq!(f.get(5, HEIGHT - 6), rgb565(0, 0, 255), "bottom-left blue");
        assert_eq!(f.get(WIDTH - 6, HEIGHT - 6), rgb565(255, 255, 0), "bottom-right yellow");
    }
}
