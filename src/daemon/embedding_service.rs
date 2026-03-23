//! Daemon-level shared embedding service.
//!
//! Owns a single `EmbeddingProvider` instance shared across all sessions.
//! Initialized eagerly at daemon startup. Tools access it through the handler.

use std::sync::Arc;

use crate::embeddings::{EmbeddingProvider, EmbeddingRuntimeStatus};

/// Shared embedding service for the daemon.
///
/// Created once at daemon startup and passed (as `Arc<EmbeddingService>`) to
/// each session handler. This avoids per-session provider initialization and
/// ensures all sessions share a single ONNX Runtime / sidecar process.
pub struct EmbeddingService {
    provider: Option<Arc<dyn EmbeddingProvider>>,
    runtime_status: Option<EmbeddingRuntimeStatus>,
}

impl EmbeddingService {
    /// Initialize the embedding service by calling the shared factory.
    ///
    /// Logs initialization start/end. The heavy lifting (provider selection,
    /// model loading, sidecar bootstrap) happens inside `create_embedding_provider`.
    pub fn initialize() -> Self {
        use crate::embeddings::create_embedding_provider;
        use tracing::info;

        info!("Initializing shared embedding service...");
        let (provider, runtime_status) = create_embedding_provider();
        let available = provider.is_some();
        info!(available, "Shared embedding service ready");

        EmbeddingService {
            provider,
            runtime_status,
        }
    }

    /// Returns a reference to the shared provider, if one was initialized.
    pub fn provider(&self) -> Option<&Arc<dyn EmbeddingProvider>> {
        self.provider.as_ref()
    }

    /// Returns the runtime status from initialization, if available.
    pub fn runtime_status(&self) -> Option<&EmbeddingRuntimeStatus> {
        self.runtime_status.as_ref()
    }

    /// Whether a usable embedding provider is available.
    pub fn is_available(&self) -> bool {
        self.provider.is_some()
    }

    /// Shut down the underlying provider (releases child processes, etc.).
    pub fn shutdown(&self) {
        if let Some(ref provider) = self.provider {
            provider.shutdown();
        }
    }

    /// Test constructor that accepts a pre-built provider (or None).
    #[cfg(test)]
    pub fn initialize_for_test(provider: Option<Arc<dyn EmbeddingProvider>>) -> Self {
        EmbeddingService {
            provider,
            runtime_status: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_service_unavailable_when_provider_none() {
        let service = EmbeddingService::initialize_for_test(None);
        assert!(!service.is_available());
        assert!(service.provider().is_none());
        assert!(service.runtime_status().is_none());
    }

    #[test]
    fn test_embedding_service_initialize_with_provider_disabled() {
        // SAFETY: This test sets an env var that affects embedding provider
        // selection. Rust 2024 requires unsafe for set_var/remove_var because
        // they are not thread-safe. This test is fine since cargo test runs
        // each test in its own thread, and the env var is restored after.
        unsafe {
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "none");
        }

        let service = EmbeddingService::initialize();

        // Clean up env before assertions (so panics don't leave it set)
        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
        }

        assert!(!service.is_available(), "provider=none should mean unavailable");
        // When explicitly disabled via "none", create_embedding_provider
        // returns (None, None) -- no runtime_status is produced because
        // there's nothing to report (it's an intentional skip, not an error).
        assert!(
            service.runtime_status().is_none(),
            "provider=none skips initialization entirely, so no runtime_status"
        );
    }
}
