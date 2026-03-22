//! IPC transport abstraction for daemon-adapter communication.
//!
//! On Unix, this wraps `tokio::net::UnixListener` / `tokio::net::UnixStream`.
//! The streams are directly compatible with rmcp's `IntoTransport` (via
//! `AsyncRead + AsyncWrite`), so no custom wrapper type is needed.

#[cfg(unix)]
mod unix {
    use std::io;
    use std::path::{Path, PathBuf};
    use tokio::net::{UnixListener, UnixStream};
    use tracing::debug;

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
        /// Returns a raw `UnixStream` that can be passed directly to rmcp's
        /// `handler.serve(stream)` (it implements `AsyncRead + AsyncWrite`).
        pub async fn accept(&self) -> io::Result<UnixStream> {
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

        /// Returns the path to the socket file.
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    /// Connector for establishing IPC connections to a running daemon.
    pub struct IpcConnector;

    impl IpcConnector {
        /// Connect to a daemon's Unix domain socket at `path`.
        ///
        /// Returns a raw `UnixStream` that can be used with rmcp's client
        /// transport (it implements `AsyncRead + AsyncWrite`).
        pub async fn connect(path: &Path) -> io::Result<UnixStream> {
            let stream = UnixStream::connect(path).await?;
            debug!("Connected to IPC socket: {}", path.display());
            Ok(stream)
        }
    }
}

#[cfg(unix)]
pub use unix::{IpcConnector, IpcListener};

#[cfg(windows)]
mod windows {
    // TODO: Implement Windows named pipe support.
    // Windows equivalent would use `\\.\pipe\julie-daemon` named pipes.
    // tokio has `tokio::net::windows::named_pipe` for this purpose.
    //
    // The API should mirror the Unix version:
    //   IpcListener::bind(pipe_name) -> accept() -> named pipe stream
    //   IpcConnector::connect(pipe_name) -> named pipe stream
}
