//! A local named pipe carrying lines of text, for `keymotion-cli` and the
//! app (the protocol on top is `kb-agent`'s).
//!
//! * Only this computer: `PIPE_REJECT_REMOTE_CLIENTS`, and the DACL denies
//!   the network logon SID besides.
//! * Only this user: an explicit security descriptor whose DACL allows the
//!   current user read and write and nobody else ([`server_sddl`]). Windows'
//!   default pipe descriptor would let Everyone and anonymous read.
//! * No squatting: the server creates the pipe with
//!   `FILE_FLAG_FIRST_PIPE_INSTANCE` whenever it holds no instance, so it
//!   never joins a pipe someone else created ([`io::ErrorKind::AddrInUse`]
//!   at start). The pipe's owner is set to the user, and the client refuses
//!   a pipe owned by anyone else, or one that refuses it
//!   ([`ClientError::NotTrusted`]). A client can also ask which process
//!   serves the pipe ([`PipeClient::server_process_id`]).
//! * Byte mode, at most [`MAX_INSTANCES`] connections at once. Each line
//!   must arrive within the read deadline (2 s by default) and fit the line
//!   limit; otherwise the connection is closed. A handler that panics ends
//!   its connection, not the server.
//! * The client connects with a timeout and tells "nobody serves this
//!   pipe" ([`ClientError::NotRunning`]) from "no answer in time"
//!   ([`ClientError::Timeout`]), and "the line never arrived, the server had
//!   closed the connection" ([`ClientError::NotDelivered`], safe to resend
//!   on a new connection) from "the server closed it after the line
//!   arrived" ([`ClientError::Closed`]).
//!
//! All I/O is overlapped, so every wait has a deadline and the server can
//! stop at any moment.

use std::ffi::c_void;
use std::fmt;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    LocalFree, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER,
    ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED,
    ERROR_SEM_TIMEOUT, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, WAIT_EVENT, WAIT_OBJECT_0,
    WAIT_TIMEOUT, WIN32_ERROR,
};
use windows::Win32::Security::{GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, WaitNamedPipeW,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcessToken, SetEvent, WaitForMultipleObjects, INFINITE,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

/// The most connections a server takes at once.
pub const MAX_INSTANCES: u32 = 4;
/// The pipe's buffer size each way (advisory for Windows).
const BUFFER_BYTES: u32 = 64 * 1024;
/// Bytes asked of each read.
const CHUNK: usize = 16 * 1024;
/// Every pipe name starts with this.
const PIPE_NAMESPACE: &str = r"\\.\pipe\";

// Declared here rather than through the `windows` crate: its
// `Win32_Security_Authorization` and `Win32_System_RemoteDesktop` features
// are large and would be compiled for these five functions. advapi32 and
// kernel32 are linked by every Windows program anyway.
#[link(name = "advapi32")]
extern "system" {
    fn ConvertSidToStringSidW(sid: *mut c_void, string_sid: *mut *mut u16) -> i32;
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
        descriptor: *mut c_void,
        revision: u32,
        information: u32,
        sddl: *mut *mut u16,
        length: *mut u32,
    ) -> i32;
    fn GetSecurityInfo(
        handle: *mut c_void,
        object_type: i32,
        information: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        descriptor: *mut *mut c_void,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
}

const SDDL_REVISION_1: u32 = 1;
const OWNER_SECURITY_INFORMATION: u32 = 1;
const DACL_SECURITY_INFORMATION: u32 = 4;
/// `SE_KERNEL_OBJECT` of `SE_OBJECT_TYPE`.
const SE_KERNEL_OBJECT: i32 = 6;

// ------------------------------------------------------------ identities

/// The current user's SID as a string, `S-1-5-21-…`.
pub fn current_user_sid() -> io::Result<String> {
    let mut raw = HANDLE::default();
    // SAFETY: the current-process pseudo handle needs no closing; valid out
    // pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) }.map_err(win_err)?;
    // SAFETY: a token handle just opened, owned from here on.
    let token = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut size = 0u32;
    // SAFETY: a size query without a buffer; it fails with
    // ERROR_INSUFFICIENT_BUFFER and sets `size`.
    let first = unsafe { GetTokenInformation(handle(&token), TokenUser, None, 0, &mut size) };
    if let Err(e) = first {
        if WIN32_ERROR::from_error(&e) != Some(ERROR_INSUFFICIENT_BUFFER) {
            return Err(win_err(e));
        }
    }
    // u64 storage keeps TOKEN_USER and the SID after it aligned.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8).max(1)];
    // SAFETY: the buffer holds at least `size` bytes, as asked for.
    unsafe { GetTokenInformation(handle(&token), TokenUser, Some(buffer.as_mut_ptr().cast()), size, &mut size) }
        .map_err(win_err)?;
    // SAFETY: on success the buffer starts with a TOKEN_USER whose SID
    // pointer points into the same buffer, alive until the end of this
    // function.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid SID; valid out pointer.
    if unsafe { ConvertSidToStringSidW(user.User.Sid.0, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the API returned a NUL-terminated string it allocated with
    // LocalAlloc; it is read once and freed once.
    Ok(unsafe { take_local_string(text) })
}

/// The logon session (Remote Desktop / fast user switching) this process
/// runs in.
pub fn current_session_id() -> io::Result<u32> {
    let mut session = 0u32;
    // SAFETY: valid out pointer.
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session)
}

/// The security descriptor every instance of a server's pipe gets: owned
/// by `user_sid` (also when the process runs as an administrator, whose
/// default owner is the Administrators group), the network logon SID denied
/// everything, `user_sid` allowed to read and write (which includes
/// creating more instances), nobody else anything, and no inherited
/// entries.
pub fn server_sddl(user_sid: &str) -> String {
    format!("O:{user_sid}D:P(D;;FA;;;NU)(A;;FRFW;;;{user_sid})")
}

// --------------------------------------------------------------- helpers

/// The raw handle for Win32 calls.
fn handle(h: &OwnedHandle) -> HANDLE {
    HANDLE(h.as_raw_handle())
}

/// A Win32 error as the matching `io::Error`.
fn win_err(e: windows::core::Error) -> io::Error {
    match WIN32_ERROR::from_error(&e) {
        Some(code) => io::Error::from_raw_os_error(code.0 as i32),
        None => io::Error::other(e),
    }
}

fn is(e: &io::Error, code: WIN32_ERROR) -> bool {
    e.raw_os_error() == Some(code.0 as i32)
}

/// NUL-terminated UTF-16.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Read and free a string Windows allocated with `LocalAlloc`.
///
/// SAFETY: `text` is null or a NUL-terminated UTF-16 string from
/// `LocalAlloc` that nothing else frees.
unsafe fn take_local_string(text: *mut u16) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: as promised by the caller.
    let s = unsafe { windows::core::PWSTR(text).to_string() }.unwrap_or_default();
    // SAFETY: freed once, here.
    unsafe { LocalFree(Some(HLOCAL(text.cast()))) };
    s
}

/// A new unnamed event: manual-reset or auto-reset, not signalled.
fn event(manual_reset: bool) -> io::Result<OwnedHandle> {
    // SAFETY: no security attributes, no name.
    let raw = unsafe { CreateEventW(None, manual_reset, false, PCWSTR::null()) }.map_err(win_err)?;
    // SAFETY: an event handle just created, owned from here on.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.0) })
}

/// Whether the event is signalled now (no waiting).
fn is_set(event: HANDLE) -> bool {
    // SAFETY: a live event handle.
    let woke = unsafe { WaitForMultipleObjects(&[event], false, 0) };
    woke == WAIT_OBJECT_0
}

/// Milliseconds left until `deadline`, for a Win32 wait (never INFINITE).
fn millis_until(deadline: Instant) -> u32 {
    let left = deadline.saturating_duration_since(Instant::now()).as_millis();
    u32::try_from(left).unwrap_or(INFINITE - 1).min(INFINITE - 1)
}

/// A security descriptor built from SDDL, freed on drop.
struct SecurityDescriptor(*mut c_void);

// SAFETY: the descriptor is a heap block that is only read after it is
// built, and freed once, on drop.
unsafe impl Send for SecurityDescriptor {}
// SAFETY: as above; shared reads only.
unsafe impl Sync for SecurityDescriptor {}

impl SecurityDescriptor {
    fn from_sddl(sddl: &str) -> io::Result<Self> {
        let text = wide(sddl);
        let mut descriptor: *mut c_void = std::ptr::null_mut();
        // SAFETY: NUL-terminated input; valid out pointer; no size wanted.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }
}

impl Drop for SecurityDescriptor {
    fn drop(&mut self) {
        // SAFETY: allocated by the conversion with LocalAlloc, freed once.
        unsafe { LocalFree(Some(HLOCAL(self.0))) };
    }
}

/// The DACL of the object behind `h`, as SDDL (`D:…`).
fn dacl_sddl(h: &OwnedHandle) -> io::Result<String> {
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: a live handle; only the descriptor is asked for, and Windows
    // allocates it with LocalAlloc.
    let status = unsafe {
        GetSecurityInfo(
            h.as_raw_handle(),
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let descriptor = SecurityDescriptor(descriptor);
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid descriptor; valid out pointer; no length wanted.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor.0,
            SDDL_REVISION_1,
            DACL_SECURITY_INFORMATION,
            &mut text,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a LocalAlloc'd NUL-terminated string, read and freed once.
    Ok(unsafe { take_local_string(text) })
}

/// The owner of the object behind `h`, as a SID string.
fn owner_sid(h: &OwnedHandle) -> io::Result<String> {
    let mut owner: *mut c_void = std::ptr::null_mut();
    let mut descriptor: *mut c_void = std::ptr::null_mut();
    // SAFETY: a live handle; the owner pointer points into the descriptor,
    // which Windows allocates with LocalAlloc.
    let status = unsafe {
        GetSecurityInfo(
            h.as_raw_handle(),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    // Freed after the owner is read: the owner lives inside it.
    let descriptor = SecurityDescriptor(descriptor);
    if owner.is_null() {
        return Ok(String::new());
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: a valid SID inside `descriptor`; valid out pointer.
    let ok = unsafe { ConvertSidToStringSidW(owner, &mut text) };
    drop(descriptor);
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a LocalAlloc'd NUL-terminated string, read and freed once.
    Ok(unsafe { take_local_string(text) })
}

// ---------------------------------------------------------- connections

/// Why reading or writing a line stopped.
#[derive(Debug)]
enum Stop {
    Timeout,
    /// The other end closed the pipe.
    Closed,
    /// The server is stopping.
    Stopping,
    /// A line longer than the limit.
    TooLong,
    Failed(io::Error),
}

impl From<io::Error> for Stop {
    fn from(e: io::Error) -> Self {
        if is(&e, ERROR_BROKEN_PIPE) || is(&e, ERROR_PIPE_NOT_CONNECTED) || is(&e, ERROR_NO_DATA) {
            Stop::Closed
        } else {
            Stop::Failed(e)
        }
    }
}

/// One end of a connected pipe, opened for overlapped I/O, with the bytes
/// read past the last line.
struct Conn {
    pipe: OwnedHandle,
    /// Manual-reset event for the overlapped calls (one at a time).
    event: OwnedHandle,
    pending: Vec<u8>,
}

impl Conn {
    fn new(pipe: OwnedHandle) -> io::Result<Self> {
        Ok(Self::with_event(pipe, event(true)?))
    }

    fn with_event(pipe: OwnedHandle, event: OwnedHandle) -> Self {
        Self { pipe, event, pending: Vec::new() }
    }

    fn into_pipe(self) -> OwnedHandle {
        self.pipe
    }

    /// Finish an overlapped call made with `ov` (`started` is what the call
    /// returned): wait for it until `deadline` or the `stop` event, and
    /// cancel it otherwise. Does not return before Windows is done with
    /// `ov` and the buffer.
    fn finish(
        &self,
        ov: &mut OVERLAPPED,
        started: windows::core::Result<()>,
        deadline: Instant,
        stop: Option<HANDLE>,
    ) -> Result<usize, Stop> {
        let pipe = handle(&self.pipe);
        if let Err(e) = started {
            if WIN32_ERROR::from_error(&e) != Some(ERROR_IO_PENDING) {
                return Err(win_err(e).into());
            }
            let mut waits = vec![handle(&self.event)];
            waits.extend(stop);
            // SAFETY: live event handles.
            let woke = unsafe { WaitForMultipleObjects(&waits, false, millis_until(deadline)) };
            if woke != WAIT_OBJECT_0 {
                let wait_error = io::Error::last_os_error();
                // SAFETY: `ov` belongs to the call in flight on this handle.
                let _ = unsafe { CancelIoEx(pipe, Some(&raw const *ov)) };
                let mut moved = 0u32;
                // SAFETY: wait until the call is over (done or cancelled),
                // so `ov` and the buffer are free again.
                let done = unsafe { GetOverlappedResult(pipe, ov, &mut moved, true) };
                return match woke {
                    WAIT_TIMEOUT if done.is_ok() => Ok(moved as usize),
                    WAIT_TIMEOUT => Err(Stop::Timeout),
                    WAIT_EVENT(1) => Err(Stop::Stopping),
                    _ => Err(Stop::Failed(wait_error)),
                };
            }
        }
        let mut moved = 0u32;
        // SAFETY: the call is complete (finished at once or signalled).
        unsafe { GetOverlappedResult(pipe, ov, &mut moved, false) }.map_err(|e| Stop::from(win_err(e)))?;
        Ok(moved as usize)
    }

    fn read_some(&self, buffer: &mut [u8], deadline: Instant, stop: Option<HANDLE>) -> Result<usize, Stop> {
        let mut ov = OVERLAPPED { hEvent: handle(&self.event), ..Default::default() };
        // SAFETY: the buffer and `ov` outlive the call: `finish` returns
        // only once Windows is done with them.
        let started = unsafe { ReadFile(handle(&self.pipe), Some(buffer), None, Some(&raw mut ov)) };
        self.finish(&mut ov, started, deadline, stop)
    }

    fn write_all(&self, mut data: &[u8], deadline: Instant, stop: Option<HANDLE>) -> Result<(), Stop> {
        while !data.is_empty() {
            let mut ov = OVERLAPPED { hEvent: handle(&self.event), ..Default::default() };
            // SAFETY: as in `read_some`.
            let started = unsafe { WriteFile(handle(&self.pipe), Some(data), None, Some(&raw mut ov)) };
            let n = self.finish(&mut ov, started, deadline, stop)?;
            if n == 0 {
                return Err(Stop::Closed);
            }
            data = &data[n.min(data.len())..];
        }
        Ok(())
    }

    /// The next line without its `\n` (and `\r`), at most `max` bytes.
    fn read_line(&mut self, max: usize, deadline: Instant, stop: Option<HANDLE>) -> Result<Vec<u8>, Stop> {
        let mut chunk = vec![0u8; CHUNK];
        loop {
            if let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.pending.drain(..=end).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if line.len() > max {
                    return Err(Stop::TooLong);
                }
                return Ok(line);
            }
            if self.pending.len() > max {
                return Err(Stop::TooLong);
            }
            // On every pass, not only when a read has to wait: bytes that
            // trickle in must not carry a line past its deadline, nor the
            // connection past the server's stop.
            if Instant::now() >= deadline {
                return Err(Stop::Timeout);
            }
            if stop.is_some_and(is_set) {
                return Err(Stop::Stopping);
            }
            let n = self.read_some(&mut chunk, deadline, stop)?;
            self.pending.extend_from_slice(&chunk[..n]);
        }
    }
}

// ---------------------------------------------------------------- server

/// What the server read from a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming<'a> {
    /// One line, without its line break.
    Line(&'a str),
    /// A line longer than the limit. The answer is sent, then the
    /// connection closes.
    TooLong,
    /// A line that is not UTF-8. The answer is sent, then the connection
    /// closes.
    NotText,
}

/// Limits of a [`PipeServer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerOptions {
    /// The longest line, in bytes, without the line break.
    pub max_line: usize,
    /// Each line must arrive within this time, counted from when the server
    /// starts waiting for it; otherwise the connection closes.
    pub read_timeout: Duration,
    /// Each answer must be taken within this time.
    pub write_timeout: Duration,
    /// Connections at once, 1 to [`MAX_INSTANCES`].
    pub max_instances: u32,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            max_line: 64 * 1024,
            read_timeout: Duration::from_secs(2),
            write_timeout: Duration::from_secs(2),
            max_instances: MAX_INSTANCES,
        }
    }
}

/// State shared by the accepting thread and the connections.
struct Shared {
    /// Manual-reset; set once, when the server stops.
    stop: OwnedHandle,
    /// Auto-reset; set when a connection ends and frees a place.
    freed: OwnedHandle,
    /// Connections being served.
    live: Mutex<u32>,
    ended: Condvar,
}

impl Shared {
    fn live(&self) -> MutexGuard<'_, u32> {
        self.live.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A connected instance, counted while its handle is open. The handle is
/// closed and the count lowered together, under the count's lock, so the
/// accepting thread knows exactly when the server holds no instance.
struct LiveGuard {
    shared: Arc<Shared>,
    pipe: Option<OwnedHandle>,
}

impl LiveGuard {
    fn new(shared: &Arc<Shared>, pipe: OwnedHandle) -> Self {
        *shared.live() += 1;
        Self { shared: shared.clone(), pipe: Some(pipe) }
    }
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        let mut live = self.shared.live();
        drop(self.pipe.take());
        *live -= 1;
        drop(live);
        self.shared.ended.notify_all();
        // SAFETY: a live event handle.
        let _ = unsafe { SetEvent(handle(&self.shared.freed)) };
    }
}

/// A running pipe server. Stops when dropped.
pub struct PipeServer {
    name: String,
    shared: Arc<Shared>,
    accepting: Option<JoinHandle<()>>,
    /// Set once `shutdown` ran, so dropping after `stop` does not wait again.
    stopped: bool,
}

/// Serve `name` with the default limits ([`ServerOptions::default`]).
/// See [`PipeServer::start`].
pub fn serve<F, H>(name: &str, handler: F) -> io::Result<PipeServer>
where
    F: Fn() -> H + Send + Sync + 'static,
    H: FnMut(Incoming<'_>) -> String + 'static,
{
    PipeServer::start(name, ServerOptions::default(), handler)
}

impl PipeServer {
    /// Create the pipe and accept connections on a background thread.
    ///
    /// `handler` is called once per connection, on that connection's
    /// thread, and returns the function that answers each line; it may keep
    /// state for the connection. An answer is one line: the server adds the
    /// line break.
    ///
    /// Fails with [`io::ErrorKind::AddrInUse`] when another process already
    /// holds `name`.
    pub fn start<F, H>(name: &str, options: ServerOptions, handler: F) -> io::Result<PipeServer>
    where
        F: Fn() -> H + Send + Sync + 'static,
        H: FnMut(Incoming<'_>) -> String + 'static,
    {
        if !name.starts_with(PIPE_NAMESPACE) || name.len() > 256 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a local pipe name"));
        }
        let options = ServerOptions { max_instances: options.max_instances.clamp(1, MAX_INSTANCES), ..options };
        let descriptor = SecurityDescriptor::from_sddl(&server_sddl(&current_user_sid()?))?;
        let wide_name = wide(name);
        let first = create_instance(&wide_name, &descriptor, options.max_instances, true)?;
        let shared =
            Arc::new(Shared { stop: event(true)?, freed: event(false)?, live: Mutex::new(0), ended: Condvar::new() });
        let accept_event = event(true)?;
        let handler = Arc::new(handler);
        let thread_shared = shared.clone();
        let accepting = std::thread::Builder::new().name("agent-pipe".into()).spawn(move || {
            accept_loop(&wide_name, &descriptor, options, first, &accept_event, &thread_shared, &handler)
        })?;
        Ok(PipeServer { name: name.to_string(), shared, accepting: Some(accepting), stopped: false })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Connections being served now.
    pub fn connections(&self) -> u32 {
        *self.shared.live()
    }

    /// Stop accepting, end the connections and wait for them (at most a
    /// few seconds for a handler that is still busy).
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        // SAFETY: a live event handle.
        let _ = unsafe { SetEvent(handle(&self.shared.stop)) };
        if let Some(thread) = self.accepting.take() {
            let _ = thread.join();
        }
        let give_up = Instant::now() + Duration::from_secs(5);
        let mut live = self.shared.live();
        while *live > 0 {
            let left = give_up.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            live = self.shared.ended.wait_timeout(live, left).map_or_else(|p| p.into_inner().0, |(g, _)| g);
        }
    }
}

impl Drop for PipeServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl fmt::Debug for PipeServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipeServer").field("name", &self.name).finish_non_exhaustive()
    }
}

fn create_instance(
    name: &[u16],
    descriptor: &SecurityDescriptor,
    max_instances: u32,
    first: bool,
) -> io::Result<OwnedHandle> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY: NUL-terminated name; the attributes and the descriptor they
    // point to outlive the call.
    let raw = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            max_instances,
            BUFFER_BYTES,
            BUFFER_BYTES,
            0,
            Some(&raw const attributes),
        )
    };
    if raw == INVALID_HANDLE_VALUE {
        let e = io::Error::last_os_error();
        if first && is(&e, ERROR_ACCESS_DENIED) {
            return Err(io::Error::new(io::ErrorKind::AddrInUse, "another process already serves this pipe"));
        }
        return Err(e);
    }
    // SAFETY: a pipe handle just created, owned from here on.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw.0) })
}

enum Accepted {
    Connected,
    Stopping,
    Failed(io::Error),
}

/// Wait for a client on `pipe`, or for the stop event.
fn accept(pipe: &OwnedHandle, wake: &OwnedHandle, stop: &OwnedHandle) -> Accepted {
    let mut ov = OVERLAPPED { hEvent: handle(wake), ..Default::default() };
    // SAFETY: `ov` outlives the call: every path below waits for it to end.
    let started = unsafe { ConnectNamedPipe(handle(pipe), Some(&raw mut ov)) };
    match started.map_err(win_err) {
        Ok(()) => return Accepted::Connected,
        Err(e) if is(&e, ERROR_PIPE_CONNECTED) => return Accepted::Connected,
        Err(e) if !is(&e, ERROR_IO_PENDING) => return Accepted::Failed(e),
        Err(_) => {}
    }
    // SAFETY: live event handles.
    let woke = unsafe { WaitForMultipleObjects(&[handle(wake), handle(stop)], false, INFINITE) };
    let mut moved = 0u32;
    if woke == WAIT_OBJECT_0 {
        // SAFETY: the call is complete.
        return match unsafe { GetOverlappedResult(handle(pipe), &ov, &mut moved, false) } {
            Ok(()) => Accepted::Connected,
            Err(e) => Accepted::Failed(win_err(e)),
        };
    }
    // SAFETY: cancel the call in flight and wait until Windows is done
    // with `ov`.
    let _ = unsafe { CancelIoEx(handle(pipe), Some(&raw const ov)) };
    let _ = unsafe { GetOverlappedResult(handle(pipe), &ov, &mut moved, true) };
    Accepted::Stopping
}

/// Whether the stop event is set, waiting up to `ms` for it.
fn stopping(shared: &Shared, ms: u32) -> bool {
    // SAFETY: a live event handle.
    let woke = unsafe { WaitForMultipleObjects(&[handle(&shared.stop)], false, ms) };
    woke == WAIT_OBJECT_0
}

fn accept_loop<F, H>(
    name: &[u16],
    descriptor: &SecurityDescriptor,
    options: ServerOptions,
    first: OwnedHandle,
    wake: &OwnedHandle,
    shared: &Arc<Shared>,
    handler: &Arc<F>,
) where
    F: Fn() -> H + Send + Sync + 'static,
    H: FnMut(Incoming<'_>) -> String + 'static,
{
    let mut next = Some(first);
    loop {
        // Wait for a free place (or the stop).
        while *shared.live() >= options.max_instances {
            // SAFETY: live event handles.
            let woke =
                unsafe { WaitForMultipleObjects(&[handle(&shared.freed), handle(&shared.stop)], false, INFINITE) };
            if woke != WAIT_OBJECT_0 {
                return;
            }
        }
        if stopping(shared, 0) {
            return;
        }
        let pipe = match next.take() {
            Some(pipe) => pipe,
            None => {
                // Under the count's lock, so no connection closes meanwhile.
                // With none open the server holds no instance at all: the
                // pipe is then created anew, and a name someone else took in
                // between is refused rather than joined.
                let live = shared.live();
                let created = create_instance(name, descriptor, options.max_instances, *live == 0);
                drop(live);
                match created {
                    Ok(pipe) => pipe,
                    Err(e) => {
                        tracing::warn!("agent pipe: could not create an instance: {e}");
                        if stopping(shared, 200) {
                            return;
                        }
                        continue;
                    }
                }
            }
        };
        match accept(&pipe, wake, &shared.stop) {
            Accepted::Connected => {
                let guard = LiveGuard::new(shared, pipe);
                // The next instance listens before this connection can end,
                // so the name does not lapse between clients. The connected
                // one is still open, so this cannot join someone else's pipe.
                if *shared.live() < options.max_instances {
                    next = create_instance(name, descriptor, options.max_instances, false)
                        .map_err(|e| tracing::warn!("agent pipe: could not create an instance: {e}"))
                        .ok();
                }
                let handler = handler.clone();
                let spawned = std::thread::Builder::new()
                    .name("agent-pipe-conn".into())
                    .spawn(move || serve_guarded(guard, options, handler.as_ref()));
                if let Err(e) = spawned {
                    // The guard went down with the closure: the place is free.
                    tracing::warn!("agent pipe: could not start a connection thread: {e}");
                }
            }
            Accepted::Stopping => return,
            Accepted::Failed(e) => {
                // Usually a client that left before it was accepted. The
                // instance is reset and listens again rather than being
                // closed, so the name does not lapse meanwhile.
                tracing::debug!("agent pipe: a connection failed: {e}");
                // SAFETY: a live pipe handle with no call in flight.
                if unsafe { DisconnectNamedPipe(handle(&pipe)) }.is_ok() {
                    next = Some(pipe);
                }
                if stopping(shared, 10) {
                    return;
                }
            }
        }
    }
}

/// Serve the connection in `guard`, then hand its pipe back so it is closed
/// under the count's lock.
fn serve_guarded<F, H>(mut guard: LiveGuard, options: ServerOptions, handler: &F)
where
    F: Fn() -> H,
    H: FnMut(Incoming<'_>) -> String + 'static,
{
    let Some(pipe) = guard.pipe.take() else { return };
    let stop = handle(&guard.shared.stop);
    let pipe = match event(true) {
        Ok(event) => {
            let mut conn = Conn::with_event(pipe, event);
            serve_connection(&mut conn, options, stop, handler);
            conn.into_pipe()
        }
        Err(e) => {
            tracing::warn!("agent pipe: {e}");
            pipe
        }
    };
    guard.pipe = Some(pipe);
}

fn serve_connection<F, H>(conn: &mut Conn, options: ServerOptions, stop: HANDLE, handler: &F)
where
    F: Fn() -> H,
    H: FnMut(Incoming<'_>) -> String + 'static,
{
    // A handler that panics ends its connection here, so the pipe still
    // goes back to be closed in order.
    let Ok(mut answer) = catch_unwind(AssertUnwindSafe(handler)) else {
        tracing::warn!("agent pipe: a connection handler panicked");
        return;
    };
    loop {
        if is_set(stop) {
            return;
        }
        let read = conn.read_line(options.max_line, Instant::now() + options.read_timeout, Some(stop));
        let (incoming, last) = match &read {
            Ok(bytes) => match std::str::from_utf8(bytes) {
                Ok(line) => (Incoming::Line(line), false),
                Err(_) => (Incoming::NotText, true),
            },
            Err(Stop::TooLong) => (Incoming::TooLong, true),
            Err(_) => return,
        };
        let Ok(reply) = catch_unwind(AssertUnwindSafe(|| answer(incoming))) else {
            tracing::warn!("agent pipe: a handler panicked; its connection is closed");
            return;
        };
        let mut out = reply.into_bytes();
        out.retain(|&b| b != b'\n' && b != b'\r');
        out.push(b'\n');
        if conn.write_all(&out, Instant::now() + options.write_timeout, Some(stop)).is_err() || last {
            return;
        }
    }
}

// ---------------------------------------------------------------- client

/// Why a client call failed.
#[derive(Debug)]
pub enum ClientError {
    /// Nobody serves this pipe: the app is not running.
    NotRunning,
    /// The pipe is not this user's: someone else owns it, or it refuses
    /// this user. Nothing was sent.
    NotTrusted(String),
    /// No connection or no answer in time.
    Timeout,
    /// The line never arrived: the server had already closed this
    /// connection (its idle deadline passed, or it stopped). Nothing was
    /// carried out; sending it again on a new connection is safe.
    NotDelivered,
    /// The server closed the connection after the line arrived, without
    /// answering.
    Closed,
    /// The answer is longer than the line limit.
    TooLong,
    Io(io::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => f.write_str("nobody serves the pipe"),
            Self::NotTrusted(why) => write!(f, "the pipe is not trusted: {why}"),
            Self::Timeout => f.write_str("no answer in time"),
            Self::NotDelivered => f.write_str("the connection was already closed; the line was not delivered"),
            Self::Closed => f.write_str("the other end closed the pipe"),
            Self::TooLong => f.write_str("the answer is too long"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<Stop> for ClientError {
    fn from(stop: Stop) -> Self {
        match stop {
            Stop::Timeout => Self::Timeout,
            Stop::Closed | Stop::Stopping => Self::Closed,
            Stop::TooLong => Self::TooLong,
            Stop::Failed(e) => Self::Io(e),
        }
    }
}

/// The client end of a pipe.
pub struct PipeClient {
    conn: Conn,
    max_line: usize,
}

impl fmt::Debug for PipeClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipeClient").finish_non_exhaustive()
    }
}

impl PipeClient {
    /// Connect to `name`, waiting at most `timeout` for a free instance.
    /// [`ClientError::NotRunning`] at once when nobody serves the name;
    /// [`ClientError::NotTrusted`] when the pipe is not owned by the current
    /// user (another account took the name first) or refuses this user.
    ///
    /// The server may identify the client but not act as it
    /// (`SECURITY_IDENTIFICATION`), in case someone else serves the name.
    pub fn connect(name: &str, timeout: Duration) -> Result<PipeClient, ClientError> {
        let user = current_user_sid().map_err(ClientError::Io)?;
        Self::connect_owned_by(name, timeout, &user)
    }

    /// [`PipeClient::connect`] to a pipe that must be owned by `owner` (a
    /// SID string).
    fn connect_owned_by(name: &str, timeout: Duration, owner: &str) -> Result<PipeClient, ClientError> {
        if !name.starts_with(PIPE_NAMESPACE) {
            return Err(ClientError::Io(io::Error::new(io::ErrorKind::InvalidInput, "not a local pipe name")));
        }
        let deadline = Instant::now() + timeout;
        let wide_name = wide(name);
        let flags: FILE_FLAGS_AND_ATTRIBUTES = FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION;
        loop {
            // SAFETY: NUL-terminated name; no security attributes or template.
            let opened = unsafe {
                CreateFileW(
                    PCWSTR(wide_name.as_ptr()),
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_NONE,
                    None,
                    OPEN_EXISTING,
                    flags,
                    None,
                )
            };
            let e = match opened {
                Ok(raw) => {
                    // SAFETY: a pipe handle just opened, owned from here on.
                    let pipe = unsafe { OwnedHandle::from_raw_handle(raw.0) };
                    // Checked before a single byte is sent.
                    let found = owner_sid(&pipe).map_err(ClientError::Io)?;
                    if found != owner {
                        return Err(ClientError::NotTrusted(format!("it belongs to {found}, not to {owner}")));
                    }
                    return Ok(PipeClient { conn: Conn::new(pipe).map_err(ClientError::Io)?, max_line: 64 * 1024 });
                }
                Err(e) => win_err(e),
            };
            if is(&e, ERROR_FILE_NOT_FOUND) {
                return Err(ClientError::NotRunning);
            }
            if is(&e, ERROR_ACCESS_DENIED) {
                return Err(ClientError::NotTrusted("it does not let this user in".to_string()));
            }
            if !is(&e, ERROR_PIPE_BUSY) {
                return Err(ClientError::Io(e));
            }
            // Every instance is busy: wait for one.
            let left = millis_until(deadline);
            if left == 0 {
                return Err(ClientError::Timeout);
            }
            // SAFETY: NUL-terminated name.
            if !unsafe { WaitNamedPipeW(PCWSTR(wide_name.as_ptr()), left) }.as_bool() {
                let e = io::Error::last_os_error();
                if is(&e, ERROR_FILE_NOT_FOUND) {
                    return Err(ClientError::NotRunning);
                }
                if !is(&e, ERROR_SEM_TIMEOUT) {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            if Instant::now() >= deadline {
                return Err(ClientError::Timeout);
            }
        }
    }

    /// The longest answer line accepted (64 KiB unless changed).
    pub fn set_max_line(&mut self, max: usize) {
        self.max_line = max;
    }

    /// Send one line; the line break is added. The line must not contain
    /// one.
    pub fn send_line(&mut self, line: &str, timeout: Duration) -> Result<(), ClientError> {
        if line.contains(['\n', '\r']) {
            return Err(ClientError::Io(io::Error::new(io::ErrorKind::InvalidInput, "a line break inside a line")));
        }
        let mut out = Vec::with_capacity(line.len() + 1);
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
        Ok(self.conn.write_all(&out, Instant::now() + timeout, None)?)
    }

    /// Read one line, without its line break.
    pub fn read_line(&mut self, timeout: Duration) -> Result<String, ClientError> {
        let bytes = self.conn.read_line(self.max_line, Instant::now() + timeout, None)?;
        String::from_utf8(bytes)
            .map_err(|_| ClientError::Io(io::Error::new(io::ErrorKind::InvalidData, "the answer is not UTF-8")))
    }

    /// Send a line and read the answer, within `timeout` in all.
    ///
    /// When the server had closed the connection before the line could be
    /// sent, an answer it left behind (a refusal) is still returned;
    /// without one the result is [`ClientError::NotDelivered`].
    pub fn request(&mut self, line: &str, timeout: Duration) -> Result<String, ClientError> {
        let deadline = Instant::now() + timeout;
        let sent = self.send_line(line, timeout);
        let left = deadline.saturating_duration_since(Instant::now());
        match sent {
            Ok(()) => self.read_line(left),
            Err(ClientError::Closed) => match self.read_line(left) {
                Ok(answer) => Ok(answer),
                Err(ClientError::Closed) => Err(ClientError::NotDelivered),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    /// The process serving the pipe, to check it is the expected program.
    pub fn server_process_id(&self) -> io::Result<u32> {
        let mut pid = 0u32;
        // SAFETY: a live pipe handle; valid out pointer.
        unsafe { GetNamedPipeServerProcessId(handle(&self.conn.pipe), &mut pid) }.map_err(win_err)?;
        Ok(pid)
    }

    /// The pipe's DACL as SDDL (`D:…`), to check who may connect.
    pub fn dacl_sddl(&self) -> io::Result<String> {
        dacl_sddl(&self.conn.pipe)
    }

    /// The pipe's owner as a SID string ([`PipeClient::connect`] already
    /// checked it is the current user).
    pub fn owner_sid(&self) -> io::Result<String> {
        owner_sid(&self.conn.pipe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A pipe name no other test or program uses.
    fn unique_name() -> String {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let mut h = RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        format!(
            r"\\.\pipe\kb-winsys-test-{}-{}-{:016x}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed),
            h.finish()
        )
    }

    fn echo() -> impl FnMut(Incoming<'_>) -> String {
        |incoming| match incoming {
            Incoming::Line(line) => format!("echo:{line}"),
            Incoming::TooLong => "too-long".to_string(),
            Incoming::NotText => "not-text".to_string(),
        }
    }

    fn quick(read_timeout_ms: u64) -> ServerOptions {
        ServerOptions { read_timeout: Duration::from_millis(read_timeout_ms), ..ServerOptions::default() }
    }

    const WAIT: Duration = Duration::from_secs(5);

    #[test]
    fn the_user_and_session_are_known() {
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"), "{sid}");
        assert!(sid.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'S'), "{sid}");
        current_session_id().unwrap();
    }

    #[test]
    fn lines_go_back_and_forth() {
        let name = unique_name();
        let server = serve(&name, echo).unwrap();
        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        assert_eq!(client.request("hello", WAIT).unwrap(), "echo:hello");
        assert_eq!(client.request("ünïcödé 黑爵", WAIT).unwrap(), "echo:ünïcödé 黑爵");
        // Two lines in one write come back as two answers.
        client.send_line("a", WAIT).unwrap();
        client.send_line("b", WAIT).unwrap();
        assert_eq!(client.read_line(WAIT).unwrap(), "echo:a");
        assert_eq!(client.read_line(WAIT).unwrap(), "echo:b");
        assert_eq!(client.server_process_id().unwrap(), std::process::id());
        assert!(client.send_line("x\ny", WAIT).is_err());
        drop(client);
        server.stop();
    }

    #[test]
    fn each_connection_keeps_its_own_state() {
        let name = unique_name();
        let server = serve(&name, || {
            let mut count = 0;
            move |_: Incoming<'_>| {
                count += 1;
                count.to_string()
            }
        })
        .unwrap();
        let mut a = PipeClient::connect(&name, WAIT).unwrap();
        let mut b = PipeClient::connect(&name, WAIT).unwrap();
        assert_eq!(a.request("x", WAIT).unwrap(), "1");
        assert_eq!(a.request("x", WAIT).unwrap(), "2");
        assert_eq!(b.request("x", WAIT).unwrap(), "1");
        drop((a, b));
        server.stop();
    }

    #[test]
    fn a_second_server_cannot_take_the_name() {
        let name = unique_name();
        let first = serve(&name, echo).unwrap();
        let second = serve(&name, echo).unwrap_err();
        assert_eq!(second.kind(), io::ErrorKind::AddrInUse);
        // The first one still answers.
        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        assert_eq!(client.request("still mine", WAIT).unwrap(), "echo:still mine");
        drop(client);
        first.stop();
        // Once it is gone, the name is free again.
        serve(&name, echo).unwrap().stop();
    }

    /// `(type, sid)` of each ACE in a `D:` SDDL string.
    fn aces(sddl: &str) -> Vec<(String, String)> {
        let dacl = &sddl[sddl.find("D:").expect("a DACL") + 2..];
        dacl.split('(')
            .skip(1)
            .map(|ace| {
                let fields: Vec<&str> = ace.trim_end_matches(')').split(';').collect();
                assert_eq!(fields.len(), 6, "{ace}");
                (fields[0].to_string(), fields[5].to_string())
            })
            .collect()
    }

    #[test]
    fn only_this_user_may_connect_and_never_from_the_network() {
        let name = unique_name();
        let server = serve(&name, echo).unwrap();
        let client = PipeClient::connect(&name, WAIT).unwrap();
        let sddl = client.dacl_sddl().unwrap();
        let user = current_user_sid().unwrap();
        // Windows writes a few well-known accounts by their SDDL alias: the
        // built-in Administrator (RID 500, which CI runners use) as `LA`, the
        // Guest (RID 501) as `LG`.
        let alias = match user.rsplit('-').next() {
            Some("500") => "LA",
            Some("501") => "LG",
            _ => "",
        };
        let mut found = aces(&sddl);
        for ace in &mut found {
            if !alias.is_empty() && ace.1 == alias {
                ace.1.clone_from(&user);
            }
        }
        found.sort();
        let mut expected = vec![("A".to_string(), user), ("D".to_string(), "NU".to_string())];
        expected.sort();
        assert_eq!(found, expected, "{sddl}");
        // The deny comes first, so it wins.
        assert!(sddl.find("(D;").unwrap() < sddl.find("(A;").unwrap(), "{sddl}");
        for nobody in ["WD", "AN", "BU", "AU", "S-1-1-0", "S-1-5-7"] {
            assert!(!sddl.contains(&format!(";{nobody})")), "{nobody} in {sddl}");
        }
        drop(client);
        server.stop();
    }

    #[test]
    fn the_pipe_belongs_to_this_user_and_no_one_else_is_trusted() {
        let name = unique_name();
        let server = serve(&name, echo).unwrap();
        let mine = PipeClient::connect(&name, WAIT).unwrap();
        // Set explicitly: an administrator's default owner would be the
        // Administrators group.
        assert_eq!(mine.owner_sid().unwrap(), current_user_sid().unwrap());
        drop(mine);
        // As if another account had taken the name first.
        let e = PipeClient::connect_owned_by(&name, WAIT, "S-1-5-18").unwrap_err();
        assert!(matches!(e, ClientError::NotTrusted(_)), "{e:?}");
        server.stop();
    }

    #[test]
    fn a_pipe_that_shuts_this_user_out_is_not_trusted() {
        // Someone else's pipe whose DACL leaves this user out.
        let name = unique_name();
        let only_system = SecurityDescriptor::from_sddl("D:P(A;;FA;;;SY)").unwrap();
        let _held = create_instance(&wide(&name), &only_system, 1, true).unwrap();
        let e = PipeClient::connect(&name, WAIT).unwrap_err();
        assert!(matches!(e, ClientError::NotTrusted(_)), "{e:?}");
    }

    #[test]
    fn the_server_keeps_its_name_while_clients_come_and_go() {
        let name = unique_name();
        let server = serve(&name, echo).unwrap();
        for i in 0..30 {
            let mut client = PipeClient::connect(&name, WAIT).unwrap();
            assert_eq!(client.request(&i.to_string(), WAIT).unwrap(), format!("echo:{i}"));
        }
        // Still its own: a second server cannot take the name.
        assert_eq!(serve(&name, echo).unwrap_err().kind(), io::ErrorKind::AddrInUse);
        server.stop();
    }

    #[test]
    fn a_missing_pipe_is_not_running_at_once() {
        let started = Instant::now();
        let e = PipeClient::connect(&unique_name(), Duration::from_secs(10)).unwrap_err();
        assert!(matches!(e, ClientError::NotRunning), "{e:?}");
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    }

    #[test]
    fn a_server_that_never_answers_times_out() {
        let name = unique_name();
        let server = serve(&name, || {
            |_: Incoming<'_>| {
                std::thread::sleep(Duration::from_millis(1500));
                "late".to_string()
            }
        })
        .unwrap();
        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        let started = Instant::now();
        let e = client.request("anyone?", Duration::from_millis(300)).unwrap_err();
        assert!(matches!(e, ClientError::Timeout), "{e:?}");
        assert!(started.elapsed() < Duration::from_millis(1200), "{:?}", started.elapsed());
        drop(client);
        server.stop();
    }

    #[test]
    fn a_silent_client_is_dropped_after_the_read_deadline() {
        let name = unique_name();
        let server = PipeServer::start(&name, quick(300), echo).unwrap();
        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        let started = Instant::now();
        let e = client.read_line(WAIT).unwrap_err();
        assert!(matches!(e, ClientError::Closed), "{e:?}");
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(250) && waited < Duration::from_secs(3), "{waited:?}");
        // A line sent on the closed connection never arrived, and says so.
        let e = client.request("late", WAIT).unwrap_err();
        assert!(matches!(e, ClientError::NotDelivered), "{e:?}");
        server.stop();
    }

    #[test]
    fn a_line_must_arrive_by_its_deadline_however_it_trickles_in() {
        let name = unique_name();
        let server = PipeServer::start(&name, quick(400), echo).unwrap();
        let slow = PipeClient::connect(&name, WAIT).unwrap();
        let started = Instant::now();
        // One byte every 100 ms for a second and never a line break: every
        // read gets data well within 400 ms, so only a deadline for the
        // whole line ends it.
        let mut cut = None;
        for _ in 0..10 {
            if slow.conn.write_all(b"x", Instant::now() + WAIT, None).is_err() {
                cut = Some(started.elapsed());
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let cut = cut.expect("the server kept a trickling line past its deadline");
        assert!(cut >= Duration::from_millis(350) && cut < Duration::from_millis(900), "{cut:?}");
        server.stop();
    }

    #[test]
    fn lines_over_the_limit_are_refused_and_the_connection_closed() {
        let name = unique_name();
        let server = serve(&name, echo).unwrap();
        let limit = ServerOptions::default().max_line;

        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        client.set_max_line(2 * limit);
        let fits = "x".repeat(limit);
        assert_eq!(client.request(&fits, WAIT).unwrap().len(), limit + "echo:".len());

        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        client.set_max_line(2 * limit);
        assert_eq!(client.request(&"x".repeat(limit + 1), WAIT).unwrap(), "too-long");
        assert!(matches!(client.read_line(WAIT).unwrap_err(), ClientError::Closed));

        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        client.conn.write_all(b"\xff\xfe\n", Instant::now() + WAIT, None).unwrap();
        assert_eq!(client.read_line(WAIT).unwrap(), "not-text");
        server.stop();
    }

    #[test]
    fn at_most_four_connections_at_once() {
        let name = unique_name();
        // A long idle deadline: a slow machine must not drop the four
        // connections while the fifth waits.
        let patient = ServerOptions { read_timeout: Duration::from_secs(60), ..ServerOptions::default() };
        let server = PipeServer::start(&name, patient, echo).unwrap();
        let mut clients: Vec<PipeClient> =
            (0..MAX_INSTANCES).map(|_| PipeClient::connect(&name, WAIT).unwrap()).collect();
        for c in &mut clients {
            assert_eq!(c.request("x", WAIT).unwrap(), "echo:x");
        }
        assert_eq!(server.connections(), MAX_INSTANCES);
        let e = PipeClient::connect(&name, Duration::from_millis(300)).unwrap_err();
        assert!(matches!(e, ClientError::Timeout), "{e:?}");
        // A place frees up when one leaves.
        clients.pop();
        let mut late = PipeClient::connect(&name, WAIT).unwrap();
        assert_eq!(late.request("y", WAIT).unwrap(), "echo:y");
        drop((clients, late));
        server.stop();
    }

    #[test]
    fn a_panicking_handler_ends_only_its_connection() {
        let name = unique_name();
        let server = serve(&name, || {
            |incoming: Incoming<'_>| match incoming {
                Incoming::Line("boom") => panic!("handler failed (expected in this test)"),
                Incoming::Line(line) => line.to_string(),
                _ => String::new(),
            }
        })
        .unwrap();
        let mut doomed = PipeClient::connect(&name, WAIT).unwrap();
        assert!(matches!(doomed.request("boom", WAIT).unwrap_err(), ClientError::Closed));
        let mut fine = PipeClient::connect(&name, WAIT).unwrap();
        assert_eq!(fine.request("ok", WAIT).unwrap(), "ok");
        drop(fine);
        server.stop();
    }

    #[test]
    fn stopping_ends_connections_and_frees_the_name() {
        let name = unique_name();
        let server = serve(&name, echo).unwrap();
        let mut client = PipeClient::connect(&name, WAIT).unwrap();
        assert_eq!(client.request("x", WAIT).unwrap(), "echo:x");
        let started = Instant::now();
        server.stop();
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        assert!(client.request("y", WAIT).is_err());
        assert!(matches!(PipeClient::connect(&name, WAIT).unwrap_err(), ClientError::NotRunning));
    }

    #[test]
    fn names_outside_the_pipe_namespace_are_refused() {
        assert_eq!(serve(r"C:\temp\x", echo).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert!(matches!(PipeClient::connect(r"C:\temp\x", WAIT).unwrap_err(), ClientError::Io(_)));
    }
}
