use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;

use crate::daemon::lifecycle::LifecyclePhase;
use crate::daemon::session::SessionTracker;
use crate::dashboard::state::DashboardState;
use crate::dashboard::{DashboardConfig, create_router};

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(AtomicBool::new(false)),
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None, // no embedding service in tests
        None,
        50,
    )
}

#[tokio::test]
async fn test_router_serves_landing_page() {
    let state = test_state();
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder().uri("/").body(Body::empty()).unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn test_router_serves_static_css() {
    let state = test_state();
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder()
        .uri("/static/app.css")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/css"),
        "expected text/css, got: {content_type}"
    );
}
