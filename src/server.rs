//! HTTP server setup for the Julie daemon.
//!
//! Binds an axum server to the configured port with CORS support and graceful shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use axum::Router;
use tokio::net::TcpListener;

use crate::api;

/// Shared application state available to all request handlers.
pub struct AppState {
    /// When the server started — used to compute uptime in the health endpoint.
    pub start_time: Instant,
}

/// Start the HTTP server on the given port.
///
/// Binds to `0.0.0.0:{port}`, serves the API routes with permissive CORS,
/// and shuts down gracefully when the `shutdown_signal` future resolves.
pub async fn start_server(
    port: u16,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<()> {
    let state = Arc::new(AppState {
        start_time: Instant::now(),
    });

    let app = Router::new()
        .nest("/api", api::routes(state.clone()))
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await.with_context(|| {
        format!(
            "Port {} is already in use. Use --port or JULIE_PORT to specify a different port.",
            port
        )
    })?;
    tracing::info!("Julie daemon listening on http://{}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .context("HTTP server error")?;

    Ok(())
}
