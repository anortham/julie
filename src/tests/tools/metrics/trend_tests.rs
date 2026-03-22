use crate::daemon::database::{CodehealthSnapshot, CodehealthSnapshotRow};
use crate::tools::metrics::trend;

fn make_snapshot_row(id: i64, security_high: i32, total_symbols: i64) -> CodehealthSnapshotRow {
    CodehealthSnapshotRow {
        id,
        workspace_id: "ws1".to_string(),
        timestamp: 1_740_000_000 + id * 86400,
        total_symbols,
        total_files: 100,
        security_high,
        security_medium: 25,
        security_low: 100,
        change_high: 8,
        change_medium: 30,
        change_low: 200,
        symbols_tested: 180,
        symbols_untested: 47,
        avg_centrality: Some(0.42),
        max_centrality: Some(0.95),
    }
}

fn make_current_snapshot(security_high: i32, total_symbols: i64) -> CodehealthSnapshot {
    CodehealthSnapshot {
        total_symbols,
        total_files: 100,
        security_high,
        security_medium: 20,
        security_low: 90,
        change_high: 6,
        change_medium: 25,
        change_low: 180,
        symbols_tested: 200,
        symbols_untested: 30,
        avg_centrality: Some(0.45),
        max_centrality: Some(0.98),
    }
}

// ---------------------------------------------------------------------------
// format_delta
// ---------------------------------------------------------------------------

#[test]
fn test_format_delta_decrease() {
    let s = trend::format_delta(14, 9);
    assert!(s.contains("14 → 9"), "got: {s}");
    assert!(s.contains("↓5"), "got: {s}");
    assert!(s.contains("-36%"), "got: {s}");
}

#[test]
fn test_format_delta_increase() {
    let s = trend::format_delta(9, 14);
    assert!(s.contains("9 → 14"), "got: {s}");
    assert!(s.contains("↑5"), "got: {s}");
    assert!(s.contains("+56%"), "got: {s}");
}

#[test]
fn test_format_delta_no_change() {
    let s = trend::format_delta(14, 14);
    assert!(
        s.contains("no change") || s.contains("unchanged"),
        "got: {s}"
    );
}

#[test]
fn test_format_delta_from_zero() {
    // old=0, new=5: can't compute a percentage from zero
    let s = trend::format_delta(0, 5);
    assert!(s.contains("0 → 5"), "got: {s}");
}

// ---------------------------------------------------------------------------
// format_comparison
// ---------------------------------------------------------------------------

#[test]
fn test_format_comparison_shows_security_high_delta() {
    // security_high: previous=14, current=9 → ↓5
    let current = make_current_snapshot(9, 7500);
    let previous = make_snapshot_row(1, 14, 7306);

    let output = trend::format_comparison(&current, &previous);
    // Must show the security_high arrow somewhere
    assert!(
        output.contains("14 → 9") || output.contains("↓5"),
        "output: {output}"
    );
}

#[test]
fn test_format_comparison_shows_symbol_count() {
    let current = make_current_snapshot(9, 7500);
    let previous = make_snapshot_row(1, 14, 7306);

    let output = trend::format_comparison(&current, &previous);
    // total_symbols: 7306 → 7500 (↑194)
    assert!(
        output.contains("7306") && output.contains("7500"),
        "output: {output}"
    );
}

#[test]
fn test_format_comparison_has_multiple_sections() {
    let current = make_current_snapshot(9, 7500);
    let previous = make_snapshot_row(1, 14, 7306);

    let output = trend::format_comparison(&current, &previous);
    let line_count = output.lines().count();
    assert!(line_count >= 3, "expected ≥3 lines, got {line_count}: {output}");
}

// ---------------------------------------------------------------------------
// format_trend_table
// ---------------------------------------------------------------------------

#[test]
fn test_format_trend_table_shows_all_rows() {
    let rows = vec![
        make_snapshot_row(3, 12, 7400),
        make_snapshot_row(2, 13, 7350),
        make_snapshot_row(1, 14, 7300),
    ];
    let output = trend::format_trend_table(&rows);
    // Header + separator + 3 data rows = at least 5 lines
    let line_count = output.lines().filter(|l: &&str| !l.is_empty()).count();
    assert!(line_count >= 3, "expected ≥3 non-empty lines, got {line_count}: {output}");
    // All three symbol counts should appear
    assert!(output.contains("7400"), "output: {output}");
    assert!(output.contains("7350"), "output: {output}");
    assert!(output.contains("7300"), "output: {output}");
}

#[test]
fn test_format_trend_table_empty_input() {
    let output = trend::format_trend_table(&[]);
    // Should be a short "no history" message, not a crash
    assert!(
        !output.is_empty(),
        "expected some output even for empty input"
    );
    assert!(output.len() < 200, "empty input should be brief: {output}");
}

#[test]
fn test_format_trend_table_single_row() {
    let rows = vec![make_snapshot_row(1, 14, 7306)];
    let output = trend::format_trend_table(&rows);
    assert!(output.contains("7306"), "output: {output}");
}
