use crate::daemon::database::SearchToolCallRow;
use crate::dashboard::search_analysis::{
    aggregate_problems, analyze_tool_calls, canonical_key, episode_stats,
    extract_reformulation_pairs,
};

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

fn fast_search_row_with_hit(
    id: i64,
    session_id: &str,
    timestamp: i64,
    query: &str,
    top_hit_name: &str,
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
                            "name": top_hit_name,
                            "file": "src/dashboard/routes/search.rs",
                            "score": 8.0
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

#[test]
fn test_episode_stats_computes_outcome_breakdown() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        useful_action_row(2, "sess-a", 102),
        fast_search_row(3, "sess-b", 200, "search handler", "search handler"),
        fast_search_row(4, "sess-b", 203, "handler search", "handler search"),
        useful_action_row(5, "sess-b", 205),
        fast_search_row(6, "sess-c", 300, "stalled query", "stalled query"),
        fast_search_row(7, "sess-d", 400, "database pool", "database pool"),
        fast_search_row(8, "sess-d", 403, "centrality badge", "centrality badge"),
        useful_action_row(9, "sess-d", 405),
    ];

    let episodes = analyze_tool_calls(&rows);
    let stats = episode_stats(&episodes);

    assert_eq!(stats.total_episodes, 4);
    assert_eq!(stats.one_shot_count, 1);
    assert_eq!(stats.reformulation_count, 1);
    assert_eq!(stats.stall_count, 1);
    assert_eq!(stats.exploratory_count, 1);
    assert!((stats.first_try_rate - 0.25).abs() < 0.01);
}

// ---------------------------------------------------------------------------
// canonical_key tests
// ---------------------------------------------------------------------------

#[test]
fn test_canonical_key_unifies_naming_conventions() {
    assert_eq!(canonical_key("SearchHandler"), canonical_key("search_handler"));
    assert_eq!(canonical_key("search_handler"), canonical_key("search::handler"));
    assert_eq!(canonical_key("SearchHandler"), canonical_key("search handler"));
}

#[test]
fn test_canonical_key_drops_filler_tokens() {
    assert_eq!(canonical_key("find the handler"), canonical_key("handler"));
    assert_eq!(canonical_key("get search results"), canonical_key("search results"));
}

#[test]
fn test_canonical_key_sorts_alphabetically() {
    assert_eq!(canonical_key("handler search"), canonical_key("search handler"));
}

#[test]
fn test_canonical_key_empty_input() {
    assert!(canonical_key("").is_empty());
    assert!(canonical_key("the a an").is_empty());
}

// ---------------------------------------------------------------------------
// aggregate_problems tests
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_problems_excludes_terminal_successful_query() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        fast_search_row(2, "sess-a", 103, "handler search", "handler search"),
        useful_action_row(3, "sess-a", 105),
    ];

    let episodes = analyze_tool_calls(&rows);
    assert_eq!(episodes[0].outcome, "reformulation_converged");

    let problems = aggregate_problems(&episodes);

    let all_representative_queries: Vec<&str> = problems
        .iter()
        .map(|p| p.representative_query.as_str())
        .collect();
    assert!(
        !all_representative_queries.contains(&"handler search"),
        "terminal successful query should be excluded"
    );
}

#[test]
fn test_aggregate_problems_counts_once_per_episode() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        fast_search_row(2, "sess-a", 103, "handler", "handler"),
        fast_search_row(3, "sess-a", 106, "handler fixed", "handler fixed"),
        useful_action_row(4, "sess-a", 108),
    ];

    let episodes = analyze_tool_calls(&rows);
    let problems = aggregate_problems(&episodes);

    let handler_problem = problems.iter().find(|p| {
        canonical_key(&p.representative_query) == canonical_key("handler")
    });
    assert!(handler_problem.is_some());
    assert_eq!(handler_problem.unwrap().failure_count, 1);
}

#[test]
fn test_aggregate_problems_triage_ranking_problem() {
    let rows = vec![
        fast_search_row_with_hit(1, "sess-a", 100, "search handler", "search_handler"),
        fast_search_row(2, "sess-a", 103, "handler search", "handler search"),
        useful_action_row(3, "sess-a", 105),
    ];

    let episodes = analyze_tool_calls(&rows);
    let problems = aggregate_problems(&episodes);

    assert!(!problems.is_empty());
    assert_eq!(problems[0].triage_signal, "ranking_problem");
}

#[test]
fn test_aggregate_problems_triage_recall_gap() {
    let rows = vec![
        fast_search_row_with_hit(1, "sess-a", 100, "search handler", "wrong_symbol"),
        fast_search_row(2, "sess-a", 103, "handler search", "handler search"),
        useful_action_row(3, "sess-a", 105),
    ];

    let episodes = analyze_tool_calls(&rows);
    let problems = aggregate_problems(&episodes);

    assert!(!problems.is_empty());
    assert_eq!(problems[0].triage_signal, "recall_gap");
}

#[test]
fn test_aggregate_problems_empty_episodes() {
    let problems = aggregate_problems(&[]);
    assert!(problems.is_empty());
}

#[test]
fn test_aggregate_problems_stalled_episode_includes_all_queries() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        fast_search_row(2, "sess-a", 103, "different topic", "different topic"),
    ];

    let episodes = analyze_tool_calls(&rows);
    assert_eq!(episodes[0].outcome, "stalled");

    let problems = aggregate_problems(&episodes);
    assert_eq!(problems.len(), 2);
}

// ---------------------------------------------------------------------------
// extract_reformulation_pairs tests
// ---------------------------------------------------------------------------

#[test]
fn test_extract_reformulation_pairs_adjacent_transitions() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        fast_search_row(2, "sess-a", 103, "handler route", "handler route"),
        fast_search_row(3, "sess-a", 106, "route handler", "route handler"),
        useful_action_row(4, "sess-a", 108),
    ];

    let episodes = analyze_tool_calls(&rows);
    let pairs = extract_reformulation_pairs(&episodes);

    assert_eq!(pairs.len(), 2);
}

#[test]
fn test_extract_reformulation_pairs_deduplication() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "search handler", "search handler"),
        fast_search_row(2, "sess-a", 103, "handler route", "handler route"),
        useful_action_row(3, "sess-a", 105),
        fast_search_row(4, "sess-b", 200, "handler search", "handler search"),
        fast_search_row(5, "sess-b", 203, "route handler", "route handler"),
        useful_action_row(6, "sess-b", 205),
    ];

    let episodes = analyze_tool_calls(&rows);
    let pairs = extract_reformulation_pairs(&episodes);

    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].occurrences, 2);
}

#[test]
fn test_extract_reformulation_pairs_empty_episodes() {
    let pairs = extract_reformulation_pairs(&[]);
    assert!(pairs.is_empty());
}

#[test]
fn test_extract_reformulation_pairs_skips_non_reformulated() {
    let rows = vec![
        fast_search_row(1, "sess-a", 100, "handler", "handler"),
        useful_action_row(2, "sess-a", 102),
    ];

    let episodes = analyze_tool_calls(&rows);
    assert_eq!(episodes[0].outcome, "one_shot_success");

    let pairs = extract_reformulation_pairs(&episodes);
    assert!(pairs.is_empty());
}
