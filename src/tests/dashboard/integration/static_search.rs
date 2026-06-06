use super::*;

#[tokio::test]
async fn test_dashboard_static_files_served() {
    let state = test_state();
    let config = DashboardConfig::default();

    // Test app.css
    let app = create_router(state.clone(), config.clone()).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "GET /static/app.css returned {}",
        response.status()
    );
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/css"),
        "expected text/css content-type for app.css, got: {content_type}"
    );

    // Test app.js
    let app = create_router(state, config).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "GET /static/app.js returned {}",
        response.status()
    );
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("javascript"),
        "expected javascript content-type for app.js, got: {content_type}"
    );
}

#[tokio::test]
async fn test_dashboard_404_for_missing_static() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/static/nonexistent.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        404,
        "GET /static/nonexistent.js should return 404, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_activity_stream_route_is_not_mounted_on_read_only_dashboard() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/events/activity")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 404);
}

#[tokio::test]
async fn test_dashboard_post_search_returns_200() {
    let state = test_state();
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("query=test&search_target=definitions"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
}

#[ignore = "dashboard live-data dark after Phase 3d.2b pool de-type; standalone registry-reader dashboard rebuilt in 3d.3"]
#[tokio::test]
async fn test_dashboard_content_search_renders_line_match_preview() {
    let (state, _temp_dir, workspace_id) = state_with_search_workspace(
        "src/lib.rs",
        "TODO: implement authentication\nfn helper() {}\n",
        "helper",
    )
    .await;
    let config = DashboardConfig::default();
    let app = create_router(state, config).unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/search")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "query=TODO&workspace={workspace_id}&search_target=content"
                )))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let html = String::from_utf8(body.to_vec()).expect("utf8 body");

    assert!(
        html.contains("TODO: implement authentication"),
        "dashboard content search should render line-level match preview: {html}"
    );
    assert!(
        html.contains("lib.rs:1"),
        "dashboard content search should carry file and line detail: {html}"
    );
}

#[tokio::test]
async fn test_search_page_does_not_link_to_search_analysis_or_compare() {
    let state = test_state();
    let config = DashboardConfig::default();

    let app = create_router(state.clone(), config.clone()).unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        200,
        "GET /search returned {}",
        response.status()
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body bytes");
    let html = String::from_utf8(body.to_vec()).expect("utf8 body");

    assert!(
        !html.contains("/search/analysis"),
        "/search should not advertise the search analysis page: {html}"
    );
    assert!(
        !html.contains("/search/compare"),
        "/search should not advertise the deleted search compare page: {html}"
    );

    let router = create_router(state, config).unwrap();
    let response = router
        .oneshot(
            Request::builder()
                .uri("/search/compare")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_project_detail_returns_404_without_daemon_db() {
    // With no daemon_db, the detail endpoint should return 404
    // (daemon_db is None, so get_workspace returns NotFound)
    let state = test_state();
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder()
        .uri("/projects/test_workspace/detail")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    // 404 because daemon_db is None
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}
