//! Tests for SessionTracker (daemon idle detection).

use crate::daemon::session::{SessionLifecyclePhase, SessionTracker};
use std::time::{Duration, Instant};

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

#[test]
fn test_evict_idle_removes_only_stale_sessions() {
    let tracker = SessionTracker::new();
    let base = Instant::now();

    let stale = tracker.add_session();
    let fresh = tracker.add_session();

    // `stale` last did work at `base`; `fresh` was active 600s later.
    tracker.touch_session_at(&stale, base);
    tracker.touch_session_at(&fresh, base + Duration::from_secs(600));

    // "Now" is base+400s with a 300s idle threshold:
    //   stale: 400s idle  >= 300s  -> evict
    //   fresh: 0s idle (saturating) < 300s -> keep
    let evicted = tracker.evict_idle(base + Duration::from_secs(400), Duration::from_secs(300));

    assert_eq!(evicted, vec![stale.clone()]);
    assert_eq!(tracker.active_count(), 1);
    assert_eq!(
        tracker.session_phase(&fresh),
        Some(SessionLifecyclePhase::Connecting),
        "the freshly-active session must survive eviction"
    );
    assert!(
        tracker.session_phase(&stale).is_none(),
        "the stale session must be gone"
    );
}

#[test]
fn test_evict_idle_returns_empty_when_none_stale() {
    let tracker = SessionTracker::new();
    let base = Instant::now();
    let id = tracker.add_session();
    tracker.touch_session_at(&id, base);

    // Threshold not yet exceeded (100s idle < 300s).
    let evicted = tracker.evict_idle(base + Duration::from_secs(100), Duration::from_secs(300));

    assert!(evicted.is_empty());
    assert_eq!(tracker.active_count(), 1);
}

#[test]
fn test_touch_session_nonexistent_returns_false() {
    let tracker = SessionTracker::new();
    assert!(!tracker.touch_session("does-not-exist"));
}
