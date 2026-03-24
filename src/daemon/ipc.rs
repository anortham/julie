//! IPC transport abstraction for daemon-adapter communication.
//!
//! On Unix, this wraps `tokio::net::UnixListener` / `tokio::net::UnixStream`.
//! On Windows, this wraps tokio named pipes (`NamedPipeServer` / `NamedPipeClient`).
//! The streams implement `AsyncRead + AsyncWrite`, so no custom wrapper is needed.

#[cfg(unix)]
mod unix {
    use std::io;
    use std::path::{Path, PathBuf};
    use tokio::net::{UnixListener, UnixStream};
    use tracing::debug;

    /// Server-side stream type (returned by `IpcListener::accept()`).
    pub type IpcStream = UnixStream;

    /// Client-side stream type (returned by `IpcConnector::connect()`).
    pub type IpcClientStream = UnixStream;

    /// Listener for incoming IPC connections over a Unix domain socket.
    ///
    /// Manages the socket file lifecycle: creates parent directories on bind,
    /// removes stale sockets, and cleans up the file on explicit cleanup.
    pub struct IpcListener {
        listener: UnixListener,
        path: PathBuf,
    }

    impl IpcListener {
        /// Bind to a Unix domain socket at `path`.
        ///
        /// If a file already exists at `path` (stale socket or leftover), it is
        /// removed before binding. Parent directories are created if missing.
        pub async fn bind(path: &Path) -> io::Result<Self> {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Remove stale socket file if present
            if path.exists() {
                debug!("Removing stale socket file: {}", path.display());
                std::fs::remove_file(path)?;
            }

            let listener = UnixListener::bind(path)?;
            debug!("IPC listener bound to: {}", path.display());

            Ok(Self {
                listener,
                path: path.to_path_buf(),
            })
        }

        /// Accept the next incoming connection.
        ///
        /// Returns a stream that can be passed directly to rmcp's
        /// `handler.serve(stream)` (it implements `AsyncRead + AsyncWrite`).
        pub async fn accept(&self) -> io::Result<IpcStream> {
            let (stream, _addr) = self.listener.accept().await?;
            debug!("Accepted IPC connection on: {}", self.path.display());
            Ok(stream)
        }

        /// Remove the socket file. Call during graceful shutdown.
        pub fn cleanup(self) {
            if self.path.exists() {
                if let Err(e) = std::fs::remove_file(&self.path) {
                    tracing::warn!(
                        "Failed to remove socket file {}: {}",
                        self.path.display(),
                        e
                    );
                } else {
                    debug!("Removed socket file: {}", self.path.display());
                }
            }
        }

        /// Returns the path to the IPC endpoint.
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    /// Connector for establishing IPC connections to a running daemon.
    pub struct IpcConnector;

    impl IpcConnector {
        /// Connect to a daemon's Unix domain socket at `path`.
        ///
        /// Returns a stream that can be used with rmcp's client
        /// transport (it implements `AsyncRead + AsyncWrite`).
        pub async fn connect(path: &Path) -> io::Result<IpcClientStream> {
            let stream = UnixStream::connect(path).await?;
            debug!("Connected to IPC socket: {}", path.display());
            Ok(stream)
        }
    }
}

#[cfg(unix)]
pub use unix::{IpcClientStream, IpcConnector, IpcListener, IpcStream};

#[cfg(windows)]
mod windows {
    use std::io;
    use std::path::{Path, PathBuf};
    use tokio::net::windows::named_pipe::{
        ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
    };
    use tokio::sync::Mutex;
    use tracing::debug;

    /// Server-side stream type (returned by `IpcListener::accept()`).
    pub type IpcStream = NamedPipeServer;

    /// Client-side stream type (returned by `IpcConnector::connect()`).
    pub type IpcClientStream = NamedPipeClient;

    /// Listener for incoming IPC connections over a Windows named pipe.
    ///
    /// Named pipes work differently from Unix sockets: each client connection
    /// consumes a pipe instance, so the listener pre-creates the next instance
    /// after each accept. The `next_server` Mutex holds the instance that the
    /// next `accept()` call will use.
    pub struct IpcListener {
        next_server: Mutex<NamedPipeServer>,
        pipe_name: PathBuf,
    }

    impl IpcListener {
        /// Bind to a Windows named pipe at `path`.
        ///
        /// `path` should be a named pipe path like `\\.\pipe\julie-daemon`.
        /// Uses `first_pipe_instance(true)` to ensure exclusive ownership.
        pub async fn bind(path: &Path) -> io::Result<Self> {
            let pipe_name = path.to_path_buf();
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(path)?;
            debug!("IPC listener bound to named pipe: {}", path.display());
            Ok(Self {
                next_server: Mutex::new(server),
                pipe_name,
            })
        }

        /// Accept the next incoming connection.
        ///
        /// Waits for a client to connect, then swaps in a fresh pipe instance
        /// for the next caller and returns the connected one.
        pub async fn accept(&self) -> io::Result<IpcStream> {
            let mut guard = self.next_server.lock().await;
            guard.connect().await?;
            debug!("Accepted IPC connection on: {}", self.pipe_name.display());

            // Replace with a fresh instance for the next accept()
            let connected =
                std::mem::replace(&mut *guard, ServerOptions::new().create(&self.pipe_name)?);
            Ok(connected)
        }

        /// No-op on Windows. Named pipes are kernel objects with no file to remove.
        pub fn cleanup(self) {
            // Nothing to clean up
        }

        /// Returns the pipe name as a path.
        pub fn path(&self) -> &Path {
            &self.pipe_name
        }
    }

    /// Connector for establishing IPC connections to a running daemon.
    pub struct IpcConnector;

    impl IpcConnector {
        /// Connect to a daemon's named pipe at `path`.
        ///
        /// Returns a `NamedPipeClient` that implements `AsyncRead + AsyncWrite`.
        ///
        /// Windows named pipes return ERROR_PIPE_BUSY (231) when all server
        /// instances are occupied. This is a transient condition: the daemon
        /// creates a new instance after each accept(). We retry with
        /// exponential backoff to ride out the brief window between the
        /// daemon consuming an instance and creating the next one.
        pub async fn connect(path: &Path) -> io::Result<IpcClientStream> {
            const MAX_ATTEMPTS: u32 = 10;
            let mut delay = std::time::Duration::from_millis(50);
            let max_delay = std::time::Duration::from_millis(500);

            for attempt in 1..=MAX_ATTEMPTS {
                match ClientOptions::new().open(path) {
                    Ok(client) => {
                        debug!("Connected to IPC named pipe: {}", path.display());
                        return Ok(client);
                    }
                    Err(e) if e.raw_os_error() == Some(231) && attempt < MAX_ATTEMPTS => {
                        // ERROR_PIPE_BUSY: all instances occupied (transient).
                        debug!(
                            "Named pipe busy (attempt {}/{}), retrying in {}ms",
                            attempt,
                            MAX_ATTEMPTS,
                            delay.as_millis()
                        );
                        tokio::time::sleep(delay).await;
                        delay = (delay * 2).min(max_delay);
                    }
                    Err(e) => return Err(e),
                }
            }

            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "Named pipe {} busy after {} attempts",
                    path.display(),
                    MAX_ATTEMPTS
                ),
            ))
        }
    }
}

#[cfg(windows)]
pub use windows::{IpcClientStream, IpcConnector, IpcListener, IpcStream};
