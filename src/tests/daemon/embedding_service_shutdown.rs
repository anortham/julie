//! Tests for `EmbeddingService::shutdown` bounded-wait behavior.
//!
//! These tests verify:
//! 1. `shutdown().await` blocks until `wait_for_exit` returns (child exited path).
//! 2. `shutdown().await` returns without panicking when `wait_for_exit` times out.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::daemon::embedding_service::EmbeddingService;
use crate::embeddings::{DeviceInfo, EmbeddingProvider};

// ---- fake providers ----

/// Provider whose `wait_for_exit` returns `true` after a configurable delay.
/// Simulates a well-behaved sidecar that exits promptly.
struct PromptExitProvider {
    exit_delay: Duration,
    wait_called: Arc<AtomicBool>,
}

impl PromptExitProvider {
    fn new(exit_delay: Duration) -> (Self, Arc<AtomicBool>) {
        let wait_called = Arc::new(AtomicBool::new(false));
        let provider = Self {
            exit_delay,
            wait_called: Arc::clone(&wait_called),
        };
        (provider, wait_called)
    }
}

impl EmbeddingProvider for PromptExitProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }

    fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "test".to_string(),
            device: "test".to_string(),
            model_name: "test-prompt-exit".to_string(),
            dimensions: 0,
        }
    }

    fn wait_for_exit(&self, _timeout: Duration) -> bool {
        self.wait_called.store(true, Ordering::SeqCst);
        std::thread::sleep(self.exit_delay);
        true
    }
}

/// Provider whose `wait_for_exit` always returns `false` (simulates a sidecar
/// that never exits within the timeout window).
struct TimeoutExitProvider {
    wait_called: Arc<AtomicBool>,
}

impl TimeoutExitProvider {
    fn new() -> (Self, Arc<AtomicBool>) {
        let wait_called = Arc::new(AtomicBool::new(false));
        let provider = Self {
            wait_called: Arc::clone(&wait_called),
        };
        (provider, wait_called)
    }
}

impl EmbeddingProvider for TimeoutExitProvider {
    fn embed_query(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(Vec::new())
    }

    fn embed_batch(&self, _texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "test".to_string(),
            device: "test".to_string(),
            model_name: "test-timeout-exit".to_string(),
            dimensions: 0,
        }
    }

    fn wait_for_exit(&self, _timeout: Duration) -> bool {
        self.wait_called.store(true, Ordering::SeqCst);
        // Return false immediately to simulate timeout without actually sleeping.
        // The EmbeddingService is responsible for the wall-clock timeout; the
        // provider signals "not exited" via the false return.
        false
    }
}

// ---- tests ----

/// `shutdown().await` blocks until `wait_for_exit` returns and completes
/// within a reasonable bound. Verifies that the async shutdown actually awaits
/// the child exit path (not fire-and-forget).
#[tokio::test]
async fn test_embedding_service_shutdown_waits_for_child_exit() {
    let exit_delay = Duration::from_millis(50);
    let (provider, wait_called) = PromptExitProvider::new(exit_delay);
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(provider);
    let service = EmbeddingService::initialize_for_test(Some(provider));

    let start = Instant::now();
    service.shutdown().await;
    let elapsed = start.elapsed();

    // wait_for_exit was called.
    assert!(
        wait_called.load(Ordering::SeqCst),
        "wait_for_exit must be called during shutdown"
    );

    // Shutdown completed after the exit delay (child exit was awaited).
    assert!(
        elapsed >= exit_delay,
        "shutdown must block until child exits: elapsed={elapsed:?}, exit_delay={exit_delay:?}"
    );

    // Shutdown finished well within the 3-second timeout + generous tolerance.
    let tolerance = Duration::from_millis(2000);
    let upper_bound = Duration::from_secs(3) + tolerance;
    assert!(
        elapsed < upper_bound,
        "shutdown took too long: {elapsed:?} >= {upper_bound:?}"
    );
}

/// `shutdown().await` returns without panicking when `wait_for_exit` indicates
/// the child did not exit within the timeout (returns `false`). This is the
/// Windows daemon restart safety path: even if the old sidecar is stuck, the
/// new daemon is not blocked indefinitely.
#[tokio::test]
async fn test_embedding_service_shutdown_returns_on_timeout() {
    let (provider, wait_called) = TimeoutExitProvider::new();
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(provider);
    let service = EmbeddingService::initialize_for_test(Some(provider));

    let start = Instant::now();
    // Must not panic, even though wait_for_exit returns false.
    service.shutdown().await;
    let elapsed = start.elapsed();

    // wait_for_exit was called so the shutdown path exercised the wait.
    assert!(
        wait_called.load(Ordering::SeqCst),
        "wait_for_exit must be called during shutdown even on timeout path"
    );

    // Shutdown returned quickly because TimeoutExitProvider returns false
    // immediately. The 3-second bound applies to real sidecar waits; with an
    // instant-returning mock this should be well under 1 second.
    let upper_bound = Duration::from_secs(1);
    assert!(
        elapsed < upper_bound,
        "shutdown returned too slowly for a mock provider: {elapsed:?}"
    );
}
