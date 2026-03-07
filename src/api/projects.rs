//! Project management endpoints (stubbed for Task 5).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::server::AppState;

/// `GET /api/projects` — returns an empty project list (stub).
pub async fn list_projects(State(_state): State<Arc<AppState>>) -> Json<Vec<()>> {
    Json(vec![])
}

/// `POST /api/projects` — not yet implemented.
pub async fn create_project(State(_state): State<Arc<AppState>>) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}

/// `DELETE /api/projects/:id` — not yet implemented.
pub async fn delete_project(
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
