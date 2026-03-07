//! Tests for the embedded web UI handler.
//!
//! Verifies that the embedded Vue SPA is served correctly with proper
//! content types, SPA fallback routing, and that all expected assets
//! are present in the binary.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt; // for `oneshot`

use crate::ui;

/// Create a router with UI routes for testing.
///
/// The UI handler doesn't need AppState (it only serves embedded static files),
/// so the test router is minimal.
fn test_ui_app() -> axum::Router {
    axum::Router::new()
        .route("/ui/", axum::routing::get(ui::ui_handler))
        .route("/ui/{*path}", axum::routing::get(ui::ui_handler))
}

// ============================================================================
// UI ROOT TESTS
// ============================================================================

#[tokio::test]
async fn test_ui_root_serves_index_html() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/html"),
        "Expected text/html, got: {}",
        content_type
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("<div id=\"app\">"),
        "index.html should contain Vue mount point"
    );
}

// ============================================================================
// SPA FALLBACK TESTS
// ============================================================================

#[tokio::test]
async fn test_ui_unknown_path_falls_back_to_index_html() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/projects")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("<div id=\"app\">"),
        "Unknown paths should fall back to index.html for SPA routing"
    );
}

#[tokio::test]
async fn test_ui_nested_unknown_path_falls_back_to_index_html() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/some/deeply/nested/route")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("<div id=\"app\">"),
        "Deeply nested unknown paths should fall back to index.html"
    );
}

// ============================================================================
// STATIC ASSET TESTS
// ============================================================================

#[tokio::test]
async fn test_ui_favicon_served_with_correct_type() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/favicon.svg")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("svg"),
        "Expected SVG content type, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_ui_index_html_references_assets() {
    // Verify the built index.html contains references to JS and CSS bundles
    // in the /ui/assets/ directory. This is more stable than hardcoding
    // Vite's content-hashed filenames, which change on every rebuild.
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8_lossy(&body);

    assert!(
        html.contains("/ui/assets/"),
        "index.html should reference assets with /ui/assets/ prefix"
    );
    assert!(
        html.contains(".js"),
        "index.html should reference a JavaScript bundle"
    );
}
