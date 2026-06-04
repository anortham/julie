//! Resident embedding-host server (Phase 3b, Task 4).
//!
//! Listens on the IPC front door established by [`super::host_transport`],
//! dispatches `health` / `embed_query` / `embed_batch` / `shutdown` requests
//! to an [`EmbeddingProvider`], and shuts down cleanly on cancellation.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use fs2::FileExt as _;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use julie_core::embeddings_contract::EmbeddingProvider;

use super::host_transport::{HostAddress, HostListener, HostServerConn};
use super::sidecar_protocol::{
    EmbedBatchRequest, EmbedBatchResult, EmbedQueryRequest, EmbedQueryResult, HealthResult,
    ProtocolError, RequestEnvelope, ResponseEnvelope, SIDECAR_PROTOCOL_SCHEMA,
    SIDECAR_PROTOCOL_VERSION,
};

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Acquire the singleton lock, bind the IPC front door, and serve embedding
/// requests until `cancel` fires.
///
/// Returns `Ok(())` after a clean cancel-driven shutdown.
/// Returns `Err` if the singleton lock is already held (another host is
/// running) or if the listener cannot be bound.
pub async fn run_embedding_host(
    addr: &HostAddress,
    lock_path: &Path,
    cancel: CancellationToken,
    provider: Arc<dyn EmbeddingProvider>,
) -> Result<()> {
    // 1. Singleton lock — refuse to start a second instance.
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .context("failed to create embedding-host lock directory")?;
    }
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(lock_path)
        .context("failed to open/create embedding-host lock file")?;
    lock_file
        .try_lock_exclusive()
        .map_err(|_| {
            anyhow::anyhow!(
                "embedding-host singleton lock already held — \
                 another host instance is running ({})",
                lock_path.display()
            )
        })?;

    // 2. Bind the front door.
    let listener = HostListener::bind(addr)
        .await
        .context("failed to bind embedding-host listener")?;

    info!("embedding-host listening");

    // 3. Accept loop.
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("embedding-host: cancel received, stopping accept loop");
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok(conn) => {
                        let p = Arc::clone(&provider);
                        tokio::spawn(serve_connection(conn, p));
                    }
                    Err(e) => {
                        warn!("embedding-host: accept error: {e}");
                    }
                }
            }
        }
    }

    // 4. Graceful shutdown: ask the provider to stop its child process.
    provider.shutdown();
    let p = Arc::clone(&provider);
    let _ =
        tokio::task::spawn_blocking(move || p.wait_for_exit(Duration::from_secs(3))).await;

    // `lock_file` drops here → fs2 lock released.
    // `listener` drops here → socket file removed (unix Drop impl).
    drop(lock_file);
    drop(listener);

    info!("embedding-host shut down cleanly");
    Ok(())
}

/// Resolve the provider via [`super::init::create_embedding_provider`] and
/// run the host.  Returns an error if no provider is available.
pub async fn run_embedding_host_default(
    addr: &HostAddress,
    lock_path: &Path,
    cancel: CancellationToken,
) -> Result<()> {
    let (provider, _status) = super::init::create_embedding_provider();
    let provider = provider.ok_or_else(|| {
        anyhow::anyhow!(
            "no embedding provider available — cannot start the resident embedding-host"
        )
    })?;
    run_embedding_host(addr, lock_path, cancel, provider).await
}

// ---------------------------------------------------------------------------
// Per-connection handler
// ---------------------------------------------------------------------------

async fn serve_connection(mut conn: HostServerConn, provider: Arc<dyn EmbeddingProvider>) {
    loop {
        let line = match conn.read_line().await {
            Ok(Some(l)) => l,
            Ok(None) => break, // clean EOF — client disconnected
            Err(e) => {
                warn!("embedding-host: read error: {e}");
                break;
            }
        };

        // Parse the outer envelope; method and params are always present.
        let envelope: RequestEnvelope<serde_json::Value> = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                // No request_id to echo; use empty string so the client gets
                // valid JSON it can at least log.
                let resp = error_line("", "parse_error", &format!("bad envelope: {e}"));
                let _ = conn.write_line(&resp).await;
                continue;
            }
        };

        let request_id = envelope.request_id.clone();

        let response_line: String = match envelope.method.as_str() {
            "health" => {
                let p = Arc::clone(&provider);
                match tokio::task::spawn_blocking(move || {
                    let info = p.device_info();
                    let dims = p.dimensions();
                    HealthResult {
                        ready: true,
                        dims: Some(dims),
                        device: Some(info.device),
                        runtime: Some(info.runtime),
                        model_id: Some(info.model_name),
                        resolved_backend: None,
                        accelerated: p.accelerated(),
                        degraded_reason: p.degraded_reason(),
                        capabilities: None,
                        load_policy: None,
                    }
                })
                .await
                {
                    Ok(result) => ok_line(&request_id, result),
                    Err(e) => error_line(
                        &request_id,
                        "internal_error",
                        &format!("health dispatch failed: {e}"),
                    ),
                }
            }

            "embed_query" => match serde_json::from_value::<EmbedQueryRequest>(envelope.params) {
                Ok(req) => {
                    let p = Arc::clone(&provider);
                    match tokio::task::spawn_blocking(move || -> anyhow::Result<EmbedQueryResult> {
                        let vector = p.embed_query(&req.text)?;
                        let dims = p.dimensions();
                        Ok(EmbedQueryResult { dims, vector })
                    })
                    .await
                    {
                        Ok(Ok(result)) => ok_line(&request_id, result),
                        Ok(Err(e)) => {
                            error_line(&request_id, "embed_error", &e.to_string())
                        }
                        Err(e) => error_line(
                            &request_id,
                            "internal_error",
                            &format!("embed_query dispatch failed: {e}"),
                        ),
                    }
                }
                Err(e) => error_line(
                    &request_id,
                    "invalid_params",
                    &format!("embed_query params: {e}"),
                ),
            },

            "embed_batch" => match serde_json::from_value::<EmbedBatchRequest>(envelope.params) {
                Ok(req) => {
                    let p = Arc::clone(&provider);
                    match tokio::task::spawn_blocking(move || -> anyhow::Result<EmbedBatchResult> {
                        let vectors = p.embed_batch(&req.texts)?;
                        let dims = p.dimensions();
                        Ok(EmbedBatchResult { dims, vectors })
                    })
                    .await
                    {
                        Ok(Ok(result)) => ok_line(&request_id, result),
                        Ok(Err(e)) => {
                            error_line(&request_id, "embed_error", &e.to_string())
                        }
                        Err(e) => error_line(
                            &request_id,
                            "internal_error",
                            &format!("embed_batch dispatch failed: {e}"),
                        ),
                    }
                }
                Err(e) => error_line(
                    &request_id,
                    "invalid_params",
                    &format!("embed_batch params: {e}"),
                ),
            },

            "shutdown" => {
                // Acknowledge and close this connection (not the whole server).
                let line = ok_line::<serde_json::Value>(&request_id, serde_json::Value::Null);
                let _ = conn.write_line(&line).await;
                break;
            }

            unknown => error_line(
                &request_id,
                "unknown_method",
                &format!("unknown method: '{unknown}'"),
            ),
        };

        if let Err(e) = conn.write_line(&response_line).await {
            warn!("embedding-host: write error: {e}");
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Response serialization helpers
// ---------------------------------------------------------------------------

/// Serialize a successful `ResponseEnvelope<T>` to a single JSON line.
///
/// If serialization of `result` fails (should never happen for our types),
/// falls back to an error envelope so the client always receives valid JSON.
fn ok_line<T: serde::Serialize>(request_id: &str, result: T) -> String {
    let env = ResponseEnvelope {
        schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
        version: SIDECAR_PROTOCOL_VERSION,
        request_id: request_id.to_string(),
        result: Some(result),
        error: None::<ProtocolError>,
    };
    serde_json::to_string(&env)
        .unwrap_or_else(|e| error_line(request_id, "serialize_error", &e.to_string()))
}

/// Serialize an error `ResponseEnvelope` to a single JSON line.
///
/// Uses `unwrap_or_default` — if this fails the caller receives `""` which
/// is better than a panic inside a connection task.
fn error_line(request_id: &str, code: &str, message: &str) -> String {
    let env = ResponseEnvelope::<serde_json::Value> {
        schema: SIDECAR_PROTOCOL_SCHEMA.to_string(),
        version: SIDECAR_PROTOCOL_VERSION,
        request_id: request_id.to_string(),
        result: None,
        error: Some(ProtocolError {
            code: code.to_string(),
            message: message.to_string(),
        }),
    };
    serde_json::to_string(&env).unwrap_or_default()
}
