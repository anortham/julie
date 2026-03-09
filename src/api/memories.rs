//! Memory and plan endpoints for the Julie daemon HTTP server.
//!
//! - `GET /api/memories` — list/search checkpoints (delegates to `recall()`)
//! - `GET /api/memories/:id` — get a single checkpoint by ID prefix match
//! - `GET /api/plans` — list plans (optional `?status` filter)
//! - `GET /api/plans/:id` — get a single plan by ID
//! - `GET /api/plans/active` — get the currently active plan (404 if none)

use std::cmp::Reverse;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::api::common::resolve_workspace_any;
use crate::memory::{self, Checkpoint, Plan, RecallOptions, RecallResult};
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Query parameter types
// ---------------------------------------------------------------------------

/// Query params for `GET /api/memories`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct MemoriesQuery {
    /// Max checkpoints to return.
    pub limit: Option<usize>,
    /// Time filter ("2h", "3d", ISO 8601).
    pub since: Option<String>,
    /// Full-text search query (triggers Tantivy search mode).
    pub search: Option<String>,
    /// Filter by plan ID.
    #[serde(rename = "planId")]
    pub plan_id: Option<String>,
    /// Workspace/project ID (defaults to first Ready workspace).
    pub project: Option<String>,
}

/// Query params for `GET /api/plans`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct PlansQuery {
    /// Filter by status ("active", "completed", "archived").
    pub status: Option<String>,
    /// Workspace/project ID (defaults to first Ready workspace).
    pub project: Option<String>,
}

/// Query params for plan endpoints that only need project routing.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ProjectQuery {
    /// Workspace/project ID (defaults to first Ready workspace).
    pub project: Option<String>,
}

// ---------------------------------------------------------------------------
// GET /api/memories
// ---------------------------------------------------------------------------

/// List or search checkpoints. Delegates to `memory::recall::recall()`.
#[utoipa::path(
    get,
    path = "/api/memories",
    tag = "memories",
    params(MemoriesQuery),
    responses(
        (status = 200, description = "List of checkpoints", body = RecallResult)
    )
)]
pub async fn list_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemoriesQuery>,
) -> Result<Json<RecallResult>, (StatusCode, String)> {
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace_any(&daemon_state, params.project.as_deref())?;
    let workspace_root = loaded_ws.path.clone();
    drop(daemon_state);

    let options = RecallOptions {
        limit: params.limit,
        since: params.since,
        search: params.search,
        plan_id: params.plan_id,
        ..Default::default()
    };

    let result = tokio::task::spawn_blocking(move || {
        memory::recall::recall(&workspace_root, options)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Recall task panicked: {}", e),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Recall failed: {}", e),
        )
    })?;

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// GET /api/memories/:id
// ---------------------------------------------------------------------------

/// Get a single checkpoint by ID or ID prefix.
///
/// Walks `.memories/` date directories and matches checkpoint files by:
/// 1. Full ID match (e.g., `checkpoint_abcd1234`)
/// 2. Hash prefix from filename (4-char prefix in filename like `174414_abcd.md`)
/// 3. Prefix match against parsed checkpoint IDs
#[utoipa::path(
    get,
    path = "/api/memories/{id}",
    tag = "memories",
    params(
        ("id" = String, Path, description = "Checkpoint ID or ID prefix"),
        ProjectQuery,
    ),
    responses(
        (status = 200, description = "Checkpoint found", body = Checkpoint),
        (status = 404, description = "Checkpoint not found")
    )
)]
pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<ProjectQuery>,
) -> Result<Json<Checkpoint>, (StatusCode, String)> {
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace_any(&daemon_state, params.project.as_deref())?;
    let workspace_root = loaded_ws.path.clone();
    drop(daemon_state);

    let result = tokio::task::spawn_blocking(move || find_checkpoint_by_id(&workspace_root, &id))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Lookup task panicked: {}", e),
            )
        })?;

    match result {
        Some(checkpoint) => Ok(Json(checkpoint)),
        None => Err((StatusCode::NOT_FOUND, "Checkpoint not found".to_string())),
    }
}

/// Walk `.memories/` date dirs and find a checkpoint matching the given ID.
///
/// Matching strategy:
/// - If `query` starts with "checkpoint_", try exact match on parsed ID
/// - Otherwise, try prefix match: strip "checkpoint_" from parsed IDs, then
///   check if the hash portion starts with `query`
fn find_checkpoint_by_id(
    workspace_root: &std::path::Path,
    query: &str,
) -> Option<Checkpoint> {
    let memories_dir = workspace_root.join(".memories");
    if !memories_dir.exists() {
        return None;
    }

    // Collect date directories
    let mut date_dirs: Vec<_> = std::fs::read_dir(&memories_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                && e.file_name()
                    .to_str()
                    .is_some_and(|n| n.len() == 10 && n.chars().nth(4) == Some('-'))
        })
        .collect();

    // Sort newest first for faster hit on recent checkpoints
    date_dirs.sort_by_key(|e| Reverse(e.file_name()));

    // Normalize the query: strip "checkpoint_" prefix if present
    let hash_query = query.strip_prefix("checkpoint_").unwrap_or(query);

    for dir_entry in &date_dirs {
        let dir_path = dir_entry.path();
        let entries = match std::fs::read_dir(&dir_path) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for file_entry in entries.filter_map(|e| e.ok()) {
            let path = file_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            // Quick filename check: filename is `HHMMSS_XXXX.md` where XXXX
            // is the first 4 chars of the hash. If the query is 4 chars or
            // fewer, we can check against the filename directly.
            let fname = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            // Extract the hash part after the underscore
            let fname_hash = fname.split('_').nth(1).unwrap_or("");

            if hash_query.len() <= 4 {
                if !fname_hash.starts_with(hash_query) {
                    continue;
                }
            } else if !hash_query.starts_with(fname_hash) {
                // If query is longer than 4, the filename hash must be a
                // prefix of the query for it to be a candidate
                continue;
            }

            // Parse the file to verify full ID match
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let checkpoint = match memory::storage::parse_checkpoint(&content) {
                Ok(cp) => cp,
                Err(_) => continue,
            };

            // Match: full ID equals, or hash portion starts with query
            let cp_hash = checkpoint
                .id
                .strip_prefix("checkpoint_")
                .unwrap_or(&checkpoint.id);

            if checkpoint.id == query || cp_hash.starts_with(hash_query) {
                return Some(checkpoint);
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// GET /api/plans
// ---------------------------------------------------------------------------

/// List plans, optionally filtered by status.
#[utoipa::path(
    get,
    path = "/api/plans",
    tag = "memories",
    params(PlansQuery),
    responses(
        (status = 200, description = "List of plans", body = Vec<Plan>)
    )
)]
pub async fn list_plans(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PlansQuery>,
) -> Result<Json<Vec<Plan>>, (StatusCode, String)> {
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace_any(&daemon_state, params.project.as_deref())?;
    let workspace_root = loaded_ws.path.clone();
    drop(daemon_state);

    let status_filter = params.status;
    let result = tokio::task::spawn_blocking(move || {
        memory::plan::list_plans(&workspace_root, status_filter.as_deref())
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("List plans task panicked: {}", e),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("List plans failed: {}", e),
        )
    })?;

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// GET /api/plans/:id
// ---------------------------------------------------------------------------

/// Get a single plan by ID.
#[utoipa::path(
    get,
    path = "/api/plans/{id}",
    tag = "memories",
    params(
        ("id" = String, Path, description = "Plan ID"),
        ProjectQuery,
    ),
    responses(
        (status = 200, description = "Plan found", body = Plan),
        (status = 404, description = "Plan not found")
    )
)]
pub async fn get_plan(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<ProjectQuery>,
) -> Result<Json<Plan>, (StatusCode, String)> {
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace_any(&daemon_state, params.project.as_deref())?;
    let workspace_root = loaded_ws.path.clone();
    drop(daemon_state);

    let result = tokio::task::spawn_blocking(move || {
        memory::plan::get_plan(&workspace_root, &id)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Get plan task panicked: {}", e),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Get plan failed: {}", e),
        )
    })?;

    match result {
        Some(plan) => Ok(Json(plan)),
        None => Err((StatusCode::NOT_FOUND, "Plan not found".to_string())),
    }
}

// ---------------------------------------------------------------------------
// GET /api/plans/active
// ---------------------------------------------------------------------------

/// Get the currently active plan, or 404 if none.
#[utoipa::path(
    get,
    path = "/api/plans/active",
    tag = "memories",
    params(ProjectQuery),
    responses(
        (status = 200, description = "Active plan found", body = Plan),
        (status = 404, description = "No active plan")
    )
)]
pub async fn get_active_plan(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ProjectQuery>,
) -> Result<Json<Plan>, (StatusCode, String)> {
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace_any(&daemon_state, params.project.as_deref())?;
    let workspace_root = loaded_ws.path.clone();
    drop(daemon_state);

    let result = tokio::task::spawn_blocking(move || {
        memory::plan::get_active_plan(&workspace_root)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Get active plan task panicked: {}", e),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Get active plan failed: {}", e),
        )
    })?;

    match result {
        Some(plan) => Ok(Json(plan)),
        None => Err((
            StatusCode::NOT_FOUND,
            "No active plan".to_string(),
        )),
    }
}
