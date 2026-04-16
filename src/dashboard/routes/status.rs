//! Status page route handler.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let health = state.dashboard.health_snapshot().await;
    let uptime = state.dashboard.uptime();
    let uptime_secs = uptime.as_secs();
    let hours = uptime_secs / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let uptime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    let active_sessions = health.control_plane.active_sessions;
    let restart_pending = health.control_plane.restart_pending;
    let errors = state.dashboard.error_entries();
    let workspace_count = health.data_plane.workspace_count;

    let mut context = Context::new();
    context.insert("active_page", "status");
    context.insert("uptime", &uptime_str);
    context.insert("active_sessions", &active_sessions);
    context.insert("workspace_count", &workspace_count);
    context.insert("restart_pending", &restart_pending);
    context.insert(
        "embedding_available",
        &health.runtime_plane.embedding_available,
    );
    context.insert(
        "embedding_initializing",
        &health.runtime_plane.embedding_initializing,
    );
    context.insert("health", &health);
    context.insert("errors", &errors);

    render_template(&state, "status.html", context).await
}

/// Returns live status values as JSON for polling.
pub async fn live(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let health = state.dashboard.health_snapshot().await;
    let uptime = state.dashboard.uptime();
    let uptime_secs = uptime.as_secs();
    let hours = uptime_secs / 3600;
    let minutes = (uptime_secs % 3600) / 60;
    let uptime_str = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    let active_sessions = health.control_plane.active_sessions;
    let workspace_count = health.data_plane.workspace_count;

    let body = serde_json::json!({
        "uptime": uptime_str,
        "active_sessions": active_sessions,
        "workspace_count": workspace_count,
        "restart_pending": health.control_plane.restart_pending,
        "embedding_available": health.runtime_plane.embedding_available,
        "embedding_initializing": health.runtime_plane.embedding_initializing,
        "health": health,
    })
    .to_string();

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body,
    ))
}
