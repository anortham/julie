//! Status page route handler.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let uptime = state.dashboard.uptime();
    let uptime_secs = uptime.as_secs();
    let hours = uptime_secs / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let uptime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    let active_sessions = state.dashboard.sessions().active_count();
    let restart_pending = state.dashboard.is_restart_pending();
    let errors = state.dashboard.error_entries();
    let embedding_available = state.dashboard.embedding_available();
    let embedding_initializing = state.dashboard.embedding_initializing();

    let workspace_count = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .map(|ws| ws.len())
        .unwrap_or(0);

    let mut context = Context::new();
    context.insert("active_page", "status");
    context.insert("uptime", &uptime_str);
    context.insert("active_sessions", &active_sessions);
    context.insert("workspace_count", &workspace_count);
    context.insert("restart_pending", &restart_pending);
    context.insert("embedding_available", &embedding_available);
    context.insert("embedding_initializing", &embedding_initializing);
    context.insert("errors", &errors);

    render_template(&state, "status.html", context).await
}

/// Returns live status values as JSON for polling.
pub async fn live(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let uptime = state.dashboard.uptime();
    let uptime_secs = uptime.as_secs();
    let hours = uptime_secs / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let uptime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    let active_sessions = state.dashboard.sessions().active_count();
    let workspace_count = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_workspaces().ok())
        .map(|ws| ws.len())
        .unwrap_or(0);

    let body = serde_json::json!({
        "uptime": uptime_str,
        "active_sessions": active_sessions,
        "workspace_count": workspace_count,
        "restart_pending": state.dashboard.is_restart_pending(),
        // Surfaced in the live polling response so the dashboard reflects
        // the embedding service transitioning from Initializing -> Ready
        // (or Unavailable) without requiring a manual page refresh. With
        // the daemon's lazy-init lifecycle this flips ~36-39s after cold
        // start.
        "embedding_available": state.dashboard.embedding_available(),
        "embedding_initializing": state.dashboard.embedding_initializing(),
    })
    .to_string();

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body,
    ))
}
