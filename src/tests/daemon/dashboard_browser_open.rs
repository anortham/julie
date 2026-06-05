//! Dashboard browser auto-open suppression (test/CI guard).
//!
//! `julie-daemon` auto-opens the dashboard in a browser on startup. The test
//! suite spawns REAL daemons — directly via `julie-daemon start` and indirectly
//! via the adapter's `spawn_daemon` — so without a guard every daemon-spawning
//! test pops a browser window (dozens during a full run). These tests pin the
//! suppression policy: any of `NEXTEST` / `CI` / `JULIE_NO_BROWSER` suppresses
//! the auto-open, while an interactive `julie daemon` (none set) still opens it.

use crate::daemon::app::{browser_open_suppressed_from, dashboard_browser_open_suppressed_by_env};

/// The pure policy: suppress iff ANY of the test/CI/opt-out signals is present.
#[test]
fn test_browser_open_suppression_policy_truth_table() {
    // No signal → the interactive auto-open is allowed.
    assert!(
        !browser_open_suppressed_from(false, false, false),
        "with no test/CI/opt-out signal, a human-launched daemon must still auto-open the dashboard"
    );

    // Any single signal → suppressed.
    assert!(
        browser_open_suppressed_from(true, false, false),
        "NEXTEST (cargo nextest / cargo xtask test) must suppress the auto-open"
    );
    assert!(
        browser_open_suppressed_from(false, true, false),
        "CI must suppress the auto-open"
    );
    assert!(
        browser_open_suppressed_from(false, false, true),
        "JULIE_NO_BROWSER must suppress the auto-open"
    );

    // Combinations → still suppressed.
    assert!(browser_open_suppressed_from(true, true, true));
    assert!(browser_open_suppressed_from(true, false, true));
}

/// End-to-end guard against the actual regression: this test runs under
/// `cargo nextest` (which sets `NEXTEST`) or in `CI`, so the env reader MUST
/// report suppression — i.e. a daemon spawned during this very run will NOT pop
/// a browser window. Guarded on the precondition so the test is also correct
/// under a plain `cargo test` invocation (where neither var is set).
#[test]
fn test_dashboard_open_suppressed_under_test_runner() {
    let under_runner =
        std::env::var_os("NEXTEST").is_some() || std::env::var_os("CI").is_some();
    if under_runner {
        assert!(
            dashboard_browser_open_suppressed_by_env(),
            "under the test runner / CI the dashboard browser auto-open MUST be suppressed — \
             this is exactly what stops the browser-window flood while tests run"
        );
    }
}
