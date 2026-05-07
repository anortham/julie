//! Tests for atomic write behavior of `write_daemon_state`.
//!
//! Verifies that concurrent readers never observe a partial (truncated) state
//! string. Every read must return either the empty string (file not yet created)
//! or one of the complete valid state strings.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crate::daemon::lifecycle::write_daemon_state;

    /// The set of valid complete state strings the writer cycles through.
    const STATES: &[&str] = &["ready", "draining", "stopping"];

    /// Every read of the state file must return either an empty string (file not
    /// yet created or transiently absent during rename) or one of the three full
    /// state strings.  A partial like `"rea"` or `"dra"` is a bug.
    #[test]
    fn test_write_daemon_state_no_partial_reads() {
        let dir = tempfile::TempDir::new().unwrap();
        let state_path = dir.path().join("daemon.state");

        let done = Arc::new(AtomicBool::new(false));

        // --- Reader thread ---
        // Reads in a tight loop for up to 600 ms.  Each read result must be
        // in the allowed set: empty (file absent/not-yet-created) or a full
        // state string.
        let reader_path = state_path.clone();
        let reader_done = Arc::clone(&done);
        let reader = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_millis(600);
            while Instant::now() < deadline && !reader_done.load(Ordering::Relaxed) {
                let content = std::fs::read_to_string(&reader_path).unwrap_or_default();
                let allowed = content.is_empty()
                    || STATES.iter().any(|&s| s == content.as_str());
                assert!(
                    allowed,
                    "Partial or unexpected state read: {:?} — concurrent readers must \
                     never observe a truncated state file",
                    content
                );
            }
        });

        // --- Writer thread ---
        // Writes 1000 times, cycling through the three state strings.
        let writer_path = state_path.clone();
        let writer = std::thread::spawn(move || {
            for i in 0..1000usize {
                let state = STATES[i % STATES.len()];
                write_daemon_state(&writer_path, state);
            }
        });

        writer.join().expect("writer thread panicked");
        done.store(true, Ordering::Relaxed);
        reader.join().expect("reader thread panicked");

        // Final sanity: the file must contain a valid state after all writes.
        let final_content = std::fs::read_to_string(&state_path).unwrap_or_default();
        assert!(
            STATES.iter().any(|&s| s == final_content.as_str()),
            "Final state file content is not a valid state: {:?}",
            final_content
        );
    }
}
