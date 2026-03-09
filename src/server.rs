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

use crate::agent;
use crate::agent::backend::BackendInfo;
use crate::agent::dispatch::DispatchManager;
use crate::api;
use crate::daemon_indexer::{self, IndexingSender};
use crate::daemon_state::DaemonState;
use crate::mcp_http;
use crate::registry::GlobalRegistry;
use crate::ui;

/// Shared application state available to all request handlers.
pub struct AppState {
    /// When the server started -- used to compute uptime in the health endpoint.
    pub start_time: Instant,
    /// Global project registry -- tracks all known projects on this machine.
    ///
    /// Wrapped in `Arc` so the background indexing worker can share the same
    /// registry instance for status updates.
    pub registry: Arc<RwLock<GlobalRegistry>>,
    /// Path to `~/.julie` (or platform equivalent) for persisting registry.
    pub julie_home: PathBuf,
    /// Daemon-wide state: loaded workspaces and per-workspace MCP services.
    pub daemon_state: Arc<RwLock<DaemonState>>,
    /// Cancellation token for shutting down all MCP sessions.
    pub cancellation_token: CancellationToken,
    /// Sender for the background indexing pipeline.
    ///
    /// API handlers, file watchers, and startup code submit `IndexRequest`
    /// messages through this channel. The background worker processes them
    /// sequentially (one project at a time).
    pub indexing_sender: IndexingSender,
    /// Agent dispatch manager — tracks active and completed dispatches.
    pub dispatch_manager: Arc<RwLock<DispatchManager>>,
    /// Detected agent backends (cached at startup for fast lookup).
    pub backends: Vec<BackendInfo>,
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

    // Create the shared registry Arc BEFORE DaemonState so both DaemonState
    // and AppState share the same Arc<RwLock<GlobalRegistry>>.
    let registry_rw = Arc::new(RwLock::new(registry));

    // Wrap DaemonState in Arc<RwLock<>> up front so that per-workspace MCP
    // service factories can capture a reference to it. This lets tool handlers
    // access all loaded workspaces for federated search (workspace="all").
    let daemon_state = Arc::new(RwLock::new(DaemonState::new(
        registry_rw.clone(),
        julie_home.clone(),
        cancellation_token.clone(),
    )));

    // Load workspaces for all registered projects (non-blocking -- only loads
    // existing indexes, doesn't trigger indexing).
    let ready_count = {
        let registry = registry_rw.read().await;
        let mut ds = daemon_state.write().await;
        ds.load_registered_projects(
            &registry,
            daemon_state.clone(),
        )
        .await;

        let loaded_count = ds.workspaces.len();
        let ready_count = ds
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

        // Start file watchers for all Ready projects
        ds.start_watchers_for_ready_projects().await;

        ready_count
    };

    // Spawn the background indexing worker — processes requests sequentially
    let indexing_sender = daemon_indexer::spawn_indexing_worker(
        registry_rw.clone(),
        daemon_state.clone(),
        julie_home.clone(),
        cancellation_token.clone(),
    );

    // Set the indexing sender on DaemonState now that the worker is spawned
    {
        let mut ds = daemon_state.write().await;
        ds.set_indexing_sender(indexing_sender.clone());
    }

    let backends = agent::backend::detect_backends();
    let dispatch_manager = Arc::new(RwLock::new(DispatchManager::with_backends(backends.clone())));

    let state = Arc::new(AppState {
        start_time: Instant::now(),
        registry: registry_rw,
        julie_home,
        daemon_state: daemon_state.clone(),
        cancellation_token: cancellation_token.clone(),
        indexing_sender,
        dispatch_manager,
        backends,
    });

    // Create the default MCP service with daemon state so federated features
    // (workspace="all") work even on the /mcp endpoint.
    let default_mcp_service =
        mcp_http::create_mcp_service(workspace_root, cancellation_token.clone(), Some(daemon_state.clone()));

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
        // Root redirect → dashboard
        .route("/", axum::routing::get(|| async { axum::response::Redirect::temporary("/ui/") }))
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.with_context(|| {
        format!(
            "Port {} is already in use. Use --port or JULIE_PORT to specify a different port.",
            port
        )
    })?;

    // Startup banner
    let project_count = {
        let reg = state.registry.read().await;
        reg.projects.len()
    };
    tracing::info!("============================================================");
    tracing::info!(
        "Julie v{} - Code Intelligence Server (daemon mode)",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("============================================================");
    tracing::info!("Port:           {}", port);
    tracing::info!("API:            http://localhost:{}/api", port);
    tracing::info!("API Docs:       http://localhost:{}/api/docs", port);
    tracing::info!("MCP:            http://localhost:{}/mcp", port);
    tracing::info!("Per-project MCP: http://localhost:{}/mcp/{{workspace_id}}", port);
    tracing::info!("Web UI:         http://localhost:{}/ui/", port);
    tracing::info!("Projects:       {} registered ({} ready)", project_count, ready_count);
    tracing::info!("============================================================");

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .context("HTTP server error");

    // Shutdown sequence
    tracing::info!("Shutting down Julie daemon...");

    // Stop all file watchers on shutdown
    {
        let ds = daemon_state.read().await;
        ds.watcher_manager.stop_all().await;
    }
    tracing::info!("File watchers stopped");

    // Cancel all active MCP sessions on shutdown
    ct_for_shutdown.cancel();
    tracing::info!("MCP sessions cancelled");

    tracing::info!("Julie daemon shutdown complete");
    result
}
