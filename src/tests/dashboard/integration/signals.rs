use super::*;

fn assert_stat_card(html: &str, label: &str, value: &str) {
    let label_marker = format!("<p class=\"stat-label\">{label}</p>");
    let label_index = html
        .find(&label_marker)
        .unwrap_or_else(|| panic!("missing stat label {label:?}\n{html}"));
    let card_start = html[..label_index]
        .rfind("<div class=\"fingerprint-card\">")
        .unwrap_or_else(|| panic!("missing stat card wrapper for {label:?}\n{html}"));
    let card = &html[card_start..label_index + label_marker.len()];
    let value_marker = format!("<p class=\"stat-value\">{value}</p>");

    assert!(
        card.contains(&value_marker),
        "stat card {label:?} did not contain value {value:?}\n{card}"
    );
}

#[tokio::test]
async fn test_signals_page_returns_200_for_indexed_workspace() {
    let (state, _temp_dir, workspace_id) = state_with_signal_workspace().await;
    let app = create_router(state, DashboardConfig::default()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/signals/{workspace_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn test_signals_page_returns_404_for_unknown_workspace() {
    let (state, _temp_dir, _workspace_id) = state_with_signal_workspace().await;
    let app = create_router(state, DashboardConfig::default()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/signals/unknown-workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 404);
}

#[tokio::test]
async fn test_signals_summary_renders_counts_and_marker_evidence() {
    let (state, _temp_dir, workspace_id) = state_with_signal_workspace().await;
    let app = create_router(state, DashboardConfig::default()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/signals/{workspace_id}/summary"))
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

    assert!(html.contains("Observed Entry Points"));
    assert!(html.contains("Auth Coverage"));
    assert!(html.contains("Review Markers"));
    assert_stat_card(&html, "Observed Entry Points", "2");
    assert_stat_card(&html, "Auth Coverage Candidates", "1");
    assert_stat_card(&html, "Review Markers", "1");
    assert!(html.contains("No auth marker observed on this symbol or owner"));
    assert!(
        html.contains("Framework or middleware-based auth that is not expressed via annotations is not visible here.")
    );
    assert!(html.contains("Controllers&#x2F;HealthController.cs:12"));
    assert!(html.contains("[HttpGet(&quot;&#x2F;health&quot;)]"));
    assert!(html.contains("[AllowAnonymous]"));
    assert!(!html.contains("Security Risk"));
    assert!(!html.contains("HIGH"));
}

#[tokio::test]
async fn test_signals_summary_empty_state_names_classified_markers() {
    let (state, _temp_dir, workspace_id) =
        state_with_search_workspace("src/lib.rs", "fn plain() {}\n", "plain").await;
    let app = create_router(state, DashboardConfig::default()).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/signals/{workspace_id}/summary"))
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

    assert!(html.contains("No classified annotation markers were found."));
}

#[tokio::test]
async fn test_signals_refresh_requires_csrf_and_returns_summary() {
    let (state, _temp_dir, workspace_id) = state_with_signal_workspace().await;
    let csrf_token = state.action_csrf_token().to_string();
    let app = create_router(state, DashboardConfig::default()).unwrap();

    let forbidden = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/signals/{workspace_id}/refresh"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("csrf_token=bad-token"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status().as_u16(), 403);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/signals/{workspace_id}/refresh"))
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf_token={csrf_token}")))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("Fresh report"));
    assert!(html.contains("Observed Entry Points"));
    assert!(html.contains("[AllowAnonymous]"));
}
