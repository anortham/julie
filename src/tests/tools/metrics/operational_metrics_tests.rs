use crate::tools::metrics::operational;
use crate::tools::metrics::session::{SessionMetrics, ToolKind};
use std::sync::Arc;

#[test]
fn test_format_bytes() {
    assert_eq!(operational::format_bytes(0), "0B");
    assert_eq!(operational::format_bytes(500), "500B");
    assert_eq!(operational::format_bytes(1024), "1.0KB");
    assert_eq!(operational::format_bytes(1_048_576), "1.0MB");
    assert_eq!(operational::format_bytes(52_000), "50.8KB");
    assert_eq!(operational::format_bytes(1_073_741_824), "1.0GB");
}

#[test]
fn test_percentile_95_basic() {
    let mut durations: Vec<f64> = (1..=20).map(|i| i as f64).collect();
    let p95 = operational::percentile_95(&mut durations);
    assert!(p95 >= 19.0 && p95 <= 20.0, "p95 was {}", p95);
}

#[test]
fn test_percentile_95_single() {
    let mut durations = vec![5.0];
    assert_eq!(operational::percentile_95(&mut durations), 5.0);
}

#[test]
fn test_percentile_95_empty() {
    let mut durations: Vec<f64> = vec![];
    assert_eq!(operational::percentile_95(&mut durations), 0.0);
}

#[test]
fn test_format_session_empty() {
    let metrics = SessionMetrics::new();
    let output = operational::format_session_from_metrics(&metrics);
    assert!(output.contains("Session Metrics"));
    assert!(output.contains("0 calls"));
    assert!(output.contains("NOT injected"));
}

#[test]
fn test_format_session_with_data() {
    let metrics = Arc::new(SessionMetrics::new());
    metrics.record(ToolKind::FastSearch, 4100, 52000, 1200);
    metrics.record(ToolKind::FastSearch, 3200, 48000, 800);
    metrics.record(ToolKind::DeepDive, 8300, 15000, 2000);

    let output = operational::format_session_from_metrics(&metrics);
    assert!(
        output.contains("fast_search"),
        "should contain fast_search: {}",
        output
    );
    assert!(
        output.contains("2 calls"),
        "should show 2 calls: {}",
        output
    );
    assert!(
        output.contains("deep_dive"),
        "should contain deep_dive: {}",
        output
    );
    assert!(output.contains("1 calls"), "should show 1 call: {}", output);
    assert!(
        output.contains("NOT injected"),
        "should contain NOT injected: {}",
        output
    );
    assert!(
        output.contains("3 calls"),
        "totals should show 3 calls: {}",
        output
    );
}

#[test]
fn test_format_history_output() {
    use crate::database::{HistorySummary, ToolCallSummary};
    use std::collections::HashMap;

    let history = HistorySummary {
        session_count: 5,
        total_calls: 100,
        total_source_bytes: 2_000_000,
        total_output_bytes: 50_000,
        per_tool: vec![
            ToolCallSummary {
                tool_name: "fast_search".to_string(),
                call_count: 60,
                avg_duration_ms: 4.2,
                total_source_bytes: 1_200_000,
                total_output_bytes: 30_000,
            },
            ToolCallSummary {
                tool_name: "deep_dive".to_string(),
                call_count: 40,
                avg_duration_ms: 8.1,
                total_source_bytes: 800_000,
                total_output_bytes: 20_000,
            },
        ],
        durations_by_tool: HashMap::from([
            ("fast_search".to_string(), vec![3.0, 4.0, 5.0, 6.0, 12.0]),
            ("deep_dive".to_string(), vec![5.0, 8.0, 10.0, 15.0]),
        ]),
    };
    let output = operational::format_history_output(&history);
    assert!(
        output.contains("Historical Metrics"),
        "missing header: {}",
        output
    );
    assert!(
        output.contains("5 sessions"),
        "missing session count: {}",
        output
    );
    assert!(
        output.contains("fast_search"),
        "missing fast_search: {}",
        output
    );
    assert!(output.contains("p95"), "missing p95: {}", output);
    assert!(
        output.contains("NOT injected"),
        "missing NOT injected: {}",
        output
    );
    assert!(
        output.contains("1.9MB"),
        "should show ~1.9MB not injected: {}",
        output
    );
}
