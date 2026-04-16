//! Narrow transport contract for daemon IPC readiness, bind, and connect behavior.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::ipc::{IpcClientStream, IpcConnector, IpcListener};

/// Contract-level readiness outcome for an IPC endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProbe {
    Ready,
    NotReady,
}

impl TransportProbe {
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Lifecycle-facing handle for an IPC endpoint.
#[derive(Debug, Clone)]
pub struct TransportEndpoint {
    endpoint: PathBuf,
}

impl TransportEndpoint {
    pub fn new(endpoint: PathBuf) -> Self {
        Self { endpoint }
    }

    pub fn path(&self) -> &Path {
        &self.endpoint
    }

    pub async fn bind_listener(&self) -> io::Result<IpcListener> {
        IpcListener::bind(self.path()).await
    }

    pub async fn connect(&self) -> io::Result<IpcClientStream> {
        IpcConnector::connect(self.path()).await
    }

    pub fn probe_readiness(&self) -> TransportProbe {
        #[cfg(unix)]
        {
            if std::os::unix::net::UnixStream::connect(self.path()).is_ok() {
                TransportProbe::Ready
            } else {
                TransportProbe::NotReady
            }
        }

        #[cfg(windows)]
        {
            if named_pipe_exists(self.path()) {
                TransportProbe::Ready
            } else {
                TransportProbe::NotReady
            }
        }
    }

    pub fn wait_for_readiness(&self, timeout: Duration) -> io::Result<()> {
        let start = Instant::now();
        let mut delay = Duration::from_millis(50);
        let max_delay = Duration::from_millis(500);

        loop {
            if self.probe_readiness().is_ready() {
                return Ok(());
            }

            if start.elapsed() >= timeout {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "Daemon IPC endpoint did not appear within {}ms",
                        timeout.as_millis()
                    ),
                ));
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);
        }
    }
}

/// Check if a Windows named pipe exists without connecting to it.
///
/// Uses `WaitNamedPipeW` with a 1ms timeout so readiness probes do not consume
/// a pipe instance before the daemon enters its accept loop.
#[cfg(windows)]
fn named_pipe_exists(pipe_path: &Path) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn WaitNamedPipeW(lpNamedPipeName: *const u16, nTimeOut: u32) -> i32;
    }

    let wide: Vec<u16> = OsStr::new(pipe_path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe { WaitNamedPipeW(wide.as_ptr(), 1) };
    if result != 0 {
        return true;
    }

    let err = io::Error::last_os_error();
    err.raw_os_error() == Some(121)
}
