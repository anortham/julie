use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, warn};

use super::EmbeddingService;
use super::state::{EmbeddingServiceSettled, EmbeddingServiceState};

impl EmbeddingService {
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
                    return EmbeddingServiceSettled::Unavailable {
                        reason: "EmbeddingService dropped before settling".to_string(),
                        runtime_status: None,
                    };
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
    /// else `None`. Used by the `wait_until_settled` fast path and by
    /// callers that want a non-blocking readiness probe.
    pub fn try_settled(&self) -> Option<EmbeddingServiceSettled> {
        Self::state_to_settled(&self.state_rx.borrow())
    }

    /// Internal alias preserved for readability inside `wait_until_settled`.
    fn snapshot_settled(&self) -> Option<EmbeddingServiceSettled> {
        self.try_settled()
    }

    /// Convert a borrowed `EmbeddingServiceState` to a settled outcome, if
    /// applicable. `Initializing` returns `None`.
    fn state_to_settled(state: &EmbeddingServiceState) -> Option<EmbeddingServiceSettled> {
        match state {
            EmbeddingServiceState::Initializing => None,
            EmbeddingServiceState::Ready {
                provider,
                runtime_status,
            } => Some(EmbeddingServiceSettled::Ready {
                provider: Arc::clone(provider),
                runtime_status: runtime_status.clone(),
            }),
            EmbeddingServiceState::Unavailable {
                reason,
                runtime_status,
            } => Some(EmbeddingServiceSettled::Unavailable {
                reason: reason.clone(),
                runtime_status: runtime_status.clone(),
            }),
        }
    }

    /// Shut down the underlying provider, if any. A no-op when the service
    /// is in `Initializing` or `Unavailable`.
    ///
    /// Sends a graceful shutdown signal to the provider, then awaits the
    /// underlying child process exit with a 3-second bound. This prevents the
    /// new daemon from racing the old sidecar's handle cleanup on Windows.
    pub async fn shutdown(&self) {
        let provider = match &*self.state_rx.borrow() {
            EmbeddingServiceState::Ready { provider, .. } => Arc::clone(provider),
            _ => return,
        };

        // Signal the provider to shut down (sends graceful shutdown RPC + kill).
        provider.shutdown();

        // Await child exit off the async executor so we don't block other tasks.
        // The 3-second bound ensures the new daemon isn't delayed indefinitely
        // if the old sidecar is stuck.
        const SIDECAR_EXIT_TIMEOUT: Duration = Duration::from_secs(3);
        let exited =
            tokio::task::spawn_blocking(move || provider.wait_for_exit(SIDECAR_EXIT_TIMEOUT))
                .await
                .unwrap_or(false);

        if !exited {
            warn!(
                "embedding sidecar did not exit within {}s; \
                 continuing shutdown — new daemon may race old sidecar handle release",
                SIDECAR_EXIT_TIMEOUT.as_secs()
            );
        }
    }
}
