//! Dashboard stats endpoint.
//!
//! `GET /api/dashboard/stats` — aggregated statistics from DaemonState,
//! DispatchManager, and detected backends.
//! `POST /api/embeddings/check` — trigger embedding provider initialization and return status.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::agent::backend::BackendInfo;
use crate::daemon_state::WorkspaceLoadStatus;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Top-level dashboard stats response.
#[derive(Debug, Serialize, ToSchema)]
pub struct DashboardStats {
    pub projects: ProjectStats,
    pub agents: AgentStats,
    pub backends: Vec<BackendStat>,
    pub embeddings: Vec<EmbeddingProjectStatus>,
    /// Number of active file watchers (one per watched project).
    pub active_watchers: usize,
}

/// Per-project embedding status for the dashboard.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EmbeddingProjectStatus {
    pub project: String,
    pub workspace_id: String,
    /// Resolved backend name: "sidecar", "ort", or null if not initialized.
    pub backend: Option<String>,
    /// Whether GPU acceleration is active.
    pub accelerated: Option<bool>,
    /// Reason for degraded performance (e.g. "MPS not available, using CPU").
    pub degraded_reason: Option<String>,
    /// Number of embeddings stored in SQLite (available even without runtime init).
    pub embedding_count: i64,
    /// Whether the embedding provider has been initialized this session.
    pub initialized: bool,
}

/// Breakdown of project counts by status.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectStats {
    pub total: usize,
    pub ready: usize,
    pub indexing: usize,
    pub error: usize,
    pub registered: usize,
    pub stale: usize,
}

/// Agent dispatch summary.
#[derive(Debug, Serialize, ToSchema)]
pub struct AgentStats {
    pub total_dispatches: usize,
    pub last_dispatch: Option<String>,
}

/// Single backend status entry.
#[derive(Debug, Serialize, ToSchema)]
pub struct BackendStat {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
}

impl From<&BackendInfo> for BackendStat {
    fn from(b: &BackendInfo) -> Self {
        Self {
            name: b.name.clone(),
            available: b.available,
            version: b.version.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `GET /api/dashboard/stats`
///
/// Aggregates stats from all subsystems into a single response.
#[utoipa::path(
    get,
    path = "/api/dashboard/stats",
    tag = "dashboard",
    responses(
        (status = 200, description = "Aggregated dashboard statistics", body = DashboardStats),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DashboardStats>, (StatusCode, String)> {
    // -- Projects --
    let project_stats = {
        let ds = state.daemon_state.read().await;
        let mut ready = 0usize;
        let mut indexing = 0usize;
        let mut error = 0usize;
        let mut registered = 0usize;
        let mut stale = 0usize;

        for ws in ds.workspaces.values() {
            match &ws.status {
                WorkspaceLoadStatus::Ready => ready += 1,
                WorkspaceLoadStatus::Indexing => indexing += 1,
                WorkspaceLoadStatus::Error(_) => error += 1,
                WorkspaceLoadStatus::Registered => registered += 1,
                WorkspaceLoadStatus::Stale => stale += 1,
            }
        }

        ProjectStats {
            total: ds.workspaces.len(),
            ready,
            indexing,
            error,
            registered,
            stale,
        }
    };

    // -- Agents --
    let agent_stats = {
        let dm = state.dispatch_manager.read().await;
        let dispatches = dm.list_dispatches();
        let total = dispatches.len();
        // Find the most recent dispatch by started_at timestamp.
        let last_dispatch = dispatches
            .iter()
            .map(|d| d.started_at.as_str())
            .max()
            .map(|s| s.to_string());

        AgentStats {
            total_dispatches: total,
            last_dispatch,
        }
    };

    // -- Backends --
    let backends: Vec<BackendStat> = state.backends.iter().map(BackendStat::from).collect();

    // -- Embeddings --
    let embeddings = gather_embedding_stats(&state).await;

    // -- Active watchers --
    let active_watchers = {
        let ds = state.daemon_state.read().await;
        ds.watcher_manager.active_watchers().await.len()
    };

    Ok(Json(DashboardStats {
        projects: project_stats,
        agents: agent_stats,
        backends,
        embeddings,
        active_watchers,
    }))
}

// ---------------------------------------------------------------------------
// Embedding check endpoint
// ---------------------------------------------------------------------------

/// `POST /api/embeddings/check`
///
/// Triggers embedding provider initialization on workspaces that haven't
/// initialized yet, then returns the updated per-project embedding status.
#[utoipa::path(
    post,
    path = "/api/embeddings/check",
    tag = "dashboard",
    responses(
        (status = 200, description = "Per-project embedding status after initialization attempt", body = Vec<EmbeddingProjectStatus>),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn check_embeddings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<EmbeddingProjectStatus>>, (StatusCode, String)> {
    // Initialize embedding providers on workspaces that don't have one yet.
    {
        let mut ds = state.daemon_state.write().await;
        let uninitialized: Vec<String> = ds
            .workspaces
            .iter()
            .filter(|(_, ws)| {
                ws.status == WorkspaceLoadStatus::Ready
                    && ws.workspace.embedding_provider.is_none()
            })
            .map(|(id, _)| id.clone())
            .collect();

        for ws_id in uninitialized {
            if let Some(loaded) = ds.workspaces.get_mut(&ws_id) {
                loaded.workspace.initialize_embedding_provider();
            }
        }
    }

    let statuses = gather_embedding_stats(&state).await;
    Ok(Json(statuses))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Gather per-project embedding status from daemon state.
async fn gather_embedding_stats(state: &Arc<AppState>) -> Vec<EmbeddingProjectStatus> {
    let ds = state.daemon_state.read().await;
    let registry = state.registry.read().await;

    let mut statuses = Vec::new();

    for entry in registry.list_projects() {
        let ws = ds.workspaces.get(&entry.workspace_id);

        let (backend, accelerated, degraded_reason, initialized) =
            if let Some(loaded) = ws {
                if let Some(ers) = &loaded.workspace.embedding_runtime_status {
                    (
                        Some(ers.resolved_backend.as_str().to_string()),
                        Some(ers.accelerated),
                        ers.degraded_reason.clone(),
                        true,
                    )
                } else {
                    (None, None, None, false)
                }
            } else {
                (None, None, None, false)
            };

        // Always try to get embedding count from SQLite, even if runtime isn't initialized.
        let embedding_count = ws
            .filter(|loaded| loaded.status == WorkspaceLoadStatus::Ready)
            .and_then(|loaded| loaded.workspace.db.as_ref())
            .and_then(|db| {
                let db = db.lock().ok()?;
                db.embedding_count().ok()
            })
            .unwrap_or(0);

        statuses.push(EmbeddingProjectStatus {
            project: entry.name.clone(),
            workspace_id: entry.workspace_id.clone(),
            backend,
            accelerated,
            degraded_reason,
            embedding_count,
            initialized,
        });
    }

    statuses
}
