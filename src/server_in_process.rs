//! In-process server helpers (Phase 3c.2, T6).
//!
//! This module provides the embedding-provider acquisition helper used by the
//! in-process serve loop (T8).  The shared 3b resident host — one
//! `CodeRankEmbed` model in VRAM — is the sole provider source; per-session
//! spawning via `create_embedding_provider()` is deliberately NOT used here
//! (it would OOM under N concurrent sessions each spawning their own sidecar).

use std::sync::Arc;

use tracing::{info, warn};

use crate::embeddings::EmbeddingProvider;
use crate::paths::DaemonPaths;

/// Acquire the shared resident embedding provider for an in-process session.
///
/// **Default-on:** uses the shared 3b resident host unless explicitly disabled.
///
/// ## Source
///
/// Calls [`connect_or_spawn_host`] — the shared host path — NOT
/// `create_embedding_provider()`, which would spawn a new sidecar per session
/// and cause OOM under N concurrent sessions.
///
/// ## Readiness gate (F2 lesson from Phase 3b)
///
/// The host connection is promoted to `Arc<dyn EmbeddingProvider>` ONLY if
/// [`RpcEmbeddingProvider::ensure_ready`] succeeds — the **fallible** health
/// handshake.  Silent-default getters (`device_info()`, `accelerated()`) are
/// deliberately NOT used as gates; they silently return defaults even when the
/// host is unhealthy (this is the exact failure mode from Phase 3b that the
/// `ensure_ready` gate was introduced to prevent).
///
/// ## Degradation
///
/// Returns `None` — degrade to keyword-only — when any of the following hold:
/// - `JULIE_EMBEDDING_PROVIDER=none/disabled/off` — explicit force-disable
/// - The host connect/spawn fails (e.g. binary missing, socket error)
/// - The host health handshake fails (host down, unresponsive, `ready=false`)
/// - The blocking task panics or is cancelled
///
/// None of these conditions block the caller; the session starts and serves
/// normally, just without semantic search.
///
/// ## Thread safety
///
/// All blocking I/O (`connect_or_spawn_host` + `ensure_ready`) runs in a
/// `tokio::task::spawn_blocking` worker thread, keeping the async runtime free.
///
/// [`connect_or_spawn_host`]: crate::embedding_host_launch::connect_or_spawn_host
/// [`RpcEmbeddingProvider::ensure_ready`]: julie_pipeline::embeddings::rpc_client::RpcEmbeddingProvider::ensure_ready
pub async fn acquire_in_process_embedding_provider(
    paths: &DaemonPaths,
) -> Option<Arc<dyn EmbeddingProvider>> {
    // Force-disable check: reuse the same JULIE_EMBEDDING_PROVIDER=none logic
    // as create_embedding_provider() (crates/julie-pipeline/src/embeddings/init.rs).
    // Do NOT invent a new env name — same knob, consistent behaviour.
    if let Ok(v) = std::env::var("JULIE_EMBEDDING_PROVIDER") {
        if matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "none" | "disabled" | "off"
        ) {
            info!(
                provider = %v,
                "In-process embedding disabled via JULIE_EMBEDDING_PROVIDER; \
                 degrading to keyword-only"
            );
            return None;
        }
    }

    let paths = paths.clone();

    // All blocking I/O off the async runtime thread.
    // host_spawn_timeout() inside connect_or_spawn_host bounds the wait (up to
    // 180s cold) — any error or timeout produces None; startup is never hung.
    let result = tokio::task::spawn_blocking(move || {
        match crate::embedding_host_launch::connect_or_spawn_host(&paths) {
            Ok(rpc) => {
                // HARD GATE: ensure_ready() runs the full health handshake and
                // surfaces errors + ready=false BEFORE we promote to
                // Arc<dyn EmbeddingProvider>.
                //
                // NEVER gate on device_info() / accelerated() here — they
                // silently return defaults even when the host is unhealthy,
                // masking an unready host as "Ready" (the F2-class failure from
                // Phase 3b that this gate was specifically introduced to prevent).
                match rpc.ensure_ready() {
                    Ok(()) => {
                        // OnceLock already populated by ensure_ready(); no extra I/O.
                        let provider: Arc<dyn EmbeddingProvider> = Arc::new(rpc);
                        Some(provider)
                    }
                    Err(e) => {
                        warn!(
                            error = %e,
                            "In-process embedding: host health handshake failed; \
                             degrading to keyword-only"
                        );
                        None
                    }
                }
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "In-process embedding: host connect/spawn failed; \
                     degrading to keyword-only"
                );
                None
            }
        }
    })
    .await;

    match result {
        Ok(provider) => provider,
        Err(join_err) => {
            warn!(
                error = %join_err,
                "In-process embedding: init task panicked or was cancelled; \
                 degrading to keyword-only"
            );
            None
        }
    }
}
