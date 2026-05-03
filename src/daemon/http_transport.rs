//! Daemon-owned Streamable HTTP MCP transport.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::Router;
use axum::http::StatusCode;
use axum::routing::get;
use rmcp::Service;
use rmcp::service::RoleServer;
use rmcp::transport::streamable_http_server::session::local::{LocalSessionManager, SessionConfig};
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::daemon::transport::TransportEndpoint;
use crate::paths::DaemonPaths;

pub const MCP_PATH: &str = "/mcp";
pub const READINESS_PATH: &str = "/mcp/ready";

#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    pub bind_host: IpAddr,
    pub mcp_path: &'static str,
    pub readiness_path: &'static str,
    pub init_timeout: Option<Duration>,
    pub keep_alive: Option<Duration>,
    pub sse_retry: Option<Duration>,
    pub completed_cache_ttl: Duration,
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self {
            bind_host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            mcp_path: MCP_PATH,
            readiness_path: READINESS_PATH,
            init_timeout: Some(Duration::from_secs(60)),
            keep_alive: Some(Duration::from_secs(300)),
            sse_retry: Some(Duration::from_secs(3)),
            completed_cache_ttl: Duration::from_secs(60),
        }
    }
}

impl HttpTransportConfig {
    pub fn session_config(&self) -> SessionConfig {
        let mut session_config = SessionConfig::default();
        session_config.keep_alive = self.keep_alive;
        session_config.sse_retry = self.sse_retry;
        session_config.completed_cache_ttl = self.completed_cache_ttl;
        session_config.init_timeout = self.init_timeout;
        session_config
    }
}

pub struct HttpTransportServer {
    local_addr: SocketAddr,
    discovery_path: std::path::PathBuf,
    cancellation: CancellationToken,
    server_task: JoinHandle<()>,
}

impl HttpTransportServer {
    pub async fn bind<S>(
        paths: DaemonPaths,
        config: HttpTransportConfig,
        service_factory: impl Fn() -> io::Result<S> + Send + Sync + 'static,
    ) -> Result<Self>
    where
        S: Service<RoleServer> + Send + 'static,
    {
        if config.bind_host != IpAddr::V4(Ipv4Addr::LOCALHOST) {
            anyhow::bail!(
                "HTTP MCP transport must bind to 127.0.0.1 until IPv6 loopback is tested, got {}",
                config.bind_host
            );
        }

        paths
            .ensure_dirs()
            .context("Failed to create daemon dirs")?;
        let listener = TcpListener::bind(SocketAddr::new(config.bind_host, 0))
            .await
            .context("Failed to bind HTTP MCP transport listener")?;
        let local_addr = listener.local_addr()?;
        let cancellation = CancellationToken::new();

        let mut sdk_config = StreamableHttpServerConfig::default();
        sdk_config.sse_retry = config.sse_retry;
        sdk_config.cancellation_token = cancellation.clone();
        sdk_config.allowed_hosts = allowed_hosts_for(local_addr);
        sdk_config.allowed_origins = vec![];
        sdk_config.session_store = None;

        let mut local_session_manager = LocalSessionManager::default();
        local_session_manager.session_config = config.session_config();
        let session_manager = Arc::new(local_session_manager);
        let mcp_service = StreamableHttpService::new(service_factory, session_manager, sdk_config);

        let router = Router::new()
            .route(config.readiness_path, get(readiness))
            .route_service(config.mcp_path, mcp_service);

        let shutdown = cancellation.clone();
        let server_task = tokio::spawn(async move {
            let result = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    shutdown.cancelled().await;
                })
                .await;
            if let Err(error) = result {
                warn!("HTTP MCP transport server error: {error}");
            }
        });

        let endpoint = TransportEndpoint::streamable_http(
            local_addr.ip().to_string(),
            local_addr.port(),
            config.mcp_path,
            config.readiness_path,
            None,
        )?;
        if let Err(error) = wait_for_readiness(&endpoint, Duration::from_secs(2)).await {
            cancellation.cancel();
            let _ = server_task.await;
            return Err(error).context("HTTP MCP transport did not become ready");
        }

        let discovery_path = paths.daemon_mcp_transport();
        endpoint
            .publish_discovery(&discovery_path)
            .context("Failed to publish HTTP MCP transport discovery")?;

        info!(
            endpoint = %endpoint.mcp_url().unwrap_or_default(),
            "HTTP MCP transport listening"
        );

        Ok(Self {
            local_addr,
            discovery_path,
            cancellation,
            server_task,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub async fn shutdown(self) -> Result<()> {
        self.cancellation.cancel();
        self.server_task
            .await
            .context("HTTP MCP transport task join failed")?;
        let _ = std::fs::remove_file(&self.discovery_path);
        Ok(())
    }
}

async fn readiness() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn wait_for_readiness(endpoint: &TransportEndpoint, timeout: Duration) -> io::Result<()> {
    let start = Instant::now();
    let mut delay = Duration::from_millis(10);
    let max_delay = Duration::from_millis(100);

    loop {
        let endpoint_for_probe = endpoint.clone();
        let ready = tokio::task::spawn_blocking(move || endpoint_for_probe.probe_readiness())
            .await
            .map_err(|error| io::Error::other(format!("readiness probe task failed: {error}")))?
            .is_ready();
        if ready {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "HTTP MCP transport readiness route did not respond",
            ));
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(max_delay);
    }
}

fn allowed_hosts_for(local_addr: SocketAddr) -> Vec<String> {
    let host = local_addr.ip().to_string();
    vec![
        "localhost".to_string(),
        format!("localhost:{}", local_addr.port()),
        host.clone(),
        format!("{}:{}", host, local_addr.port()),
    ]
}
