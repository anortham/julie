//! Windows named event for graceful daemon shutdown.
//!
//! The daemon creates a named event at startup. `julie stop` and `julie restart`
//! signal this event to trigger a clean shutdown through the normal cleanup path
//! (drain sessions, flush logs, remove PID file).
//!
//! This replaces `taskkill /F` which terminates the process immediately, losing
//! buffered log output and skipping cleanup.

use std::io;

/// A Windows named event handle. Closes the handle on drop.
pub struct ShutdownEvent {
    handle: isize,
}

// SAFETY: Windows HANDLEs are process-global and safe to wait on from any thread.
unsafe impl Send for ShutdownEvent {}
unsafe impl Sync for ShutdownEvent {}

unsafe extern "system" {
    fn CreateEventW(
        lpEventAttributes: *const std::ffi::c_void,
        bManualReset: i32,
        bInitialState: i32,
        lpName: *const u16,
    ) -> isize;
    fn OpenEventW(dwDesiredAccess: u32, bInheritHandle: i32, lpName: *const u16) -> isize;
    fn SetEvent(hEvent: isize) -> i32;
    fn WaitForSingleObject(hHandle: isize, dwMilliseconds: u32) -> u32;
    fn CloseHandle(handle: isize) -> i32;
}

const INFINITE: u32 = 0xFFFFFFFF;
const EVENT_MODIFY_STATE: u32 = 0x0002;

fn to_wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

impl ShutdownEvent {
    /// Create a new named auto-reset event in the non-signaled state.
    ///
    /// If an event with this name already exists (e.g., stale from a crashed
    /// daemon), the existing event is opened instead.
    pub fn create(name: &str) -> io::Result<Self> {
        let wide = to_wide(name);
        let handle = unsafe { CreateEventW(std::ptr::null(), 0, 0, wide.as_ptr()) };
        if handle == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { handle })
    }

    /// Block the calling thread until the event is signaled.
    ///
    /// Intended to be called from `tokio::task::spawn_blocking`.
    pub fn wait(&self) {
        unsafe { WaitForSingleObject(self.handle, INFINITE) };
    }
}

impl Drop for ShutdownEvent {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

/// Signal an existing named shutdown event, waking any waiter.
///
/// Used by `julie stop` and `julie restart` to trigger graceful shutdown.
/// Returns `Ok(true)` if signaled, `Ok(false)` if the event doesn't exist
/// (daemon not running or old version without event support).
pub fn signal_shutdown(event_name: &str) -> io::Result<bool> {
    let wide = to_wide(event_name);
    let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, wide.as_ptr()) };
    if handle == 0 {
        let err = io::Error::last_os_error();
        // ERROR_FILE_NOT_FOUND (2): event doesn't exist
        if err.raw_os_error() == Some(2) {
            return Ok(false);
        }
        return Err(err);
    }

    let result = unsafe { SetEvent(handle) };
    unsafe { CloseHandle(handle) };

    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(true)
    }
}
