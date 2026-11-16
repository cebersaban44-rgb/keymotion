//! Program icons, for the lists of games and per-app profile rules and for a
//! card's colour.
//!
//! Only the program's file is read: the first icon in it, the one Explorer
//! shows (`SHDefExtractIconW`), or Windows' default program icon when the
//! file has none (`SHGetFileInfoW`). No program is opened, nothing is loaded
//! into one and nothing is hooked: games run under anti-cheat software
//! (keymotion-notes: oyun-oturumu-ve-otomatik-profil.md §5.2). A running
//! game's file is read like any other file.
//!
//! The icon is drawn into a bitmap of the size asked for (`DrawIconEx`).
//! Windows draws an icon with an alpha channel premultiplied; an old icon
//! has none, and its black-and-white mask says which pixels show.

use std::path::Path;

/// Largest icon a program file holds.
const MAX_SIZE: u32 = 256;

/// The icon of a program file, `size`×`size` pixels, as RGBA (straight
/// alpha, rows top to bottom). A file without an icon of its own gets
/// Windows' default program icon. `None` when `path` is empty or not a
/// file, when `size` is 0 or above 256, or when Windows gives no icon.
/// Reads the file: call it off the UI thread.
pub fn exe_icon(path: &str, size: u32) -> Option<Vec<u8>> {
    if path.is_empty() || size == 0 || size > MAX_SIZE {
        return None;
    }
    // The shell answers a path that is not there with a generic icon.
    if !Path::new(path).is_file() {
        return None;
    }
    sys::exe_icon(path, size)
}

/// What Windows drew (blue, green, red, alpha; premultiplied) as RGBA with
/// straight alpha. `opaque` is the icon's mask, one per pixel, for an old
/// icon without an alpha channel: Windows leaves every alpha at 0 for it.
#[cfg_attr(not(windows), allow(dead_code))]
fn to_rgba(mut pixels: Vec<u8>, opaque: &[bool]) -> Vec<u8> {
    let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
    for (i, p) in pixels.chunks_exact_mut(4).enumerate() {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = if opaque.get(i).copied().unwrap_or(false) { 255 } else { 0 };
        }
        match p[3] {
            // Nothing shows: no colour either, whatever the icon left there.
            0 => p[..3].fill(0),
            255 => {}
            a => {
                let a = u16::from(a);
                for c in &mut p[..3] {
                    *c = u8::try_from((u16::from(*c) * 255 + a / 2) / a).unwrap_or(u8::MAX);
                }
            }
        }
    }
    pixels
}

#[cfg(windows)]
mod sys {
    use super::to_rgba;
    use std::ffi::c_void;
    use std::sync::{Mutex, PoisonError};
    use windows::core::HSTRING;
    use windows::Win32::Foundation::S_OK;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    };
    use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{SHDefExtractIconW, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_FLAGS, DI_MASK, DI_NORMAL, HICON};

    /// An icon, destroyed when dropped.
    pub struct Icon(pub HICON);

    impl Drop for Icon {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: the icon was made for this value and is destroyed
                // once, here.
                let _ = unsafe { DestroyIcon(self.0) };
            }
        }
    }

    /// A drawing surface in memory, deleted when dropped.
    struct MemoryDc(HDC);

    impl Drop for MemoryDc {
        fn drop(&mut self) {
            // SAFETY: made by CreateCompatibleDC, deleted once, here.
            let _ = unsafe { DeleteDC(self.0) };
        }
    }

    /// A bitmap, deleted when dropped. Declared after its surface and before
    /// [`Selected`], so it is out of the surface before it goes.
    struct Bitmap(HBITMAP);

    impl Drop for Bitmap {
        fn drop(&mut self) {
            // SAFETY: made by CreateDIBSection, deleted once, here.
            let _ = unsafe { DeleteObject(self.0.into()) };
        }
    }

    /// A bitmap put into a surface: the surface's own one goes back when
    /// dropped.
    struct Selected {
        dc: HDC,
        old: HGDIOBJ,
    }

    impl Drop for Selected {
        fn drop(&mut self) {
            // SAFETY: the surface is still live (dropped after this); `old`
            // came out of it.
            unsafe { SelectObject(self.dc, self.old) };
        }
    }

    pub fn exe_icon(path: &str, size: u32) -> Option<Vec<u8>> {
        let path = HSTRING::from(path);
        let icon = from_file(&path, size).or_else(|| shell_default(&path))?;
        let (pixels, opaque) = render(&icon, size)?;
        Some(to_rgba(pixels, &opaque))
    }

    /// The first icon in the file at `size` (Windows picks the closest image
    /// and scales it). `None` when the file has no icon or cannot be read.
    pub fn from_file(path: &HSTRING, size: u32) -> Option<Icon> {
        let mut large = HICON::default();
        // SAFETY: a NUL-terminated path that outlives the call and one out
        // pointer; no small icon is asked for. The size's low word is the
        // large icon's (callers keep it at 256 or below).
        let result = unsafe { SHDefExtractIconW(path, 0, 0, Some(&mut large), None, size & 0xFFFF) };
        let icon = Icon(large);
        // S_FALSE: the file has no icon.
        (result == S_OK && !icon.0.is_invalid()).then_some(icon)
    }

    /// The shell's icon for the file: Windows' default program icon when the
    /// file has none of its own.
    pub fn shell_default(path: &HSTRING) -> Option<Icon> {
        // Two first calls at once can both fail (the shell sets up its icon
        // list on the first one): one at a time. A poisoned lock guards no
        // data, so it is taken all the same.
        static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        // The shell wants COM on the calling thread: single-threaded, undone
        // below when this call started it. A thread already in the other
        // mode can still use it.
        // SAFETY: plain COM start-up for this thread.
        let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let mut info = SHFILEINFOW::default();
        // SAFETY: a NUL-terminated path that outlives the call and an out
        // structure with its size.
        let found = unsafe {
            SHGetFileInfoW(
                path,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            )
        };
        if init.is_ok() {
            // SAFETY: balances the successful CoInitializeEx above. The icon
            // is not a COM object and stays valid.
            unsafe { CoUninitialize() };
        }
        let icon = Icon(info.hIcon);
        (found != 0 && !icon.0.is_invalid()).then_some(icon)
    }

    /// Draw `icon` at `size`×`size`: the pixels as Windows drew them (blue,
    /// green, red, alpha; premultiplied, rows top to bottom) and, per pixel,
    /// whether the icon's mask shows it.
    pub fn render(icon: &Icon, size: u32) -> Option<(Vec<u8>, Vec<bool>)> {
        let side = i32::try_from(size).ok()?;
        let len = size as usize * size as usize * 4;
        // SAFETY: a memory surface like the screen's; no other arguments.
        let dc = MemoryDc(unsafe { CreateCompatibleDC(None) });
        if dc.0.is_invalid() {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: side,
                // Negative: rows top to bottom.
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: a filled-in header of a 32-bit bitmap without a palette and
        // an out pointer for its pixels.
        let bitmap = Bitmap(unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?);
        if bits.is_null() {
            return None;
        }
        // SAFETY: both handles are live; the bitmap is in no other surface.
        let old = unsafe { SelectObject(dc.0, bitmap.0.into()) };
        if old.is_invalid() {
            return None;
        }
        let _selected = Selected { dc: dc.0, old };

        let draw = |flags: DI_FLAGS| -> Option<Vec<u8>> {
            // SAFETY: `bits` holds the bitmap's `len` bytes, which live as
            // long as `bitmap`; nothing else writes to them now.
            unsafe { std::ptr::write_bytes(bits.cast::<u8>(), 0, len) };
            // SAFETY: a live surface and icon.
            unsafe { DrawIconEx(dc.0, 0, 0, icon.0, side, side, 0, None, flags) }.ok()?;
            // GDI may hold drawing back: finish it before reading the pixels.
            // SAFETY: no arguments.
            let _ = unsafe { GdiFlush() };
            // SAFETY: as above; the pixels are copied out before the next
            // drawing.
            Some(unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), len) }.to_vec())
        };
        // The mask first: black where an old icon shows, white where not.
        let mask = draw(DI_MASK)?;
        let opaque = mask.chunks_exact(4).map(|p| p[..3] == [0, 0, 0]).collect();
        Some((draw(DI_NORMAL)?, opaque))
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn exe_icon(_path: &str, _size: u32) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_pixels_come_back_straight() {
        let drawn = vec![
            0, 0, 64, 128, // half-see-through red, premultiplied
            10, 20, 30, 255, // solid
            5, 5, 5, 0, // see-through, with colour left over
        ];
        let rgba = to_rgba(drawn, &[false; 3]);
        assert_eq!(rgba, [128, 0, 0, 128, 30, 20, 10, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn old_icons_take_their_alpha_from_the_mask() {
        let drawn = vec![10, 20, 30, 0, 0, 0, 0, 0, 255, 255, 255, 0];
        let rgba = to_rgba(drawn, &[true, false, true]);
        assert_eq!(rgba, [30, 20, 10, 255, 0, 0, 0, 0, 255, 255, 255, 255]);
    }

    #[test]
    fn nothing_for_a_missing_file_or_an_empty_path() {
        assert_eq!(exe_icon("", 32), None);
        assert_eq!(exe_icon(r"C:\definitely\not\here.exe", 32), None);
    }

    #[cfg(windows)]
    mod on_windows {
        use super::super::{exe_icon, sys};
        use windows::core::HSTRING;
        use windows::Win32::System::Threading::{GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS, GR_USEROBJECTS};
        use windows::Win32::UI::WindowsAndMessaging::CreateIcon;

        const NOTEPAD: &str = r"C:\Windows\System32\notepad.exe";

        fn alphas(rgba: &[u8]) -> Vec<u8> {
            rgba.chunks_exact(4).map(|p| p[3]).collect()
        }

        #[test]
        fn notepad_s_icon_at_32() {
            let rgba = exe_icon(NOTEPAD, 32).expect("notepad has an icon");
            assert_eq!(rgba.len(), 32 * 32 * 4);
            let alphas = alphas(&rgba);
            assert!(alphas.contains(&255), "something solid");
            assert!(alphas.contains(&0), "something see-through");
            // The top-left corner of a program icon is empty.
            assert_eq!(alphas[0], 0);
            let first = &rgba[..4];
            assert!(rgba.chunks_exact(4).any(|p| p != first), "not one colour");
        }

        #[test]
        fn notepad_s_icon_at_48() {
            let rgba = exe_icon(NOTEPAD, 48).expect("notepad has an icon");
            assert_eq!(rgba.len(), 48 * 48 * 4);
            assert!(alphas(&rgba).contains(&255));
        }

        #[test]
        fn nothing_for_a_folder_or_a_size_out_of_range() {
            assert_eq!(exe_icon(r"C:\Windows", 32), None);
            assert_eq!(exe_icon(NOTEPAD, 0), None);
            assert_eq!(exe_icon(NOTEPAD, 257), None);
            assert!(exe_icon(NOTEPAD, 256).is_some_and(|v| v.len() == 256 * 256 * 4));
        }

        #[test]
        fn a_program_without_an_icon_gets_the_default_one() {
            // Rust puts no icon into the test program.
            let me = std::env::current_exe().unwrap().display().to_string();
            assert!(sys::from_file(&HSTRING::from(me.as_str()), 32).is_none(), "{me} has no icon of its own");
            let rgba = exe_icon(&me, 32).expect("the default program icon");
            assert!(alphas(&rgba).contains(&255));
        }

        #[test]
        fn windows_draws_icons_premultiplied() {
            let icon = sys::from_file(&HSTRING::from(NOTEPAD), 48).expect("notepad has an icon");
            let (drawn, _) = sys::render(&icon, 48).unwrap();
            assert!(drawn.chunks_exact(4).any(|p| p[3] != 0 && p[3] != 255), "soft edges");
            for p in drawn.chunks_exact(4) {
                assert!(p[..3].iter().all(|&c| c <= p[3]), "colour above alpha: {p:?}");
            }
        }

        #[test]
        fn old_icons_are_drawn_from_their_mask() {
            // A black-and-white 32×32 icon: the left half shows (white on
            // top, black below), the right half does not.
            let mut and = Vec::new();
            let mut xor = Vec::new();
            for y in 0..32 {
                and.extend([0x00, 0x00, 0xFF, 0xFF]);
                xor.extend(if y < 16 { [0xFF, 0xFF, 0x00, 0x00] } else { [0x00; 4] });
            }
            // SAFETY: two 32×32 one-bit masks, rows of four bytes.
            let icon = unsafe { CreateIcon(None, 32, 32, 1, 1, and.as_ptr(), xor.as_ptr()) }.unwrap();
            let icon = sys::Icon(icon);
            let (drawn, opaque) = sys::render(&icon, 32).unwrap();
            let rgba = super::super::to_rgba(drawn, &opaque);
            let px = |x: usize, y: usize| &rgba[(y * 32 + x) * 4..][..4];
            assert_eq!(px(3, 3), [255, 255, 255, 255]);
            assert_eq!(px(3, 20), [0, 0, 0, 255]);
            assert_eq!(px(20, 3), [0, 0, 0, 0]);
            assert_eq!(px(31, 31), [0, 0, 0, 0]);
        }

        #[test]
        fn no_handles_are_left_behind() {
            let me = std::env::current_exe().unwrap().display().to_string();
            let count = || {
                // SAFETY: this process's own pseudo handle; nothing is opened.
                let process = unsafe { GetCurrentProcess() };
                // SAFETY: as above.
                unsafe { GetGuiResources(process, GR_GDIOBJECTS) + GetGuiResources(process, GR_USEROBJECTS) }
            };
            // The shell keeps a few things after its first call.
            exe_icon(NOTEPAD, 32);
            exe_icon(&me, 32);
            let before = count();
            for _ in 0..200 {
                assert!(exe_icon(NOTEPAD, 32).is_some());
                assert!(exe_icon(&me, 48).is_some());
            }
            // A leak would add hundreds; other tests running meanwhile only
            // a few.
            let after = count();
            assert!(after < before + 50, "{before} → {after}");
        }
    }
}
