use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

use crate::dashboard::state::DashboardState;
use crate::dashboard::{DashboardConfig, create_router};
use crate::registry::lifecycle::LifecyclePhase;
use crate::registry::session::SessionTracker;

fn test_state() -> DashboardState {
    DashboardState::new(
        Arc::new(SessionTracker::new()),
        None,
        Arc::new(RwLock::new(LifecyclePhase::Ready)),
        Instant::now(),
        None, // no embedding service in tests
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
async fn test_router_is_read_only_without_action_or_sse_routes() {
    let state = test_state();
    let config = DashboardConfig::default();

    for path in ["/", "/projects", "/metrics", "/search"] {
        let router = create_router(state.clone(), config.clone()).unwrap();
        let response = router
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "GET {path}");
    }

    for (method, path) in [
        (Method::POST, "/projects/register"),
        (Method::POST, "/projects/ws_123/open"),
        (Method::POST, "/projects/ws_123/refresh"),
        (Method::POST, "/projects/ws_123/delete"),
        (Method::POST, "/search/compare"),
        (Method::GET, "/search/compare"),
        (Method::POST, "/signals/ws_123/refresh"),
        (Method::GET, "/events/activity"),
    ] {
        let router = create_router(state.clone(), config.clone()).unwrap();
        let response = router
            .oneshot(
                Request::builder()
                    .method(method.clone())
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ),
            "{method} {path} should not be mounted on read-only dashboard, got {}",
            response.status()
        );
    }
}

/// Collect an axum body into a String for JSON-parsing assertions.
async fn body_to_string(body: Body) -> String {
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("body bytes");
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

/// A1.7: The `/status/live` JSON response includes `recovery_markers`. The
/// dashboard surfaces this so operators can see that the previous daemon
/// shutdown timed out with in-flight requests. Default is an empty array.
#[tokio::test]
async fn test_router_status_live_includes_recovery_markers() {
    use crate::registry::shutdown::RecoveryMarker;

    let markers = Arc::new(vec![RecoveryMarker {
        shutdown_timestamp_micros: 1_700_000_000_000_000,
        drain_timeout_secs: 60,
        active_sessions_at_timeout: 2,
        affected_workspaces: vec!["ws_test".to_string()],
    }]);
    let state = test_state().with_recovery_markers(markers);
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder()
        .uri("/status/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body_str = body_to_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body_str).expect("parse json");

    let markers_json = json
        .get("recovery_markers")
        .expect("recovery_markers field must be present in /status/live");
    let markers_arr = markers_json
        .as_array()
        .expect("recovery_markers must be a JSON array");
    assert_eq!(markers_arr.len(), 1, "exactly one marker should be present");
    assert_eq!(
        markers_arr[0]["active_sessions_at_timeout"], 2,
        "marker fields must round-trip through the endpoint"
    );
    assert_eq!(
        markers_arr[0]["affected_workspaces"],
        serde_json::json!(["ws_test"]),
        "marker affected_workspaces must round-trip"
    );
}

/// Without any recovery markers attached, `/status/live` still includes the
/// field — as an empty array.
#[tokio::test]
async fn test_router_status_live_recovery_markers_empty_by_default() {
    let state = test_state();
    let config = DashboardConfig::default();
    let router = create_router(state, config).unwrap();

    let request = Request::builder()
        .uri("/status/live")
        .body(Body::empty())
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let body_str = body_to_string(response.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body_str).expect("parse json");

    assert_eq!(
        json["recovery_markers"],
        serde_json::json!([]),
        "recovery_markers must default to an empty array"
    );
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
