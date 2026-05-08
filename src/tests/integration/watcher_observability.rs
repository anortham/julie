//! Regression tests for watcher INFO-level observability.
//!
//! These tests assert that specific INFO log lines fire during watcher
//! operations so that future log-format changes or level-demotion regressions
//! surface immediately.
//!
//! Each async test installs the `LogCapture` subscriber as the thread-default
//! at the beginning of the test function and reverts it when the guard is
//! dropped. This avoids the `block_on`-inside-a-runtime panic that occurs when
//! using `with_default(subscriber, || { runtime.block_on(...) })`.

use crate::database::SymbolDatabase;
use crate::extractors::ExtractorManager;
use crate::watcher::handlers::handle_file_created_or_modified_static;
use crate::watcher::observability::LogCapture;
use crate::workspace::mutation_gate::acquire_gate;
use std::fs;
use std::sync::{Arc, Mutex};
use tracing_subscriber::layer::SubscriberExt;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn make_db(workspace_root: &std::path::Path) -> Arc<Mutex<SymbolDatabase>> {
    let db_path = workspace_root.join("obs_test.db");
    Arc::new(Mutex::new(
        SymbolDatabase::new(&db_path).expect("Failed to create test database"),
    ))
}

/// Install a `LogCapture` as the default subscriber for the current thread and
/// return the capture handle + the `DefaultGuard` (which resets the subscriber
/// when dropped).
fn install_capture() -> (LogCapture, tracing::subscriber::DefaultGuard) {
    let capture = LogCapture::new();
    let subscriber = tracing_subscriber::registry().with(capture.layer());
    let guard = tracing::subscriber::set_default(subscriber);
    (capture, guard)
}

// ---------------------------------------------------------------------------
// Test 1: indexing an unchanged file logs a "skipped" INFO line
// ---------------------------------------------------------------------------

/// After a file is indexed once, a second identical write produces an INFO
/// "unchanged" log (the hash matched).
#[tokio::test]
async fn test_hash_match_logs_skipped_info() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("obs_hash_skip");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("skipped.rs");
    fs::write(&test_file, "fn foo() {}").unwrap();
    let abs = test_file.canonicalize().unwrap();

    let db = make_db(&workspace_root);
    let extractor = Arc::new(ExtractorManager::new());

    // First index — establishes the hash (no capture needed here).
    {
        let guard = acquire_gate("obs_hash_skip_first").await;
        handle_file_created_or_modified_static(
            abs.clone(),
            &db,
            &extractor,
            &workspace_root,
            None,
            &guard,
        )
        .await
        .expect("first index should succeed");
    }

    // Second index — content unchanged, should produce an INFO "unchanged" log.
    let (capture, _sub_guard) = install_capture();

    {
        let guard = acquire_gate("obs_hash_skip_second").await;
        handle_file_created_or_modified_static(
            abs.clone(),
            &db,
            &extractor,
            &workspace_root,
            None,
            &guard,
        )
        .await
        .expect("second index should succeed");
    }

    // Drop the subscriber guard before asserting (not strictly necessary but clean).
    drop(_sub_guard);

    let entries = capture.entries();
    let info_entries: Vec<_> = entries.iter().filter(|e| e.level == "INFO").collect();

    assert!(
        info_entries
            .iter()
            .any(|e| e.message.contains("skipped.rs") && e.message.contains("unchanged")),
        "Expected an INFO log mentioning 'skipped.rs' and 'unchanged'. Got: {:?}",
        info_entries.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Test 2: indexing a new/changed file logs an INFO line with symbol count
// ---------------------------------------------------------------------------

/// When a file is successfully indexed, an INFO log appears with the file path
/// and extraction details.
#[tokio::test]
async fn test_indexed_file_logs_symbol_count_info() {
    let temp_dir = crate::tests::helpers::unique_temp_dir("obs_indexed");
    let workspace_root = temp_dir.path().canonicalize().unwrap();

    let test_file = workspace_root.join("indexed.rs");
    fs::write(&test_file, "pub fn alpha() {}\npub fn beta() {}").unwrap();
    let abs = test_file.canonicalize().unwrap();

    let db = make_db(&workspace_root);
    let extractor = Arc::new(ExtractorManager::new());

    let (capture, _sub_guard) = install_capture();

    {
        let guard = acquire_gate("obs_indexed").await;
        handle_file_created_or_modified_static(
            abs.clone(),
            &db,
            &extractor,
            &workspace_root,
            None,
            &guard,
        )
        .await
        .expect("index should succeed");
    }

    drop(_sub_guard);

    let entries = capture.entries();
    let info_entries: Vec<_> = entries.iter().filter(|e| e.level == "INFO").collect();

    // Must log an INFO with "indexed.rs" and extraction details.
    let has_indexed_line = info_entries.iter().any(|e| {
        e.message.contains("indexed.rs")
            && (e.message.contains("symbol") || e.message.contains("extracted"))
    });

    assert!(
        has_indexed_line,
        "Expected an INFO log mentioning 'indexed.rs' and symbol count or 'extracted'. Got: {:?}",
        info_entries.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Test 3: rate limiter suppresses burst beyond threshold
// ---------------------------------------------------------------------------

/// The rate limiter must allow the first N events through and suppress further
/// events within the same window.
#[test]
fn test_rate_limiter_suppresses_burst() {
    use crate::watcher::observability::RateLimiter;

    let limiter = RateLimiter::new(5); // max 5 per second

    // The first 5 calls should be allowed.
    let mut allowed = 0usize;
    for _ in 0..5 {
        if limiter.should_emit() {
            allowed += 1;
        }
    }
    assert_eq!(allowed, 5, "First 5 events should all pass through");

    // The 6th call within the same second should be suppressed.
    assert!(
        !limiter.should_emit(),
        "6th event within the same second should be suppressed"
    );
}

// ---------------------------------------------------------------------------
// Test 4: rate limiter resets after window elapses
// ---------------------------------------------------------------------------

/// After the window expires, the rate limiter allows events again.
#[test]
fn test_rate_limiter_resets_after_window() {
    use crate::watcher::observability::RateLimiter;
    use std::time::Duration;

    let limiter = RateLimiter::new_with_window(2, Duration::from_millis(50));

    assert!(limiter.should_emit(), "First event should be allowed");
    assert!(limiter.should_emit(), "Second event should be allowed");
    assert!(!limiter.should_emit(), "Third event should be suppressed");

    // Wait for window to expire.
    std::thread::sleep(Duration::from_millis(60));

    assert!(
        limiter.should_emit(),
        "After window reset, first event should be allowed again"
    );
}

// ---------------------------------------------------------------------------
// Test 5: LogCapture layer captures INFO events
// ---------------------------------------------------------------------------

/// Verify that `LogCapture` correctly captures INFO-level tracing events,
/// which are the primary level this module promotes to.
#[test]
fn test_log_capture_captures_info() {
    use tracing::info;

    let (capture, _sub_guard) = install_capture();

    info!("watcher batch summary: files=3 unchanged=1 extracted=2");

    drop(_sub_guard);

    let entries = capture.entries();
    assert_eq!(entries.len(), 1, "Should have captured 1 INFO entry");
    assert_eq!(entries[0].level, "INFO");
    assert!(
        entries[0].message.contains("watcher batch summary"),
        "Message should contain 'watcher batch summary', got: {}",
        entries[0].message
    );
}

// ---------------------------------------------------------------------------
// Test 6: gate-wait timing wrapper logs INFO when wait exceeds threshold
// ---------------------------------------------------------------------------

/// When `timed_acquire_gate` waits more than the configured threshold, it emits
/// an INFO line with the wait duration.
#[tokio::test]
async fn test_gate_wait_logging_above_threshold() {
    use crate::watcher::observability::timed_acquire_gate;
    use std::time::Duration;

    // Use a unique workspace_id to avoid cross-test contention.
    let workspace_id = "obs_gate_timing_workspace_unique_xkcd_042";

    // Hold the gate so the second acquisition has to wait.
    let held_guard = acquire_gate(workspace_id).await;

    // Install the subscriber BEFORE spawning — captures logs on this task's thread.
    let (capture, _sub_guard) = install_capture();

    // Spawn a background task that releases the held guard after 150ms.
    let release_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        drop(held_guard);
    });

    // timed_acquire_gate with 100ms threshold — will block for ~150ms, so it
    // should log an INFO line with the wait time.
    let _guard = timed_acquire_gate(workspace_id, Duration::from_millis(100)).await;
    drop(_guard);

    release_task.await.expect("release task should complete");

    drop(_sub_guard);

    let entries = capture.entries();
    let info_entries: Vec<_> = entries.iter().filter(|e| e.level == "INFO").collect();

    assert!(
        info_entries
            .iter()
            .any(|e| e.message.contains("gate") && e.message.contains("ms")),
        "Expected an INFO log about gate wait with duration in ms. Got: {:?}",
        info_entries.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}
