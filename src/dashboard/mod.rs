//! Dashboard HTTP server: router, template engine, and static asset serving.

pub mod error_buffer;
pub mod routes;
pub mod state;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use tera::Tera;
use tokio::sync::RwLock;

use crate::dashboard::state::DashboardState;

// ---------------------------------------------------------------------------
// Embedded assets
// ---------------------------------------------------------------------------

/// All files under `dashboard/` embedded into the binary at compile time.
#[derive(rust_embed::Embed)]
#[folder = "dashboard/"]
struct DashboardAssets;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A Tera instance shared across handlers, wrapped for interior mutability
/// (dev-mode reload) and thread safety.
pub type SharedTera = Arc<RwLock<Tera>>;

// ---------------------------------------------------------------------------
// DashboardConfig
// ---------------------------------------------------------------------------

/// Configuration for the dashboard server.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// When true, templates are re-read from disk on every render.
    /// Automatically enabled when `dashboard/templates/` exists on disk.
    pub dev_mode: bool,

    /// Path to the `dashboard/` directory.
    pub dashboard_dir: PathBuf,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        let dashboard_dir = PathBuf::from("dashboard");
        let dev_mode = dashboard_dir.join("templates").is_dir();
        Self {
            dev_mode,
            dashboard_dir,
        }
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// Full application state threaded through every route handler.
#[derive(Clone)]
pub struct AppState {
    /// Dashboard-level shared state (sessions, events, etc.).
    pub dashboard: DashboardState,

    /// The Tera template engine instance.
    pub tera: SharedTera,

    /// Configuration (dev mode, paths).
    pub config: DashboardConfig,
}

// ---------------------------------------------------------------------------
// Tera initialisation
// ---------------------------------------------------------------------------

/// Build the Tera template engine according to the config.
///
/// - Dev mode: loads templates from disk via glob.
/// - Release mode: loads templates from the embedded assets.
pub fn init_tera(config: &DashboardConfig) -> Result<Tera, tera::Error> {
    if config.dev_mode {
        let pattern = config
            .dashboard_dir
            .join("templates")
            .join("**")
            .join("*.html")
            .to_string_lossy()
            .replace('\\', "/");
        Tera::new(&pattern)
    } else {
        let mut tera = Tera::default();
        for path in DashboardAssets::iter() {
            if path.starts_with("templates/")
                && path.ends_with(".html")
                && let Some(file) = DashboardAssets::get(&path)
            {
                let name = path.strip_prefix("templates/").unwrap_or(&path);
                let content = std::str::from_utf8(&file.data)
                    .map_err(|e| tera::Error::msg(format!("UTF-8 error in {path}: {e}")))?;
                tera.add_raw_template(name, content)?;
            }
        }
        Ok(tera)
    }
}

// ---------------------------------------------------------------------------
// Public factory
// ---------------------------------------------------------------------------

/// Build the Axum router for the dashboard.
pub fn create_router(
    dashboard: DashboardState,
    config: DashboardConfig,
) -> Result<Router, tera::Error> {
    let tera = init_tera(&config)?;
    let shared_tera: SharedTera = Arc::new(RwLock::new(tera));

    let app_state = AppState {
        dashboard,
        tera: shared_tera,
        config,
    };

    let router = Router::new()
        .route("/", get(routes::status::index))
        .route("/status/live", get(routes::status::live))
        .route("/projects", get(routes::projects::index))
        .route(
            "/projects/register",
            post(routes::projects_actions::register),
        )
        .route("/projects/statuses", get(routes::projects::statuses))
        .route("/projects/table", get(routes::projects::table))
        .route("/projects/{id}/detail", get(routes::projects::detail))
        .route("/projects/{id}/open", post(routes::projects_actions::open))
        .route(
            "/projects/{id}/refresh",
            post(routes::projects_actions::refresh),
        )
        .route(
            "/projects/{id}/delete",
            post(routes::projects_actions::delete),
        )
        .route("/metrics", get(routes::metrics::index))
        .route("/metrics/table", get(routes::metrics::table))
        .route("/metrics/summary", get(routes::metrics::summary))
        .route("/search", get(routes::search::index))
        .route("/search", post(routes::search::search))
        .route(
            "/intelligence/{workspace_id}",
            get(routes::intelligence::index),
        )
        .route(
            "/intelligence/{workspace_id}/stories",
            get(routes::intelligence::story_cards),
        )
        .route("/events/activity", get(routes::events::activity_stream))
        .route("/static/{*path}", get(serve_static))
        .with_state(app_state);

    Ok(router)
}

// ---------------------------------------------------------------------------
// Template rendering helper
// ---------------------------------------------------------------------------

/// Render a named Tera template with the given context.
///
/// Injects `version` (from `CARGO_PKG_VERSION`) into every context.
/// In dev mode, reloads all templates from disk before rendering.
pub async fn render_template(
    state: &AppState,
    template_name: &str,
    mut context: tera::Context,
) -> Result<Html<String>, StatusCode> {
    context.insert("version", env!("CARGO_PKG_VERSION"));
    context.insert("csrf_token", state.dashboard.action_csrf_token());

    if state.config.dev_mode {
        let mut tera = state.tera.write().await;
        if let Err(e) = tera.full_reload() {
            tracing::error!("Tera reload failed: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        match tera.render(template_name, &context) {
            Ok(html) => Ok(Html(html)),
            Err(e) => {
                tracing::error!("Tera render failed for {template_name}: {e:#}");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        let tera = state.tera.read().await;
        match tera.render(template_name, &context) {
            Ok(html) => Ok(Html(html)),
            Err(e) => {
                tracing::error!("Tera render failed for {template_name}: {e:#}");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Static file handler
// ---------------------------------------------------------------------------

/// Serve a file from `dashboard/static/`.
///
/// In dev mode, reads from disk; in release mode reads from embedded assets.
pub async fn serve_static(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    let asset_path = format!("static/{path}");

    let data: Vec<u8> = if state.config.dev_mode {
        let full_path = state.config.dashboard_dir.join("static").join(&path);
        match tokio::fs::read(&full_path).await {
            Ok(bytes) => bytes,
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        }
    } else {
        match DashboardAssets::get(&asset_path) {
            Some(file) => file.data.into_owned(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let content_type = content_type_for(&path);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static(content_type))
        .body(Body::from(data))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Determine the MIME content-type from a file extension.
fn content_type_for(path: &str) -> &'static str {
    if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else {
        "application/octet-stream"
    }
}
