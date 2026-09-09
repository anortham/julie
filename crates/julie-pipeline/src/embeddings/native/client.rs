//! Low-level IPC transport and pure reply decoders for native embeddings sidecar.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::net::UnixStream as StdUnixStream;
#[cfg(unix)]
use std::path::PathBuf;

pub use super::decoders::*;

/// Maximum allowed payload size for requests and responses (32 MiB).
pub const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

/// Reads a line from `reader` up to `max_bytes`, bounded by an optional absolute deadline
/// and per-read timeout clamp hook.
pub fn read_line_bounded_deadline<R: BufRead, F>(
    reader: &mut R,
    out: &mut Vec<u8>,
    max_bytes: usize,
    deadline: Option<Instant>,
    per_read_timeout: Option<Duration>,
    mut clamp_stream_timeout: F,
) -> io::Result<usize>
where
    F: FnMut(Option<Duration>) -> io::Result<()>,
{
    out.clear();
    let deadline = deadline.or_else(|| per_read_timeout.map(|t| Instant::now() + t));
    let mut total_read = 0;
    loop {
        // Enforce absolute deadline check at the start of every read attempt
        if let Some(dl) = deadline {
            let now = Instant::now();
            if now >= dl {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "overall deadline exceeded while reading response",
                ));
            }
            let remaining = dl.saturating_duration_since(now);
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "overall deadline exceeded while reading response",
                ));
            }
            // Clamp per-read buffer timeout to min(per_read_timeout, remaining)
            let effective = match per_read_timeout {
                Some(pr) => pr.min(remaining),
                None => remaining,
            };
            clamp_stream_timeout(Some(effective))?;
        } else if let Some(pr) = per_read_timeout {
            clamp_stream_timeout(Some(pr))?;
        }

        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(total_read);
        }

        if let Some(pos) = available.iter().position(|&b| b == b'\n') {
            let take_len = pos + 1;
            if total_read + take_len > max_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("response line exceeds maximum allowed size of {max_bytes} bytes"),
                ));
            }
            out.extend_from_slice(&available[..take_len]);
            reader.consume(take_len);
            total_read += take_len;
            return Ok(total_read);
        } else {
            let chunk_len = available.len();
            if total_read + chunk_len > max_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("response line exceeds maximum allowed size of {max_bytes} bytes"),
                ));
            }
            out.extend_from_slice(available);
            reader.consume(chunk_len);
            total_read += chunk_len;
        }

        // Check deadline immediately after consuming chunk to catch trickling bytes
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "overall deadline exceeded while reading response line",
                ));
            }
        }
    }
}

/// Reads a line from `reader` up to `max_bytes` without unbounded allocation.
pub fn read_line_bounded<R: BufRead>(
    reader: &mut R,
    out: &mut Vec<u8>,
    max_bytes: usize,
) -> io::Result<usize> {
    read_line_bounded_deadline(reader, out, max_bytes, None, None, |_| Ok(()))
}

pub fn strip_trailing_newline(buf: &mut Vec<u8>) {
    while let Some(&last) = buf.last() {
        if last == b'\n' || last == b'\r' {
            buf.pop();
        } else {
            break;
        }
    }
}

/// Detects if an I/O error was caused by a dropped or broken connection.
pub fn is_connection_dropped(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::NotConnected
    )
}

/// Cross-platform IPC connection to the native sidecar broker.
pub struct NativeClientConn {
    #[cfg(unix)]
    writer: StdUnixStream,
    #[cfg(unix)]
    reader_stream: StdUnixStream,
    #[cfg(unix)]
    reader: BufReader<StdUnixStream>,
    #[cfg(unix)]
    endpoint: PathBuf,

    #[cfg(windows)]
    writer: std::fs::File,
    #[cfg(windows)]
    reader: BufReader<std::fs::File>,
    #[cfg(windows)]
    pipe_name: String,
    #[cfg(windows)]
    timeout: Option<Duration>,
}

impl NativeClientConn {
    #[cfg(unix)]
    pub fn connect(endpoint: &Path, timeout: Option<Duration>) -> io::Result<Self> {
        let writer = match timeout {
            Some(t) if !t.is_zero() => {
                let ep = endpoint.to_path_buf();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(StdUnixStream::connect(&ep));
                });
                match rx.recv_timeout(t) {
                    Ok(res) => res?,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "timed out connecting to native broker at '{}' after {:?}",
                                endpoint.display(),
                                t
                            ),
                        ));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        return Err(io::Error::new(
                            io::ErrorKind::ConnectionRefused,
                            "connect worker thread terminated unexpectedly",
                        ));
                    }
                }
            }
            _ => StdUnixStream::connect(endpoint)?,
        };
        writer.set_write_timeout(timeout)?;
        let reader_stream = writer.try_clone()?;
        reader_stream.set_read_timeout(timeout)?;
        let reader = BufReader::new(reader_stream.try_clone()?);
        Ok(Self {
            writer,
            reader_stream,
            reader,
            endpoint: endpoint.to_path_buf(),
        })
    }

    #[cfg(windows)]
    pub fn connect(pipe_name: &str, timeout: Option<Duration>) -> io::Result<Self> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(15);
        let limit = timeout.unwrap_or(Duration::from_secs(5));

        let writer = loop {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(pipe_name)
            {
                Ok(f) => break f,
                Err(e) if start.elapsed() < limit => {
                    std::thread::sleep(poll_interval);
                }
                Err(e) => return Err(e),
            }
        };
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Self {
            writer,
            reader,
            pipe_name: pipe_name.to_string(),
            timeout,
        })
    }

    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        #[cfg(unix)]
        {
            self.writer.set_write_timeout(timeout)?;
            self.reader_stream.set_read_timeout(timeout)?;
        }
        #[cfg(windows)]
        {
            self.timeout = timeout;
        }
        Ok(())
    }

    /// Sends a JSON request and reads a JSON response, enforcing timeouts and size bounds.
    pub fn round_trip(
        &mut self,
        request_bytes: &[u8],
        timeout: Option<Duration>,
    ) -> io::Result<Vec<u8>> {
        if request_bytes.len() + 1 > MAX_PAYLOAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("request payload exceeds {MAX_PAYLOAD_BYTES} bytes"),
            ));
        }

        let deadline = timeout.map(|t| std::time::Instant::now() + t);
        if let Some(dl) = deadline {
            let rem = dl.saturating_duration_since(std::time::Instant::now());
            if rem.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "deadline exceeded before request write",
                ));
            }
            self.set_timeout(Some(rem))?;
        }

        self.writer.write_all(request_bytes)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;

        if let Some(dl) = deadline {
            let rem = dl.saturating_duration_since(std::time::Instant::now());
            if rem.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "deadline exceeded before response read",
                ));
            }
            self.set_timeout(Some(rem))?;
        }

        let mut out = Vec::new();
        #[cfg(unix)]
        let reader_stream = &self.reader_stream;
        #[cfg(unix)]
        let mut clamp_fn =
            |t: Option<Duration>| -> io::Result<()> { reader_stream.set_read_timeout(t) };
        #[cfg(windows)]
        let mut clamp_fn = |_t: Option<Duration>| -> io::Result<()> { Ok(()) };

        let bytes_read = read_line_bounded_deadline(
            &mut self.reader,
            &mut out,
            MAX_PAYLOAD_BYTES,
            deadline,
            timeout,
            &mut clamp_fn,
        )?;
        if bytes_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "broker closed connection before sending response",
            ));
        }

        if let Some(dl) = deadline {
            if std::time::Instant::now() >= dl {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "deadline exceeded after response read",
                ));
            }
        }

        strip_trailing_newline(&mut out);
        Ok(out)
    }

    #[cfg(unix)]
    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    #[cfg(windows)]
    pub fn endpoint_str(&self) -> &str {
        &self.pipe_name
    }

    /// Returns the operating system process ID of the connected peer broker.
    #[cfg(unix)]
    pub fn peer_pid(&self) -> io::Result<u32> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self.writer.as_raw_fd();
            let mut ucred = libc::ucred {
                pid: 0,
                uid: 0,
                gid: 0,
            };
            let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
            let res = unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    &mut ucred as *mut _ as *mut libc::c_void,
                    &mut len,
                )
            };
            if res == 0 && ucred.pid > 0 {
                Ok(ucred.pid as u32)
            } else {
                Err(io::Error::last_os_error())
            }
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self.writer.as_raw_fd();
            let mut pid: libc::pid_t = 0;
            let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
            let res = unsafe {
                libc::getsockopt(
                    fd,
                    libc::SOL_LOCAL,
                    libc::LOCAL_PEERPID,
                    &mut pid as *mut _ as *mut libc::c_void,
                    &mut len,
                )
            };
            if res == 0 && pid > 0 {
                Ok(pid as u32)
            } else {
                Err(io::Error::last_os_error())
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "peer pid retrieval unsupported on this unix platform",
            ))
        }
    }

    /// Returns the operating system process ID of the connected peer broker.
    #[cfg(windows)]
    pub fn peer_pid(&self) -> io::Result<u32> {
        use std::os::windows::io::AsRawHandle;
        let handle = self.writer.as_raw_handle();
        let mut pid: u32 = 0;
        unsafe extern "system" {
            fn GetNamedPipeServerProcessId(
                Pipe: *mut std::ffi::c_void,
                ServerProcessId: *mut u32,
            ) -> i32;
        }
        let res = unsafe { GetNamedPipeServerProcessId(handle as *mut _, &mut pid) };
        if res != 0 && pid > 0 {
            Ok(pid)
        } else {
            Err(io::Error::last_os_error())
        }
    }
}
