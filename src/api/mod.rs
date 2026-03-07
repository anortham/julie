//! API route definitions for the Julie daemon HTTP server.

pub mod health;
pub mod projects;

use std::sync::Arc;

use axum::{Router, routing::get};

use crate::server::AppState;

/// Build the `/api` router with all sub-routes.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .route("/projects", get(projects::list_projects).post(projects::create_project))
        .route("/projects/{id}", axum::routing::delete(projects::delete_project))
        .route("/projects/{id}/status", get(projects::get_project_status))
        .route("/projects/{id}/index", axum::routing::post(projects::trigger_index))
        .with_state(state)
}
