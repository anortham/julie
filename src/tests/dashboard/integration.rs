use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use crate::daemon::session::SessionTracker;
use crate::dashboard::state::DashboardState;
use crate::dashboard::{DashboardConfig, create_router};

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        None, // no embedding service in tests
        None,
        50,
    )
}

#[tokio::test]
async fn test_all_dashboard_pages_return_200() {
    let state = test_state();
    let config = DashboardConfig::default();

    for path in ["/", "/projects", "/metrics", "/search"] {
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
