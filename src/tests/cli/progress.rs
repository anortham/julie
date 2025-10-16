/// Inline tests extracted from cli/progress.rs
///
/// Tests for ProgressReporter functionality including progress reporting,
/// rate calculation, and completion reporting.

use crate::cli::ProgressReporter;

#[test]
fn test_progress_reporter() {
    let mut reporter = ProgressReporter::new(100);

    // Simulate progress
    for i in (0..=100).step_by(10) {
        reporter.report(i);
    }

    reporter.complete(500);
}
