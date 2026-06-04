//! INFO-level observability helpers for the file watcher.
//!
//! Provides:
//! - `RateLimiter` — token-bucket rate limiter using `AtomicU64`, no extra crates.
//! - `LogCapture` — a `tracing_subscriber::Layer` that records INFO+ events for tests.
//! - `timed_acquire_gate` — wraps mutation-gate acquisition and logs INFO if wait > threshold.
//!
//! All public types are available for use in integration tests via
//! `crate::watcher::observability`.

use crate::workspace::mutation_gate::{MutationGuard, Registry as MutationGateRegistry};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::Level;
use tracing_subscriber::Layer;

// ---------------------------------------------------------------------------
// RateLimiter
// ---------------------------------------------------------------------------

/// Per-window token-bucket rate limiter.
///
/// Tracks how many events have been emitted in the current window using two
/// `AtomicU64` values (window start in nanoseconds + count). No locks, no
/// allocation per call.
pub struct RateLimiter {
    /// Maximum events allowed per window.
    max_per_window: u64,
    /// Window duration in nanoseconds.
    window_nanos: u64,
    /// Start of the current window, in nanoseconds since UNIX epoch.
    window_start_nanos: AtomicU64,
    /// Number of events emitted in the current window.
    window_count: AtomicU64,
}

impl RateLimiter {
    /// Create a rate limiter that allows `max_per_window` events per second.
    pub fn new(max_per_window: u64) -> Self {
        Self::new_with_window(max_per_window, Duration::from_secs(1))
    }

    /// Create a rate limiter with an explicit window duration (useful for tests).
    pub fn new_with_window(max_per_window: u64, window: Duration) -> Self {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self {
            max_per_window,
            window_nanos: window.as_nanos() as u64,
            window_start_nanos: AtomicU64::new(now_nanos),
            window_count: AtomicU64::new(0),
        }
    }

    /// Returns `true` if this event should be emitted (i.e., within budget).
    ///
    /// Thread-safe via atomics. May allow slightly more than `max_per_window`
    /// events under contention at window boundaries — acceptable for logging.
    pub fn should_emit(&self) -> bool {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let window_start = self.window_start_nanos.load(Ordering::Relaxed);
        let window_nanos = self.window_nanos;

        if now_nanos.saturating_sub(window_start) >= window_nanos {
            // Start of a new window. Store the new window start aligned to the
            // window boundary so windows don't drift.
            let new_start = (now_nanos / window_nanos) * window_nanos;
            self.window_start_nanos.store(new_start, Ordering::Relaxed);
            self.window_count.store(1, Ordering::Relaxed);
            return true;
        }

        // Within the current window: increment and check against limit.
        let prev = self.window_count.fetch_add(1, Ordering::Relaxed);
        prev < self.max_per_window
    }
}

// ---------------------------------------------------------------------------
// timed_acquire_gate
// ---------------------------------------------------------------------------

/// Acquire the mutation gate for `workspace_id`, logging an INFO line if the
/// wait exceeds `threshold`.
///
/// This wrapper is for watcher-side acquisitions where gate-wait latency
/// matters for steady-state operations. Catch-up indexing holding the gate
/// for a long time will surface immediately in the daemon log.
pub async fn timed_acquire_gate<'a>(
    workspace_id: &'a str,
    threshold: Duration,
) -> MutationGuard<'a> {
    timed_acquire_gate_with_registry(MutationGateRegistry::global(), workspace_id, threshold).await
}

pub async fn timed_acquire_gate_with_registry<'a>(
    registry: &MutationGateRegistry,
    workspace_id: &'a str,
    threshold: Duration,
) -> MutationGuard<'a> {
    let start = Instant::now();
    let guard = registry.acquire(workspace_id).await;
    let elapsed = start.elapsed();
    if elapsed >= threshold {
        tracing::info!(
            workspace_id = workspace_id,
            wait_ms = elapsed.as_millis() as u64,
            "Waited {}ms for mutation gate on workspace {}",
            elapsed.as_millis(),
            workspace_id,
        );
    }
    guard
}

pub async fn timed_acquire_gate_with_registry_or_cancelled(
    registry: &MutationGateRegistry,
    workspace_id: &str,
    threshold: Duration,
    cancel_flag: &AtomicBool,
) -> Option<MutationGuard<'static>> {
    let start = Instant::now();
    let mut logged_wait = false;

    loop {
        if let Some(guard) = registry.try_acquire(workspace_id) {
            let elapsed = start.elapsed();
            if elapsed >= threshold {
                tracing::info!(
                    workspace_id = workspace_id,
                    wait_ms = elapsed.as_millis() as u64,
                    "Waited {}ms for mutation gate on workspace {}",
                    elapsed.as_millis(),
                    workspace_id,
                );
            }
            return Some(guard);
        }

        if cancel_flag.load(Ordering::Acquire) {
            let elapsed = start.elapsed();
            tracing::warn!(
                workspace_id = workspace_id,
                wait_ms = elapsed.as_millis() as u64,
                "Cancelled mutation gate wait during watcher shutdown for workspace {} after {}ms",
                workspace_id,
                elapsed.as_millis(),
            );
            return None;
        }

        if !logged_wait && start.elapsed() >= threshold {
            logged_wait = true;
            let elapsed = start.elapsed();
            tracing::info!(
                workspace_id = workspace_id,
                wait_ms = elapsed.as_millis() as u64,
                "Waited {}ms for mutation gate on workspace {}",
                elapsed.as_millis(),
                workspace_id,
            );
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

// ---------------------------------------------------------------------------
// LogCapture (test helper)
// ---------------------------------------------------------------------------

/// A captured log entry.
#[derive(Debug, Clone)]
pub struct CapturedEntry {
    pub level: String,
    pub message: String,
    pub target: String,
}

struct MessageVisitor {
    message: String,
}

impl tracing::field::Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_owned();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}

/// Thread-safe ring buffer that captures INFO+ tracing events, intended for
/// use in tests. Clone is cheap (Arc inside).
#[derive(Clone)]
pub struct LogCapture {
    inner: Arc<Mutex<VecDeque<CapturedEntry>>>,
}

impl LogCapture {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Returns a `tracing_subscriber::Layer` that feeds this capture buffer.
    pub fn layer(&self) -> LogCaptureLayer {
        LogCaptureLayer {
            capture: self.clone(),
        }
    }

    /// Returns all captured entries, oldest first.
    pub fn entries(&self) -> Vec<CapturedEntry> {
        self.inner
            .lock()
            .expect("LogCapture mutex poisoned")
            .iter()
            .cloned()
            .collect()
    }

    fn push(&self, entry: CapturedEntry) {
        self.inner
            .lock()
            .expect("LogCapture mutex poisoned")
            .push_back(entry);
    }
}

/// The `tracing_subscriber::Layer` implementation for `LogCapture`.
pub struct LogCaptureLayer {
    capture: LogCapture,
}

impl<S> Layer<S> for LogCaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let level = *event.metadata().level();
        // Capture INFO and above (INFO, WARN, ERROR).
        if level > Level::INFO {
            return;
        }

        let mut visitor = MessageVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        self.capture.push(CapturedEntry {
            level: level.to_string(),
            message: visitor.message,
            target: event.metadata().target().to_owned(),
        });
    }
}
