use super::*;

#[tokio::test]
async fn test_all_dashboard_pages_return_200() {
    let state = test_state();
    let config = DashboardConfig::default();

    for path in [
        "/",
        "/projects",
        "/metrics",
        "/search",
        "/search/analysis",
        "/search/compare",
    ] {
        let app = create_router(state.clone(), config.clone()).unwrap();
        let response = app
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.status().as_u16(),
            200,
            "GET {} returned {}",
            path,
            response.status()
        );
    }
}

#[tokio::test]
async fn test_metrics_page_renders_aggregated_tool_history() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .upsert_workspace("ready-b", "/proj/b", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-b", 8, 1, None, None, None)
        .unwrap();

    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "fast_search",
            12.0,
            Some(3),
            Some(12_000),
            Some(800),
            true,
            None,
        )
        .unwrap();
    daemon_db
        .insert_tool_call(
            "ready-b",
            "session-b",
            "deep_dive",
            25.0,
            Some(1),
            Some(8_000),
            Some(1_200),
            false,
            None,
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics?days=7")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("All Workspaces"));
    assert!(html.contains("Success Rate"));
    assert!(html.contains("Context Saved"));
    assert!(html.contains("fast_search"));
    assert!(html.contains("deep_dive"));
}

#[tokio::test]
async fn test_metrics_page_counts_failed_tool_calls_in_success_rate() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "fast_search",
            12.0,
            Some(3),
            Some(12_000),
            Some(800),
            true,
            None,
        )
        .unwrap();
    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "get_symbols",
            6.0,
            None,
            None,
            Some(0),
            false,
            Some(r#"{"error":{"message":"invalid mode"}}"#),
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics?days=7&workspace=ready-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(
        html.contains("50%"),
        "success rate should include failed calls:\n{html}"
    );
    assert!(
        html.contains("1 failed"),
        "failure count should be visible next to the success rate:\n{html}"
    );
}

#[tokio::test]
async fn test_metrics_table_renders_input_bytes_for_tools() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .insert_tool_call_with_input_bytes(
            "ready-a",
            "session-a",
            "edit_file",
            18.0,
            Some(1),
            Some(16_000),
            Some(4_096),
            Some(1_024),
            true,
            Some(r#"{"dry_run":true}"#),
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics/table?days=7&workspace=ready-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("edit_file"));
    assert!(
        html.contains("4.0 KB edit request"),
        "edit tools should show request bytes with an edit-specific label:\n{html}"
    );
    assert!(
        html.contains("15.6 KB examined"),
        "table should keep source-byte context visible:\n{html}"
    );
}

#[tokio::test]
async fn test_query_metrics_formats_null_input_bytes() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "fast_search",
            12.0,
            Some(3),
            Some(12_000),
            Some(800),
            true,
            None,
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics/table?days=7&workspace=ready-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("fast_search"));
    assert!(
        html.contains("Julie request bytes not recorded"),
        "old rows with NULL input_bytes should render a compatibility label:\n{html}"
    );
    assert!(
        html.contains("11.7 KB examined"),
        "source bytes should still render when input_bytes is NULL:\n{html}"
    );
}

#[tokio::test]
async fn test_metrics_table_filters_to_selected_workspace() {
    let (state, _tmp) = test_state_with_db();
    let daemon_db = state.daemon_db().expect("daemon db").clone();

    daemon_db
        .upsert_workspace("ready-b", "/proj/b", "ready")
        .unwrap();
    daemon_db
        .update_workspace_stats("ready-b", 8, 1, None, None, None)
        .unwrap();

    daemon_db
        .insert_tool_call(
            "ready-a",
            "session-a",
            "fast_search",
            12.0,
            Some(3),
            Some(12_000),
            Some(800),
            true,
            None,
        )
        .unwrap();
    daemon_db
        .insert_tool_call(
            "ready-b",
            "session-b",
            "deep_dive",
            25.0,
            Some(1),
            Some(8_000),
            Some(1_200),
            true,
            None,
        )
        .unwrap();

    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics/table?days=7&workspace=ready-a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("fast_search"));
    assert!(!html.contains("deep_dive"));
}
