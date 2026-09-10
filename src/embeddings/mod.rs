//! Semantic embedding infrastructure — relocated to `julie_pipeline::embeddings`.

// Re-export all public items from julie_pipeline::embeddings
pub use julie_pipeline::embeddings::*;

// Re-export submodules so `crate::embeddings::factory::*` etc. remain valid
pub use julie_pipeline::embeddings::factory;
pub use julie_pipeline::embeddings::init;
pub use julie_pipeline::embeddings::log_fields;
pub use julie_pipeline::embeddings::metadata;
pub use julie_pipeline::embeddings::pipeline;
pub use julie_pipeline::embeddings::sidecar_protocol;

use std::sync::Arc;
use tracing::warn;

/// Build the in-process embedding provider off the async runtime thread.
/// `JULIE_EMBEDDING_PROVIDER=none` and every launch failure yield `None`
/// (keyword-only); startup never blocks on semantics.
pub async fn acquire_in_process_embedding_provider() -> Option<Arc<dyn EmbeddingProvider>> {
    match tokio::task::spawn_blocking(|| create_embedding_provider().0).await {
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
