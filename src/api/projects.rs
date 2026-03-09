//! Project management endpoints.
//!
//! - `GET /api/projects` — list all registered projects
//! - `POST /api/projects` — register a new project by path
//! - `DELETE /api/projects/:id` — remove a project by workspace ID
//! - `GET /api/projects/:id/status` — get current project status
//! - `GET /api/projects/:id/stats` — get detailed project statistics
//! - `POST /api/projects/:id/index` — trigger background indexing
//! - `POST /api/launch/editor` — open a project in the user's configured editor
//! - `POST /api/launch/terminal` — open a terminal at a project path

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::common::{display_path, resolve_workspace};
use crate::daemon_indexer::IndexRequest;
use crate::daemon_state::{DaemonState, WorkspaceLoadStatus};
use crate::registry::ProjectStatus;
use crate::server::AppState;

/// Response body for a single project.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectResponse {
    pub workspace_id: String,
    pub name: String,
    pub path: String,
    pub status: String,
    pub last_indexed: Option<String>,
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
    pub embedding_status: Option<EmbeddingStatusResponse>,
}

/// Embedding runtime status for a project.
#[derive(Debug, Serialize, ToSchema)]
pub struct EmbeddingStatusResponse {
    /// Resolved backend: "sidecar", "ort", "unresolved", etc.
    pub backend: String,
    /// Whether the backend has GPU/hardware acceleration.
    pub accelerated: bool,
    /// If the backend fell back from a preferred option, explains why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
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
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    pub path: String,
}

/// Response body for `POST /api/projects`.
#[derive(Debug, Serialize, ToSchema)]
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
#[utoipa::path(
    get,
    path = "/api/projects",
    tag = "projects",
    responses(
        (status = 200, description = "List of all registered projects", body = Vec<ProjectResponse>)
    )
)]
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

            // Extract embedding runtime status from loaded workspace.
            let embedding_status = daemon_state
                .workspaces
                .get(&entry.workspace_id)
                .filter(|ws| ws.status == WorkspaceLoadStatus::Ready)
                .and_then(|ws| {
                    ws.workspace.embedding_runtime_status.as_ref().map(|ers| {
                        EmbeddingStatusResponse {
                            backend: ers.resolved_backend.as_str().to_string(),
                            accelerated: ers.accelerated,
                            degraded_reason: ers.degraded_reason.clone(),
                        }
                    })
                });

            ProjectResponse {
                workspace_id: entry.workspace_id.clone(),
                name: entry.name.clone(),
                path: display_path(&entry.path),
                status: format_status(&live_status),
                last_indexed: entry.last_indexed.clone(),
                symbol_count: entry.symbol_count,
                file_count: entry.file_count,
                embedding_status,
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
#[utoipa::path(
    post,
    path = "/api/projects",
    tag = "projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "Project registered successfully", body = CreateProjectResponse),
        (status = 400, description = "Invalid path (does not exist or not a directory)"),
        (status = 409, description = "Project already registered", body = CreateProjectResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<CreateProjectResponse>), (StatusCode, String)> {
    let project_path = PathBuf::from(&body.path);

    let result = DaemonState::register_project(&state.daemon_state, &project_path)
        .await
        .map_err(|e| {
            // Map validation errors (path doesn't exist, not a dir) to 400,
            // everything else to 500.
            let msg = e.to_string();
            if msg.starts_with("Path does not exist") || msg.starts_with("Path is not a directory")
            {
                (StatusCode::BAD_REQUEST, msg)
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to register project: {}", msg),
                )
            }
        })?;

    // Look up the live status from the daemon state for the response
    let status = {
        let ds = state.daemon_state.read().await;
        format_status(&ds.project_status_for(&result.workspace_id))
    };

    let response = CreateProjectResponse {
        workspace_id: result.workspace_id.clone(),
        name: result.name,
        path: display_path(&result.path),
        status,
    };

    if result.already_existed {
        Ok((StatusCode::CONFLICT, Json(response)))
    } else {
        Ok((StatusCode::CREATED, Json(response)))
    }
}

/// `DELETE /api/projects/:id` -- remove a project by workspace ID.
///
/// Returns 204 No Content on success, 404 Not Found if the project doesn't exist.
#[utoipa::path(
    delete,
    path = "/api/projects/{id}",
    tag = "projects",
    params(
        ("id" = String, Path, description = "Workspace ID of the project to remove")
    ),
    responses(
        (status = 204, description = "Project removed successfully"),
        (status = 404, description = "Project not found")
    )
)]
pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    // Use the shared deregister_project method which handles:
    // 1. Correct lock ordering (DaemonState before registry)
    // 2. Removing from GlobalRegistry + persisting to disk
    // 3. Removing from DaemonState (workspace + MCP service + watcher)
    match DaemonState::deregister_project(&state.daemon_state, &id).await {
        Ok(Some(_)) => StatusCode::NO_CONTENT,
        Ok(None) => StatusCode::NOT_FOUND,
        Err(e) => {
            tracing::error!("Failed to deregister project '{}': {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Response body for project status.
#[derive(Debug, Serialize, ToSchema)]
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
#[utoipa::path(
    get,
    path = "/api/projects/{id}/status",
    tag = "projects",
    params(
        ("id" = String, Path, description = "Workspace ID of the project")
    ),
    responses(
        (status = 200, description = "Project status", body = ProjectStatusResponse),
        (status = 404, description = "Project not found")
    )
)]
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
#[derive(Debug, Deserialize, ToSchema)]
pub struct TriggerIndexRequest {
    /// If true, force a full re-index even if indexes already exist.
    #[serde(default)]
    pub force: bool,
}

/// Response body for `POST /api/projects/:id/index`.
#[derive(Debug, Serialize, ToSchema)]
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
#[utoipa::path(
    post,
    path = "/api/projects/{id}/index",
    tag = "projects",
    params(
        ("id" = String, Path, description = "Workspace ID of the project to index")
    ),
    request_body(content = TriggerIndexRequest, description = "Optional indexing options", content_type = "application/json"),
    responses(
        (status = 202, description = "Indexing queued", body = TriggerIndexResponse),
        (status = 404, description = "Project not found"),
        (status = 409, description = "Already indexing", body = TriggerIndexResponse),
        (status = 500, description = "Failed to queue indexing request")
    )
)]
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

// ---------------------------------------------------------------------------
// Project stats
// ---------------------------------------------------------------------------

/// Per-language file count.
#[derive(Debug, Serialize, ToSchema)]
pub struct LanguageCount {
    pub language: String,
    pub file_count: i64,
}

/// Per-kind symbol count.
#[derive(Debug, Serialize, ToSchema)]
pub struct SymbolKindCount {
    pub kind: String,
    pub count: i64,
}

/// Detailed project statistics response.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectStatsResponse {
    pub total_symbols: i64,
    pub total_files: i64,
    pub total_relationships: i64,
    pub db_size_mb: f64,
    pub embedding_count: i64,
    pub languages: Vec<LanguageCount>,
    pub symbol_kinds: Vec<SymbolKindCount>,
}

/// `GET /api/projects/:id/stats` — get detailed project statistics.
#[utoipa::path(
    get,
    path = "/api/projects/{id}/stats",
    tag = "projects",
    params(("id" = String, Path, description = "Workspace ID")),
    responses(
        (status = 200, description = "Project statistics", body = ProjectStatsResponse),
        (status = 404, description = "Project not found or not ready")
    )
)]
pub async fn get_project_stats(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProjectStatsResponse>, (StatusCode, String)> {
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace(&daemon_state, Some(&id))?;

    let db = loaded_ws
        .workspace
        .db
        .as_ref()
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database not available for this workspace".to_string(),
        ))?;

    let db_lock = db.lock().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Database lock error: {}", e),
        )
    })?;

    let stats = db_lock.get_stats().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get database stats: {}", e),
        )
    })?;

    let languages: Vec<LanguageCount> = db_lock
        .count_files_by_language()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to count languages: {}", e),
            )
        })?
        .into_iter()
        .map(|(language, file_count)| LanguageCount {
            language,
            file_count,
        })
        .collect();

    let symbol_kinds: Vec<SymbolKindCount> = db_lock
        .count_symbols_by_kind()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to count symbol kinds: {}", e),
            )
        })?
        .into_iter()
        .map(|(kind, count)| SymbolKindCount { kind, count })
        .collect();

    Ok(Json(ProjectStatsResponse {
        total_symbols: stats.total_symbols,
        total_files: stats.total_files,
        total_relationships: stats.total_relationships,
        db_size_mb: stats.db_size_mb,
        embedding_count: stats.embedding_count,
        languages,
        symbol_kinds,
    }))
}

// ── Launch endpoints ──────────────────────────────────────────────────

/// Request body for launching an editor at a project path.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LaunchEditorRequest {
    /// Editor command to run (e.g. "code", "code-insiders", "cursor", "zed")
    pub editor: String,
    /// Absolute path to the project directory
    pub path: String,
}

/// Request body for opening a terminal at a project path.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LaunchTerminalRequest {
    /// Absolute path to the project directory
    pub path: String,
}

/// Generic launch response.
#[derive(Debug, Serialize, ToSchema)]
pub struct LaunchResponse {
    pub ok: bool,
}

/// `POST /api/launch/editor` — open a project directory in the user's editor.
///
/// Spawns a detached process: `{editor} {path}`. Returns immediately.
#[utoipa::path(
    post,
    path = "/api/launch/editor",
    request_body = LaunchEditorRequest,
    responses(
        (status = 200, description = "Editor launched", body = LaunchResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Failed to spawn editor")
    ),
    tag = "projects"
)]
pub async fn launch_editor(
    Json(body): Json<LaunchEditorRequest>,
) -> Result<Json<LaunchResponse>, (StatusCode, String)> {
    // Allowlist of known editors to prevent arbitrary command execution
    const ALLOWED_EDITORS: &[&str] = &[
        "code", "code-insiders", "cursor", "zed", "vim", "nvim", "neovim",
        "emacs", "emacsclient", "subl", "sublime_text",
        "idea", "goland", "pycharm", "webstorm", "rustrover", "clion", "rider",
        "atom", "lapce", "helix", "hx", "kate", "gedit", "nano",
    ];

    let editor_name = std::path::Path::new(&body.editor)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&body.editor);

    if !ALLOWED_EDITORS.contains(&editor_name) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unknown editor '{}'. Allowed: {}",
                editor_name,
                ALLOWED_EDITORS.join(", ")
            ),
        ));
    }

    let path = PathBuf::from(&body.path);
    if !path.exists() {
        return Err((StatusCode::BAD_REQUEST, format!("Path does not exist: {}", body.path)));
    }

    tokio::process::Command::new(&body.editor)
        .arg(&body.path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to launch {}: {}", body.editor, e))
        })?;

    Ok(Json(LaunchResponse { ok: true }))
}

/// `POST /api/launch/terminal` — open a system terminal at the given path.
///
/// On macOS, runs `open -a Terminal {path}`. On Linux, tries common terminals.
#[utoipa::path(
    post,
    path = "/api/launch/terminal",
    request_body = LaunchTerminalRequest,
    responses(
        (status = 200, description = "Terminal launched", body = LaunchResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Failed to spawn terminal")
    ),
    tag = "projects"
)]
pub async fn launch_terminal(
    Json(body): Json<LaunchTerminalRequest>,
) -> Result<Json<LaunchResponse>, (StatusCode, String)> {
    let path = PathBuf::from(&body.path);
    if !path.exists() {
        return Err((StatusCode::BAD_REQUEST, format!("Path does not exist: {}", body.path)));
    }

    let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec!["-a", "Terminal", &body.path])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/c", "start", "cmd", "/k", "cd", "/d", &body.path])
    } else {
        ("xdg-terminal-emulator", vec!["--working-directory", &body.path])
    };

    tokio::process::Command::new(cmd)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to open terminal: {}", e))
        })?;

    Ok(Json(LaunchResponse { ok: true }))
}
