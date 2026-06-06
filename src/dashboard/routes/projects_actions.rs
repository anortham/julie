use std::path::PathBuf;

use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::dashboard::AppState;
use crate::dashboard::routes::projects::{ProjectsNotice, render_projects_page};
use crate::dashboard::state::DashboardDaemonPhase;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};
use tracing::warn;

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

pub(crate) async fn dashboard_handler(
    state: &AppState,
) -> anyhow::Result<(JulieServerHandler, tempfile::TempDir, String)> {
    let anchor_dir = tempfile::tempdir()?;
    let anchor_path = anchor_dir.path().to_path_buf();
    let anchor_id = generate_workspace_id(&anchor_path.to_string_lossy())?;
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        WorkspaceStartupHint {
            path: workspace_root,
            source: Some(WorkspaceStartupSource::Cwd),
        },
        state.dashboard.daemon_db().cloned(),
        None,
        None,
        Some(state.dashboard.sender()),
    )
    .await?;

    handler
        .initialize_workspace_with_force(Some(anchor_path.to_string_lossy().to_string()), false)
        .await?;

    Ok((handler, anchor_dir, anchor_id))
}

pub(crate) async fn disconnect_dashboard_attached_workspaces(handler: &JulieServerHandler) {
    for workspace_id in handler.session_attached_workspace_ids().await {
        if let Err(error) = handler.detach_workspace_for_session(&workspace_id).await {
            warn!(
                workspace_id,
                "Failed to detach dashboard workspace session: {error}"
            );
        }
    }
}

pub(crate) async fn cleanup_dashboard_anchor(state: &AppState, anchor_id: &str) {
    // Pool detached (Phase 3d.2b-ii) — no pool eviction or index cleanup.
    if let Some(daemon_db) = state.dashboard.daemon_db() {
        let _ = daemon_db.delete_workspace(anchor_id);
    }
}

async fn run_workspace_action(state: &AppState, tool: ManageWorkspaceTool) -> ProjectsNotice {
    if let Some(notice) = workspace_action_blocked_notice(state) {
        return notice;
    }

    let (handler, _anchor_dir, anchor_id) = match dashboard_handler(state).await {
        Ok(handler) => handler,
        Err(error) => return ProjectsNotice::error("Workspace Action Failed", error.to_string()),
    };

    let action_result = tool.call_tool(&handler).await;
    disconnect_dashboard_attached_workspaces(&handler).await;
    cleanup_dashboard_anchor(state, &anchor_id).await;

    match action_result {
        Ok(result) => {
            let mut notice = ProjectsNotice::from_text(extract_text_from_result(&result));
            if result.is_error.unwrap_or(false) {
                notice.kind = "danger".to_string();
            }
            notice
        }
        Err(error) => ProjectsNotice::error("Workspace Action Failed", error.to_string()),
    }
}

fn workspace_action_blocked_notice(state: &AppState) -> Option<ProjectsNotice> {
    if state.dashboard.accepts_workspace_actions() {
        return None;
    }

    let phase = state.dashboard.daemon_phase_kind();
    let detail = if phase != DashboardDaemonPhase::Ready {
        format!(
            "daemon {} is not accepting dashboard workspace actions. Reload after shutdown or restart completes.",
            phase.label()
        )
    } else {
        "daemon restart is pending. Reload after restart completes.".to_string()
    };

    Some(ProjectsNotice::error("Workspace Action Blocked", detail))
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
