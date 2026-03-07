//! Embedded web UI serving.
//!
//! At compile time, `rust-embed` bakes the contents of `ui/dist/` into the
//! binary. At runtime we serve those files under `/ui/` with correct
//! content-type headers and SPA fallback (any path that doesn't match a
//! real file returns `index.html`).

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

/// Serve a file from the embedded assets, or fall back to index.html.
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
        None => {
            // SPA fallback: serve index.html for any unknown path
            match UiAssets::get("index.html") {
                Some(content) => Html(content.data).into_response(),
                None => (StatusCode::NOT_FOUND, "UI not available").into_response(),
            }
        }
    }
}
