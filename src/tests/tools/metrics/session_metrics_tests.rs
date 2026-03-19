use crate::tools::metrics::session::{SessionMetrics, ToolCallReport, ToolKind};
use std::sync::Arc;

#[test]
fn test_tool_kind_ordinal_covers_all_tools() {
    assert_eq!(ToolKind::FastSearch as u8, 0);
    assert_eq!(ToolKind::FastRefs as u8, 1);
    assert_eq!(ToolKind::GetSymbols as u8, 2);
    assert_eq!(ToolKind::DeepDive as u8, 3);
    assert_eq!(ToolKind::GetContext as u8, 4);
    assert_eq!(ToolKind::RenameSymbol as u8, 5);
    assert_eq!(ToolKind::ManageWorkspace as u8, 6);
    assert_eq!(ToolKind::QueryMetrics as u8, 7);
}

#[test]
fn test_tool_kind_from_name() {
    assert!(matches!(
        ToolKind::from_name("fast_search"),
        Some(ToolKind::FastSearch)
    ));
    assert!(matches!(
        ToolKind::from_name("deep_dive"),
        Some(ToolKind::DeepDive)
    ));
    assert!(ToolKind::from_name("nonexistent").is_none());
}

#[test]
fn test_tool_kind_name_roundtrip() {
    for i in 0..ToolKind::COUNT {
        // Create each kind by ordinal via from_name on the name
        let kind = match i {
            0 => ToolKind::FastSearch,
            1 => ToolKind::FastRefs,
            2 => ToolKind::GetSymbols,
            3 => ToolKind::DeepDive,
            4 => ToolKind::GetContext,
            5 => ToolKind::RenameSymbol,
            6 => ToolKind::ManageWorkspace,
            7 => ToolKind::QueryMetrics,
            _ => unreachable!(),
        };
        let name = kind.name();
        let roundtripped = ToolKind::from_name(name).unwrap();
        assert_eq!(roundtripped as u8, kind as u8);
    }
}

#[test]
fn test_session_metrics_new_starts_at_zero() {
    let metrics = SessionMetrics::new();
    assert_eq!(metrics.total_calls(), 0);
    assert_eq!(metrics.total_output_bytes(), 0);
    assert_eq!(metrics.total_source_bytes(), 0);
    assert!(!metrics.session_id.is_empty());
}

#[test]
fn test_session_metrics_record_increments_atomics() {
    let metrics = Arc::new(SessionMetrics::new());
    metrics.record(ToolKind::FastSearch, 1500, 200, 5000);

    assert_eq!(metrics.total_calls(), 1);
    assert_eq!(metrics.total_source_bytes(), 200);
    assert_eq!(metrics.total_output_bytes(), 5000);

    let tool = &metrics.per_tool[ToolKind::FastSearch as usize];
    assert_eq!(tool.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(
        tool.output_bytes
            .load(std::sync::atomic::Ordering::Relaxed),
        5000
    );
}

#[test]
fn test_session_metrics_multiple_tools() {
    let metrics = Arc::new(SessionMetrics::new());
    metrics.record(ToolKind::FastSearch, 1000, 100, 500);
    metrics.record(ToolKind::FastSearch, 2000, 300, 800);
    metrics.record(ToolKind::DeepDive, 5000, 1000, 2000);

    assert_eq!(metrics.total_calls(), 3);
    assert_eq!(metrics.total_source_bytes(), 1400);
    assert_eq!(metrics.total_output_bytes(), 3300);

    let search = &metrics.per_tool[ToolKind::FastSearch as usize];
    assert_eq!(search.calls.load(std::sync::atomic::Ordering::Relaxed), 2);

    let dive = &metrics.per_tool[ToolKind::DeepDive as usize];
    assert_eq!(dive.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn test_tool_call_report_empty() {
    let report = ToolCallReport::empty();
    assert_eq!(report.result_count, None);
    assert_eq!(report.source_bytes, None);
    assert_eq!(report.output_bytes, 0);
    assert_eq!(report.metadata, serde_json::Value::Null);
}

#[tokio::test]
async fn test_handler_has_session_metrics() {
    let handler = crate::handler::JulieServerHandler::new_for_test()
        .await
        .unwrap();
    assert_eq!(handler.session_metrics.total_calls(), 0);
    assert!(!handler.session_metrics.session_id.is_empty());
}
