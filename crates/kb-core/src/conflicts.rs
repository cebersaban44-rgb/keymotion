//! Programs that talk to the keyboard's vendor HID channel.
//!
//! The keyboard runs one protocol session at a time: when two programs send
//! commands at once, packets of one land in the other's session and the
//! keyboard swallows or misreads them (docs/DURUM.md §6, lesson 5). Live
//! (RAM-only) frames recover on the next frame, but a broken write to the
//! keyboard's memory does not, so writes are refused while one of these runs.

/// A known program that may talk to the same HID channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conflict {
    /// The official AJAZZ driver (`DeviceDriver.exe`, installed under a
    /// folder named after the keyboard).
    AjazzDriver,
    SignalRgb,
    OpenRgb,
    /// This project's developer tool, `kb.exe`.
    KbCli,
}

impl Conflict {
    /// Stable id, used in the backend's error text (lib/errors.ts maps it).
    pub fn id(self) -> &'static str {
        match self {
            Self::AjazzDriver => "ajazz-driver",
            Self::SignalRgb => "signalrgb",
            Self::OpenRgb => "openrgb",
            Self::KbCli => "kb-cli",
        }
    }
}

/// Executable names, compared without case.
const BY_NAME: &[(&str, Conflict)] = &[
    ("devicedriver.exe", Conflict::AjazzDriver),
    ("signalrgb.exe", Conflict::SignalRgb),
    ("openrgb.exe", Conflict::OpenRgb),
    ("kb.exe", Conflict::KbCli),
];

/// Any program whose path names the keyboard is taken to be its driver
/// (the official one installs as `…\AK980 PRO\…`).
const PATH_MARK: &str = "ak980";

/// Which known program a running executable is, if any. `name` is the file
/// name (`SignalRgb.exe`); `path` the full path when the OS gave it.
pub fn conflict_of(name: &str, path: Option<&str>) -> Option<Conflict> {
    let name = name.to_ascii_lowercase();
    if let Some((_, c)) = BY_NAME.iter().find(|(n, _)| *n == name) {
        return Some(*c);
    }
    let marked = |s: &str| s.to_ascii_lowercase().contains(PATH_MARK);
    (marked(&name) || path.is_some_and(marked)).then_some(Conflict::AjazzDriver)
}

/// The first known program among `running` (pid, file name, full path),
/// leaving out this process itself.
pub fn find_conflict<'a>(
    running: impl IntoIterator<Item = (u32, &'a str, Option<&'a str>)>,
    own_pid: u32,
) -> Option<Conflict> {
    running.into_iter().filter(|(pid, ..)| *pid != own_pid).find_map(|(_, name, path)| conflict_of(name, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_programs_match_by_name_without_case() {
        assert_eq!(conflict_of("DeviceDriver.exe", None), Some(Conflict::AjazzDriver));
        assert_eq!(
            conflict_of("SignalRgb.exe", Some(r"C:\Users\u\AppData\Local\VortxEngine\app-2.3\SignalRgb.exe")),
            Some(Conflict::SignalRgb)
        );
        assert_eq!(conflict_of("OpenRGB.exe", None), Some(Conflict::OpenRgb));
        assert_eq!(conflict_of("KB.EXE", None), Some(Conflict::KbCli));
    }

    #[test]
    fn a_path_naming_the_keyboard_is_its_driver() {
        let path = r"C:\Program Files (x86)\AJAZZ\AK980 PRO\Driver.exe";
        assert_eq!(conflict_of("Driver.exe", Some(path)), Some(Conflict::AjazzDriver));
        assert_eq!(conflict_of("ak980pro-tool.exe", None), Some(Conflict::AjazzDriver));
    }

    #[test]
    fn other_programs_do_not_match() {
        assert_eq!(conflict_of("explorer.exe", Some(r"C:\Windows\explorer.exe")), None);
        // Names only close to a known one.
        assert_eq!(conflict_of("kbd.exe", None), None);
        assert_eq!(conflict_of("OpenRGB-helper.exe", None), None);
        assert_eq!(conflict_of("DeviceDriver.dll", None), None);
        assert_eq!(conflict_of("Code.exe", None), None);
    }

    #[test]
    fn the_app_itself_is_left_out() {
        let running = [
            (4, "System", None),
            (100, "keymotion.exe", Some(r"D:\Tools\AK980\keymotion.exe")),
            (200, "notepad.exe", Some(r"C:\Windows\notepad.exe")),
        ];
        assert_eq!(find_conflict(running, 100), None, "own process, even under a folder named after the keyboard");
        assert_eq!(find_conflict(running, 1), Some(Conflict::AjazzDriver));
        let with_signal = [(200, "notepad.exe", None), (300, "SignalRgb.exe", None)];
        assert_eq!(find_conflict(with_signal, 100).map(Conflict::id), Some("signalrgb"));
    }
}
