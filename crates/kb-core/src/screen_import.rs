//! First stage of the screen import pipeline: decode a user's file into
//! full frames, then build the one format the keyboard takes
//! ([`ScreenAnimation`]). Files are untrusted: size, frame and memory
//! limits apply, and bad data is an error, never a panic.
//!
//! The rule (owner's request, 2026-11-10): whatever the user brings, the
//! keyboard only ever receives what the app prepared, previewed and the
//! user confirmed. Scaling, cropping and frame thinning come in the next
//! stage; for now an image must already be 240×135.

use kb_protocol::sonix::screen::{self, delay_from_ms, rgb565, Frame, ScreenAnimation};
use kb_protocol::sonix::Invalid;

/// Largest file the importer reads.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Largest canvas accepted (either side).
pub const MAX_SIDE: u16 = 4096;
/// Most frames decoded from one file (thinning to 140 happens later).
pub const MAX_SOURCE_FRAMES: usize = 2000;
/// Decoder memory limit per frame (a 4096×4096 RGBA frame).
const FRAME_MEMORY_LIMIT: u64 = 4096 * 4096 * 4;
/// What browsers show for a GIF delay of 0 or 10 ms. The keyboard's own
/// factory animation (delay 0 in the official driver's file) plays at this
/// speed: the owner compared it with a browser preview on 2026-11-10.
pub const GIF_DEFAULT_DELAY_MS: u32 = 100;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("file is larger than 64 MB")]
    FileTooLarge,
    #[error("not a readable GIF: {0}")]
    Decode(String),
    #[error("image is {w}×{h}; at most 4096×4096 is accepted")]
    CanvasTooLarge { w: u16, h: u16 },
    #[error("more than {MAX_SOURCE_FRAMES} frames")]
    TooManySourceFrames,
    #[error("the file has no frames")]
    Empty,
    #[error("image is {w}×{h}; this stage needs exactly 240×135 (scaling comes with the studio)")]
    WrongSize { w: u16, h: u16 },
    #[error("{0} frames; the keyboard takes at most 140")]
    TooManyFrames(usize),
    #[error(transparent)]
    Invalid(#[from] Invalid),
}

/// One composited frame, RGBA, canvas-sized.
#[derive(Clone)]
pub struct DecodedFrame {
    pub rgba: Vec<u8>,
    pub delay_ms: u32,
}

/// A decoded file: canvas size and fully composited frames.
pub struct Decoded {
    pub width: u16,
    pub height: u16,
    pub frames: Vec<DecodedFrame>,
}

/// Decode a GIF, compositing every frame onto the canvas (disposal
/// methods applied), so each output frame is what a viewer would show.
pub fn decode_gif(bytes: &[u8]) -> Result<Decoded, ImportError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ImportError::FileTooLarge);
    }
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    opts.set_memory_limit(gif::MemoryLimit::Bytes(std::num::NonZeroU64::new(FRAME_MEMORY_LIMIT).expect("non-zero")));
    let mut dec = opts.read_info(bytes).map_err(|e| ImportError::Decode(e.to_string()))?;
    let (w, h) = (dec.width(), dec.height());
    if w == 0 || h == 0 {
        return Err(ImportError::Empty);
    }
    if w > MAX_SIDE || h > MAX_SIDE {
        return Err(ImportError::CanvasTooLarge { w, h });
    }
    let (cw, ch) = (w as usize, h as usize);
    let mut canvas = vec![0u8; cw * ch * 4];
    let mut frames = Vec::new();
    while let Some(f) = dec.read_next_frame().map_err(|e| ImportError::Decode(e.to_string()))? {
        if frames.len() == MAX_SOURCE_FRAMES {
            return Err(ImportError::TooManySourceFrames);
        }
        let before = (f.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let (fl, ft, fw, fh) = (f.left as usize, f.top as usize, f.width as usize, f.height as usize);
        for y in 0..fh {
            let cy = ft + y;
            if cy >= ch {
                break;
            }
            for x in 0..fw {
                let cx = fl + x;
                if cx >= cw {
                    break;
                }
                let s = (y * fw + x) * 4;
                let Some(px) = f.buffer.get(s..s + 4) else { continue };
                if px[3] != 0 {
                    let d = (cy * cw + cx) * 4;
                    canvas[d..d + 4].copy_from_slice(px);
                }
            }
        }
        let delay_ms = match f.delay as u32 * 10 {
            0 | 10 => GIF_DEFAULT_DELAY_MS,
            ms => ms,
        };
        frames.push(DecodedFrame { rgba: canvas.clone(), delay_ms });
        match f.dispose {
            gif::DisposalMethod::Background => {
                for y in ft..(ft + fh).min(ch) {
                    for x in fl..(fl + fw).min(cw) {
                        let d = (y * cw + x) * 4;
                        canvas[d..d + 4].fill(0);
                    }
                }
            }
            gif::DisposalMethod::Previous => {
                if let Some(b) = before {
                    canvas = b;
                }
            }
            _ => {}
        }
    }
    if frames.is_empty() {
        return Err(ImportError::Empty);
    }
    Ok(Decoded { width: w, height: h, frames })
}

/// RGBA (exactly 240×135) to an RGB565 frame. Transparent pixels show as
/// black, the panel's background.
pub fn rgba_to_frame(rgba: &[u8]) -> Result<Frame, ImportError> {
    if rgba.len() != screen::FRAME_PIXELS * 4 {
        return Err(Invalid::Screen("frame must be 240×135 pixels").into());
    }
    let pixels = rgba.chunks_exact(4).map(|p| if p[3] == 0 { 0 } else { rgb565(p[0], p[1], p[2]) }).collect();
    Ok(Frame::from_pixels(pixels)?)
}

/// Build the keyboard format from a decoded file that already has the
/// panel's size. `delay_ms` overrides every frame's delay.
pub fn exact_size_animation(decoded: &Decoded, delay_ms: Option<u32>) -> Result<ScreenAnimation, ImportError> {
    if (decoded.width as usize, decoded.height as usize) != (screen::WIDTH, screen::HEIGHT) {
        return Err(ImportError::WrongSize { w: decoded.width, h: decoded.height });
    }
    if decoded.frames.len() > screen::MAX_FRAMES {
        return Err(ImportError::TooManyFrames(decoded.frames.len()));
    }
    let frames = decoded
        .frames
        .iter()
        .map(|f| Ok((rgba_to_frame(&f.rgba)?, delay_from_ms(delay_ms.unwrap_or(f.delay_ms)))))
        .collect::<Result<Vec<_>, ImportError>>()?;
    Ok(ScreenAnimation::new(frames)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2-frame GIF: red at 50 ms, then a blue 10×10 square at (5, 5)
    /// over it (disposal Keep) with delay 0.
    fn sample_gif(width: u16, height: u16) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let palette = [255, 0, 0, 0, 0, 255];
            let mut enc = gif::Encoder::new(&mut out, width, height, &palette).unwrap();
            let f = gif::Frame {
                width,
                height,
                delay: 5,
                buffer: std::borrow::Cow::Owned(vec![0u8; width as usize * height as usize]),
                ..Default::default()
            };
            enc.write_frame(&f).unwrap();
            let g = gif::Frame {
                left: 5,
                top: 5,
                width: 10,
                height: 10,
                delay: 0,
                buffer: std::borrow::Cow::Owned(vec![1u8; 100]),
                ..Default::default()
            };
            enc.write_frame(&g).unwrap();
        }
        out
    }

    #[test]
    fn decodes_and_composites() {
        let d = decode_gif(&sample_gif(240, 135)).unwrap();
        assert_eq!((d.width, d.height, d.frames.len()), (240, 135, 2));
        assert_eq!(d.frames[0].delay_ms, 50);
        assert_eq!(d.frames[1].delay_ms, GIF_DEFAULT_DELAY_MS, "delay 0 plays like in a browser");
        let px = |f: &DecodedFrame, x: usize, y: usize| f.rgba[(y * 240 + x) * 4..][..3].to_vec();
        assert_eq!(px(&d.frames[1], 0, 0), vec![255, 0, 0], "frame 1 keeps frame 0 underneath");
        assert_eq!(px(&d.frames[1], 6, 6), vec![0, 0, 255]);

        let anim = exact_size_animation(&d, None).unwrap();
        assert_eq!(anim.frames().len(), 2);
        assert_eq!(anim.frames()[0].1, 25, "50 ms = 25 units");
        assert_eq!(anim.frames()[1].0.get(6, 6), rgb565(0, 0, 255));
        assert_eq!(exact_size_animation(&d, Some(100)).unwrap().frames()[0].1, 50);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(decode_gif(b"not a gif"), Err(ImportError::Decode(_))));
        let d = decode_gif(&sample_gif(100, 50)).unwrap();
        assert!(matches!(exact_size_animation(&d, None), Err(ImportError::WrongSize { w: 100, h: 50 })));
        let big = vec![0u8; MAX_FILE_BYTES + 1];
        assert!(matches!(decode_gif(&big), Err(ImportError::FileTooLarge)));
    }
}
