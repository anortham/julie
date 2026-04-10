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

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::embeddings::{EmbeddingProvider, EmbeddingRuntimeStatus};

/// Shared embedding service for the daemon.
///
/// Created once at daemon startup. Starts in `Initializing`; a background
/// task drives provider creation and calls `publish_ready` or
/// `publish_unavailable` when it finishes. All accessors are non-blocking.
/// Use `wait_until_settled` to park on the transition out of `Initializing`
/// with a bounded timeout.
pub struct EmbeddingService {
    state_tx: watch::Sender<EmbeddingServiceState>,
    state_rx: watch::Receiver<EmbeddingServiceState>,
}

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
    Ready(Arc<dyn EmbeddingProvider>),
    /// Service published `Unavailable`; the reason is carried here. Callers
    /// that need the runtime status can query `EmbeddingService::runtime_status`.
    Unavailable(String),
    /// Deadline elapsed while the service was still `Initializing`.
    Timeout,
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
        matches!(
            *self.state_rx.borrow(),
            EmbeddingServiceState::Ready { .. }
        )
    }

    /// `true` iff state is not `Initializing`. Non-blocking.
    pub fn is_settled(&self) -> bool {
        !matches!(
            *self.state_rx.borrow(),
            EmbeddingServiceState::Initializing
        )
    }

    /// Wait for the service to leave `Initializing`, up to `timeout`.
    ///
    /// Returns immediately (via the fast path) if the service is already
    /// settled. Otherwise clones the `watch::Receiver` and loops
    /// `changed().await` under a `tokio::time::timeout`. Multiple concurrent
    /// waiters all receive the same settlement because they observe the same
    /// `watch` value.
    pub async fn wait_until_settled(&self, timeout: Duration) -> EmbeddingServiceSettled {
        // Fast path: inspect current state without awaiting.
        if let Some(settled) = self.snapshot_settled() {
            return settled;
        }

        // Slow path: park on the watch receiver. We clone the service's
        // receiver rather than calling `subscribe()` on the sender so the
        // initial `borrow_and_update()` marks the current (Initializing)
        // value as "seen", and `changed().await` fires only on genuinely
        // new updates.
        let mut rx = self.state_rx.clone();
        // Mark the current Initializing value as seen so `changed()` waits
        // for the NEXT transition.
        let _ = rx.borrow_and_update();

        let fut = async {
            loop {
                if rx.changed().await.is_err() {
                    // Sender dropped — the service is gone. Treat as Unavailable
                    // so callers fall back to keyword-only cleanly.
                    return EmbeddingServiceSettled::Unavailable(
                        "EmbeddingService dropped before settling".to_string(),
                    );
                }
                if let Some(settled) = Self::state_to_settled(&rx.borrow()) {
                    return settled;
                }
                // State changed but still not settled — extremely unlikely
                // (we never transition back to Initializing), but loop defensively.
            }
        };

        match tokio::time::timeout(timeout, fut).await {
            Ok(settled) => settled,
            Err(_elapsed) => {
                debug!(
                    timeout_ms = timeout.as_millis() as u64,
                    "EmbeddingService: wait_until_settled timed out"
                );
                EmbeddingServiceSettled::Timeout
            }
        }
    }

    /// Return `Some(settled)` if the current state is already terminal,
    /// else `None`. Used by the `wait_until_settled` fast path.
    fn snapshot_settled(&self) -> Option<EmbeddingServiceSettled> {
        Self::state_to_settled(&self.state_rx.borrow())
    }

    /// Convert a borrowed `EmbeddingServiceState` to a settled outcome, if
    /// applicable. `Initializing` returns `None`.
    fn state_to_settled(state: &EmbeddingServiceState) -> Option<EmbeddingServiceSettled> {
        match state {
            EmbeddingServiceState::Initializing => None,
            EmbeddingServiceState::Ready { provider, .. } => {
                Some(EmbeddingServiceSettled::Ready(Arc::clone(provider)))
            }
            EmbeddingServiceState::Unavailable { reason, .. } => {
                Some(EmbeddingServiceSettled::Unavailable(reason.clone()))
            }
        }
    }

    /// Shut down the underlying provider, if any. A no-op when the service
    /// is in `Initializing` or `Unavailable`.
    pub fn shutdown(&self) {
        if let EmbeddingServiceState::Ready { provider, .. } = &*self.state_rx.borrow() {
            provider.shutdown();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Existing test preserved: service constructed with None provider
    /// should report unavailable and have no runtime status.
    #[test]
    fn test_embedding_service_unavailable_when_provider_none() {
        let service = EmbeddingService::initialize_for_test(None);
        assert!(!service.is_available());
        assert!(service.provider().is_none());
        assert!(service.runtime_status().is_none());
        assert!(service.is_settled(), "None provider should settle to Unavailable");
    }

    /// `initializing()` starts in `Initializing`. `is_available` is false,
    /// `is_settled` is false, `provider` is None.
    #[test]
    fn test_initializing_state() {
        let service = EmbeddingService::initializing();
        assert!(!service.is_available());
        assert!(!service.is_settled());
        assert!(service.provider().is_none());
        assert!(service.runtime_status().is_none());
    }

    /// `wait_until_settled` returns `Ready` immediately when the service is
    /// already `Ready` before the call.
    #[tokio::test]
    async fn test_wait_until_settled_already_ready() {
        let fake_provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider::default());
        let status = synthetic_status();
        let service = EmbeddingService::initializing();
        service.publish_ready(fake_provider, status);

        let outcome = service
            .wait_until_settled(Duration::from_millis(100))
            .await;
        assert!(matches!(outcome, EmbeddingServiceSettled::Ready(_)));
    }

    /// `wait_until_settled` returns `Unavailable` immediately when the service
    /// is already `Unavailable` before the call.
    #[tokio::test]
    async fn test_wait_until_settled_already_unavailable() {
        let service = EmbeddingService::initializing();
        service.publish_unavailable("boom".to_string(), None);

        let outcome = service
            .wait_until_settled(Duration::from_millis(100))
            .await;
        match outcome {
            EmbeddingServiceSettled::Unavailable(reason) => assert_eq!(reason, "boom"),
            _ => panic!("expected Unavailable"),
        }
    }

    /// `wait_until_settled` returns `Ready` when `publish_ready` fires
    /// concurrently during the wait — covers the common lazy-init case.
    #[tokio::test]
    async fn test_wait_until_settled_publish_during_wait() {
        let service = Arc::new(EmbeddingService::initializing());

        let publisher = {
            let service = Arc::clone(&service);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let fake_provider: Arc<dyn EmbeddingProvider> = Arc::new(FakeProvider::default());
                service.publish_ready(fake_provider, synthetic_status());
            })
        };

        let outcome = service
            .wait_until_settled(Duration::from_millis(500))
            .await;
        publisher.await.unwrap();
        assert!(
            matches!(outcome, EmbeddingServiceSettled::Ready(_)),
            "waiter should observe Ready published during the wait"
        );
    }

    /// `wait_until_settled` returns `Timeout` when the service never settles.
    #[tokio::test]
    async fn test_wait_until_settled_timeout() {
        let service = EmbeddingService::initializing();
        let outcome = service
            .wait_until_settled(Duration::from_millis(20))
            .await;
        assert!(matches!(outcome, EmbeddingServiceSettled::Timeout));
    }

    /// Multiple concurrent waiters all receive the settlement.
    #[tokio::test]
    async fn test_wait_until_settled_multiple_waiters() {
        let service = Arc::new(EmbeddingService::initializing());

        let waiter_count = 4;
        let mut handles = Vec::new();
        for _ in 0..waiter_count {
            let service = Arc::clone(&service);
            handles.push(tokio::spawn(async move {
                service
                    .wait_until_settled(Duration::from_millis(500))
                    .await
            }));
        }

        // Give the waiters a moment to park, then publish.
        tokio::time::sleep(Duration::from_millis(20)).await;
        service.publish_unavailable("shared reason".to_string(), None);

        for handle in handles {
            let outcome = handle.await.unwrap();
            match outcome {
                EmbeddingServiceSettled::Unavailable(reason) => {
                    assert_eq!(reason, "shared reason")
                }
                _ => panic!("expected all waiters to observe Unavailable"),
            }
        }
    }

    /// Existing test preserved: calling the sync `initialize()` compat path
    /// with the provider explicitly disabled via env var should settle to
    /// `Unavailable` with no runtime status (matches pre-refactor behavior
    /// where `create_embedding_provider` returns `(None, None)` for the
    /// intentional-skip case).
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

        assert!(
            !service.is_available(),
            "provider=none should settle to Unavailable"
        );
        // When explicitly disabled via "none", create_embedding_provider
        // returns (None, None) -- no runtime_status is produced because
        // there's nothing to report (it's an intentional skip, not an error).
        assert!(
            service.runtime_status().is_none(),
            "provider=none skips initialization entirely, so no runtime_status"
        );
        assert!(service.is_settled(), "service should be settled after initialize()");
    }

    /// `publish_unavailable` that carries a runtime status exposes it via
    /// `runtime_status()` — preserves pre-refactor dashboard observability.
    #[test]
    fn test_unavailable_with_runtime_status_is_queryable() {
        let status = synthetic_status();
        let service = EmbeddingService::initializing();
        service.publish_unavailable("degraded".to_string(), Some(status.clone()));

        assert!(!service.is_available());
        let fetched = service.runtime_status().expect("status should be present");
        assert_eq!(fetched.degraded_reason, status.degraded_reason);
    }

    // ---- test helpers ----

    fn synthetic_status() -> EmbeddingRuntimeStatus {
        EmbeddingRuntimeStatus {
            requested_backend: crate::embeddings::EmbeddingBackend::Unresolved,
            resolved_backend: crate::embeddings::EmbeddingBackend::Unresolved,
            accelerated: false,
            degraded_reason: None,
        }
    }

    /// Minimal no-op provider for exercising the state machine. Doesn't
    /// actually produce embeddings.
    #[derive(Default)]
    struct FakeProvider;

    impl EmbeddingProvider for FakeProvider {
        fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(Vec::new())
        }

        fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(Vec::new())
        }

        fn dimensions(&self) -> usize {
            0
        }

        fn device_info(&self) -> crate::embeddings::DeviceInfo {
            crate::embeddings::DeviceInfo {
                runtime: "test".to_string(),
                device: "test".to_string(),
                model_name: "test-model".to_string(),
                dimensions: 0,
            }
        }
    }
}
