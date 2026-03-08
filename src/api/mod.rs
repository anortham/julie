//! API route definitions for the Julie daemon HTTP server.

pub mod agents;
pub mod common;
pub mod dashboard;
pub mod health;
pub mod memories;
pub mod projects;
pub mod search;

use std::sync::Arc;

use axum::{Router, routing::{get, post}};

use crate::server::AppState;

/// Build the `/api` router with all sub-routes.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .route("/projects", get(projects::list_projects).post(projects::create_project))
        .route("/projects/{id}", axum::routing::delete(projects::delete_project))
        .route("/projects/{id}/status", get(projects::get_project_status))
        .route("/projects/{id}/index", axum::routing::post(projects::trigger_index))
        .route("/search", axum::routing::post(search::search))
        .route("/search/debug", axum::routing::post(search::search_debug))
        // Agent dispatch routes (note: /agents/history and /agents/backends BEFORE /agents/{id})
        .route("/agents/dispatch", post(agents::dispatch_agent))
        .route("/agents/history", get(agents::list_dispatches))
        .route("/agents/backends", get(agents::list_backends))
        .route("/agents/{id}/stream", get(agents::stream_dispatch))
        .route("/agents/{id}", get(agents::get_dispatch))
        // Memory + plan routes (note: /plans/active BEFORE /plans/{id})
        .route("/memories", get(memories::list_memories))
        .route("/memories/{id}", get(memories::get_memory))
        .route("/plans", get(memories::list_plans))
        .route("/plans/active", get(memories::get_active_plan))
        .route("/plans/{id}", get(memories::get_plan))
        // Dashboard
        .route("/dashboard/stats", get(dashboard::stats))
        .with_state(state)
}
