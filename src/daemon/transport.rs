//! Narrow transport contract for daemon HTTP readiness behavior.

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Contract-level readiness outcome for a daemon endpoint.
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

/// Transport mode advertised in daemon discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    StreamableHttp,
}

/// Lifecycle-facing handle for a daemon endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TransportEndpoint {
    StreamableHttp {
        scheme: String,
        host: String,
        port: u16,
        mcp_path: String,
        readiness_path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        token_path: Option<PathBuf>,
    },
}

impl TransportEndpoint {
    pub fn streamable_http(
        host: impl Into<String>,
        port: u16,
        mcp_path: impl Into<String>,
        readiness_path: impl Into<String>,
        token_path: Option<PathBuf>,
    ) -> io::Result<Self> {
        let endpoint = Self::StreamableHttp {
            scheme: "http".to_string(),
            host: host.into(),
            port,
            mcp_path: mcp_path.into(),
            readiness_path: readiness_path.into(),
            token_path,
        };
        endpoint.validate()?;
        Ok(endpoint)
    }

    pub fn mode(&self) -> TransportMode {
        TransportMode::StreamableHttp
    }

    pub fn mcp_url(&self) -> Option<String> {
        let Self::StreamableHttp {
            scheme,
            host,
            port,
            mcp_path,
            ..
        } = self;
        Some(format!(
            "{scheme}://{}:{port}{mcp_path}",
            host_header_host(host)
        ))
    }

    pub fn token_path(&self) -> Option<&Path> {
        let Self::StreamableHttp { token_path, .. } = self;
        token_path.as_deref()
    }

    pub fn publish_discovery(&self, path: &Path) -> io::Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let body = serde_json::to_string(self).map_err(invalid_discovery)?;
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, body)?;
        std::fs::rename(temp_path, path)?;
        Ok(())
    }

    pub fn read_discovery(path: &Path) -> io::Result<Self> {
        let body = std::fs::read_to_string(path)?;
        let endpoint: Self = serde_json::from_str(&body).map_err(invalid_discovery)?;
        endpoint.validate()?;
        Ok(endpoint)
    }

    pub fn probe_readiness(&self) -> TransportProbe {
        self.probe_http_readiness()
    }

    fn probe_http_readiness(&self) -> TransportProbe {
        match self.probe_http_readiness_inner() {
            Ok(true) => TransportProbe::Ready,
            Ok(false) | Err(_) => TransportProbe::NotReady,
        }
    }

    fn probe_http_readiness_inner(&self) -> io::Result<bool> {
        let Self::StreamableHttp {
            host,
            port,
            readiness_path,
            token_path,
            ..
        } = self;

        let addr = (host.as_str(), *port)
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty HTTP address"))?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(250))?;
        stream.set_read_timeout(Some(Duration::from_millis(250)))?;
        stream.set_write_timeout(Some(Duration::from_millis(250)))?;

        let mut request = format!(
            "GET {readiness_path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n",
            host_header(host, *port)
        );
        if let Some(path) = token_path {
            let token = std::fs::read_to_string(path)?;
            let token = token.trim();
            if token.is_empty() || token.contains('\r') || token.contains('\n') {
                return Ok(false);
            }
            request.push_str("Authorization: Bearer ");
            request.push_str(token);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");

        stream.write_all(request.as_bytes())?;
        let mut response = [0u8; 128];
        let n = stream.read(&mut response)?;
        if n == 0 {
            return Ok(false);
        }

        let response = String::from_utf8_lossy(&response[..n]);
        let status_code = response
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok());
        Ok(matches!(status_code, Some(200..=399)))
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
                        "Daemon transport endpoint did not become ready within {}ms",
                        timeout.as_millis()
                    ),
                ));
            }

            std::thread::sleep(delay);
            delay = (delay * 2).min(max_delay);
        }
    }

    fn validate(&self) -> io::Result<()> {
        let Self::StreamableHttp {
            scheme,
            host,
            port,
            mcp_path,
            readiness_path,
            ..
        } = self;

        if scheme != "http" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "daemon HTTP transport must use http scheme",
            ));
        }
        if !is_local_http_host(host) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("daemon HTTP transport host must be localhost, got {host}"),
            ));
        }
        if *port == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "daemon HTTP transport port cannot be 0 in discovery",
            ));
        }
        validate_http_path(mcp_path)?;
        validate_http_path(readiness_path)?;
        Ok(())
    }
}

fn invalid_discovery(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

fn validate_http_path(path: &str) -> io::Result<()> {
    if path.starts_with('/') && !path.contains('\r') && !path.contains('\n') {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("HTTP transport paths must be absolute and single-line, got {path:?}"),
        ))
    }
}

fn is_local_http_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

fn host_header(host: &str, port: u16) -> String {
    format!("{}:{port}", host_header_host(host))
}

fn host_header_host(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}
