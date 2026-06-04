//! Cross-platform IPC transport seam for the resident embedding-host (Phase 3b).
//!
//! This module is **protocol-agnostic**: it moves newline-delimited *lines* of
//! bytes between a session process and the embedding-host. It knows nothing
//! about the embedding envelope — callers ([`super::rpc_client`] /
//! [`super::host_server`]) marshal `RequestEnvelope`/`ResponseEnvelope` JSON.
//!
//! Two halves:
//! - **Blocking client** ([`HostClientConn`]): used inside the synchronous
//!   `EmbeddingProvider` trait methods, which already run under
//!   `tokio::task::spawn_blocking` at every call site. Mirrors the blocking
//!   stdin/stdout framing of `SidecarProcess::send_request_with_timeout`
//!   (`sidecar_provider.rs`): write one JSON line + `\n` + flush, read one line.
//! - **Async server** ([`HostListener`] / [`HostServerConn`]): the host's tokio
//!   accept loop, modeled on `HttpTransportServer`'s listener structure.
//!
//! Platform support: **Unix domain sockets** are the proven path (implemented +
//! tested). **Windows named pipes** are `cfg`-gated, written to the documented
//! `tokio::net::windows::named_pipe` API, and are **not exercised on the CI/dev
//! box** (matches the Phase 3 design's "Unix proof is the must-have" stance).

use std::io::{self, BufRead, BufReader, Write};

#[cfg(unix)]
use std::path::PathBuf;

use julie_core::paths::DaemonPaths;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader as TokioBufReader};

#[cfg(unix)]
use std::os::unix::net::UnixStream as StdUnixStream;
#[cfg(unix)]
use tokio::net::UnixListener;

#[cfg(windows)]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

/// Platform-specific address of the embedding-host front door.
///
/// Construct once from [`DaemonPaths`]; pass by reference to the client and
/// server. The inner representation differs per platform (socket path vs pipe
/// name) but callers never need to branch on it.
#[derive(Clone, Debug)]
pub struct HostAddress {
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(windows)]
    pipe: String,
}

impl HostAddress {
    /// Derive the address for the global per-`$JULIE_HOME` embedding-host.
    pub fn from_paths(paths: &DaemonPaths) -> Self {
        #[cfg(unix)]
        {
            Self {
                socket: paths.embedding_host_socket(),
            }
        }
        #[cfg(windows)]
        {
            Self {
                pipe: paths.embedding_host_pipe_name(),
            }
        }
    }

    /// The unix socket path, for stale-file cleanup by the host.
    #[cfg(unix)]
    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket
    }
}

// ---------------------------------------------------------------------------
// Blocking client
// ---------------------------------------------------------------------------

/// A blocking connection to the embedding-host.
///
/// One request is in flight at a time (strictly request/response), matching the
/// synchronous `EmbeddingProvider` trait. Hold one per logical client behind a
/// `Mutex` for interior mutability.
pub struct HostClientConn {
    #[cfg(unix)]
    writer: StdUnixStream,
    #[cfg(unix)]
    reader: BufReader<StdUnixStream>,
    #[cfg(windows)]
    writer: std::fs::File,
    #[cfg(windows)]
    reader: BufReader<std::fs::File>,
}

impl HostClientConn {
    /// Open a blocking connection to the host. Errors if the host is not
    /// listening (socket/pipe absent or refused).
    pub fn connect(addr: &HostAddress) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let writer = StdUnixStream::connect(&addr.socket)?;
            let reader = BufReader::new(writer.try_clone()?);
            Ok(Self { writer, reader })
        }
        #[cfg(windows)]
        {
            // A Windows named pipe is opened like a file. Byte-mode framing
            // (newline-delimited) matches the server side below.
            let writer = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&addr.pipe)?;
            let reader = BufReader::new(writer.try_clone()?);
            Ok(Self { writer, reader })
        }
    }

    /// Send one request line and read exactly one response line.
    ///
    /// `request_line` MUST be a single line (no embedded `\n`) — JSON produced
    /// by `serde_json::to_string` satisfies this. The returned string has its
    /// trailing newline stripped.
    pub fn round_trip(&mut self, request_line: &str) -> io::Result<String> {
        self.writer.write_all(request_line.as_bytes())?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;

        let mut line = String::new();
        let n = self.reader.read_line(&mut line)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "embedding-host closed the connection before responding",
            ));
        }
        Ok(line.trim_end_matches(['\n', '\r']).to_string())
    }
}

// ---------------------------------------------------------------------------
// Async server
// ---------------------------------------------------------------------------

/// The host's listening socket/pipe. On unix the bound socket file is removed
/// on drop so a restart can re-bind cleanly.
pub struct HostListener {
    #[cfg(unix)]
    inner: UnixListener,
    #[cfg(unix)]
    socket_path: PathBuf,
    #[cfg(windows)]
    pipe: String,
    // The next idle pipe instance awaiting a client (Windows accept loop).
    #[cfg(windows)]
    pending: tokio::sync::Mutex<Option<NamedPipeServer>>,
}

impl HostListener {
    /// Bind the front door. On unix this removes any stale socket file first.
    pub async fn bind(addr: &HostAddress) -> io::Result<Self> {
        #[cfg(unix)]
        {
            // `UnixListener::bind` fails with AddrInUse if the path exists, even
            // when no process is listening (a leaked socket file). Clear it.
            let _ = std::fs::remove_file(&addr.socket);
            let inner = UnixListener::bind(&addr.socket)?;
            Ok(Self {
                inner,
                socket_path: addr.socket.clone(),
            })
        }
        #[cfg(windows)]
        {
            let first = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&addr.pipe)?;
            Ok(Self {
                pipe: addr.pipe.clone(),
                pending: tokio::sync::Mutex::new(Some(first)),
            })
        }
    }

    /// Accept the next client connection.
    pub async fn accept(&self) -> io::Result<HostServerConn> {
        #[cfg(unix)]
        {
            let (stream, _peer) = self.inner.accept().await?;
            let (read_half, write_half) = stream.into_split();
            Ok(HostServerConn {
                reader: TokioBufReader::new(Box::new(read_half)),
                writer: Box::new(write_half),
            })
        }
        #[cfg(windows)]
        {
            // Named-pipe accept loop: wait for a client on the current idle
            // instance, then immediately create the next instance so the
            // following accept() has something to wait on.
            let mut guard = self.pending.lock().await;
            let server = guard
                .take()
                .expect("HostListener invariant: a pending pipe instance always exists");
            server.connect().await?;
            let next = ServerOptions::new().create(&self.pipe)?;
            *guard = Some(next);
            drop(guard);
            let (read_half, write_half) = tokio::io::split(server);
            Ok(HostServerConn {
                reader: TokioBufReader::new(Box::new(read_half)),
                writer: Box::new(write_half),
            })
        }
    }
}

#[cfg(unix)]
impl Drop for HostListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// A single accepted connection on the host side. Async, one request/response
/// at a time (the client is strictly sequential).
pub struct HostServerConn {
    reader: TokioBufReader<Box<dyn AsyncRead + Send + Unpin>>,
    writer: Box<dyn AsyncWrite + Send + Unpin>,
}

impl HostServerConn {
    /// Read one request line. Returns `Ok(None)` on clean EOF (client hung up).
    pub async fn read_line(&mut self) -> io::Result<Option<String>> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end_matches(['\n', '\r']).to_string()))
    }

    /// Write one response line (a `\n` terminator is appended and flushed).
    pub async fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }
}
