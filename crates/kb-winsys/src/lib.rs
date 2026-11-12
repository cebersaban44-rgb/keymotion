//! Readings from Windows for the key-light gauges ("Sistem göstergeleri").
//!
//! The workspace denies `unsafe`; Win32 and COM calls need it, so they live
//! here, behind small safe types, and nowhere else
//! (docs/research/ekran-ve-veri-kaynaklari.md §5).
//!
//! * Processor: `GetSystemTimes`, the share of time not idle between two
//!   samples.
//! * Graphics: the `GPU Engine` performance counters, the source Task
//!   Manager uses, for every vendor. Per graphics card the busiest engine
//!   type (3D, video, copy, …); of the cards, the busiest. No temperature:
//!   there is no vendor-free way to read it.
//! * Memory: `GlobalMemoryStatusEx`.
//! * Sound: Core Audio, polled. The default speakers' volume and mute, the
//!   default microphone's mute (only the Windows mute: a mute button inside
//!   a call app does not reach Windows). The caller looks the default
//!   devices up again now and then, so a switch of device is followed.
//! * Running programs: a Toolhelp snapshot, with each program's path where
//!   Windows gives it, so the app can refuse to write to the keyboard while
//!   another program talks to it.
//! * Shutdown: while the app writes the keyboard's flash, Windows is told
//!   why shutting down now is a bad idea (`ShutdownBlockReasonCreate`).
//! * Web pages: opened in the default browser (`ShellExecuteW`, [`shell`]).
//!
//! Types holding COM or PDH handles are not `Send`: make them on the thread
//! that uses them.

#![allow(unsafe_code)]

pub mod appid;
pub mod os;
pub mod secret;
pub mod shell;
pub mod startup;

use std::collections::HashMap;

/// Volume of the default speakers.
#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    /// 0–1, as the Windows volume slider shows it.
    pub level: f32,
    pub muted: bool,
    /// Windows' id of the device, to tell a device switch from a change.
    pub device: String,
}

/// Processor load between two calls of [`CpuMeter::sample`].
#[derive(Debug, Default)]
pub struct CpuMeter {
    /// (idle, total) time of the last sample, in 100 ns units.
    last: Option<(u64, u64)>,
}

impl CpuMeter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Busy share (0–100) since the last call; `None` on the first call.
    pub fn sample(&mut self) -> Option<f32> {
        let now = sys::cpu_times()?;
        let prev = self.last.replace(now)?;
        cpu_share(prev, now)
    }
}

/// Busy share (0–100) between two (idle, total) samples.
pub fn cpu_share(prev: (u64, u64), now: (u64, u64)) -> Option<f32> {
    let idle = now.0.checked_sub(prev.0)?;
    let total = now.1.checked_sub(prev.1)?;
    if total == 0 {
        return None;
    }
    Some(((1.0 - idle as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32)
}

/// Memory in use (0–100).
pub fn memory_load() -> Option<f32> {
    sys::memory_load()
}

/// Milliseconds since the last keyboard or mouse input in this session
/// (`GetLastInputInfo`); a locked screen gets no input, so it counts too.
/// For the reminders' "nobody is at the computer".
pub fn idle_ms() -> Option<u64> {
    sys::idle_ms()
}

/// A running program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProgram {
    pub pid: u32,
    /// Executable file name, e.g. `SignalRgb.exe`.
    pub name: String,
    /// Full path; `None` when Windows does not give it (protected or
    /// elevated processes).
    pub path: Option<String>,
}

/// Every program running now (empty when the list cannot be read). Takes a
/// few milliseconds: call it before an action, not in a loop.
pub fn running_programs() -> Vec<RunningProgram> {
    sys::running_programs()
}

/// The `GPU Engine` counters' instance name parts: the card (LUID) and the
/// engine type. Names look like
/// `pid_1234_luid_0x00000000_0x0000D1F2_phys_0_eng_0_engtype_3D`.
fn engine_parts(name: &str) -> Option<(&str, &str)> {
    let rest = &name[name.find("luid_")? + 5..];
    let luid = &rest[..rest.find("_phys").unwrap_or(rest.len())];
    let engine = &name[name.find("engtype_")? + 8..];
    Some((luid, engine))
}

/// Graphics load (0–100) from the `GPU Engine` counter values: the busiest
/// engine type of the busiest card, each engine type summed over the
/// processes using it (as Task Manager does). `None` without any engine.
pub fn gpu_busiest<'a>(items: impl IntoIterator<Item = (&'a str, f64)>) -> Option<f32> {
    let mut engines: HashMap<(&str, &str), f64> = HashMap::new();
    for (name, value) in items {
        if let Some(key) = engine_parts(name) {
            if value.is_finite() {
                *engines.entry(key).or_default() += value.max(0.0);
            }
        }
    }
    engines.into_values().reduce(f64::max).map(|v| v.clamp(0.0, 100.0) as f32)
}

/// Tell Windows that the app is writing to the keyboard: if the user shuts
/// down or signs out meanwhile, Windows lists the app with `reason`
/// (`ShutdownBlockReasonCreate`). `hwnd` is the main window's handle; the
/// call only works on the thread that created that window. False when
/// Windows refused.
pub fn block_shutdown(hwnd: isize, reason: &str) -> bool {
    sys::block_shutdown(hwnd, reason)
}

/// The write is over: shutting down is fine again
/// (`ShutdownBlockReasonDestroy`). Same thread rule as [`block_shutdown`].
pub fn unblock_shutdown(hwnd: isize) -> bool {
    sys::unblock_shutdown(hwnd)
}

/// Graphics load from the performance counters.
pub struct GpuMeter(sys::Gpu);

impl GpuMeter {
    /// `None` when the counters are not there (old Windows, no driver).
    pub fn new() -> Option<Self> {
        sys::Gpu::new().map(Self)
    }

    /// Load (0–100) since the last call. Call about once a second.
    pub fn sample(&mut self) -> Option<f32> {
        self.0.sample()
    }
}

/// Speaker volume and microphone mute (Core Audio).
pub struct Audio(sys::Audio);

impl Audio {
    /// Starts COM on this thread and opens the default devices. `None`
    /// when Core Audio is not there.
    pub fn new() -> Option<Self> {
        sys::Audio::new().map(Self)
    }

    /// Look the default devices up again (the user may have switched).
    pub fn refresh(&mut self) {
        self.0.refresh();
    }

    /// `None` without speakers (or after an error; refresh to try again).
    pub fn speakers(&mut self) -> Option<Volume> {
        self.0.speakers()
    }

    /// `None` without a microphone.
    pub fn mic_muted(&mut self) -> Option<bool> {
        self.0.mic_muted()
    }
}

#[cfg(windows)]
mod sys {
    use super::{gpu_busiest, Volume};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
        PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER,
        PDH_HQUERY, PDH_MORE_DATA,
    };
    use windows::Win32::System::SystemInformation::{GetTickCount, GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows::Win32::System::Threading::GetSystemTimes;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    pub fn running_programs() -> Vec<super::RunningProgram> {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        fn text(wide: &[u16]) -> String {
            let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
            String::from_utf16_lossy(&wide[..end])
        }

        fn path_of(pid: u32) -> Option<String> {
            // SAFETY: plain call; the handle is closed below.
            let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            // SAFETY: `buf` holds `len` UTF-16 units; the call writes at most that many.
            let read =
                unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
            // SAFETY: the handle came from OpenProcess above and is closed once.
            let _ = unsafe { CloseHandle(process) };
            read.ok()?;
            Some(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]))
        }

        // SAFETY: plain call; the snapshot handle is closed below.
        let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        // SAFETY: a valid snapshot handle and an entry with its size set, as the API asks.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
        while more {
            let pid = entry.th32ProcessID;
            // Process 0 is the idle process: no executable behind it.
            if pid != 0 {
                out.push(super::RunningProgram { pid, name: text(&entry.szExeFile), path: path_of(pid) });
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
        }
        // SAFETY: the handle came from CreateToolhelp32Snapshot and is closed once.
        let _ = unsafe { CloseHandle(snapshot) };
        out
    }

    // Declared here rather than through the `windows` crate: its
    // `Win32_System_Shutdown` feature would rebuild the whole crate for two
    // functions. Both live in user32, which every window links anyway.
    #[link(name = "user32")]
    extern "system" {
        fn ShutdownBlockReasonCreate(hwnd: *mut core::ffi::c_void, reason: *const u16) -> i32;
        fn ShutdownBlockReasonDestroy(hwnd: *mut core::ffi::c_void) -> i32;
    }

    pub fn block_shutdown(hwnd: isize, reason: &str) -> bool {
        let wide: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated UTF-16 and outlives the call
        // (Windows copies it). A wrong handle only makes the call fail.
        unsafe { ShutdownBlockReasonCreate(hwnd as *mut core::ffi::c_void, wide.as_ptr()) != 0 }
    }

    pub fn unblock_shutdown(hwnd: isize) -> bool {
        // SAFETY: no pointers besides the handle; a wrong handle only makes
        // the call fail.
        unsafe { ShutdownBlockReasonDestroy(hwnd as *mut core::ffi::c_void) != 0 }
    }

    pub fn idle_ms() -> Option<u64> {
        let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        // SAFETY: a valid out pointer with its size set, as the API asks.
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return None;
        }
        // SAFETY: no arguments. Both counts are the same 32-bit tick
        // counter, so the difference is right across its wrap.
        let now = unsafe { GetTickCount() };
        Some(u64::from(now.wrapping_sub(info.dwTime)))
    }

    fn ticks(t: FILETIME) -> u64 {
        (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
    }

    /// (idle, total) processor time; kernel time includes idle time.
    pub fn cpu_times() -> Option<(u64, u64)> {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        // SAFETY: three valid out pointers to stack values.
        unsafe { GetSystemTimes(Some(&raw mut idle), Some(&raw mut kernel), Some(&raw mut user)) }.ok()?;
        Some((ticks(idle), ticks(kernel) + ticks(user)))
    }

    pub fn memory_load() -> Option<f32> {
        let mut m = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        // SAFETY: a valid out pointer with its length set, as the API asks.
        unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
        if m.ullTotalPhys == 0 {
            return None;
        }
        Some(((1.0 - m.ullAvailPhys as f64 / m.ullTotalPhys as f64) * 100.0).clamp(0.0, 100.0) as f32)
    }

    pub struct Gpu {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
    }

    impl Gpu {
        pub fn new() -> Option<Self> {
            let mut query = PDH_HQUERY::default();
            // SAFETY: valid out pointer; a null data source means live data.
            if unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut query) } != 0 {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            // The English name works on every Windows language.
            // SAFETY: the query handle is open; valid out pointer.
            let added =
                unsafe { PdhAddEnglishCounterW(query, w!("\\GPU Engine(*)\\Utilization Percentage"), 0, &mut counter) };
            if added != 0 {
                // SAFETY: the query handle is open and closed once.
                unsafe { PdhCloseQuery(query) };
                return None;
            }
            // Rates need two samples: this is the first.
            // SAFETY: the query handle is open.
            unsafe { PdhCollectQueryData(query) };
            Some(Self { query, counter })
        }

        pub fn sample(&mut self) -> Option<f32> {
            // SAFETY: the query handle stays open while `self` lives.
            if unsafe { PdhCollectQueryData(self.query) } != 0 {
                return None;
            }
            // Engines come and go with processes: the size can grow between
            // the two calls, so ask again a few times.
            for _ in 0..3 {
                let (mut size, mut count) = (0u32, 0u32);
                // SAFETY: size query with no buffer; valid out pointers.
                let r =
                    unsafe { PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None) };
                if r != PDH_MORE_DATA {
                    return None;
                }
                // u64 storage keeps the items 8-byte aligned.
                let mut buf = vec![0u64; (size as usize).div_ceil(8)];
                let items = buf.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
                // SAFETY: the buffer holds `size` bytes, as the API asked for.
                let r = unsafe {
                    PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items))
                };
                if r == PDH_MORE_DATA {
                    continue;
                }
                if r != 0 {
                    return None;
                }
                // SAFETY: the API wrote `count` items at the start of the
                // buffer; their names point into the same buffer, which
                // lives until the end of this block.
                let items = unsafe { std::slice::from_raw_parts(items, count as usize) };
                let named: Vec<(String, f64)> = items
                    .iter()
                    .filter(|i| matches!(i.FmtValue.CStatus, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA))
                    // SAFETY: names are NUL-terminated strings in `buf`;
                    // the value was asked for as a double.
                    .filter_map(|i| {
                        Some((unsafe { i.szName.to_string() }.ok()?, unsafe { i.FmtValue.Anonymous.doubleValue }))
                    })
                    .collect();
                return gpu_busiest(named.iter().map(|(n, v)| (n.as_str(), *v))).or(Some(0.0));
            }
            None
        }
    }

    impl Drop for Gpu {
        fn drop(&mut self) {
            // SAFETY: the query handle is open and closed only here.
            unsafe { PdhCloseQuery(self.query) };
        }
    }

    /// Undoes `CoInitializeEx` when the thread's audio reader goes away.
    struct Com(bool);

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: pairs with the successful CoInitializeEx on this
                // thread; every COM object was released before (field order).
                unsafe { CoUninitialize() };
            }
        }
    }

    pub struct Audio {
        enumerator: IMMDeviceEnumerator,
        speakers: Option<(String, IAudioEndpointVolume)>,
        mic: Option<IAudioEndpointVolume>,
        /// Last field: dropped after the COM objects above.
        _com: Com,
    }

    impl Audio {
        pub fn new() -> Option<Self> {
            // S_FALSE (already started) also needs its CoUninitialize; a
            // thread already in another mode can still use COM.
            // SAFETY: plain COM start-up for this thread.
            let com = Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok());
            // SAFETY: COM is started on this thread.
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()?;
            let mut audio = Self { enumerator, speakers: None, mic: None, _com: com };
            audio.refresh();
            Some(audio)
        }

        fn endpoint(&self, flow: EDataFlow) -> Option<(String, IAudioEndpointVolume)> {
            // SAFETY: COM calls on live interfaces of this thread.
            let device = unsafe { self.enumerator.GetDefaultAudioEndpoint(flow, eConsole) }.ok()?;
            let id = unsafe { device.GetId() }
                .ok()
                .map(|p| {
                    // SAFETY: GetId returns a NUL-terminated string the
                    // caller frees with CoTaskMemFree, once.
                    let s = unsafe { p.to_string() }.unwrap_or_default();
                    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
                    s
                })
                .unwrap_or_default();
            // SAFETY: as above; no activation parameters.
            let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }.ok()?;
            Some((id, volume))
        }

        pub fn refresh(&mut self) {
            self.speakers = self.endpoint(eRender);
            self.mic = self.endpoint(eCapture).map(|(_, v)| v);
        }

        pub fn speakers(&mut self) -> Option<Volume> {
            let (device, v) = self.speakers.as_ref()?;
            // SAFETY: COM calls on a live interface of this thread.
            let read = unsafe { (v.GetMasterVolumeLevelScalar(), v.GetMute()) };
            match read {
                (Ok(level), Ok(muted)) => {
                    Some(Volume { level: level.clamp(0.0, 1.0), muted: muted.as_bool(), device: device.clone() })
                }
                _ => {
                    // The device went away: look again on the next refresh.
                    self.speakers = None;
                    None
                }
            }
        }

        pub fn mic_muted(&mut self) -> Option<bool> {
            let v = self.mic.as_ref()?;
            // SAFETY: COM call on a live interface of this thread.
            match unsafe { v.GetMute() } {
                Ok(m) => Some(m.as_bool()),
                Err(_) => {
                    self.mic = None;
                    None
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Volume;

    pub fn running_programs() -> Vec<super::RunningProgram> {
        Vec::new()
    }

    pub fn cpu_times() -> Option<(u64, u64)> {
        None
    }

    pub fn memory_load() -> Option<f32> {
        None
    }

    pub fn idle_ms() -> Option<u64> {
        None
    }

    pub fn block_shutdown(_hwnd: isize, _reason: &str) -> bool {
        false
    }

    pub fn unblock_shutdown(_hwnd: isize) -> bool {
        false
    }

    pub struct Gpu;

    impl Gpu {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn sample(&mut self) -> Option<f32> {
            None
        }
    }

    pub struct Audio;

    impl Audio {
        pub fn new() -> Option<Self> {
            None
        }
        pub fn refresh(&mut self) {}
        pub fn speakers(&mut self) -> Option<Volume> {
            None
        }
        pub fn mic_muted(&mut self) -> Option<bool> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_share_from_two_samples() {
        // 1000 ticks passed, 250 of them idle.
        assert_eq!(cpu_share((100, 1000), (350, 2000)), Some(75.0));
        assert_eq!(cpu_share((0, 0), (0, 0)), None);
        // A clock going back gives nothing rather than nonsense.
        assert_eq!(cpu_share((10, 1000), (5, 2000)), None);
    }

    #[test]
    fn graphics_takes_the_busiest_engine_of_the_busiest_card() {
        let items = [
            // Card A: two processes on 3D (30 + 25), video decode 40.
            ("pid_1_luid_0x00000000_0x0000AAAA_phys_0_eng_0_engtype_3D", 30.0),
            ("pid_2_luid_0x00000000_0x0000AAAA_phys_0_eng_0_engtype_3D", 25.0),
            ("pid_2_luid_0x00000000_0x0000AAAA_phys_0_eng_3_engtype_VideoDecode", 40.0),
            // Card B: copy 20.
            ("pid_3_luid_0x00000000_0x0000BBBB_phys_0_eng_1_engtype_Copy", 20.0),
            ("not an engine", 99.0),
        ];
        assert_eq!(gpu_busiest(items), Some(55.0));
        assert_eq!(gpu_busiest([("pid_1_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 180.0)]), Some(100.0));
        assert_eq!(gpu_busiest([]), None);
    }

    #[cfg(windows)]
    #[test]
    fn the_running_list_has_this_process() {
        let list = running_programs();
        let own = list.iter().find(|p| p.pid == std::process::id()).expect("own process listed");
        assert!(own.name.to_ascii_lowercase().ends_with(".exe"), "{own:?}");
        assert!(own.path.as_deref().is_some_and(|p| p.ends_with(&own.name)), "{own:?}");
    }

    #[test]
    fn engine_names_split_into_card_and_type() {
        assert_eq!(
            engine_parts("pid_8540_luid_0x00000000_0x0000F2A4_phys_0_eng_3_engtype_VideoDecode"),
            Some(("0x00000000_0x0000F2A4", "VideoDecode"))
        );
        assert_eq!(engine_parts("pid_1_eng_0"), None);
    }
}
