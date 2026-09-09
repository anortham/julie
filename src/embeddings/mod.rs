//! Semantic embedding infrastructure — relocated to `julie_pipeline::embeddings`.

// Re-export all public items from julie_pipeline::embeddings
pub use julie_pipeline::embeddings::*;

// Re-export submodules so `crate::embeddings::factory::*` etc. remain valid
pub use julie_pipeline::embeddings::factory;
pub use julie_pipeline::embeddings::init;
pub use julie_pipeline::embeddings::metadata;
pub use julie_pipeline::embeddings::pipeline;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_bootstrap;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_embedded;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_protocol;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_provider;
#[cfg(feature = "embeddings-sidecar")]
pub use julie_pipeline::embeddings::sidecar_supervisor;

// log_fields re-exported for the top crate
pub use julie_pipeline::embeddings::log_fields;

use julie_core::paths::RegistryPaths;
use std::sync::Arc;
use tracing::{info, warn};

pub async fn acquire_in_process_embedding_provider(
    paths: &RegistryPaths,
) -> Option<Arc<dyn EmbeddingProvider>> {
    // Force-disable check: reuse the same JULIE_EMBEDDING_PROVIDER=none logic
    // as create_embedding_provider() (crates/julie-pipeline/src/embeddings/init.rs).
    // Do NOT invent a new env name — same knob, consistent behaviour.
    if let Ok(v) = std::env::var("JULIE_EMBEDDING_PROVIDER") {
        let trimmed = v.trim().to_ascii_lowercase();
        if matches!(trimmed.as_str(), "none" | "disabled" | "off") {
            info!(
                provider = %v,
                "In-process embedding disabled via JULIE_EMBEDDING_PROVIDER; \
                 degrading to keyword-only"
            );
            return None;
        }
        if trimmed == "native" {
            let result = tokio::task::spawn_blocking(|| {
                let (provider, _status) = crate::embeddings::create_embedding_provider();
                provider
            })
            .await;
            return match result {
                Ok(p) => p,
                Err(e) => {
                    warn!("In-process native embedding init panicked: {e}");
                    None
                }
            };
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
