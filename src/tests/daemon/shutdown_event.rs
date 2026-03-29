use std::path::PathBuf;
use std::sync::Arc;

use crate::daemon::shutdown_event::{self, ShutdownEvent};
use crate::paths::DaemonPaths;

#[test]
fn test_shutdown_event_name_deterministic() {
    let paths = DaemonPaths::with_home(PathBuf::from(r"C:\Users\test\.julie"));
    let name1 = paths.daemon_shutdown_event();
    let name2 = paths.daemon_shutdown_event();
    assert_eq!(name1, name2, "Event name must be deterministic");
    assert!(
        name1.starts_with(r"Local\julie-daemon-shutdown-"),
        "Event name must use Local\\ prefix: {}",
        name1
    );
}

#[test]
fn test_shutdown_event_name_isolation() {
    let paths_a = DaemonPaths::with_home(PathBuf::from(r"C:\Users\alice\.julie"));
    let paths_b = DaemonPaths::with_home(PathBuf::from(r"C:\Users\bob\.julie"));
    assert_ne!(
        paths_a.daemon_shutdown_event(),
        paths_b.daemon_shutdown_event(),
        "Different homes must produce different event names"
    );
}

#[test]
fn test_create_and_signal_event() {
    // Use a unique name to avoid colliding with a real daemon
    let event_name = format!("Local\\julie-test-shutdown-{}", std::process::id());

    let event = Arc::new(ShutdownEvent::create(&event_name).expect("create event"));

    // Spawn a thread that waits on the event
    let event_clone = Arc::clone(&event);
    let waiter = std::thread::spawn(move || {
        event_clone.wait();
        true
    });

    // Give the waiter thread time to block
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Signal the event
    let signaled = shutdown_event::signal_shutdown(&event_name).expect("signal_shutdown");
    assert!(signaled, "signal_shutdown should return true");

    // Waiter should complete
    let result = waiter.join().expect("waiter thread panicked");
    assert!(result, "waiter should have been woken");
}

#[test]
fn test_signal_nonexistent_event() {
    let result = shutdown_event::signal_shutdown("Local\\julie-test-nonexistent-event-99999999");
    assert_eq!(
        result.unwrap(),
        false,
        "signal_shutdown on missing event should return Ok(false)"
    );
}
