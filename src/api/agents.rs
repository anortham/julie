//! Agent dispatch endpoints for the Julie daemon HTTP server.
//!
//! - `POST /api/agents/dispatch` — dispatch a task to an agent backend
//! - `GET /api/agents/:id` — get dispatch detail (status, output, timing)
//! - `GET /api/agents/:id/stream` — SSE stream of dispatch output
//! - `GET /api/agents/history` — list past dispatches
//! - `GET /api/agents/backends` — list detected agent backends

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::agent::backend::{AgentBackend, BackendInfo};
use crate::agent::claude_backend::ClaudeBackend;
use crate::agent::context_assembly::{self, ContextHints};
use crate::agent::dispatch;
use crate::api::common::resolve_workspace;
use crate::server::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

/// Request body for `POST /api/agents/dispatch`.
#[derive(Debug, Deserialize)]
pub struct DispatchRequest {
    /// The task description to dispatch to the agent.
    pub task: String,
    /// Workspace/project ID. If omitted, uses the first Ready workspace.
    #[serde(default)]
    pub project: Option<String>,
    /// Optional hints for context assembly.
    #[serde(default)]
    pub hints: Option<HintsInput>,
}

/// Optional hints for context assembly (mirrors `ContextHints`).
#[derive(Debug, Deserialize)]
pub struct HintsInput {
    /// Specific files to include context from.
    #[serde(default)]
    pub files: Option<Vec<String>>,
    /// Specific symbol names to look up.
    #[serde(default)]
    pub symbols: Option<Vec<String>>,
    /// Additional free-form context to include verbatim.
    #[serde(default)]
    pub extra_context: Option<String>,
}

/// Response for `POST /api/agents/dispatch`.
#[derive(Debug, Serialize)]
pub struct DispatchResponse {
    pub id: String,
    pub status: String,
}

/// Query params for `GET /api/agents/history`.
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Maximum number of dispatches to return.
    pub limit: Option<usize>,
    /// Filter by project name.
    pub project: Option<String>,
}

/// Response for `GET /api/agents/history`.
#[derive(Debug, Serialize)]
pub struct HistoryResponse {
    pub dispatches: Vec<DispatchSummary>,
}

/// Summary of a dispatch for list endpoints.
#[derive(Debug, Serialize)]
pub struct DispatchSummary {
    pub id: String,
    pub task: String,
    pub project: String,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for `GET /api/agents/:id`.
#[derive(Debug, Serialize)]
pub struct DispatchDetail {
    pub id: String,
    pub task: String,
    pub project: String,
    pub status: String,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response for `GET /api/agents/backends`.
#[derive(Debug, Serialize)]
pub struct BackendsResponse {
    pub backends: Vec<BackendInfo>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/agents/backends` — list detected agent backends with availability.
pub async fn list_backends(
    State(state): State<Arc<AppState>>,
) -> Json<BackendsResponse> {
    Json(BackendsResponse {
        backends: state.backends.clone(),
    })
}

/// `GET /api/agents/history` — list past dispatches with optional filters.
pub async fn list_dispatches(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HistoryQuery>,
) -> Json<HistoryResponse> {
    let dm = state.dispatch_manager.read().await;
    let all = dm.list_dispatches();

    let filtered: Vec<&crate::agent::dispatch::AgentDispatch> = all
        .into_iter()
        .filter(|d| {
            if let Some(ref project) = params.project {
                d.project == *project
            } else {
                true
            }
        })
        .collect();

    let limit = params.limit.unwrap_or(filtered.len());
    let dispatches = filtered
        .into_iter()
        .take(limit)
        .map(|d| DispatchSummary {
            id: d.id.clone(),
            task: d.task.clone(),
            project: d.project.clone(),
            status: d.status.as_str().to_string(),
            started_at: d.started_at.clone(),
            completed_at: d.completed_at.clone(),
            error: d.error.clone(),
        })
        .collect();

    Json(HistoryResponse { dispatches })
}

/// `GET /api/agents/:id` — get a single dispatch's detail.
pub async fn get_dispatch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<DispatchDetail>, (StatusCode, String)> {
    let dm = state.dispatch_manager.read().await;
    let dispatch = dm.get_dispatch(&id).ok_or((
        StatusCode::NOT_FOUND,
        format!("Dispatch not found: {}", id),
    ))?;

    Ok(Json(DispatchDetail {
        id: dispatch.id.clone(),
        task: dispatch.task.clone(),
        project: dispatch.project.clone(),
        status: dispatch.status.as_str().to_string(),
        started_at: dispatch.started_at.clone(),
        completed_at: dispatch.completed_at.clone(),
        output: dispatch.output.clone(),
        error: dispatch.error.clone(),
    }))
}

/// `GET /api/agents/:id/stream` — SSE stream of dispatch output lines.
pub async fn stream_dispatch(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<
    Sse<std::pin::Pin<Box<dyn tokio_stream::Stream<Item = Result<Event, Infallible>> + Send>>>,
    (StatusCode, String),
> {
    let dm = state.dispatch_manager.read().await;

    // Check if the dispatch exists
    let dispatch = dm.get_dispatch(&id).ok_or((
        StatusCode::NOT_FOUND,
        format!("Dispatch not found: {}", id),
    ))?;

    // If already completed/failed, return any existing output as a single event + done
    if dispatch.status != crate::agent::dispatch::DispatchStatus::Running {
        let output = dispatch.output.clone();
        let status = dispatch.status.as_str().to_string();
        drop(dm);

        let stream = tokio_stream::once(Ok::<_, Infallible>(Event::default().data(output)))
            .chain(tokio_stream::once(Ok::<_, Infallible>(
                Event::default().event("done").data(status),
            )));

        return Ok(Sse::new(Box::pin(stream)));
    }

    // Subscribe to the broadcast channel for live output
    let rx = dm.subscribe(&id).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        "Failed to subscribe to dispatch".to_string(),
    ))?;
    drop(dm);

    let stream = BroadcastStream::new(rx)
        .map(|msg| match msg {
            Ok(data) => Ok(Event::default().data(data)),
            Err(_) => Ok(Event::default().event("done").data("completed")),
        })
        // BroadcastStream ends when the sender is dropped, which signals completion.
        // After the stream ends, append a "done" event.
        .chain(tokio_stream::once(Ok::<_, Infallible>(
            Event::default().event("done").data("stream_ended"),
        )));

    Ok(Sse::new(Box::pin(stream)))
}

/// `POST /api/agents/dispatch` — dispatch a task to an agent backend.
///
/// Spawns the agent in the background and returns immediately with the dispatch ID.
pub async fn dispatch_agent(
    State(state): State<Arc<AppState>>,
    Json(body): Json<DispatchRequest>,
) -> Result<Json<DispatchResponse>, (StatusCode, String)> {
    // 1. Resolve workspace
    let daemon_state = state.daemon_state.read().await;
    let loaded_ws = resolve_workspace(&daemon_state, body.project.as_deref())?;

    let workspace_root = loaded_ws.workspace.root.clone();
    let search_index = loaded_ws.workspace.search_index.clone();
    drop(daemon_state);

    // 2. Assemble context
    let hints = body.hints.map(|h| ContextHints {
        files: h.files,
        symbols: h.symbols,
        extra_context: h.extra_context,
    });

    let prompt = context_assembly::assemble_context(
        Some(&workspace_root),
        search_index.as_ref(),
        &body.task,
        hints,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Context assembly failed: {}", e),
        )
    })?;

    // 3. Find an available backend
    let backend = state
        .backends
        .iter()
        .find(|b| b.available)
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "No available agent backend".to_string(),
        ))?;
    let backend_name = backend.name.clone();

    // 4. Start dispatch in the manager (creates broadcast channel)
    let project_name = body
        .project
        .unwrap_or_else(|| "default".to_string());
    let dispatch_id = {
        let mut dm = state.dispatch_manager.write().await;
        dm.start_dispatch(body.task.clone(), project_name.clone())
    };

    // 5. Spawn background task to run the agent
    let dm = state.dispatch_manager.clone();
    let id = dispatch_id.clone();
    tokio::spawn(async move {
        // Get the broadcast sender for real-time streaming to SSE subscribers
        let tx = {
            let dm_read = dm.read().await;
            match dm_read.get_broadcast_tx(&id) {
                Some(tx) => tx,
                None => {
                    drop(dm_read);
                    let mut dm_write = dm.write().await;
                    dm_write.fail_dispatch(&id, "Dispatch broadcast channel not found");
                    return;
                }
            }
        };

        // Create the backend and verify availability
        let claude = ClaudeBackend::new();
        if !claude.is_available() {
            let mut dm_write = dm.write().await;
            dm_write.fail_dispatch(&id, "Claude CLI not available");
            return;
        }

        // Dispatch to the backend — it streams output through the broadcast channel
        let handle = match claude.dispatch(&prompt, tx) {
            Ok(h) => h,
            Err(e) => {
                let mut dm_write = dm.write().await;
                dm_write.fail_dispatch(&id, &format!("Failed to spawn agent: {}", e));
                return;
            }
        };

        // Await the result
        match handle.await {
            Ok(Ok(output)) => {
                let mut dm_write = dm.write().await;
                // Store the accumulated output (backend already broadcasted lines)
                dm_write.append_output(&id, &output);
                dm_write.complete_dispatch(&id);

                // Save result as checkpoint
                if let Some(d) = dm_write.get_dispatch(&id) {
                    let _ = dispatch::save_result_as_checkpoint(
                        &workspace_root,
                        d,
                        &backend_name,
                    )
                    .await;
                }
            }
            Ok(Err(e)) => {
                let mut dm_write = dm.write().await;
                dm_write.fail_dispatch(&id, &format!("Agent error: {}", e));
            }
            Err(e) => {
                let mut dm_write = dm.write().await;
                dm_write.fail_dispatch(&id, &format!("Task join error: {}", e));
            }
        }
    });

    // 6. Return immediately
    Ok(Json(DispatchResponse {
        id: dispatch_id,
        status: "running".to_string(),
    }))
}
