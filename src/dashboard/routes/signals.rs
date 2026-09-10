use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use tera::Context;

use crate::analysis::early_warnings::{EarlyWarningReport, ReportSummary};
use crate::dashboard::routes::intelligence::require_registered_workspace;
use crate::dashboard::{AppState, render_template};

#[derive(Debug, Deserialize)]
pub struct RefreshSignalsForm {
    pub csrf_token: String,
}

pub async fn index(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    require_registered_workspace(&state, &workspace_id)?;

    let mut context = Context::new();
    context.insert("active_page", "signals");
    context.insert("workspace_id", &workspace_id);

    render_template(&state, "signals.html", context).await
}

pub async fn summary(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    render_summary(&state, &workspace_id, false).await
}

pub async fn refresh(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Form(form): Form<RefreshSignalsForm>,
) -> Result<Response, StatusCode> {
    if form.csrf_token != state.dashboard.action_csrf_token() {
        return Ok((
            StatusCode::FORBIDDEN,
            Html("Dashboard action token check failed. Reload the page and try again.".to_string()),
        )
            .into_response());
    }

    render_summary(&state, &workspace_id, true)
        .await
        .map(IntoResponse::into_response)
}

async fn render_summary(
    state: &AppState,
    workspace_id: &str,
    fresh: bool,
) -> Result<Html<String>, StatusCode> {
    let report = load_report(state, workspace_id, fresh).await?;

    let mut context = Context::new();
    context.insert("workspace_id", workspace_id);
    context.insert("report", &report);

    render_template(state, "partials/signals_summary.html", context).await
}

async fn load_report(
    state: &AppState,
    workspace_id: &str,
    fresh: bool,
) -> Result<EarlyWarningReport, StatusCode> {
    require_registered_workspace(state, workspace_id)?;
    let _ = fresh;
    Ok(EarlyWarningReport {
        workspace_id: workspace_id.to_string(),
        file_pattern: None,
        generated_at: 0,
        from_cache: false,
        facts_revision: 0,
        projection_revision: 0,
        config_schema_version: 0,
        summary: ReportSummary {
            entry_points: 0,
            auth_coverage_candidates: 0,
            review_markers: 0,
            scheduler_signals: 0,
            entry_point_linkage_gaps: 0,
            high_centrality_linkage_gaps: 0,
        },
        entry_points: Vec::new(),
        auth_coverage_candidates: Vec::new(),
        review_markers: Vec::new(),
        scheduler_signals: Vec::new(),
        entry_point_linkage_gaps: Vec::new(),
        high_centrality_linkage_gaps: Vec::new(),
    })
}
