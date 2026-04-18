use std::path::PathBuf;

use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::dashboard::AppState;
use crate::dashboard::routes::projects::{ProjectsNotice, render_projects_page};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

#[derive(Debug, Deserialize)]
pub struct RegisterWorkspaceForm {
    pub path: String,
    pub csrf_token: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceActionForm {
    pub csrf_token: String,
}

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn dashboard_handler(
    state: &AppState,
) -> anyhow::Result<(JulieServerHandler, tempfile::TempDir, String)> {
    let anchor_dir = tempfile::tempdir()?;
    let anchor_path = anchor_dir.path().to_path_buf();
    let anchor_id = generate_workspace_id(&anchor_path.to_string_lossy())?;
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint(
        WorkspaceStartupHint {
            path: workspace_root,
            source: Some(WorkspaceStartupSource::Cwd),
        },
        state.dashboard.daemon_db().cloned(),
        None,
        None,
        Some(state.dashboard.sender()),
        state.dashboard.watcher_pool().cloned(),
        state.dashboard.workspace_pool().cloned(),
    )
    .await?;

    handler
        .initialize_workspace_with_force(Some(anchor_path.to_string_lossy().to_string()), false)
        .await?;

    Ok((handler, anchor_dir, anchor_id))
}

async fn disconnect_dashboard_attached_workspaces(state: &AppState, handler: &JulieServerHandler) {
    let Some(pool) = state.dashboard.workspace_pool() else {
        return;
    };

    for workspace_id in handler.session_attached_workspace_ids().await {
        pool.sync_indexed_from_db(&workspace_id).await;
        pool.disconnect_session(&workspace_id).await;
    }
}

async fn cleanup_dashboard_anchor(state: &AppState, anchor_id: &str) {
    if let Some(pool) = state.dashboard.workspace_pool() {
        pool.evict_workspace(anchor_id).await;
    }

    if let Some(watcher_pool) = state.dashboard.watcher_pool() {
        let _ = watcher_pool.remove_if_inactive(anchor_id).await;
    }

    if let Some(daemon_db) = state.dashboard.daemon_db() {
        let _ = daemon_db.delete_workspace(anchor_id);
    }

    if let Some(pool) = state.dashboard.workspace_pool() {
        let anchor_index_dir = pool.indexes_dir().join(anchor_id);
        if anchor_index_dir.exists() {
            let _ = tokio::fs::remove_dir_all(anchor_index_dir).await;
        }
    }
}

async fn run_workspace_action(state: &AppState, tool: ManageWorkspaceTool) -> ProjectsNotice {
    let (handler, _anchor_dir, anchor_id) = match dashboard_handler(state).await {
        Ok(handler) => handler,
        Err(error) => return ProjectsNotice::error("Workspace Action Failed", error.to_string()),
    };

    let action_result = tool.call_tool(&handler).await;
    disconnect_dashboard_attached_workspaces(state, &handler).await;
    cleanup_dashboard_anchor(state, &anchor_id).await;

    match action_result {
        Ok(result) => ProjectsNotice::from_text(extract_text_from_result(&result)),
        Err(error) => ProjectsNotice::error("Workspace Action Failed", error.to_string()),
    }
}

fn csrf_invalid_notice() -> ProjectsNotice {
    ProjectsNotice::error(
        "Workspace Action Blocked",
        "Dashboard action token check failed. Reload the page and try again.",
    )
}

async fn reject_invalid_csrf(state: &AppState) -> Result<Response, StatusCode> {
    render_projects_page(state, Some(csrf_invalid_notice()))
        .await
        .map(|html| (StatusCode::FORBIDDEN, html).into_response())
}

fn csrf_token_valid(state: &AppState, submitted_token: &str) -> bool {
    submitted_token == state.dashboard.action_csrf_token()
}

pub async fn register(
    State(state): State<AppState>,
    Form(form): Form<RegisterWorkspaceForm>,
) -> Result<Response, StatusCode> {
    if !csrf_token_valid(&state, &form.csrf_token) {
        return reject_invalid_csrf(&state).await;
    }

    let path = form.path.trim();
    let notice = if path.is_empty() {
        ProjectsNotice::error("Workspace Registration Failed", "Path is required.")
    } else {
        run_workspace_action(
            &state,
            ManageWorkspaceTool {
                operation: "register".to_string(),
                path: Some(path.to_string()),
                force: Some(false),
                name: None,
                workspace_id: None,
                detailed: None,
            },
        )
        .await
    };

    render_projects_page(&state, Some(notice))
        .await
        .map(IntoResponse::into_response)
}

pub async fn open(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Form(form): Form<WorkspaceActionForm>,
) -> Result<Response, StatusCode> {
    if !csrf_token_valid(&state, &form.csrf_token) {
        return reject_invalid_csrf(&state).await;
    }

    let notice = run_workspace_action(
        &state,
        ManageWorkspaceTool {
            operation: "open".to_string(),
            path: None,
            force: Some(false),
            name: None,
            workspace_id: Some(workspace_id),
            detailed: None,
        },
    )
    .await;

    render_projects_page(&state, Some(notice))
        .await
        .map(IntoResponse::into_response)
}

pub async fn refresh(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Form(form): Form<WorkspaceActionForm>,
) -> Result<Response, StatusCode> {
    if !csrf_token_valid(&state, &form.csrf_token) {
        return reject_invalid_csrf(&state).await;
    }

    let notice = run_workspace_action(
        &state,
        ManageWorkspaceTool {
            operation: "refresh".to_string(),
            path: None,
            force: Some(false),
            name: None,
            workspace_id: Some(workspace_id),
            detailed: None,
        },
    )
    .await;

    render_projects_page(&state, Some(notice))
        .await
        .map(IntoResponse::into_response)
}

pub async fn delete(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Form(form): Form<WorkspaceActionForm>,
) -> Result<Response, StatusCode> {
    if !csrf_token_valid(&state, &form.csrf_token) {
        return reject_invalid_csrf(&state).await;
    }

    let notice = run_workspace_action(
        &state,
        ManageWorkspaceTool {
            operation: "remove".to_string(),
            path: None,
            force: Some(false),
            name: None,
            workspace_id: Some(workspace_id),
            detailed: None,
        },
    )
    .await;

    render_projects_page(&state, Some(notice))
        .await
        .map(IntoResponse::into_response)
}
