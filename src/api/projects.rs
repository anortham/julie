//! Project management endpoints.
//!
//! - `GET /api/projects` — list all registered projects
//! - `POST /api/projects` — register a new project by path
//! - `DELETE /api/projects/:id` — remove a project by workspace ID
//! - `GET /api/projects/:id/status` — get current project status
//! - `POST /api/projects/:id/index` — trigger background indexing

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::daemon_indexer::IndexRequest;
use crate::registry::ProjectStatus;
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

/// `GET /api/projects` -- list all registered projects with live status.
///
/// The status is derived from the daemon's loaded workspace pool (DaemonState),
/// not just the static registry entry. This means a project that was `Registered`
/// in the TOML file will show as `Ready` if the daemon successfully loaded its
/// workspace on startup.
pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProjectResponse>> {
    let registry = state.registry.read().await;
    let daemon_state = state.daemon_state.read().await;

    let projects: Vec<ProjectResponse> = registry
        .list_projects()
        .iter()
        .map(|entry| {
            // Use the live daemon state for status if available,
            // otherwise fall back to the registry's static status.
            let live_status = daemon_state.project_status_for(&entry.workspace_id);
            ProjectResponse {
                workspace_id: entry.workspace_id.clone(),
                name: entry.name.clone(),
                path: entry.path.to_string_lossy().into_owned(),
                status: format_status(&live_status),
                last_indexed: entry.last_indexed.clone(),
                symbol_count: entry.symbol_count,
                file_count: entry.file_count,
            }
        })
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

    let result = registry.register_project(&project_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to register project: {}", e),
        )
    })?;

    let workspace_id = result.workspace_id().to_string();
    let entry = registry.get_project(&workspace_id).unwrap();
    let response = CreateProjectResponse {
        workspace_id: entry.workspace_id.clone(),
        name: entry.name.clone(),
        path: entry.path.to_string_lossy().into_owned(),
        status: format_status(&entry.status),
    };

    if result.is_already_exists() {
        return Ok((StatusCode::CONFLICT, Json(response)));
    }

    // Persist to disk
    if let Err(e) = registry.save(&state.julie_home) {
        tracing::error!("Failed to save registry after adding project: {}", e);
        // Don't fail the request -- project is registered in memory
    }

    // Drop registry lock before acquiring daemon_state lock to minimize lock scope
    drop(registry);

    // Register the workspace in daemon state so it gets an MCP service
    // and shows correct live status immediately.
    {
        let mut daemon_state = state.daemon_state.write().await;
        daemon_state.register_workspace(
            response.workspace_id.clone(),
            project_path,
            &state.cancellation_token,
        );
    }

    // Start file watcher if the workspace is Ready (has .julie/ with indexes)
    {
        let daemon_state = state.daemon_state.read().await;
        daemon_state
            .start_watcher_if_ready(&response.workspace_id)
            .await;
    }

    Ok((StatusCode::CREATED, Json(response)))
}

/// `DELETE /api/projects/:id` -- remove a project by workspace ID.
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
        // Don't fail the request -- project is removed in memory
    }

    // Drop registry lock before acquiring daemon_state lock to minimize lock scope
    drop(registry);

    // Clean up daemon state (workspace + MCP service)
    {
        let mut daemon_state = state.daemon_state.write().await;
        daemon_state.remove_workspace(&id).await;
    }

    StatusCode::NO_CONTENT
}

/// Response body for project status.
#[derive(Debug, Serialize)]
pub struct ProjectStatusResponse {
    pub workspace_id: String,
    pub status: String,
    pub last_indexed: Option<String>,
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
}

/// `GET /api/projects/:id/status` -- get the current status of a project.
///
/// Returns the live status from daemon state (not just the static registry entry).
/// Returns 404 if the project is not registered.
pub async fn get_project_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProjectStatusResponse>, StatusCode> {
    let registry = state.registry.read().await;
    let entry = registry.get_project(&id).ok_or(StatusCode::NOT_FOUND)?;
    let daemon_state = state.daemon_state.read().await;
    let live_status = daemon_state.project_status_for(&id);

    Ok(Json(ProjectStatusResponse {
        workspace_id: entry.workspace_id.clone(),
        status: format_status(&live_status),
        last_indexed: entry.last_indexed.clone(),
        symbol_count: entry.symbol_count,
        file_count: entry.file_count,
    }))
}

/// Request body for `POST /api/projects/:id/index`.
#[derive(Debug, Deserialize)]
pub struct TriggerIndexRequest {
    /// If true, force a full re-index even if indexes already exist.
    #[serde(default)]
    pub force: bool,
}

/// Response body for `POST /api/projects/:id/index`.
#[derive(Debug, Serialize)]
pub struct TriggerIndexResponse {
    pub workspace_id: String,
    pub status: String,
    pub message: String,
}

/// `POST /api/projects/:id/index` -- trigger background indexing for a project.
///
/// Queues the project for indexing in the background worker. Returns 202 Accepted
/// immediately (does not wait for indexing to complete).
///
/// Accepts an optional JSON body: `{ "force": true }` to force re-indexing.
/// If no body is provided, defaults to incremental indexing (force=false).
///
/// Returns 404 if the project is not registered.
pub async fn trigger_index(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: Option<Json<TriggerIndexRequest>>,
) -> Result<(StatusCode, Json<TriggerIndexResponse>), (StatusCode, String)> {
    let force = body.map(|b| b.force).unwrap_or(false);

    // Look up the project in the registry and check status
    let project_path = {
        let registry = state.registry.read().await;
        let entry = registry.get_project(&id).ok_or((
            StatusCode::NOT_FOUND,
            format!("Project not found: {}", id),
        ))?;

        // Reject if already indexing (unless force re-index)
        if !force && entry.status == crate::registry::ProjectStatus::Indexing {
            return Ok((
                StatusCode::CONFLICT,
                Json(TriggerIndexResponse {
                    workspace_id: id,
                    status: "indexing".to_string(),
                    message: "Already indexing. Use force=true to re-queue.".to_string(),
                }),
            ));
        }

        entry.path.clone()
    };

    // Queue the indexing request
    let request = IndexRequest {
        workspace_id: id.clone(),
        project_path,
        force,
    };

    state.indexing_sender.send(request).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to queue indexing request: {}", e),
        )
    })?;

    Ok((
        StatusCode::ACCEPTED,
        Json(TriggerIndexResponse {
            workspace_id: id,
            status: "indexing".to_string(),
            message: if force {
                "Force re-indexing queued".to_string()
            } else {
                "Indexing queued".to_string()
            },
        }),
    ))
}
