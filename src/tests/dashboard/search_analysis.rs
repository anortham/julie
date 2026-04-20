use crate::daemon::database::SearchToolCallRow;
use crate::dashboard::search_analysis::analyze_tool_calls;

fn fast_search_row(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
    normalized_query: &str,
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
                "normalized_query": normalized_query,
                "intent": "code_investigation",
                "search_target": "definitions",
                "trace": {
                    "top_hits": [
                        {
                            "name": "search_handler",
                            "file": "src/dashboard/routes/search.rs"
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
