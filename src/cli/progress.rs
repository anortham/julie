/// Progress reporting for long-running CLI operations
///
/// Provides user-friendly progress updates to stderr, leaving stdout
/// clean for piped output.

use std::time::Instant;

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Started { total_files: usize },
    Progress { processed: usize, total: usize },
    Completed { total: usize, duration_ms: u64 },
}

pub struct ProgressReporter {
    start_time: Instant,
    total_files: usize,
    last_report: Instant,
}

impl ProgressReporter {
    /// Create a new progress reporter
    pub fn new(total_files: usize) -> Self {
        eprintln!("🚀 Starting extraction: {} files", total_files);
        let now = Instant::now();
        Self {
            start_time: now,
            total_files,
            last_report: now,
        }
    }

    /// Report progress (throttled to avoid spam)
    pub fn report(&mut self, processed: usize) {
        // Throttle: only report every 100ms
        let now = Instant::now();
        if now.duration_since(self.last_report).as_millis() < 100 && processed < self.total_files
        {
            return;
        }
        self.last_report = now;

        let elapsed = self.start_time.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            processed as f64 / elapsed
        } else {
            0.0
        };

        let pct = if self.total_files > 0 {
            (processed as f64 / self.total_files as f64 * 100.0) as u32
        } else {
            0
        };

        eprintln!(
            "⚡ Progress: {}/{} ({}%) - {:.0} files/sec",
            processed, self.total_files, pct, rate
        );
    }

    /// Report completion
    pub fn complete(&self, total_symbols: usize) {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let file_rate = if elapsed > 0.0 {
            self.total_files as f64 / elapsed
        } else {
            0.0
        };

        eprintln!(
            "✅ Extraction complete: {} symbols from {} files in {:.2}s ({:.0} files/sec)",
            total_symbols, self.total_files, elapsed, file_rate
        );
    }

    /// Report error
    pub fn error(&self, message: &str) {
        eprintln!("❌ Error: {}", message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_reporter() {
        let mut reporter = ProgressReporter::new(100);

        // Simulate progress
        for i in (0..=100).step_by(10) {
            reporter.report(i);
        }

        reporter.complete(500);
    }
}
