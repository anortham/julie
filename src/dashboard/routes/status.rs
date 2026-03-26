//! Status page route handler.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

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
    context.insert("errors", &errors);

    render_template(&state, "status.html", context).await
}
