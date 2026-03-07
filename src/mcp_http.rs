//! MCP Streamable HTTP transport integration.
//!
//! Provides a `StreamableHttpService` that can be mounted in axum to serve
//! MCP clients over HTTP instead of stdio. Each client connection gets its
//! own session with its own `JulieServerHandler` instance.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
    session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

use crate::handler::JulieServerHandler;

/// Create an MCP Streamable HTTP service that can be mounted in axum.
///
/// Each new MCP session triggers the factory closure, which creates a fresh
/// `JulieServerHandler` with the given workspace root. The handler implements
/// `ServerHandler` (via `#[tool_router]`), which automatically provides
/// `Service<RoleServer>` — exactly what `StreamableHttpService` requires.
///
/// # Arguments
/// * `workspace_root` - Path to the workspace root for handler initialization
/// * `cancellation_token` - Token to signal shutdown of all active MCP sessions
pub fn create_mcp_service(
    workspace_root: PathBuf,
    cancellation_token: CancellationToken,
) -> StreamableHttpService<JulieServerHandler> {
    let config = StreamableHttpServerConfig {
        cancellation_token,
        ..Default::default()
    };
    let session_manager = Arc::new(LocalSessionManager::default());

    StreamableHttpService::new(
        move || {
            // The service_factory closure must be sync (Fn() -> Result<S, io::Error>).
            // JulieServerHandler construction is synchronous in practice — it only
            // creates Arcs and empty state. We use the sync constructor here.
            JulieServerHandler::new_sync(workspace_root.clone())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
        },
        session_manager,
        config,
    )
}
