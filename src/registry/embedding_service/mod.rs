//! Daemon-level shared embedding service.
//!
//! Owns a single `EmbeddingProvider` instance shared across all sessions,
//! behind a lazy-init state machine. The service is constructed in the
//! `Initializing` state on daemon startup, then a background task drives
//! provider creation and publishes `Ready` or `Unavailable` when it finishes.
//!
//! Why a state machine with `tokio::sync::watch`:
//!
//! - Callers need to observe transitions (e.g. workspace indexing should wait
//!   for the provider instead of silently skipping). A simple `Option` can't
//!   express "not yet known, but will be soon" without polling.
//! - `watch::Receiver::changed()` fires for any update the receiver hasn't
//!   yet observed, including one that happened before `.await` was called.
//!   That avoids the TOCTOU hazard you'd get with a naive `RwLock + Notify`
//!   pair where `Notify::notify_waiters()` is edge-triggered and loses
//!   notifications if no one is currently parked on `notified()`.
//! - The state itself is cheap to clone (`Arc<dyn EmbeddingProvider>` plus a
//!   small status struct), so publishing a new `EmbeddingServiceState` via
//!   `send_replace` is a tiny atomic operation.

mod lifecycle;
mod state;

pub use state::{EmbeddingServiceSettled, EmbeddingServiceState};

use std::sync::Arc;

use tokio::sync::watch;
use tracing::{info, warn};

use crate::embeddings::{EmbeddingProvider, EmbeddingRuntimeStatus};

/// Shared embedding service for the daemon.
///
/// Created once at daemon startup. Starts in `Initializing`; a background
/// task drives provider creation and calls `publish_ready` or
/// `publish_unavailable` when it finishes. All accessors are non-blocking.
/// Use `wait_until_settled` to park on the transition out of `Initializing`
/// with a bounded timeout.
pub struct EmbeddingService {
    pub(crate) state_tx: watch::Sender<EmbeddingServiceState>,
    pub(crate) state_rx: watch::Receiver<EmbeddingServiceState>,
}

impl EmbeddingService {
    /// Construct a new service in the `Initializing` state.
    ///
    /// Daemon startup uses this and then spawns a background task that calls
    /// `publish_ready` or `publish_unavailable` when initialization finishes.
    pub fn initializing() -> Self {
        let (state_tx, state_rx) = watch::channel(EmbeddingServiceState::Initializing);
        EmbeddingService { state_tx, state_rx }
    }

    /// Synchronously initialize the shared embedding service by running the
    /// factory inline and publishing the result.
    ///
    /// This is the pre-lazy-init compatibility path: it blocks on
    /// `create_embedding_provider` and returns a fully settled service. Task 2
    /// of the daemon lazy-init plan replaces callers with
    /// `initializing() + background task`, at which point this method can go
    /// away. Kept for now so Task 1 is a drop-in refactor of the type.
    pub fn initialize() -> Self {
        use crate::embeddings::create_embedding_provider;

        info!("Initializing shared embedding service (synchronous compat path)...");
        let service = Self::initializing();
        let (provider, runtime_status) = create_embedding_provider();

        match (provider, runtime_status) {
            (Some(provider), Some(status)) => {
                service.publish_ready(provider, status);
            }
            (Some(provider), None) => {
                // Should not happen per create_embedding_provider invariants,
                // but be defensive: still publish Ready because we have a
                // working provider. Synthesize a minimal status.
                let status = EmbeddingRuntimeStatus {
                    requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                    resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                    accelerated: false,
                    degraded_reason: Some("provider returned without runtime status".to_string()),
                };
                service.publish_ready(provider, status);
            }
            (None, status) => {
                let reason = status
                    .as_ref()
                    .and_then(|s| s.degraded_reason.clone())
                    .unwrap_or_else(|| {
                        "embedding provider disabled or failed to initialize".to_string()
                    });
                service.publish_unavailable(reason, status);
            }
        }

        info!(
            available = service.is_available(),
            "Shared embedding service ready"
        );
        service
    }

    /// Transition to `Ready`. Wakes any waiters parked on `wait_until_settled`.
    pub fn publish_ready(
        &self,
        provider: Arc<dyn EmbeddingProvider>,
        runtime_status: EmbeddingRuntimeStatus,
    ) {
        info!("EmbeddingService: publishing Ready");
        self.state_tx.send_replace(EmbeddingServiceState::Ready {
            provider,
            runtime_status,
        });
    }

    /// Transition to `Unavailable`. Wakes any waiters parked on
    /// `wait_until_settled`. Callers must still supply a human-readable
    /// `reason`; `runtime_status` is optional because some failure paths don't
    /// produce one (e.g. explicit `JULIE_EMBEDDING_PROVIDER=none`).
    pub fn publish_unavailable(
        &self,
        reason: String,
        runtime_status: Option<EmbeddingRuntimeStatus>,
    ) {
        warn!(%reason, "EmbeddingService: publishing Unavailable");
        self.state_tx
            .send_replace(EmbeddingServiceState::Unavailable {
                reason,
                runtime_status,
            });
    }

    /// Return the provider if the service is currently `Ready`, else `None`.
    /// Cheap: clones an `Arc`.
    pub fn provider(&self) -> Option<Arc<dyn EmbeddingProvider>> {
        match &*self.state_rx.borrow() {
            EmbeddingServiceState::Ready { provider, .. } => Some(Arc::clone(provider)),
            _ => None,
        }
    }

    /// Return the current runtime status, if any. Returns the status from both
    /// `Ready` (always Some) and `Unavailable` (may be Some or None).
    pub fn runtime_status(&self) -> Option<EmbeddingRuntimeStatus> {
        match &*self.state_rx.borrow() {
            EmbeddingServiceState::Ready { runtime_status, .. } => Some(runtime_status.clone()),
            EmbeddingServiceState::Unavailable { runtime_status, .. } => runtime_status.clone(),
            EmbeddingServiceState::Initializing => None,
        }
    }

    /// `true` iff state is `Ready`. Non-blocking.
    pub fn is_available(&self) -> bool {
        matches!(*self.state_rx.borrow(), EmbeddingServiceState::Ready { .. })
    }

    /// `true` iff state is not `Initializing`. Non-blocking.
    pub fn is_settled(&self) -> bool {
        !matches!(*self.state_rx.borrow(), EmbeddingServiceState::Initializing)
    }

    /// Test constructor that accepts a pre-built provider (or None).
    ///
    /// Preserves the pre-refactor signature so existing tests continue to
    /// work. If `provider` is `Some`, the service is published to `Ready` with
    /// a synthetic runtime status. If `None`, it's published to `Unavailable`
    /// with no runtime status (matching the pre-refactor observable behavior).
    #[cfg(test)]
    pub fn initialize_for_test(provider: Option<Arc<dyn EmbeddingProvider>>) -> Self {
        let service = Self::initializing();
        match provider {
            Some(p) => {
                let status = EmbeddingRuntimeStatus {
                    requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                    resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
                    accelerated: false,
                    degraded_reason: None,
                };
                service.publish_ready(p, status);
            }
            None => {
                service.publish_unavailable("test: no provider".to_string(), None);
            }
        }
        service
    }
}
