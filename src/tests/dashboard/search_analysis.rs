use crate::daemon::database::SearchToolCallRow;
use crate::dashboard::search_analysis::{analyze_tool_calls, compute_flags, compute_summary};

fn fast_search_row(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
    normalized_query: &str,
) -> SearchToolCallRow {
    fast_search_row_in_workspace(id, session_id, "ws1", timestamp, query, normalized_query)
}

fn fast_search_row_in_workspace(
    id: i64,
    session_id: &str,
    workspace_id: &str,
    timestamp: i64,
    query: &str,
    normalized_query: &str,
) -> SearchToolCallRow {
    SearchToolCallRow {
        id,
        workspace_id: workspace_id.to_string(),
        session_id: session_id.to_string(),
        timestamp,
        tool_name: "fast_search".to_string(),
        metadata: Some(
            serde_json::json!({
                "query": query,
                "normalized_query": normalized_query,
                "intent": "code_investigation",
                "search_target": "definitions",
                "trace": {
                    "strategy": "definitions_first",
                    "result_count": 5,
                    "returned_hit_count": 5,
                    "relaxed": false,
                    "top_hits": [
                        {
                            "name": "search_handler",
                            "file": "src/dashboard/routes/search.rs",
                            "score": 12.5
                        }
                    ]
                }
            })
            .to_string(),
        ),
    }
}

fn fast_search_row_no_trace(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
) -> SearchToolCallRow {
    SearchToolCallRow {
        id,
        workspace_id: "ws1".to_string(),
        session_id: session_id.to_string(),
        timestamp,
        tool_name: "fast_search".to_string(),
        metadata: Some(
            serde_json::json!({
                "query": query,
                "normalized_query": query,
                "intent": "code_investigation",
                "search_target": "definitions"
            })
            .to_string(),
        ),
    }
}

fn fast_search_row_zero_hits(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
) -> SearchToolCallRow {
    SearchToolCallRow {
        id,
        workspace_id: "ws1".to_string(),
        session_id: session_id.to_string(),
        timestamp,
        tool_name: "fast_search".to_string(),
        metadata: Some(
            serde_json::json!({
                "query": query,
                "normalized_query": query,
                "intent": "code_investigation",
                "search_target": "definitions",
                "trace": {
                    "strategy": "definitions_first",
                    "result_count": 0,
                    "returned_hit_count": 0,
                    "relaxed": false,
                    "top_hits": []
                }
            })
            .to_string(),
        ),
    }
}

fn fast_search_row_low_score(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
) -> SearchToolCallRow {
    SearchToolCallRow {
        id,
        workspace_id: "ws1".to_string(),
        session_id: session_id.to_string(),
        timestamp,
        tool_name: "fast_search".to_string(),
        metadata: Some(
            serde_json::json!({
                "query": query,
                "normalized_query": query,
                "intent": "code_investigation",
                "search_target": "definitions",
                "trace": {
                    "strategy": "definitions_first",
                    "result_count": 3,
                    "returned_hit_count": 3,
                    "relaxed": false,
                    "top_hits": [
                        {
                            "name": "something",
                            "file": "src/lib.rs",
                            "score": 2.0
                        }
                    ]
                }
            })
            .to_string(),
        ),
    }
}

fn fast_search_row_relaxed(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
) -> SearchToolCallRow {
    SearchToolCallRow {
        id,
        workspace_id: "ws1".to_string(),
        session_id: session_id.to_string(),
        timestamp,
        tool_name: "fast_search".to_string(),
        metadata: Some(
            serde_json::json!({
                "query": query,
                "normalized_query": query,
                "intent": "code_investigation",
                "search_target": "definitions",
                "trace": {
                    "strategy": "definitions_first",
                    "result_count": 1,
                    "returned_hit_count": 1,
                    "relaxed": true,
                    "top_hits": [
                        {
                            "name": "handler",
                            "file": "src/lib.rs",
                            "score": 6.0
                        }
                    ]
                }
            })
            .to_string(),
        ),
    }
}

fn useful_action_row(id: i64, session_id: &str, timestamp: i64) -> SearchToolCallRow {
    SearchToolCallRow {
        id,
        workspace_id: "ws1".to_string(),
        session_id: session_id.to_string(),
        timestamp,
        tool_name: "deep_dive".to_string(),
        metadata: Some(
            serde_json::json!({
                "target": {
                    "target_symbol_name": "search_handler",
                    "target_file_path": "src/dashboard/routes/search.rs"
                }
            })
            .to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Episode builder tests (kept from prior iteration)
// ---------------------------------------------------------------------------

#[test]
fn test_search_analysis_groups_nearby_searches_into_one_episode() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        fast_search_row(2, "sess-a", 105, "centrality badge", "centrality badge"),
        useful_action_row(3, "sess-a", 107),
    ];

    let episodes = analyze_tool_calls(&rows);

    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].search_count, 2);
    assert_eq!(episodes[0].downstream_tool.as_deref(), Some("deep_dive"));
    assert_eq!(episodes[0].outcome, "exploratory_success");
}

#[test]
fn test_search_analysis_flags_reformulation_when_queries_overlap_and_converge() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        fast_search_row(2, "sess-a", 103, "handler search", "handler search"),
        useful_action_row(3, "sess-a", 105),
    ];

    let episodes = analyze_tool_calls(&rows);

    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].outcome, "reformulation_converged");
    assert!(episodes[0].suspicious);
}

#[test]
fn test_search_analysis_splits_episodes_after_non_search_boundary() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        useful_action_row(2, "sess-a", 101),
        fast_search_row(3, "sess-a", 102, "search compare", "search compare"),
    ];

    let episodes = analyze_tool_calls(&rows);

    assert_eq!(episodes.len(), 2);
    assert_eq!(episodes[0].outcome, "one_shot_success");
    assert_eq!(episodes[1].outcome, "stalled");
}

#[test]
fn test_search_analysis_splits_on_workspace_boundary() {
    let rows = vec![
        fast_search_row_in_workspace(1, "sess-a", "ws1", 100, "handler", "handler"),
        fast_search_row_in_workspace(2, "sess-a", "ws2", 103, "handler", "handler"),
    ];

    let episodes = analyze_tool_calls(&rows);

    assert_eq!(episodes.len(), 2);
    assert_eq!(episodes[0].workspace_id, "ws1");
    assert_eq!(episodes[1].workspace_id, "ws2");
}

#[test]
fn test_search_analysis_workspace_boundary_on_non_search_tool() {
    let rows = vec![
        fast_search_row_in_workspace(1, "sess-a", "ws1", 100, "handler", "handler"),
        SearchToolCallRow {
            id: 2,
            workspace_id: "ws2".to_string(),
            session_id: "sess-a".to_string(),
            timestamp: 102,
            tool_name: "deep_dive".to_string(),
            metadata: None,
        },
    ];

    let episodes = analyze_tool_calls(&rows);

    assert_eq!(episodes.len(), 1);
    assert_eq!(episodes[0].outcome, "stalled");
    assert_eq!(episodes[0].workspace_id, "ws1");
}

#[test]
fn test_search_analysis_null_trace_produces_none_fields() {
    let rows = vec![
        fast_search_row_no_trace(1, "sess-a", 100, "handler"),
        useful_action_row(2, "sess-a", 102),
    ];

    let episodes = analyze_tool_calls(&rows);

    assert_eq!(episodes.len(), 1);
    let query = &episodes[0].queries[0];
    assert!(query.top_hit_score.is_none());
    assert!(query.result_count.is_none());
    assert!(query.strategy.is_none());
    assert!(query.relaxed.is_none());
    assert!(query.top_hit_name.is_none());
}

#[test]
fn test_search_analysis_trace_fields_populated() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        useful_action_row(2, "sess-a", 102),
    ];

    let episodes = analyze_tool_calls(&rows);

    let query = &episodes[0].queries[0];
    assert!((query.top_hit_score.unwrap() - 12.5).abs() < 0.01);
    assert_eq!(query.result_count, Some(5));
    assert_eq!(query.strategy.as_deref(), Some("definitions_first"));
    assert_eq!(query.relaxed, Some(false));
}

// ---------------------------------------------------------------------------
// compute_flags tests
// ---------------------------------------------------------------------------

#[test]
fn test_compute_flags_zero_hits() {
    let rows = vec![fast_search_row_zero_hits(1, "sess-a", 100, "nonexistent")];
    let mut episodes = analyze_tool_calls(&rows);
    compute_flags(&mut episodes[0]);

    assert!(episodes[0].flags.contains(&"zero_hits".to_string()));
    assert!(episodes[0].flags.contains(&"no_follow_up".to_string()));
}

#[test]
fn test_compute_flags_no_follow_up() {
    let rows = vec![fast_search_row(1, "sess-a", 100, "handler", "handler")];
    let mut episodes = analyze_tool_calls(&rows);
    compute_flags(&mut episodes[0]);

    assert!(episodes[0].flags.contains(&"no_follow_up".to_string()));
    assert!(!episodes[0].flags.contains(&"zero_hits".to_string()));
}

#[test]
fn test_compute_flags_repeat_query() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        fast_search_row(2, "sess-a", 103, "handler search", "handler search"),
        useful_action_row(3, "sess-a", 105),
    ];
    let mut episodes = analyze_tool_calls(&rows);
    compute_flags(&mut episodes[0]);

    assert!(episodes[0].flags.contains(&"repeat_query".to_string()));
    assert!(!episodes[0].flags.contains(&"no_follow_up".to_string()));
}

#[test]
fn test_compute_flags_low_score() {
    let rows = vec![
        fast_search_row_low_score(1, "sess-a", 100, "obscure thing"),
        useful_action_row(2, "sess-a", 102),
    ];
    let mut episodes = analyze_tool_calls(&rows);
    compute_flags(&mut episodes[0]);

    assert!(episodes[0].flags.contains(&"low_score".to_string()));
}

#[test]
fn test_compute_flags_relaxed() {
    let rows = vec![
        fast_search_row_relaxed(1, "sess-a", 100, "vague query"),
        useful_action_row(2, "sess-a", 102),
    ];
    let mut episodes = analyze_tool_calls(&rows);
    compute_flags(&mut episodes[0]);

    assert!(episodes[0].flags.contains(&"relaxed".to_string()));
}

#[test]
fn test_compute_flags_clean_episode() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        useful_action_row(2, "sess-a", 102),
    ];
    let mut episodes = analyze_tool_calls(&rows);
    compute_flags(&mut episodes[0]);

    assert!(episodes[0].flags.is_empty());
}

// ---------------------------------------------------------------------------
// compute_summary tests
// ---------------------------------------------------------------------------

#[test]
fn test_compute_summary_empty_episodes() {
    let summary = compute_summary(&[]);
    assert_eq!(summary.episode_count, 0);
    assert_eq!(summary.zero_hit_count, 0);
    assert!(summary.median_top_score.is_none());
}

#[test]
fn test_compute_summary_counts() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        useful_action_row(2, "sess-a", 102),
        fast_search_row_zero_hits(3, "sess-b", 200, "nonexistent"),
        fast_search_row(4, "sess-c", 300, "search handler", "search handler"),
        fast_search_row(5, "sess-c", 303, "handler search", "handler search"),
        useful_action_row(6, "sess-c", 305),
    ];

    let mut episodes = analyze_tool_calls(&rows);
    for ep in &mut episodes {
        compute_flags(ep);
    }
    let summary = compute_summary(&episodes);

    assert_eq!(summary.episode_count, 3);
    assert_eq!(summary.zero_hit_count, 1);
    assert_eq!(summary.repeat_query_count, 1);
    assert!(summary.median_top_score.is_some());
}
