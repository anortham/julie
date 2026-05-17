//! Daemon-owned Streamable HTTP MCP transport.

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rmcp::Service;
use rmcp::service::RoleServer;
use rmcp::transport::streamable_http_server::session::local::{LocalSessionManager, SessionConfig};
use rmcp::transport::{StreamableHttpServerConfig, StreamableHttpService};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::daemon::token_file;
use crate::daemon::transport::TransportEndpoint;
use crate::paths::DaemonPaths;

pub const MCP_PATH: &str = "/mcp";
pub const READINESS_PATH: &str = "/mcp/ready";

pub(crate) fn generate_bearer_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    pub bind_host: IpAddr,
    pub mcp_path: &'static str,
    pub readiness_path: &'static str,
    pub init_timeout: Option<Duration>,
    pub keep_alive: Option<Duration>,
    pub sse_retry: Option<Duration>,
    pub completed_cache_ttl: Duration,
    pub bearer_token: Option<String>,
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
            bearer_token: None,
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

/// Shutdown-state value: server is accepting requests normally.
pub const TRANSPORT_RUNNING: u8 = 0;
/// Shutdown-state value: server is draining — new requests get 503.
pub const TRANSPORT_DRAINING: u8 = 1;
/// Shutdown-state value: drain timed out — in-flight requests get 502.
pub const TRANSPORT_ABORTED: u8 = 2;

/// Wrapper around `Arc<AtomicU8>` for the HTTP transport's shutdown state.
///
/// Shared between [`HttpTransportServer`], the request-gate middleware, and
/// `DaemonHandle::shutdown`. The gate middleware returns:
///   - 200/whatever-the-handler-does when state == `TRANSPORT_RUNNING`
///   - 503 Service Unavailable when state == `TRANSPORT_DRAINING` (caller
///     should retry — daemon is draining sessions before shutdown)
///   - 502 Bad Gateway when state == `TRANSPORT_ABORTED` (caller's request
///     did not complete; state may be partially mutated)
#[derive(Debug, Clone, Default)]
pub struct TransportShutdownState {
    inner: Arc<AtomicU8>,
}

impl TransportShutdownState {
    /// Construct in the running state.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicU8::new(TRANSPORT_RUNNING)),
        }
    }

    /// Current state byte.
    pub fn current(&self) -> u8 {
        self.inner.load(Ordering::Acquire)
    }

    /// Flip to draining. Returns the previous value (so the caller can detect
    /// double-shutdown attempts).
    pub fn mark_draining(&self) -> u8 {
        self.inner.swap(TRANSPORT_DRAINING, Ordering::AcqRel)
    }

    /// Flip to aborted. Returns the previous value.
    pub fn mark_aborted(&self) -> u8 {
        self.inner.swap(TRANSPORT_ABORTED, Ordering::AcqRel)
    }
}

pub struct HttpTransportServer {
    local_addr: SocketAddr,
    discovery_path: std::path::PathBuf,
    token_path: Option<std::path::PathBuf>,
    cancellation: CancellationToken,
    server_task: JoinHandle<()>,
    shutdown_state: TransportShutdownState,
}

impl HttpTransportServer {
    /// Bind a new MCP HTTP transport on an auto-assigned loopback port.
    ///
    /// Convenience wrapper around `bind_with_listener` that performs its own
    /// `TcpListener::bind` against `config.bind_host`. Used by the existing
    /// test suite and any caller that does not want to pre-bind a listener.
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
        Self::bind_with_listener(listener, paths, config, service_factory).await
    }

    /// Bind a new MCP HTTP transport on an externally-provided listener.
    ///
    /// Lets callers (notably `DaemonApp::serve`) own the listening socket so
    /// they can pick a specific port up-front, share the address with health
    /// probes, or inject a pre-bound socket from a test harness. The listener
    /// must already be bound to loopback; remote-reachable addresses are
    /// rejected as a defense-in-depth check on top of the bind validation.
    pub async fn bind_with_listener<S>(
        listener: TcpListener,
        paths: DaemonPaths,
        config: HttpTransportConfig,
        service_factory: impl Fn() -> io::Result<S> + Send + Sync + 'static,
    ) -> Result<Self>
    where
        S: Service<RoleServer> + Send + 'static,
    {
        validate_route_path(config.mcp_path)?;
        validate_route_path(config.readiness_path)?;

        paths
            .ensure_dirs()
            .context("Failed to create daemon dirs")?;

        let local_addr = listener
            .local_addr()
            .context("Failed to query MCP transport listener local_addr")?;
        if !local_addr.ip().is_loopback() {
            anyhow::bail!(
                "HTTP MCP transport listener must be bound to loopback, got {}",
                local_addr.ip()
            );
        }

        let cancellation = CancellationToken::new();
        let token_path = if let Some(token) = config.bearer_token.as_deref() {
            validate_bearer_token(token)?;
            let token_path = paths.token_file();
            token_file::write_token(&token_path, token)
                .context("Failed to write HTTP MCP transport bearer token")?;
            Some(token_path)
        } else {
            None
        };

        let mut sdk_config = StreamableHttpServerConfig::default();
        sdk_config.sse_retry = config.sse_retry;
        sdk_config.cancellation_token = cancellation.clone();
        sdk_config.allowed_hosts = allowed_hosts_for(local_addr);
        sdk_config.allowed_origins = allowed_origins_for(local_addr);
        sdk_config.session_store = None;

        let mut local_session_manager = LocalSessionManager::default();
        local_session_manager.session_config = config.session_config();
        let session_manager = Arc::new(local_session_manager);
        let mcp_service = StreamableHttpService::new(service_factory, session_manager, sdk_config);

        let shutdown_state = TransportShutdownState::new();
        let readiness_path_owned: Arc<str> = Arc::from(config.readiness_path);

        let mut router = Router::new()
            .route(config.readiness_path, get(readiness))
            .route_service(config.mcp_path, mcp_service);
        if let Some(token) = config.bearer_token.clone() {
            router = router.layer(middleware::from_fn_with_state(
                Arc::<str>::from(token),
                require_bearer_token,
            ));
        }
        // Shutdown gate runs AFTER auth so we don't leak service-state info to
        // unauthenticated clients. Readiness route is exempted so adapters can
        // still probe a draining daemon and see its lifecycle phase via
        // discovery.json + dashboard rather than a confusing 503.
        router = router.layer(middleware::from_fn_with_state(
            (shutdown_state.clone(), readiness_path_owned),
            shutdown_gate,
        ));

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
            token_path.clone(),
        )?;
        if let Err(error) = wait_for_readiness(&endpoint, Duration::from_secs(2)).await {
            cancellation.cancel();
            let _ = server_task.await;
            if let Some(path) = &token_path {
                let _ = std::fs::remove_file(path);
            }
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
            token_path,
            cancellation,
            server_task,
            shutdown_state,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Handle to the shared shutdown state. Cloning is cheap (Arc).
    ///
    /// `DaemonHandle::shutdown` uses this to flip the transport into 503
    /// (draining) at the start of shutdown and 502 (aborted) when the
    /// drain timeout expires.
    pub fn shutdown_state(&self) -> TransportShutdownState {
        self.shutdown_state.clone()
    }

    /// Absolute path to the bearer token file the adapter must read in order
    /// to authenticate against this transport's HTTP endpoint.
    ///
    /// Returns `None` when the transport was bound without a bearer token
    /// (e.g. test harnesses that disable auth).  A1.8 publishes this into
    /// `discovery.json` so adapters have a single file to consult.
    pub fn token_path(&self) -> Option<&std::path::Path> {
        self.token_path.as_deref()
    }

    pub async fn shutdown(self) -> Result<()> {
        let force_after =
            (self.shutdown_state.current() == TRANSPORT_ABORTED).then_some(Duration::from_secs(5));
        self.shutdown_inner(force_after).await
    }

    pub async fn shutdown_forced(self, force_after: Duration) -> Result<()> {
        self.shutdown_inner(Some(force_after)).await
    }

    async fn shutdown_inner(self, force_after: Option<Duration>) -> Result<()> {
        let Self {
            discovery_path,
            token_path,
            cancellation,
            server_task,
            ..
        } = self;

        cancellation.cancel();
        await_server_task(server_task, force_after).await?;
        let _ = std::fs::remove_file(&discovery_path);
        if let Some(path) = token_path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
}

async fn await_server_task(
    mut server_task: JoinHandle<()>,
    force_after: Option<Duration>,
) -> Result<()> {
    if let Some(force_after) = force_after {
        match tokio::time::timeout(force_after, &mut server_task).await {
            Ok(result) => {
                result.context("HTTP MCP transport task join failed")?;
            }
            Err(_) => {
                warn!(
                    timeout_ms = force_after.as_millis(),
                    "HTTP MCP transport graceful shutdown timed out; aborting server task"
                );
                server_task.abort();
                let _ = server_task.await;
            }
        }
    } else {
        server_task
            .await
            .context("HTTP MCP transport task join failed")?;
    }
    Ok(())
}

async fn readiness() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// Short-circuit middleware that enforces the transport's shutdown state.
///
/// - `TRANSPORT_RUNNING`: passes the request through untouched.
/// - `TRANSPORT_DRAINING`: returns `503 Service Unavailable` with a
///   `Retry-After: 1` hint so the caller knows to retry — the daemon is
///   draining sessions before shutdown.
/// - `TRANSPORT_ABORTED`: returns `502 Bad Gateway` — the drain timer
///   expired with sessions still active and the daemon is being torn down.
///   The caller's request did not complete; state may be partially
///   mutated. A recovery marker has been written to disk.
///
/// The readiness route is exempted: adapters still need a 204 from
/// `/mcp/ready` while the daemon is draining so they can distinguish
/// "transport still up but in lifecycle stop" from "transport gone".
async fn shutdown_gate(
    State((state, readiness_path)): State<(TransportShutdownState, Arc<str>)>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if request.uri().path() == readiness_path.as_ref() {
        return next.run(request).await;
    }

    match state.current() {
        TRANSPORT_RUNNING => next.run(request).await,
        TRANSPORT_DRAINING => {
            let mut response = StatusCode::SERVICE_UNAVAILABLE.into_response();
            response
                .headers_mut()
                .insert("Retry-After", "1".parse().expect("static value"));
            response
        }
        TRANSPORT_ABORTED => StatusCode::BAD_GATEWAY.into_response(),
        // Defensive default: unknown state byte (impossible under current
        // constants) — fail closed with 502 rather than silently passing.
        _ => StatusCode::BAD_GATEWAY.into_response(),
    }
}

async fn require_bearer_token(
    State(expected): State<Arc<str>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected.as_ref());

    if authorized {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
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

fn allowed_origins_for(local_addr: SocketAddr) -> Vec<String> {
    vec![
        format!("http://localhost:{}", local_addr.port()),
        format!("http://127.0.0.1:{}", local_addr.port()),
    ]
}

fn validate_bearer_token(token: &str) -> io::Result<()> {
    if !token.trim().is_empty() && !token.contains('\r') && !token.contains('\n') {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "HTTP MCP bearer token must be non-empty and single-line",
        ))
    }
}

fn validate_route_path(path: &str) -> io::Result<()> {
    if path.starts_with('/') && !path.contains('\r') && !path.contains('\n') {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("HTTP MCP route path must be absolute and single-line, got {path:?}"),
        ))
    }
}
