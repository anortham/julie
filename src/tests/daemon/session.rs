//! Tests for SessionTracker (daemon idle detection).

use crate::daemon::session::{SessionLifecyclePhase, SessionTracker};

#[test]
fn test_new_session_increments_count() {
    let tracker = SessionTracker::new();
    assert_eq!(tracker.active_count(), 0);

    let _id1 = tracker.add_session();
    assert_eq!(tracker.active_count(), 1);

    let _id2 = tracker.add_session();
    assert_eq!(tracker.active_count(), 2);
}

#[test]
fn test_remove_session_decrements_count() {
    let tracker = SessionTracker::new();
    let id1 = tracker.add_session();
    let id2 = tracker.add_session();
    assert_eq!(tracker.active_count(), 2);

    tracker.remove_session(&id1);
    assert_eq!(tracker.active_count(), 1);

    tracker.remove_session(&id2);
    assert_eq!(tracker.active_count(), 0);
}

#[test]
fn test_is_idle_when_no_sessions() {
    let tracker = SessionTracker::new();
    assert!(tracker.is_idle());
}

#[test]
fn test_not_idle_when_sessions_active() {
    let tracker = SessionTracker::new();
    let id = tracker.add_session();
    assert!(!tracker.is_idle());

    tracker.remove_session(&id);
    assert!(tracker.is_idle());
}

#[test]
fn test_remove_nonexistent_session_is_noop() {
    let tracker = SessionTracker::new();
    let id = tracker.add_session();
    assert_eq!(tracker.active_count(), 1);

    // Removing a session that doesn't exist should not panic or change count
    tracker.remove_session("nonexistent-uuid");
    assert_eq!(tracker.active_count(), 1);

    tracker.remove_session(&id);
    assert_eq!(tracker.active_count(), 0);
}

#[test]
fn test_session_ids_are_unique() {
    let tracker = SessionTracker::new();
    let id1 = tracker.add_session();
    let id2 = tracker.add_session();
    let id3 = tracker.add_session();

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
}

#[test]
fn test_new_session_starts_in_connecting_phase() {
    let tracker = SessionTracker::new();

    let session_id = tracker.add_session();

    assert_eq!(
        tracker.session_phase(&session_id),
        Some(SessionLifecyclePhase::Connecting)
    );
}

#[test]
fn test_session_phase_counts_follow_transitions() {
    let tracker = SessionTracker::new();
    let first_session = tracker.add_session();
    let second_session = tracker.add_session();

    tracker.set_phase(&first_session, SessionLifecyclePhase::Bound);
    tracker.set_phase(&second_session, SessionLifecyclePhase::Serving);

    let counts = tracker.phase_counts();
    assert_eq!(counts.connecting, 0);
    assert_eq!(counts.bound, 1);
    assert_eq!(counts.serving, 1);
    assert_eq!(counts.closing, 0);

    tracker.set_phase(&first_session, SessionLifecyclePhase::Closing);

    let counts = tracker.phase_counts();
    assert_eq!(counts.connecting, 0);
    assert_eq!(counts.bound, 0);
    assert_eq!(counts.serving, 1);
    assert_eq!(counts.closing, 1);
}
