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

pub struct ProviderAcquireError {
    pub message: String,
    pub retryable: bool,
}

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

pub async fn acquire_in_process_embedding_provider_with_prepare()
-> Result<Arc<dyn EmbeddingProvider>, ProviderAcquireError> {
    tokio::task::spawn_blocking(|| {
        let (provider, status) = create_embedding_provider();
        if let Some(provider) = provider {
            return Ok(provider);
        }

        let Some(status) = status else {
            return Err(ProviderAcquireError {
                message: "embedding provider is disabled".to_string(),
                retryable: false,
            });
        };
        let startup_error = status
            .degraded_reason
            .unwrap_or_else(|| "embedding provider unavailable".to_string());
        if !matches!(status.resolved_backend, EmbeddingBackend::Native)
            || !startup_error.contains("MODEL_NOT_PREPARED")
        {
            return Err(ProviderAcquireError {
                message: startup_error,
                retryable: false,
            });
        }

        let launch = julie_pipeline::embeddings::native::NativeLaunchConfig::try_new(
            std::env::var_os("JULIE_NATIVE_SIDECAR_PROGRAM")
                .as_deref()
                .map(std::path::Path::new),
            std::env::var("JULIE_NATIVE_SIDECAR_MODEL").ok().as_deref(),
            std::env::var_os("JULIE_EMBEDDING_CACHE_DIR")
                .as_deref()
                .map(std::path::Path::new),
        )
        .map_err(|error| ProviderAcquireError {
            message: format!("{startup_error}; {error}"),
            retryable: false,
        })?;
        julie_pipeline::embeddings::native::run_prepare(
            &launch.executable_path,
            &launch.cache_root,
            &launch.model_id,
        )
        .map_err(|error| ProviderAcquireError {
            message: format!("{startup_error}; {error}"),
            retryable: true,
        })?;
        let (provider, status) = create_embedding_provider();
        provider.ok_or_else(|| ProviderAcquireError {
            message: status
                .and_then(|status| status.degraded_reason)
                .unwrap_or_else(|| {
                    "embedding provider unavailable after model preparation".to_string()
                }),
            retryable: false,
        })
    })
    .await
    .map_err(|error| ProviderAcquireError {
        message: format!("embedding initialization task failed: {error}"),
        retryable: true,
    })?
}
