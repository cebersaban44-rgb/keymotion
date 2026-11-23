//! Pictures for a screen card (`keymotion-cli card --image`): what the
//! keyboard's screen takes from an agent, checked on both ends before
//! anything is drawn.
//!
//! Rules (owner's request: pictures wear the screen's memory, so they are
//! kept small; plan §12.1 row 10):
//!
//! * Formats the app's web view draws: PNG, JPEG, GIF and WebP, told apart
//!   by their first bytes, never by a file name.
//! * At most [`MAX_IMAGE_BYTES`] (40 KiB): the picture travels in one
//!   protocol line, base64 inside JSON, and a 240×135 screen needs no more.
//! * Sides from [`MIN_SIDE`] to [`MAX_SIDE`] pixels; the app fits the
//!   picture to the screen.
//! * A card is a still: an animated GIF, APNG or WebP shows its first
//!   frame. One frame is the smallest upload the screen takes (16 of its
//!   4 KiB pages); every further frame would be another 16.
//!
//! Only the headers are read here (no decoder, so keymotion-cli stays
//! small); the web view decodes the picture itself and refuses one it
//! cannot read.

use crate::error::{AgentError, ErrorCode};
use base64::Engine as _;

/// The most bytes a picture may have.
pub const MAX_IMAGE_BYTES: usize = 40 * 1024;
/// The smallest and largest side, in pixels.
pub const MIN_SIDE: u32 = 8;
pub const MAX_SIDE: u32 = 2048;

/// A picture format the screen card takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ImageFormat {
    /// Its media type, for the web view.
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }
}

/// What the header says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// It moves (animated GIF, APNG or WebP): only its first frame shows.
    pub animated: bool,
}

fn bad(message: impl Into<String>) -> AgentError {
    AgentError::new(ErrorCode::BadValue, message)
        .with_hint("Give a PNG, JPEG, GIF or WebP picture of at most 40 KiB, e.g. 240x135 pixels.")
}

/// Check a picture: its format, its size in bytes and its sides.
pub fn check(bytes: &[u8]) -> Result<ImageInfo, AgentError> {
    if bytes.is_empty() {
        return Err(bad("The picture is empty."));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AgentError::new(
            ErrorCode::TooLarge,
            format!("The picture has {} bytes; at most {} fit.", bytes.len(), MAX_IMAGE_BYTES),
        )
        .with_hint("Make it smaller: 240x135 pixels is the screen's size; a JPEG or PNG of it fits."));
    }
    let info = inspect(bytes).ok_or_else(|| bad("This is not a PNG, JPEG, GIF or WebP picture, or it is damaged."))?;
    let sides = MIN_SIDE..=MAX_SIDE;
    if !sides.contains(&info.width) || !sides.contains(&info.height) {
        return Err(bad(format!(
            "The picture is {}x{} pixels; each side must be {MIN_SIDE} to {MAX_SIDE}.",
            info.width, info.height
        )));
    }
    Ok(info)
}

/// The picture as base64 (standard alphabet, padded), as the protocol
/// carries it.
pub fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// The picture from the protocol's base64, checked.
pub fn decode(text: &str) -> Result<(Vec<u8>, ImageInfo), AgentError> {
    // 4 characters for every 3 bytes, a little room for padding.
    if text.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 + 4 {
        return Err(AgentError::new(
            ErrorCode::TooLarge,
            format!("The picture is larger than {MAX_IMAGE_BYTES} bytes."),
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|_| AgentError::new(ErrorCode::BadValue, "The picture is not valid base64."))?;
    let info = check(&bytes)?;
    Ok((bytes, info))
}

/// The format and sides from the header; `None` when it is none of the
/// four or too short to tell.
pub fn inspect(b: &[u8]) -> Option<ImageInfo> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(b)
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        jpeg(b)
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        gif(b)
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        webp(b)
    } else {
        None
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?)))
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?)))
}

fn le24(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 3)?;
    Some(u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16)
}

/// PNG: the first chunk is IHDR; an `acTL` chunk before the image data
/// makes it an APNG.
fn png(b: &[u8]) -> Option<ImageInfo> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    let (width, height) = (be32(b, 16)?, be32(b, 20)?);
    let mut at = 8;
    let mut animated = false;
    while let (Some(len), Some(kind)) = (be32(b, at), b.get(at + 4..at + 8)) {
        if kind == b"acTL" {
            animated = true;
        }
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        at = at.checked_add(12)?.checked_add(len as usize)?;
    }
    Some(ImageInfo { format: ImageFormat::Png, width, height, animated })
}

/// JPEG: the sides are in the first start-of-frame segment.
fn jpeg(b: &[u8]) -> Option<ImageInfo> {
    let mut at = 2;
    loop {
        // Fill bytes may come before a marker.
        while b.get(at) == Some(&0xFF) && b.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if *b.get(at)? != 0xFF {
            return None;
        }
        let marker = *b.get(at + 1)?;
        let len = be16(b, at + 2)? as usize;
        let sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if sof {
            let (height, width) = (be16(b, at + 5)?, be16(b, at + 7)?);
            return Some(ImageInfo { format: ImageFormat::Jpeg, width, height, animated: false });
        }
        if marker == 0xD9 || marker == 0xDA || len < 2 {
            return None;
        }
        at = at.checked_add(2 + len)?;
    }
}

/// GIF: the logical screen's sides; more than one image block moves. An
/// image must lie inside the screen: a renderer grows its canvas to the
/// frame, so a 40 KiB file declaring a 65535x65535 frame on a small screen
/// would ask for gigabytes.
fn gif(b: &[u8]) -> Option<ImageInfo> {
    let (width, height) = (le16(b, 6)?, le16(b, 8)?);
    let flags = *b.get(10)?;
    let mut at = 13;
    if flags & 0x80 != 0 {
        at += 3 * (1usize << ((flags & 0x07) + 1));
    }
    let mut frames = 0;
    // Sub-blocks: a length byte, that many bytes, until a zero length.
    let skip_blocks = |mut at: usize| -> Option<usize> {
        loop {
            let n = *b.get(at)? as usize;
            at += 1;
            if n == 0 {
                return Some(at);
            }
            at += n;
        }
    };
    while frames < 2 {
        match *b.get(at)? {
            0x21 => at = skip_blocks(at + 2)?,
            0x2C => {
                frames += 1;
                let (left, top) = (le16(b, at + 1)?, le16(b, at + 3)?);
                let (w, h) = (le16(b, at + 5)?, le16(b, at + 7)?);
                if w == 0 || h == 0 || left + w > width || top + h > height {
                    return None;
                }
                let local = *b.get(at + 9)?;
                at += 10;
                if local & 0x80 != 0 {
                    at += 3 * (1usize << ((local & 0x07) + 1));
                }
                at = skip_blocks(at + 1)?;
            }
            0x3B => break,
            _ => return None,
        }
    }
    (frames > 0).then_some(ImageInfo { format: ImageFormat::Gif, width, height, animated: frames > 1 })
}

/// WebP: a simple (`VP8 `), lossless (`VP8L`) or extended (`VP8X`) file.
fn webp(b: &[u8]) -> Option<ImageInfo> {
    let kind = b.get(12..16)?;
    let (width, height, animated) = match kind {
        b"VP8 " => {
            // A key frame's start code, then 14-bit sides.
            if b.get(23..26)? != [0x9D, 0x01, 0x2A] {
                return None;
            }
            (le16(b, 26)? & 0x3FFF, le16(b, 28)? & 0x3FFF, false)
        }
        b"VP8L" => {
            if *b.get(20)? != 0x2F {
                return None;
            }
            let bits = u32::from_le_bytes(b.get(21..25)?.try_into().ok()?);
            ((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1, false)
        }
        b"VP8X" => {
            let flags = *b.get(20)?;
            (le24(b, 24)? + 1, le24(b, 27)? + 1, flags & 0x02 != 0)
        }
        _ => return None,
    };
    Some(ImageInfo { format: ImageFormat::Webp, width, height, animated })
}

#[cfg(test)]
pub(crate) mod samples {
    //! Small pictures built by hand, header and all, for the tests.

    /// A PNG's header with these sides (no pixels: enough for [`super::inspect`]).
    pub fn png(width: u32, height: u32, animated: bool) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend(13u32.to_be_bytes());
        b.extend(b"IHDR");
        b.extend(width.to_be_bytes());
        b.extend(height.to_be_bytes());
        b.extend([8, 2, 0, 0, 0]);
        b.extend([0; 4]);
        if animated {
            b.extend(8u32.to_be_bytes());
            b.extend(b"acTL");
            b.extend([0; 8]);
            b.extend([0; 4]);
        }
        b.extend(0u32.to_be_bytes());
        b.extend(b"IDAT");
        b.extend([0; 4]);
        b
    }

    /// A JPEG with an APP0 segment, then a baseline frame of these sides.
    pub fn jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        b.extend(b"JFIF\0");
        b.extend([1, 1, 0, 0, 1, 0, 1, 0, 0]);
        b.extend([0xFF, 0xC0, 0x00, 0x11, 0x08]);
        b.extend(height.to_be_bytes());
        b.extend(width.to_be_bytes());
        b.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
        b.extend([0xFF, 0xD9]);
        b
    }

    /// A GIF of `frames` 1x1 images on a `width` x `height` screen.
    pub fn gif(width: u16, height: u16, frames: usize) -> Vec<u8> {
        gif_frame(width, height, frames, (0, 0, 1, 1))
    }

    /// A GIF of `frames` images at `left`, `top`, `w` x `h`.
    pub fn gif_frame(width: u16, height: u16, frames: usize, (left, top, w, h): (u16, u16, u16, u16)) -> Vec<u8> {
        let mut b = b"GIF89a".to_vec();
        b.extend(width.to_le_bytes());
        b.extend(height.to_le_bytes());
        b.extend([0x80, 0, 0]);
        b.extend([0, 0, 0, 255, 255, 255]);
        // A comment extension first.
        b.extend([0x21, 0xFE, 2, b'h', b'i', 0]);
        for _ in 0..frames {
            b.extend([0x21, 0xF9, 4, 0, 10, 0, 0, 0]);
            b.push(0x2C);
            for v in [left, top, w, h] {
                b.extend(v.to_le_bytes());
            }
            b.push(0);
            b.extend([2, 2, 0x4C, 0x01, 0]);
        }
        b.push(0x3B);
        b
    }

    /// A lossless WebP's header with these sides.
    pub fn webp(width: u32, height: u32) -> Vec<u8> {
        let mut b = b"RIFF".to_vec();
        b.extend(30u32.to_le_bytes());
        b.extend(b"WEBPVP8L");
        b.extend(10u32.to_le_bytes());
        b.push(0x2F);
        let bits = (width - 1) | (height - 1) << 14;
        b.extend(bits.to_le_bytes());
        b.extend([0; 5]);
        b
    }
}

#[cfg(test)]
mod tests {
    use super::samples;
    use super::*;

    #[test]
    fn the_four_formats_are_read_from_their_headers() {
        let png = inspect(&samples::png(240, 135, false)).unwrap();
        assert_eq!((png.format, png.width, png.height, png.animated), (ImageFormat::Png, 240, 135, false));
        assert!(inspect(&samples::png(240, 135, true)).unwrap().animated, "APNG");
        let jpeg = inspect(&samples::jpeg(320, 180)).unwrap();
        assert_eq!((jpeg.format, jpeg.width, jpeg.height), (ImageFormat::Jpeg, 320, 180));
        let gif = inspect(&samples::gif(64, 32, 1)).unwrap();
        assert_eq!((gif.format, gif.width, gif.height, gif.animated), (ImageFormat::Gif, 64, 32, false));
        assert!(inspect(&samples::gif(64, 32, 3)).unwrap().animated);
        let webp = inspect(&samples::webp(240, 135)).unwrap();
        assert_eq!((webp.format, webp.width, webp.height), (ImageFormat::Webp, 240, 135));
        assert_eq!(ImageFormat::Webp.mime(), "image/webp");
    }

    #[test]
    fn what_is_not_a_picture_is_refused() {
        for junk in [
            &b""[..],
            b"hello world",
            b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            b"BM\x36\x00\x00\x00",
            b"\x89PNG\r\n\x1a\n",
            b"GIF89a\x01",
            b"\xFF\xD8\xFF\xD9",
            b"RIFF\x00\x00\x00\x00WEBPVP8X",
        ] {
            let e = check(junk).unwrap_err();
            assert_eq!(e.code, ErrorCode::BadValue, "{junk:?}");
        }
        // A GIF that ends before any image.
        assert!(inspect(b"GIF89a\x01\x00\x01\x00\x00\x00\x00\x3B").is_none());
    }

    #[test]
    fn a_gif_frame_must_lie_inside_its_screen() {
        assert!(check(&samples::gif_frame(240, 135, 1, (0, 0, 240, 135))).is_ok());
        assert!(check(&samples::gif_frame(240, 135, 1, (40, 35, 200, 100))).is_ok());
        // A small screen, a huge first frame: the renderer would grow its
        // canvas to the frame.
        for frame in [(0, 0, 65_535, 65_535), (0, 0, 241, 135), (1, 0, 240, 135), (0, 100, 10, 36), (0, 0, 0, 10)] {
            let e = check(&samples::gif_frame(240, 135, 1, frame)).unwrap_err();
            assert_eq!(e.code, ErrorCode::BadValue, "{frame:?}");
        }
        // The second frame too.
        let mut two = samples::gif_frame(240, 135, 2, (0, 0, 10, 10));
        let second = two.iter().rposition(|&b| b == 0x2C).unwrap();
        two[second + 5..second + 7].copy_from_slice(&5_000u16.to_le_bytes());
        assert!(check(&two).is_err());
    }

    #[test]
    fn size_and_sides_have_limits() {
        assert!(check(&samples::png(240, 135, false)).is_ok());
        assert!(check(&samples::png(MIN_SIDE, MAX_SIDE, false)).is_ok());
        for (w, h) in [(7, 100), (100, 7), (2049, 100), (100, 4000), (0, 0)] {
            let e = check(&samples::png(w, h, false)).unwrap_err();
            assert_eq!(e.code, ErrorCode::BadValue, "{w}x{h}");
            assert!(e.message.contains(&format!("{w}x{h}")), "{}", e.message);
        }
        let mut big = samples::png(240, 135, false);
        big.resize(MAX_IMAGE_BYTES + 1, 0);
        assert_eq!(check(&big).unwrap_err().code, ErrorCode::TooLarge);
        big.truncate(MAX_IMAGE_BYTES);
        assert!(check(&big).is_ok(), "exactly the most");
    }

    #[test]
    fn base64_round_trip_and_bad_text() {
        let png = samples::png(240, 135, false);
        let (back, info) = decode(&encode(&png)).unwrap();
        assert_eq!((back, info.format), (png, ImageFormat::Png));
        assert_eq!(decode("not base64!").unwrap_err().code, ErrorCode::BadValue);
        assert_eq!(decode(&"A".repeat(60_000)).unwrap_err().code, ErrorCode::TooLarge);
        // A full-size picture still fits one protocol line, with room for the rest.
        let mut big = samples::png(240, 135, false);
        big.resize(MAX_IMAGE_BYTES, 0);
        assert!(encode(&big).len() + 1024 < crate::MAX_LINE_BYTES);
    }
}
