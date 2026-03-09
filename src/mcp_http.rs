//! MCP Streamable HTTP transport integration.
//!
//! Provides a `StreamableHttpService` that can be mounted in axum to serve
//! MCP clients over HTTP instead of stdio. Each client connection gets its
//! own session with its own `JulieServerHandler` instance.
//!
//! Supports two routing modes:
//! - `/mcp` -- default endpoint (backward compatible, uses the workspace_root
//!   passed on the command line)
//! - `/mcp/{workspace_id}` -- per-workspace endpoint, where the workspace_id
//!   identifies a registered project in the daemon's workspace pool

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
    session::local::LocalSessionManager,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::daemon_state::DaemonState;
use crate::handler::JulieServerHandler;
use crate::server::AppState;

/// Create an MCP Streamable HTTP service that can be mounted in axum.
///
/// Each new MCP session triggers the factory closure, which creates a fresh
/// `JulieServerHandler` with the given workspace root. The handler implements
/// `ServerHandler` (via `#[tool_router]`), which automatically provides
/// `Service<RoleServer>` -- exactly what `StreamableHttpService` requires.
///
/// # Arguments
/// * `workspace_root` - Path to the workspace root for handler initialization
/// * `cancellation_token` - Token to signal shutdown of all active MCP sessions
/// * `daemon_state` - Optional shared daemon state for federated features (workspace="all")
pub fn create_mcp_service(
    workspace_root: PathBuf,
    cancellation_token: CancellationToken,
    daemon_state: Option<Arc<RwLock<DaemonState>>>,
) -> StreamableHttpService<JulieServerHandler> {
    let config = StreamableHttpServerConfig {
        cancellation_token,
        ..Default::default()
    };
    let session_manager = Arc::new(LocalSessionManager::default());

    StreamableHttpService::new(
        move || {
            // The service_factory closure must be sync (Fn() -> Result<S, io::Error>).
            // JulieServerHandler construction is synchronous in practice -- it only
            // creates Arcs and empty state. We use the sync constructor here.
            let handler = match daemon_state.clone() {
                Some(ds) => JulieServerHandler::new_with_daemon_state(workspace_root.clone(), ds),
                None => JulieServerHandler::new_sync(workspace_root.clone()),
            };
            handler.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        },
        session_manager,
        config,
    )
}

/// Axum handler for per-workspace MCP requests at `/mcp/{workspace_id}`.
///
/// Looks up the workspace's `StreamableHttpService` in the daemon state and
/// delegates the entire HTTP request to it. This preserves the full MCP
/// Streamable HTTP protocol (POST for messages, GET for SSE streams,
/// DELETE for session teardown).
///
/// Returns 404 if the workspace_id is not found in the daemon's workspace pool.
pub async fn workspace_mcp_handler(
    Path(workspace_id): Path<String>,
    State(state): State<Arc<AppState>>,
    request: axum::http::Request<Body>,
) -> Response {
    // Look up the MCP service for this workspace.
    let daemon_state = state.daemon_state.read().await;
    let service = daemon_state.mcp_services.get(&workspace_id);

    let Some(service) = service.cloned() else {
        return (
            StatusCode::NOT_FOUND,
            format!("Workspace '{}' not found. Use GET /api/projects to list available workspaces.", workspace_id),
        )
            .into_response();
    };
    // Drop the read lock before delegating to the service (which may be long-lived for SSE).
    drop(daemon_state);

    // Delegate to the StreamableHttpService via its public handle() method.
    // This bypasses the tower Service trait (which is a dev-dependency) and
    // calls the handler directly.
    let response = service.handle(request).await;
    response.map(axum::body::Body::new).into_response()
}
