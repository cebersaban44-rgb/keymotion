//! The factory animation from AJAZZ's official driver, if the user installed
//! it. The app does not ship AJAZZ's animation; it can back up the copy the
//! driver put on this PC (docs/urun-eksikleri.md B1).
//!
//! The driver keeps it as `gif/<model folder>/0.gif` under its install
//! folder (docs/research/ekran-derin-arastirma.md §1.2). Install folders
//! differ, so the usual program folders are searched a few levels deep for
//! GIF files of the screen's size in a folder whose path names the model.
//! Only each candidate's first ten bytes are read.

use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Folder levels searched below each root (root/a/b/c/d).
const MAX_DEPTH: usize = 4;
/// Most folders one search reads, so a crowded disk cannot make it slow.
const MAX_DIRS: usize = 40_000;
/// Text the folder path must contain (any case).
const MODEL_MARK: &str = "ak980";
/// The driver's own name for the file; preferred when several match.
const DRIVER_NAME: &str = "0.gif";
/// Large folders right under a root that never hold the driver.
const SKIP_TOP: &[&str] = &[
    "windowsapps",
    "windows kits",
    "windows defender",
    "microsoft visual studio",
    "microsoft sdks",
    "reference assemblies",
    "dotnet",
    "msbuild",
    "common files",
];

/// The logical screen size in a GIF's header (`GIF87a`/`GIF89a`, then
/// width and height, little-endian). `None` when it is not a GIF.
pub fn gif_size(head: &[u8]) -> Option<(u16, u16)> {
    if head.len() < 10 || !(head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a")) {
        return None;
    }
    Some((u16::from_le_bytes([head[6], head[7]]), u16::from_le_bytes([head[8], head[9]])))
}

fn file_gif_size(path: &Path) -> Option<(u16, u16)> {
    let mut head = [0u8; 10];
    fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    gif_size(&head)
}

/// Where Windows installs programs: both Program Files folders and the
/// per-user `%LOCALAPPDATA%\Programs`.
pub fn search_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Programs"));
    }
    let mut seen = Vec::new();
    roots.retain(|r| {
        let key = r.to_string_lossy().to_lowercase();
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    roots
}

/// The factory animation below `roots`: a GIF of `width`×`height` in a
/// folder whose path names the model, at most [`MAX_DEPTH`] folders down.
/// Shallow folders are read first; links and junctions are not followed.
/// Of several matches, the driver's own `0.gif` wins, then path order.
pub fn find_factory_gif(roots: &[PathBuf], width: u16, height: u16) -> Option<PathBuf> {
    let mut queue: VecDeque<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    let mut found = Vec::new();
    let mut read = 0;
    while let Some((dir, depth)) = queue.pop_front() {
        if read == MAX_DIRS {
            tracing::debug!(folders = read, "official driver search stopped early");
            break;
        }
        read += 1;
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        let named = dir.to_string_lossy().to_lowercase().contains(MODEL_MARK);
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            // `file_type` does not follow links: a link or junction is
            // neither a folder nor a file here.
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                let skip =
                    depth == 0 && SKIP_TOP.contains(&entry.file_name().to_string_lossy().to_lowercase().as_str());
                if depth < MAX_DEPTH && !skip {
                    subdirs.push(path);
                }
            } else if kind.is_file()
                && named
                && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"))
                && file_gif_size(&path) == Some((width, height))
            {
                found.push(path);
            }
        }
        subdirs.sort();
        queue.extend(subdirs.into_iter().map(|d| (d, depth + 1)));
    }
    tracing::debug!(folders = read, matches = found.len(), "official driver searched");
    found.sort_by_key(|p| (!p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(DRIVER_NAME)), p.clone()));
    found.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A GIF header of the given size (the rest of the file does not
    /// matter to the search).
    fn header(w: u16, h: u16) -> Vec<u8> {
        let mut v = b"GIF89a".to_vec();
        v.extend_from_slice(&w.to_le_bytes());
        v.extend_from_slice(&h.to_le_bytes());
        v.extend_from_slice(&[0xF7, 0, 0, 0, 0, 0]);
        v
    }

    #[test]
    fn reads_the_size_from_the_header() {
        assert_eq!(gif_size(&header(240, 135)), Some((240, 135)));
        let mut old = header(320, 200);
        old[3..6].copy_from_slice(b"87a");
        assert_eq!(gif_size(&old), Some((320, 200)));
        assert_eq!(gif_size(b"\x89PNG\r\n\x1a\n\0\0\0\0"), None, "not a GIF");
        assert_eq!(gif_size(b"GIF89a\xF0"), None, "too short");
    }

    #[test]
    fn finds_the_drivers_gif_in_a_model_folder() {
        let root = std::env::temp_dir().join(format!("klavye-driver-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let put = |rel: &str, bytes: &[u8]| {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, bytes).unwrap();
        };
        // Not named after the model, or the wrong size, or too deep.
        put("Other Vendor/gif/Keyboard/0.gif", &header(240, 135));
        put("AJAZZ Driver/gif/AJAZZ AK980 Keyboard/logo.gif", &header(100, 100));
        put("a/b/c/d/AK980/0.gif", &header(240, 135));
        // The driver's layout: <install>/gif/<model folder>/.
        put("AJAZZ Driver/gif/AJAZZ AK980 三模RGB带屏 Keyboard/1.gif", &header(240, 135));
        put("AJAZZ Driver/gif/AJAZZ AK980 三模RGB带屏 Keyboard/0.gif", &header(240, 135));
        put("AJAZZ Driver/gif/AJAZZ AK980 三模RGB带屏 Keyboard/notes.txt", b"GIF89a\xF0\0\x87\0");

        let found = find_factory_gif(std::slice::from_ref(&root), 240, 135).expect("found");
        assert!(found.ends_with("AJAZZ Driver/gif/AJAZZ AK980 三模RGB带屏 Keyboard/0.gif"), "{found:?}");

        // Without 0.gif the other match is taken; nothing too deep is.
        fs::remove_file(root.join("AJAZZ Driver/gif/AJAZZ AK980 三模RGB带屏 Keyboard/0.gif")).unwrap();
        let found = find_factory_gif(std::slice::from_ref(&root), 240, 135).expect("found");
        assert!(found.ends_with("1.gif"), "{found:?}");
        fs::remove_dir_all(root.join("AJAZZ Driver")).unwrap();
        assert_eq!(find_factory_gif(std::slice::from_ref(&root), 240, 135), None);
        assert_eq!(find_factory_gif(&[root.join("missing")], 240, 135), None, "a missing root is fine");
        let _ = fs::remove_dir_all(&root);
    }
}
