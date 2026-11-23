//! Whether a path stays on this computer, found out without reaching
//! anywhere: what a drive letter stands for (`QueryDosDeviceW`, Windows'
//! own table of names; no connection is made) and what kind of link a file
//! or folder is (its reparse tag, read with the link itself opened, never
//! what it leads to). keymotion-cli reads the files an agent names only
//! when they are here.

use std::path::Path;

/// Where a drive letter leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drive {
    /// A device of this computer: a disk's volume, a CD, a RAM disk.
    Local,
    /// A network share, a network redirector, a remote desktop's drive, or
    /// a name of another kind that is not known to be local.
    Remote,
    /// A folder of another drive (`subst P: C:\work`): its path (`C:\work`).
    Folder(String),
    /// No drive by that letter.
    Missing,
}

/// Windows' network redirectors, by their device names (any case): SMB
/// (`Mup`, `LanmanRedirector`), WebDAV, a remote desktop's drives
/// (`\\tsclient`), WSL's `\\wsl$`, DFS.
const REDIRECTORS: [&str; 6] = [
    r"\Device\Mup",
    r"\Device\LanmanRedirector",
    r"\Device\WebDavRedirector",
    r"\Device\RdpDr",
    r"\Device\P9Rdr",
    r"\Device\WinDfs",
];

/// The prefixes of a name that leads to another name (`DefineDosDevice`,
/// `subst`): `\??\UNC\…` is a share, `\??\C:\…` a folder.
const OTHER_NAMES: [&str; 3] = [r"\??\", r"\DosDevices\", r"\GLOBAL??\"];

/// What drive `letter` (A–Z) stands for.
pub fn drive(letter: char) -> Drive {
    classify(sys::dos_device(letter).as_deref())
}

/// [`drive`], from the name Windows gives for it (`None`: no such drive).
pub fn classify(target: Option<&str>) -> Drive {
    let Some(target) = target else { return Drive::Missing };
    let starts = |prefix: &str| {
        target.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(prefix))
            && matches!(target.as_bytes().get(prefix.len()), None | Some(b'\\'))
    };
    if REDIRECTORS.iter().any(|r| starts(r)) {
        return Drive::Remote;
    }
    let Some(rest) = OTHER_NAMES
        .iter()
        .find_map(|p| target.get(..p.len()).filter(|t| t.eq_ignore_ascii_case(p)).map(|_| &target[p.len()..]))
    else {
        return Drive::Local;
    };
    let bytes = rest.as_bytes();
    let on_a_drive = bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes.get(2), None | Some(b'\\'));
    if on_a_drive {
        let mut folder = rest.to_string();
        if folder.len() == 2 {
            folder.push('\\');
        }
        return Drive::Folder(folder);
    }
    Drive::Remote
}

/// What a file or folder is, looked at without following it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// Not there.
    Missing,
    /// A file or a folder (a cloud file's placeholder too: it is no link).
    Plain,
    /// A symbolic link: it may lead anywhere, a share included.
    Symlink,
    /// A junction or a volume mounted in a folder: Windows lets these lead
    /// only to a volume of this computer.
    MountPoint,
    /// Another kind of link (a WSL link, …).
    OtherLink,
    /// It could not be looked at (no right to): nothing is known of it.
    Unknown,
}

/// `IO_REPARSE_TAG_SYMLINK`.
const TAG_SYMLINK: u32 = 0xA000_000C;
/// `IO_REPARSE_TAG_MOUNT_POINT`.
const TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// The tag's "name surrogate" bit: the reparse point stands for another
/// file or folder (a link).
const NAME_SURROGATE: u32 = 0x2000_0000;
/// `FILE_ATTRIBUTE_REPARSE_POINT`.
const ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// What `path` is.
pub fn entry(path: &Path) -> Entry {
    sys::attributes(path)
        .map_or_else(|missing| if missing { Entry::Missing } else { Entry::Unknown }, |(a, t)| kind(a, t))
}

/// [`entry`] from a file's attributes and reparse tag.
pub fn kind(attributes: u32, tag: u32) -> Entry {
    if attributes & ATTRIBUTE_REPARSE_POINT == 0 {
        return Entry::Plain;
    }
    match tag {
        TAG_SYMLINK => Entry::Symlink,
        TAG_MOUNT_POINT => Entry::MountPoint,
        t if t & NAME_SURROGATE != 0 => Entry::OtherLink,
        _ => Entry::Plain,
    }
}

#[cfg(windows)]
mod sys {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FileAttributeTagInfo, GetFileInformationByHandleEx, QueryDosDeviceW, FILE_ATTRIBUTE_TAG_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    pub fn dos_device(letter: char) -> Option<String> {
        if !letter.is_ascii_alphabetic() {
            return None;
        }
        let name: Vec<u16> = format!("{}:", letter.to_ascii_uppercase()).encode_utf16().chain(Some(0)).collect();
        let mut buf = vec![0u16; 1024];
        // SAFETY: a NUL-terminated name and a buffer with its length.
        let n = unsafe { QueryDosDeviceW(PCWSTR(name.as_ptr()), Some(&mut buf)) } as usize;
        if n == 0 {
            return None;
        }
        // The first of the NUL-separated names.
        let first = buf[..n.min(buf.len())].split(|&c| c == 0).next().unwrap_or_default();
        Some(String::from_utf16_lossy(first))
    }

    /// The attributes and reparse tag of `path` itself; `Err(true)` when it
    /// is not there, `Err(false)` when it could not be looked at.
    pub fn attributes(path: &Path) -> Result<(u32, u32), bool> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: a NUL-terminated path; no security attributes or template.
        // OPEN_REPARSE_POINT opens a link itself, never what it leads to.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
        }
        .map_err(|e| e.code() == ERROR_FILE_NOT_FOUND.to_hresult() || e.code() == ERROR_PATH_NOT_FOUND.to_hresult())?;
        let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
        // SAFETY: an open handle and a buffer of the class's size.
        let read = unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileAttributeTagInfo,
                (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        };
        // SAFETY: the handle opened above, closed once.
        let _ = unsafe { CloseHandle(handle) };
        read.map(|()| (info.FileAttributes, info.ReparseTag)).map_err(|_| false)
    }
}

#[cfg(not(windows))]
mod sys {
    use std::path::Path;

    pub fn dos_device(_letter: char) -> Option<String> {
        None
    }

    pub fn attributes(path: &Path) -> Result<(u32, u32), bool> {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.file_type().is_symlink() => Ok((super::ATTRIBUTE_REPARSE_POINT, super::TAG_SYMLINK)),
            Ok(_) => Ok((0, 0)),
            Err(e) => Err(e.kind() == std::io::ErrorKind::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drive_letter_is_known_by_its_device() {
        assert_eq!(classify(Some(r"\Device\HarddiskVolume3")), Drive::Local);
        assert_eq!(classify(Some(r"\Device\CdRom0")), Drive::Local);
        assert_eq!(classify(None), Drive::Missing);
        // Network drives, however Windows spells them.
        for remote in [
            r"\Device\LanmanRedirector\;Z:000000000001a2b3\example.invalid\share",
            r"\Device\Mup\;LanmanRedirector\;Z:000000000001a2b3\example.invalid\share",
            r"\device\mup\example.invalid\share",
            r"\Device\WebDavRedirector\;W:000000000001a2b3\example.invalid\DavWWWRoot",
            r"\Device\RdpDr\;T:1\tsclient\C",
            r"\Device\P9Rdr\;L:000000000001a2b3\wsl$\Ubuntu",
            r"\??\UNC\example.invalid\share",
            r"\DosDevices\UNC\example.invalid\share",
        ] {
            assert_eq!(classify(Some(remote)), Drive::Remote, "{remote}");
        }
        // Another name not known to be local is taken as not local.
        assert_eq!(classify(Some(r"\??\Volume{00000000-0000-0000-0000-000000000000}\")), Drive::Remote);
        // `subst P: C:\work`: that folder.
        assert_eq!(classify(Some(r"\??\C:\work")), Drive::Folder(r"C:\work".into()));
        assert_eq!(classify(Some(r"\??\D:")), Drive::Folder(r"D:\".into()));
        // A device whose name only starts like a redirector's.
        assert_eq!(classify(Some(r"\Device\MupLike0")), Drive::Local);
    }

    #[test]
    fn a_link_is_told_by_its_tag() {
        assert_eq!(kind(0x10, 0), Entry::Plain);
        assert_eq!(kind(0x410, TAG_SYMLINK), Entry::Symlink);
        assert_eq!(kind(0x410, TAG_MOUNT_POINT), Entry::MountPoint);
        // A WSL link (IO_REPARSE_TAG_LX_SYMLINK) is a link of another kind;
        // a OneDrive placeholder (IO_REPARSE_TAG_CLOUD_6) is a file.
        assert_eq!(kind(0x400, 0xA000_001D), Entry::OtherLink);
        assert_eq!(kind(0x400, 0x9000_601A), Entry::Plain);
    }

    #[cfg(windows)]
    #[test]
    fn files_folders_and_junctions_on_this_computer() {
        let dir = std::env::temp_dir().join(format!("kb-winsys-local-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("real")).unwrap();
        std::fs::write(dir.join("real").join("x.json"), "[]").unwrap();
        assert_eq!(entry(&dir.join("real")), Entry::Plain);
        assert_eq!(entry(&dir.join("real").join("x.json")), Entry::Plain);
        assert_eq!(entry(&dir.join("nothing")), Entry::Missing);
        assert_eq!(entry(&dir.join("nothing").join("x.json")), Entry::Missing);
        let junction = dir.join("link");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(dir.join("real"))
            .output()
            .is_ok_and(|o| o.status.success());
        if made {
            assert_eq!(entry(&junction), Entry::MountPoint);
        }
        let _ = std::fs::remove_dir(&junction);
        let _ = std::fs::remove_dir_all(&dir);
        // The system drive is a disk of this computer.
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert_eq!(drive(system.chars().next().unwrap()), Drive::Local);
    }
}
