//! Project management endpoints.
//!
//! - `GET /api/projects` — list all registered projects
//! - `POST /api/projects` — register a new project by path
//! - `DELETE /api/projects/:id` — remove a project by workspace ID

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::registry::{ProjectEntry, ProjectStatus};
use crate::server::AppState;

/// Response body for a single project.
#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub workspace_id: String,
    pub name: String,
    pub path: String,
    pub status: String,
    pub last_indexed: Option<String>,
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
}

impl From<&ProjectEntry> for ProjectResponse {
    fn from(entry: &ProjectEntry) -> Self {
        Self {
            workspace_id: entry.workspace_id.clone(),
            name: entry.name.clone(),
            path: entry.path.to_string_lossy().into_owned(),
            status: format_status(&entry.status),
            last_indexed: entry.last_indexed.clone(),
            symbol_count: entry.symbol_count,
            file_count: entry.file_count,
        }
    }
}

fn format_status(status: &ProjectStatus) -> String {
    match status {
        ProjectStatus::Registered => "registered".to_string(),
        ProjectStatus::Indexing => "indexing".to_string(),
        ProjectStatus::Ready => "ready".to_string(),
        ProjectStatus::Stale => "stale".to_string(),
        ProjectStatus::Error(msg) => format!("error: {}", msg),
    }
}

/// Request body for `POST /api/projects`.
#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub path: String,
}

/// Response body for `POST /api/projects`.
#[derive(Debug, Serialize)]
pub struct CreateProjectResponse {
    pub workspace_id: String,
    pub name: String,
    pub path: String,
    pub status: String,
}

/// `GET /api/projects` — list all registered projects with status.
pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProjectResponse>> {
    let registry = state.registry.read().await;
    let projects: Vec<ProjectResponse> = registry
        .list_projects()
        .iter()
        .map(|entry| ProjectResponse::from(*entry))
        .collect();
    Json(projects)
}

/// `POST /api/projects` — register a new project by path.
///
/// Expects JSON body: `{ "path": "/absolute/path/to/project" }`
///
/// Returns 201 Created with the project info, or 409 Conflict if already registered.
/// Returns 400 Bad Request if the path doesn't exist or isn't a directory.
pub async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<CreateProjectResponse>), (StatusCode, String)> {
    let project_path = PathBuf::from(&body.path);

    // Validate the path exists and is a directory
    if !project_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Path does not exist: {}", body.path),
        ));
    }
    if !project_path.is_dir() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Path is not a directory: {}", body.path),
        ));
    }

    let mut registry = state.registry.write().await;

    // Check if already registered (by resolving the workspace ID)
    let canonical = project_path.canonicalize().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to resolve path: {}", e),
        )
    })?;
    let path_str = canonical.to_string_lossy();
    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&path_str).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to generate workspace ID: {}", e),
            )
        })?;

    if registry.get_project(&workspace_id).is_some() {
        let entry = registry.get_project(&workspace_id).unwrap();
        return Ok((
            StatusCode::CONFLICT,
            Json(CreateProjectResponse {
                workspace_id: entry.workspace_id.clone(),
                name: entry.name.clone(),
                path: entry.path.to_string_lossy().into_owned(),
                status: format_status(&entry.status),
            }),
        ));
    }

    let workspace_id = registry.register_project(&project_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to register project: {}", e),
        )
    })?;

    let entry = registry.get_project(&workspace_id).unwrap();
    let response = CreateProjectResponse {
        workspace_id: entry.workspace_id.clone(),
        name: entry.name.clone(),
        path: entry.path.to_string_lossy().into_owned(),
        status: format_status(&entry.status),
    };

    // Persist to disk
    if let Err(e) = registry.save(&state.julie_home) {
        tracing::error!("Failed to save registry after adding project: {}", e);
        // Don't fail the request — project is registered in memory
    }

    Ok((StatusCode::CREATED, Json(response)))
}

/// `DELETE /api/projects/:id` — remove a project by workspace ID.
///
/// Returns 204 No Content on success, 404 Not Found if the project doesn't exist.
pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let mut registry = state.registry.write().await;

    if !registry.remove_project(&id) {
        return StatusCode::NOT_FOUND;
    }

    // Persist to disk
    if let Err(e) = registry.save(&state.julie_home) {
        tracing::error!("Failed to save registry after removing project: {}", e);
        // Don't fail the request — project is removed in memory
    }

    StatusCode::NO_CONTENT
}
