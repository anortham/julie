//! HTTP server setup for the Julie daemon.
//!
//! Binds an axum server to the configured port with CORS support and graceful shutdown.
//! Mounts per-workspace MCP endpoints at `/mcp/{workspace_id}` and a default fallback
//! at `/mcp`, alongside API routes at `/api`.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::api;
use crate::daemon_state::DaemonState;
use crate::mcp_http;
use crate::registry::GlobalRegistry;
use crate::ui;

/// Shared application state available to all request handlers.
pub struct AppState {
    /// When the server started -- used to compute uptime in the health endpoint.
    pub start_time: Instant,
    /// Global project registry -- tracks all known projects on this machine.
    pub registry: RwLock<GlobalRegistry>,
    /// Path to `~/.julie` (or platform equivalent) for persisting registry.
    pub julie_home: PathBuf,
    /// Daemon-wide state: loaded workspaces and per-workspace MCP services.
    pub daemon_state: Arc<RwLock<DaemonState>>,
    /// Cancellation token for shutting down all MCP sessions.
    pub cancellation_token: CancellationToken,
}

/// Start the HTTP server on the given port.
///
/// Binds to `0.0.0.0:{port}`, serves the API routes at `/api` and the MCP
/// Streamable HTTP transport at `/mcp` and `/mcp/{workspace_id}`, with
/// permissive CORS and graceful shutdown.
///
/// On startup, loads existing workspace indexes for all registered projects
/// without blocking on indexing. Projects without a `.julie/` directory are
/// marked as `Registered`.
pub async fn start_server(
    port: u16,
    workspace_root: PathBuf,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
    registry: GlobalRegistry,
    julie_home: PathBuf,
) -> Result<()> {
    // Create a cancellation token that will be cancelled when the server shuts down.
    // This ensures all active MCP sessions are cleaned up on shutdown.
    let cancellation_token = CancellationToken::new();
    let ct_for_shutdown = cancellation_token.clone();

    // Load workspaces for all registered projects (non-blocking -- only loads
    // existing indexes, doesn't trigger indexing).
    let mut daemon_state = DaemonState::new();
    daemon_state
        .load_registered_projects(&registry, &cancellation_token)
        .await;

    let loaded_count = daemon_state.workspaces.len();
    let ready_count = daemon_state
        .workspaces
        .values()
        .filter(|w| w.status == crate::daemon_state::WorkspaceLoadStatus::Ready)
        .count();
    tracing::info!(
        "Loaded {}/{} project workspace(s) (Ready: {})",
        loaded_count,
        registry.projects.len(),
        ready_count,
    );

    let daemon_state = Arc::new(RwLock::new(daemon_state));

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: RwLock::new(registry),
        julie_home,
        daemon_state: daemon_state.clone(),
        cancellation_token: cancellation_token.clone(),
    });

    // Create the default MCP service for backward compatibility.
    // This serves the workspace_root passed on the command line (or cwd).
    let default_mcp_service =
        mcp_http::create_mcp_service(workspace_root, cancellation_token.clone());

    // Build the router. The per-workspace MCP handler needs AppState,
    // so we add it with `.with_state()` before merging the default MCP
    // service (which is stateless / self-contained).
    let workspace_mcp_router = Router::new()
        .route(
            "/mcp/{workspace_id}",
            axum::routing::any(mcp_http::workspace_mcp_handler),
        )
        .with_state(state.clone());

    let app = Router::new()
        .nest("/api", api::routes(state.clone()))
        .merge(workspace_mcp_router)
        // Default MCP endpoint at /mcp (backward compatible)
        .route_service("/mcp", default_mcp_service)
        // Embedded Vue UI at /ui/
        .route("/ui/", axum::routing::get(ui::ui_handler))
        .route("/ui/{*path}", axum::routing::get(ui::ui_handler))
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.with_context(|| {
        format!(
            "Port {} is already in use. Use --port or JULIE_PORT to specify a different port.",
            port
        )
    })?;
    tracing::info!("Julie daemon listening on http://{}", addr);
    tracing::info!(
        "MCP Streamable HTTP endpoint: http://{}:{}/mcp",
        addr.ip(),
        addr.port()
    );
    tracing::info!(
        "Per-workspace MCP: http://{}:{}/mcp/{{workspace_id}}",
        addr.ip(),
        addr.port()
    );
    tracing::info!(
        "Web UI: http://{}:{}/ui/",
        addr.ip(),
        addr.port()
    );

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .context("HTTP server error");

    // Cancel all active MCP sessions on shutdown
    ct_for_shutdown.cancel();

    result
}
