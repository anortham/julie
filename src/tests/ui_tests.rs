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

// ============================================================================
// STATIC ASSET 404 TESTS (missing assets should NOT get SPA fallback)
// ============================================================================

#[tokio::test]
async fn test_ui_missing_js_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/assets/nonexistent.js")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .js files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_missing_css_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/assets/nonexistent.css")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .css files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_missing_png_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/images/logo.png")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .png files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_missing_woff2_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/fonts/myfont.woff2")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .woff2 files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_missing_svg_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/icons/missing.svg")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .svg files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_missing_map_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/assets/index.js.map")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .map files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_missing_json_returns_404() {
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/manifest.json")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Missing .json files should return 404, not SPA fallback HTML"
    );
}

#[tokio::test]
async fn test_ui_spa_fallback_still_works_for_extensionless_routes() {
    // Verify extensionless paths still get SPA fallback (not broken by the 404 change)
    let app = test_ui_app();
    let req = Request::builder()
        .uri("/ui/settings/workspace")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Extensionless SPA routes should still get index.html fallback"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("<div id=\"app\">"),
        "SPA routes should receive index.html content"
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
