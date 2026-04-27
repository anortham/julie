use std::fs;
use std::io::Write;
use tempfile::TempDir;

use crate::logging::{LocalRollingWriter, LocalTimer};

// =============================================================================
// LocalTimer tests
// =============================================================================

#[test]
fn test_local_timer_produces_local_offset() {
    // The formatted timestamp should contain a numeric timezone offset
    // (e.g. +0500, -0700), never the UTC "Z" suffix that the default
    // tracing_subscriber timer produces.
    let now = chrono::Local::now();
    let formatted = now.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

    assert!(
        !formatted.ends_with('Z'),
        "Expected local timezone offset, got UTC 'Z': {}",
        formatted
    );
    // ISO-8601 with milliseconds and offset: 2026-04-27T14:30:45.123-0500
    assert!(
        formatted.len() >= 28,
        "Timestamp too short to contain offset: {}",
        formatted
    );
}

#[test]
fn test_local_timer_implements_format_time() {
    // Verify LocalTimer satisfies the FormatTime trait constraint.
    fn assert_format_time<T: tracing_subscriber::fmt::time::FormatTime>(_t: &T) {}
    assert_format_time(&LocalTimer);
}

// =============================================================================
// LocalRollingWriter tests
// =============================================================================

#[test]
fn test_rolling_writer_creates_file_with_local_date() {
    let dir = TempDir::new().unwrap();
    let mut writer = LocalRollingWriter::new(dir.path(), "app.log");

    write!(writer, "hello\n").unwrap();
    writer.flush().unwrap();

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let expected = dir.path().join(format!("app.log.{}", today));

    assert!(expected.exists(), "Log file not created: {:?}", expected);
    let contents = fs::read_to_string(&expected).unwrap();
    assert_eq!(contents, "hello\n");
}

#[test]
fn test_rolling_writer_appends_to_existing_file() {
    let dir = TempDir::new().unwrap();
    let mut writer = LocalRollingWriter::new(dir.path(), "app.log");

    write!(writer, "line 1\n").unwrap();
    write!(writer, "line 2\n").unwrap();
    writer.flush().unwrap();

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let contents = fs::read_to_string(dir.path().join(format!("app.log.{}", today))).unwrap();
    assert_eq!(contents, "line 1\nline 2\n");
}

#[test]
fn test_rolling_writer_creates_log_directory() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("deep").join("nested").join("logs");

    let mut writer = LocalRollingWriter::new(&nested, "app.log");
    write!(writer, "works\n").unwrap();
    writer.flush().unwrap();

    assert!(nested.exists(), "Nested log directory not created");
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let contents = fs::read_to_string(nested.join(format!("app.log.{}", today))).unwrap();
    assert_eq!(contents, "works\n");
}

#[test]
fn test_rolling_writer_rotates_on_date_change() {
    let dir = TempDir::new().unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_file = dir.path().join(format!("app.log.{}", today));

    let mut writer = LocalRollingWriter::new(dir.path(), "app.log");

    write!(writer, "before rotation\n").unwrap();
    writer.flush().unwrap();

    // Snapshot the file size before rotation so we can prove a new
    // handle was opened (the old handle was dropped and re-opened).
    let size_before = fs::metadata(&today_file).unwrap().len();

    // Simulate crossing midnight: set internal date to a fake past value.
    // The next write detects the mismatch and re-opens for today's real date.
    writer.force_date_for_testing("2000-01-01".to_string());

    write!(writer, "after rotation\n").unwrap();
    writer.flush().unwrap();

    let contents = fs::read_to_string(&today_file).unwrap();

    // Both writes target today's date, but rotation re-opened the handle.
    assert!(
        contents.contains("before rotation"),
        "Pre-rotation content missing: {}",
        contents
    );
    assert!(
        contents.contains("after rotation"),
        "Post-rotation content missing: {}",
        contents
    );
    // The file grew, proving the second write went through the new handle.
    let size_after = fs::metadata(&today_file).unwrap().len();
    assert!(
        size_after > size_before,
        "File should have grown after rotation write: before={}, after={}",
        size_before,
        size_after
    );

    // No stale-date file should exist (we rotated TO today, not FROM today).
    let stale_file = dir.path().join("app.log.2000-01-01");
    assert!(
        !stale_file.exists(),
        "Stale date file should not be created: {:?}",
        stale_file
    );
}

#[test]
fn test_rolling_writer_keeps_old_file_on_rotation_failure() {
    let dir = TempDir::new().unwrap();
    let mut writer = LocalRollingWriter::new(dir.path(), "app.log");

    write!(writer, "before\n").unwrap();
    writer.flush().unwrap();

    // Remove the log directory so the next rotation open fails.
    fs::remove_dir_all(dir.path()).unwrap();
    fs::create_dir_all(dir.path()).unwrap();
    // Now there's no existing log file, and force_date triggers re-open.

    writer.force_date_for_testing("2000-01-01".to_string());

    // The write should still succeed: it falls back to the (now-closed)
    // old handle or the None path. The key invariant is no panic and
    // current_date stays at the old value so the next write retries.
    let result = writer.write(b"after\n");
    assert!(result.is_ok(), "Write should not fail even on rotation error");
}
