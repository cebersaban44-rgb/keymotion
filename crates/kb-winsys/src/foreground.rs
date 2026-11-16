//! The program in front, for the game session and the per-app profiles.
//!
//! Games run under anti-cheat software, so this module never opens a handle
//! to another program (keymotion-notes: oyun-oturumu-ve-otomatik-profil.md
//! §5.2; docs site: developers/architecture):
//!
//! * The window in front comes from an out-of-context WinEvent hook
//!   (`EVENT_SYSTEM_FOREGROUND`), on a thread of its own with a message
//!   loop. Nothing is loaded into other programs. A timer looks again every
//!   few seconds, as some windows never send the event.
//! * Its process id comes from `GetWindowThreadProcessId`, and the
//!   program's path from `NtQuerySystemInformation`
//!   (`SystemProcessIdInformation`), which reads it from the system
//!   without opening the process. Without a path, a Toolhelp snapshot gives
//!   the file name.
//! * Whether a program still runs: a Toolhelp snapshot of processes (no
//!   modules, no threads).
//!
//! The window's title is never read.

use std::sync::mpsc::Sender;

/// The program whose window is in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Front {
    pub pid: u32,
    /// Executable file name, e.g. `League of Legends.exe`; empty when
    /// unknown.
    pub name: String,
    /// Full path, when Windows gives it.
    pub path: Option<String>,
    /// The window's class name, to tell the desktop and the shell apart.
    pub class: String,
    /// The window covers its whole monitor (full screen or borderless).
    pub covers_monitor: bool,
    pub minimized: bool,
}

/// Windows of the shell that come to the front without being a program the
/// user works in: the desktop, the taskbar, Alt+Tab and Task View, the
/// Start menu and search, the lock screen, menus.
const SHELL_CLASSES: [&str; 11] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
    "ForegroundStaging",
    "TaskSwitcherWnd",
    "TopLevelWindowForOverflowXamlIsland",
    "NotifyIconOverflowWindow",
    "#32768",
];
const SHELL_PROGRAMS: [&str; 9] = [
    "LockApp.exe",
    "StartMenuExperienceHost.exe",
    "SearchHost.exe",
    "SearchApp.exe",
    "ShellExperienceHost.exe",
    "ShellHost.exe",
    "TextInputHost.exe",
    "ApplicationFrameHost.exe",
    "dwm.exe",
];

impl Front {
    /// Part of Windows' own shell (see [`SHELL_CLASSES`]), not a program.
    pub fn is_shell(&self) -> bool {
        SHELL_CLASSES.iter().any(|c| self.class == *c)
            || SHELL_PROGRAMS.iter().any(|p| self.name.eq_ignore_ascii_case(p))
            || self.name.is_empty()
    }
}

/// The last part of a Windows path.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// A device path (`\Device\HarddiskVolume3\Games\x.exe`) as a path with a
/// drive letter, given the devices behind the drives (`("C:",
/// "\Device\HarddiskVolume3")`). `None` when no drive matches.
pub fn dos_path(device_path: &str, drives: &[(String, String)]) -> Option<String> {
    drives.iter().find_map(|(letter, device)| {
        let rest = device_path.strip_prefix(device.as_str())?;
        rest.starts_with('\\').then(|| format!("{letter}{rest}"))
    })
}

/// Watches the window in front and sends each change to `tx`. Dropping it
/// stops the thread.
pub struct Watcher {
    thread: u32,
    join: Option<std::thread::JoinHandle<()>>,
}

impl Watcher {
    /// `None` when the hook could not be set.
    pub fn start(tx: Sender<Front>) -> Option<Self> {
        let (thread, join) = sys::start(tx)?;
        Some(Self { thread, join: Some(join) })
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        sys::stop(self.thread);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// The window in front now, once.
pub fn front_now() -> Option<Front> {
    sys::front_now()
}

/// The program's full path, without opening it; `None` when Windows does
/// not give it.
pub fn image_path(pid: u32) -> Option<String> {
    sys::image_path(pid)
}

/// The program `pid` still runs and is still `name` (a process id can be
/// reused by another program).
pub fn is_running(pid: u32, name: &str) -> bool {
    sys::process_names().iter().any(|(p, n)| *p == pid && n.eq_ignore_ascii_case(name))
}

/// Every running process as (id, executable file name). No handles.
pub fn process_names() -> Vec<(u32, String)> {
    sys::process_names()
}

/// Every running process as (id, executable file name, full path when
/// Windows gives it). No handles.
pub fn processes_with_paths() -> Vec<(u32, String, Option<String>)> {
    sys::processes_with_paths()
}

#[cfg(windows)]
mod sys {
    use super::{dos_path, file_name, Front};
    use std::cell::RefCell;
    use std::sync::mpsc::{self, Sender};
    use windows::core::BOOL;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::Storage::FileSystem::{GetLogicalDrives, QueryDosDeviceW};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, EnumChildWindows, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindowRect,
        GetWindowThreadProcessId, IsIconic, KillTimer, PostThreadMessageW, SetTimer, TranslateMessage,
        EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, MSG, WINEVENT_OUTOFCONTEXT,
        WINEVENT_SKIPOWNPROCESS, WM_QUIT, WM_TIMER,
    };

    /// The timer's look at the window in front, for windows that send no
    /// event.
    const POLL_MS: u32 = 5_000;
    /// `OBJID_WINDOW`: the event is about the window itself.
    const OBJID_WINDOW: i32 = 0;
    /// `SystemProcessIdInformation`.
    const SYSTEM_PROCESS_ID_INFORMATION: u32 = 88;
    /// `STATUS_INFO_LENGTH_MISMATCH`: the buffer was too small.
    const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

    #[repr(C)]
    struct UnicodeString {
        /// Bytes in use, without a NUL.
        length: u16,
        /// Bytes in the buffer.
        maximum_length: u16,
        buffer: *mut u16,
    }

    #[repr(C)]
    struct ProcessIdInformation {
        process_id: usize,
        image_name: UnicodeString,
    }

    // Not in the `windows` crate without the WDK features; ntdll is part of
    // every Windows, and raw-dylib needs no import library.
    #[link(name = "ntdll", kind = "raw-dylib")]
    extern "system" {
        fn NtQuerySystemInformation(class: u32, info: *mut core::ffi::c_void, length: u32, returned: *mut u32) -> i32;
    }

    struct State {
        tx: Sender<Front>,
        /// The window last sent and whether it was minimized.
        last: Option<(isize, bool)>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn text(wide: &[u16]) -> String {
        let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
        String::from_utf16_lossy(&wide[..end])
    }

    /// Send the window in front when it changed.
    fn report() {
        // SAFETY: no arguments.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return;
        }
        // SAFETY: any window handle; a gone window only answers false.
        let minimized = unsafe { IsIconic(hwnd) }.as_bool();
        let key = (hwnd.0 as isize, minimized);
        let changed = STATE.with_borrow_mut(|s| match s {
            Some(s) if s.last != Some(key) => {
                s.last = Some(key);
                true
            }
            _ => false,
        });
        if !changed {
            return;
        }
        if let Some(front) = describe(hwnd) {
            STATE.with_borrow(|s| {
                if let Some(s) = s {
                    let _ = s.tx.send(front);
                }
            });
        }
    }

    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        _event: u32,
        _hwnd: HWND,
        object: i32,
        _child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if object == OBJID_WINDOW {
            report();
        }
    }

    pub fn start(tx: Sender<Front>) -> Option<(u32, std::thread::JoinHandle<()>)> {
        let (ready_tx, ready_rx) = mpsc::channel::<Option<u32>>();
        let join = std::thread::Builder::new()
            .name("foreground".into())
            .spawn(move || {
                STATE.set(Some(State { tx, last: None }));
                let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
                // SAFETY: an out-of-context hook: `on_event` runs on this
                // thread, inside GetMessageW below; no DLL goes anywhere.
                let front = unsafe {
                    SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_event), 0, 0, flags)
                };
                // SAFETY: as above.
                let minimize = unsafe {
                    SetWinEventHook(
                        EVENT_SYSTEM_MINIMIZESTART,
                        EVENT_SYSTEM_MINIMIZEEND,
                        None,
                        Some(on_event),
                        0,
                        0,
                        flags,
                    )
                };
                if front.is_invalid() {
                    let _ = ready_tx.send(None);
                    return;
                }
                // SAFETY: a thread timer without a callback; its WM_TIMER
                // comes through GetMessageW below.
                let timer = unsafe { SetTimer(None, 0, POLL_MS, None) };
                // SAFETY: no arguments.
                let _ = ready_tx.send(Some(unsafe { GetCurrentThreadId() }));
                report();
                let mut msg = MSG::default();
                // SAFETY: a valid MSG to fill; the loop ends on WM_QUIT (0)
                // or an error (-1).
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
                    if msg.message == WM_TIMER {
                        report();
                        continue;
                    }
                    // SAFETY: the message GetMessageW just filled.
                    unsafe {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                // SAFETY: the timer and hooks set above, on this thread.
                unsafe {
                    let _ = KillTimer(None, timer);
                    let _ = UnhookWinEvent(front);
                    if !minimize.is_invalid() {
                        let _ = UnhookWinEvent(minimize);
                    }
                }
                STATE.set(None);
            })
            .ok()?;
        match ready_rx.recv() {
            Ok(Some(thread)) => Some((thread, join)),
            _ => {
                let _ = join.join();
                None
            }
        }
    }

    pub fn stop(thread: u32) {
        // SAFETY: plain call; a thread that has ended only makes it fail.
        let _ = unsafe { PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }

    pub fn front_now() -> Option<Front> {
        // SAFETY: no arguments.
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.0.is_null()).then(|| describe(hwnd)).flatten()
    }

    fn class_of(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: the buffer's length goes with it.
        let n = unsafe { GetClassNameW(hwnd, &mut buf) };
        text(&buf[..(n.max(0) as usize).min(buf.len())])
    }

    fn pid_of(hwnd: HWND) -> u32 {
        let mut pid = 0u32;
        // SAFETY: a valid out pointer; a gone window leaves it 0.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid
    }

    /// A UWP app's window belongs to ApplicationFrameHost; the app's own
    /// window is a child owned by another process.
    fn uwp_app(frame: HWND, host: u32) -> Option<u32> {
        struct Find {
            host: u32,
            found: u32,
        }
        unsafe extern "system" fn each(child: HWND, data: LPARAM) -> BOOL {
            // SAFETY: `data` is the `Find` below, alive for the whole call.
            let find = unsafe { &mut *(data.0 as *mut Find) };
            let pid = pid_of(child);
            if pid != 0 && pid != find.host {
                find.found = pid;
                return BOOL(0);
            }
            BOOL(1)
        }
        let mut find = Find { host, found: 0 };
        // SAFETY: `find` outlives the call; the callback only reads windows.
        let _ = unsafe { EnumChildWindows(Some(frame), Some(each), LPARAM(&raw mut find as isize)) };
        (find.found != 0).then_some(find.found)
    }

    fn covers_monitor(hwnd: HWND) -> bool {
        let mut rect = RECT::default();
        // SAFETY: a valid out pointer.
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
            return false;
        }
        // SAFETY: plain call; the nearest monitor always exists.
        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        // SAFETY: a valid out pointer with its size set.
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
    }

    fn describe(hwnd: HWND) -> Option<Front> {
        let mut pid = pid_of(hwnd);
        if pid == 0 {
            return None;
        }
        let class = class_of(hwnd);
        if class == "ApplicationFrameWindow" {
            pid = uwp_app(hwnd, pid).unwrap_or(pid);
        }
        let path = image_path(pid);
        let name = match &path {
            Some(p) => file_name(p).to_string(),
            None => process_names().into_iter().find(|(p, _)| *p == pid).map(|(_, n)| n).unwrap_or_default(),
        };
        Some(Front {
            pid,
            name,
            path,
            class,
            covers_monitor: covers_monitor(hwnd),
            // SAFETY: any window handle.
            minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        })
    }

    /// The device behind each drive letter, as `("C:", "\Device\…")`.
    fn drives() -> Vec<(String, String)> {
        // SAFETY: no arguments.
        let mask = unsafe { GetLogicalDrives() };
        let mut out = Vec::new();
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = format!("{}:", char::from(b'A' + i as u8));
            let name: Vec<u16> = letter.encode_utf16().chain(Some(0)).collect();
            let mut buf = [0u16; 512];
            // SAFETY: a NUL-terminated name and a buffer with its length.
            let n = unsafe { QueryDosDeviceW(windows::core::PCWSTR(name.as_ptr()), Some(&mut buf)) };
            if n > 0 {
                // The first of the NUL-separated targets.
                out.push((letter, text(&buf[..(n as usize).min(buf.len())])));
            }
        }
        out
    }

    pub fn image_path(pid: u32) -> Option<String> {
        image_path_on(pid, &drives())
    }

    pub fn processes_with_paths() -> Vec<(u32, String, Option<String>)> {
        let drives = drives();
        process_names().into_iter().map(|(pid, name)| (pid, name, image_path_on(pid, &drives))).collect()
    }

    fn image_path_on(pid: u32, drives: &[(String, String)]) -> Option<String> {
        if pid == 0 {
            return None;
        }
        let mut units = 512usize;
        for _ in 0..3 {
            let mut buf = vec![0u16; units];
            let mut info = ProcessIdInformation {
                process_id: pid as usize,
                image_name: UnicodeString {
                    length: 0,
                    maximum_length: (units * 2).min(u16::MAX as usize) as u16,
                    buffer: buf.as_mut_ptr(),
                },
            };
            let mut returned = 0u32;
            // SAFETY: `info` points at `buf`, whose size in bytes is
            // `maximum_length`; the call writes at most that much there.
            let status = unsafe {
                NtQuerySystemInformation(
                    SYSTEM_PROCESS_ID_INFORMATION,
                    (&raw mut info).cast(),
                    size_of::<ProcessIdInformation>() as u32,
                    &mut returned,
                )
            };
            if status == STATUS_INFO_LENGTH_MISMATCH {
                // `maximum_length` now holds the bytes needed.
                units = (usize::from(info.image_name.maximum_length) / 2 + 1).max(units * 2);
                continue;
            }
            if status < 0 {
                return None;
            }
            let len = (usize::from(info.image_name.length) / 2).min(buf.len());
            let device = String::from_utf16_lossy(&buf[..len]);
            return dos_path(&device, drives).or_else(|| (!device.is_empty()).then_some(device));
        }
        None
    }

    pub fn process_names() -> Vec<(u32, String)> {
        // SAFETY: plain call; the snapshot handle is closed below.
        let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        // SAFETY: a valid snapshot handle and an entry with its size set.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
        while more {
            if entry.th32ProcessID != 0 {
                out.push((entry.th32ProcessID, text(&entry.szExeFile)));
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
        }
        // SAFETY: the handle came from CreateToolhelp32Snapshot and is closed once.
        let _ = unsafe { CloseHandle(snapshot) };
        out
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Front;
    use std::sync::mpsc::Sender;

    pub fn start(_tx: Sender<Front>) -> Option<(u32, std::thread::JoinHandle<()>)> {
        None
    }
    pub fn stop(_thread: u32) {}
    pub fn front_now() -> Option<Front> {
        None
    }
    pub fn image_path(_pid: u32) -> Option<String> {
        None
    }
    pub fn process_names() -> Vec<(u32, String)> {
        Vec::new()
    }
    pub fn processes_with_paths() -> Vec<(u32, String, Option<String>)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(name: &str, class: &str) -> Front {
        Front { pid: 42, name: name.into(), path: None, class: class.into(), covers_monitor: false, minimized: false }
    }

    #[test]
    fn the_shell_is_not_a_program() {
        assert!(front("explorer.exe", "Progman").is_shell());
        assert!(front("explorer.exe", "Shell_TrayWnd").is_shell());
        assert!(front("explorer.exe", "XamlExplorerHostIslandWindow").is_shell());
        assert!(front("searchhost.exe", "Windows.UI.Core.CoreWindow").is_shell());
        assert!(front("", "Chrome_WidgetWin_1").is_shell());
        // A File Explorer window is a program the user works in.
        assert!(!front("explorer.exe", "CabinetWClass").is_shell());
        assert!(!front("League of Legends.exe", "RiotWindowClass").is_shell());
    }

    #[test]
    fn device_paths_get_their_drive_letter() {
        let drives = vec![
            ("C:".to_string(), r"\Device\HarddiskVolume3".to_string()),
            ("D:".to_string(), r"\Device\HarddiskVolume30".to_string()),
        ];
        assert_eq!(dos_path(r"\Device\HarddiskVolume3\Games\x.exe", &drives).as_deref(), Some(r"C:\Games\x.exe"));
        // Volume30 is not Volume3 followed by more text.
        assert_eq!(dos_path(r"\Device\HarddiskVolume30\y.exe", &drives).as_deref(), Some(r"D:\y.exe"));
        assert_eq!(dos_path(r"\Device\Mup\server\share\z.exe", &drives), None);
    }

    #[test]
    fn file_names() {
        assert_eq!(file_name(r"C:\Riot Games\League of Legends\Game\League of Legends.exe"), "League of Legends.exe");
        assert_eq!(file_name("game.exe"), "game.exe");
    }

    /// The current process can be found without a handle, by path and in
    /// the snapshot.
    #[cfg(windows)]
    #[test]
    fn finds_this_process_without_a_handle() {
        let pid = std::process::id();
        let path = image_path(pid).expect("path");
        let exe = std::env::current_exe().unwrap();
        assert!(path.eq_ignore_ascii_case(&exe.to_string_lossy()), "{path} vs {}", exe.display());
        assert!(is_running(pid, file_name(&path)));
        assert!(!is_running(pid, "not-this.exe"));
    }

    /// By hand: `cargo test -p kb-winsys -- --ignored --nocapture watch`,
    /// then switch windows for ten seconds.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn watch_the_front_for_ten_seconds() {
        let (tx, rx) = std::sync::mpsc::channel();
        let watcher = Watcher::start(tx).expect("hook");
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while let Some(left) = end.checked_duration_since(std::time::Instant::now()) {
            if let Ok(f) = rx.recv_timeout(left) {
                println!(
                    "{} | {} | shell {} | covers {} | min {}",
                    f.name,
                    f.class,
                    f.is_shell(),
                    f.covers_monitor,
                    f.minimized
                );
            }
        }
        drop(watcher);
    }
}
