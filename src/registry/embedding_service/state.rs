use std::sync::Arc;

use crate::embeddings::{EmbeddingProvider, EmbeddingRuntimeStatus};

/// The three observable states of the embedding service.
///
/// `Clone` is required because `watch` distributes values by cloning them to
/// each receiver. `EmbeddingRuntimeStatus` already derives `Clone`, and
/// `Arc<dyn EmbeddingProvider>` clones cheaply.
#[derive(Clone)]
pub enum EmbeddingServiceState {
    /// Initial state. The background init task has not yet published a result.
    Initializing,
    /// Provider successfully created.
    Ready {
        provider: Arc<dyn EmbeddingProvider>,
        runtime_status: EmbeddingRuntimeStatus,
    },
    /// Provider creation failed, was disabled, or the background task panicked.
    /// `runtime_status` is `Some` when the failure produced a status (e.g. the
    /// backend resolver reported a degraded reason) and `None` when the
    /// provider was intentionally skipped (e.g. `JULIE_EMBEDDING_PROVIDER=none`)
    /// or the background task panicked before producing one.
    Unavailable {
        reason: String,
        runtime_status: Option<EmbeddingRuntimeStatus>,
    },
}

/// The outcome of waiting for the service to settle out of `Initializing`.
pub enum EmbeddingServiceSettled {
    /// Service published `Ready`; the provider is available.
    Ready {
        provider: Arc<dyn EmbeddingProvider>,
        runtime_status: EmbeddingRuntimeStatus,
    },
    /// Service published `Unavailable`; the reason is carried here. Callers
    /// that need the runtime status can query `EmbeddingService::runtime_status`.
    Unavailable {
        reason: String,
        runtime_status: Option<EmbeddingRuntimeStatus>,
    },
    /// Deadline elapsed while the service was still `Initializing`.
    Timeout,
}
