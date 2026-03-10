//! Embedded web UI serving.
//!
//! At compile time, `rust-embed` bakes the contents of `ui/dist/` into the
//! binary. At runtime we serve those files under `/ui/` with correct
//! content-type headers and SPA fallback. Navigation routes (extensionless
//! or `.html` paths) that don't match a real file return `index.html` so
//! Vue Router can handle client-side routing. Static asset requests (`.js`,
//! `.css`, `.png`, etc.) return 404 when not found, preventing browsers from
//! trying to parse HTML as JavaScript or CSS.

use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use rust_embed::Embed;

/// Embedded assets from the Vue build output.
#[derive(Embed)]
#[folder = "ui/dist/"]
struct UiAssets;

/// Handler for `GET /ui/*path` — serves embedded Vue SPA assets.
///
/// If the requested path matches a file in the embedded assets, it is served
/// with the correct MIME type. Otherwise, `index.html` is served so that
/// Vue Router can handle client-side routing.
pub async fn ui_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches("/ui/");
    // Treat empty path (i.e. "/ui/") as index.html
    let path = if path.is_empty() { "index.html" } else { path };

    serve_embedded_file(path)
}

/// Static asset extensions that should return 404 when not found, rather than
/// falling back to `index.html`. Without this, a missing `.js` request would
/// return HTML, causing the browser to try parsing it as JavaScript.
const STATIC_ASSET_EXTENSIONS: &[&str] = &[
    ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".woff",
    ".woff2", ".ttf", ".eot", ".map", ".json", ".webp", ".avif", ".mp4",
    ".webm", ".pdf", ".zip", ".wasm",
];

/// Returns true if the path has an extension that identifies it as a static
/// asset (as opposed to an SPA navigation route).
fn is_static_asset(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    STATIC_ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Serve a file from the embedded assets, or fall back to index.html.
///
/// Static asset requests (`.js`, `.css`, `.png`, etc.) return 404 when the
/// file is not found. Navigation routes (extensionless or `.html`) get the
/// SPA fallback so Vue Router can handle them client-side.
fn serve_embedded_file(path: &str) -> Response {
    match UiAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                content.data,
            )
                .into_response()
        }
        None if is_static_asset(path) => {
            // Static asset not found — return 404 instead of HTML, which
            // would cause confusing parse errors in the browser.
            (StatusCode::NOT_FOUND, "Not found").into_response()
        }
        None => {
            // SPA fallback: serve index.html for navigation routes
            match UiAssets::get("index.html") {
                Some(content) => Html(content.data).into_response(),
                None => (StatusCode::NOT_FOUND, "UI not available").into_response(),
            }
        }
    }
}
