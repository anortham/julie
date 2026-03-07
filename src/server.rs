//! HTTP server setup for the Julie daemon.
//!
//! Binds an axum server to the configured port with CORS support and graceful shutdown.
//! Mounts the MCP Streamable HTTP transport at `/mcp` alongside API routes at `/api`.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use axum::Router;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::api;
use crate::mcp_http;

/// Shared application state available to all request handlers.
pub struct AppState {
    /// When the server started — used to compute uptime in the health endpoint.
    pub start_time: Instant,
}

/// Start the HTTP server on the given port.
///
/// Binds to `0.0.0.0:{port}`, serves the API routes at `/api` and the MCP
/// Streamable HTTP transport at `/mcp`, with permissive CORS and graceful shutdown.
///
/// Each MCP client connection gets its own session with a fresh `JulieServerHandler`.
pub async fn start_server(
    port: u16,
    workspace_root: PathBuf,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let state = Arc::new(AppState {
        start_time: Instant::now(),
    });

    // Create a cancellation token that will be cancelled when the server shuts down.
    // This ensures all active MCP sessions are cleaned up on shutdown.
    let cancellation_token = CancellationToken::new();
    let ct_for_shutdown = cancellation_token.clone();

    // Create the MCP Streamable HTTP service.
    // This is a tower::Service that handles MCP protocol over HTTP/SSE.
    let mcp_service = mcp_http::create_mcp_service(workspace_root, cancellation_token);

    let app = Router::new()
        .nest("/api", api::routes(state.clone()))
        .route_service("/mcp", mcp_service)
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.with_context(|| {
        format!(
            "Port {} is already in use. Use --port or JULIE_PORT to specify a different port.",
            port
        )
    })?;
    tracing::info!("Julie daemon listening on http://{}", addr);
    tracing::info!("MCP Streamable HTTP endpoint: http://{}:{}/mcp", addr.ip(), addr.port());

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .context("HTTP server error");

    // Cancel all active MCP sessions on shutdown
    ct_for_shutdown.cancel();

    result
}
