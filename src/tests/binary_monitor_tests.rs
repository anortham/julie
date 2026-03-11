use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio_util::sync::CancellationToken;

/// Test that the monitor detects a binary newer than start_time
#[test]
fn test_binary_is_newer_detection() {
    // Create a temp file to act as our "binary"
    let dir = tempfile::tempdir().unwrap();
    let fake_binary = dir.path().join("julie-server");
    std::fs::write(&fake_binary, b"v1").unwrap();

    // Start time is "now" — binary mtime should be <= start_time
    let start_time = SystemTime::now();

    assert!(
        !crate::binary_monitor::is_binary_newer(&fake_binary, start_time),
        "Binary written before start_time should not be detected as newer"
    );

    // Sleep briefly, then "rebuild" the binary
    std::thread::sleep(Duration::from_millis(100));
    std::fs::write(&fake_binary, b"v2").unwrap();

    assert!(
        crate::binary_monitor::is_binary_newer(&fake_binary, start_time),
        "Binary written after start_time should be detected as newer"
    );
}

/// Test that the monitor triggers cancellation when binary changes
#[tokio::test]
async fn test_monitor_cancels_on_binary_change() {
    let dir = tempfile::tempdir().unwrap();
    let fake_binary = dir.path().join("julie-server");
    std::fs::write(&fake_binary, b"v1").unwrap();

    let ct = CancellationToken::new();
    let ct_clone = ct.clone();
    let binary_path = fake_binary.clone();

    // Start monitor with a fast poll interval for testing
    let handle = tokio::spawn(async move {
        crate::binary_monitor::run_monitor(
            binary_path,
            ct_clone,
            Duration::from_millis(50),
        )
        .await;
    });

    // Give monitor time to start and do its first (no-change) check
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!ct.is_cancelled(), "Should not be cancelled yet");

    // "Rebuild" the binary — sleep 500ms first to ensure mtime is clearly
    // after start_time (some filesystems have 1s mtime granularity)
    tokio::time::sleep(Duration::from_millis(500)).await;
    std::fs::write(&fake_binary, b"v2").unwrap();

    // Wait for monitor to detect and cancel (up to 500ms for poll + scheduling jitter)
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(ct.is_cancelled(), "Should be cancelled after binary change");

    let _ = handle.await;
}

/// Test that the monitor respects cancellation (doesn't hang)
#[tokio::test]
async fn test_monitor_stops_on_external_cancellation() {
    let dir = tempfile::tempdir().unwrap();
    let fake_binary = dir.path().join("julie-server");
    std::fs::write(&fake_binary, b"v1").unwrap();

    let ct = CancellationToken::new();
    let ct_clone = ct.clone();
    let binary_path = fake_binary.clone();

    let handle = tokio::spawn(async move {
        crate::binary_monitor::run_monitor(
            binary_path,
            ct_clone,
            Duration::from_millis(50),
        )
        .await;
    });

    // Cancel externally (simulating Ctrl+C shutdown)
    tokio::time::sleep(Duration::from_millis(100)).await;
    ct.cancel();

    // Monitor should exit promptly
    let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
    assert!(result.is_ok(), "Monitor should exit within 1s of cancellation");
}

use serial_test::serial;

/// Verify spawn() returns a handle when JULIE_NO_BINARY_WATCH is unset.
/// Uses #[serial] because it mutates process-global env vars.
#[tokio::test]
#[serial]
async fn test_spawn_returns_handle() {
    // Ensure env var is not set
    // SAFETY: test is #[serial] so no concurrent env mutation
    unsafe { std::env::remove_var("JULIE_NO_BINARY_WATCH") };

    let ct = CancellationToken::new();
    let handle = crate::binary_monitor::spawn(ct.clone());
    assert!(handle.is_some(), "spawn() should return a JoinHandle");

    // Clean up: cancel so the spawned task exits
    ct.cancel();
    if let Some(h) = handle {
        let _ = tokio::time::timeout(Duration::from_secs(1), h).await;
    }
}

/// Verify spawn() returns None when disabled.
/// Uses #[serial] because it mutates process-global env vars.
#[tokio::test]
#[serial]
async fn test_spawn_disabled_via_env() {
    // SAFETY: test is #[serial] so no concurrent env mutation
    unsafe { std::env::set_var("JULIE_NO_BINARY_WATCH", "1") };
    let ct = CancellationToken::new();
    let handle = crate::binary_monitor::spawn(ct);
    assert!(handle.is_none(), "spawn() should return None when disabled");
    // SAFETY: test is #[serial] so no concurrent env mutation
    unsafe { std::env::remove_var("JULIE_NO_BINARY_WATCH") };
}
