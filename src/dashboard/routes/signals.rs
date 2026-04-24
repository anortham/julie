use axum::extract::{Form, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use tera::Context;

use crate::analysis::early_warnings::{
    EarlyWarningReport, EarlyWarningReportOptions, generate_early_warning_report,
};
use crate::dashboard::routes::intelligence::open_workspace_db;
use crate::dashboard::{AppState, render_template};
use crate::search::language_config::LanguageConfigs;

#[derive(Debug, Deserialize)]
pub struct RefreshSignalsForm {
    pub csrf_token: String,
}

pub async fn index(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    open_workspace_db(&state, &workspace_id)?;

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
    if let Some(pool) = state.dashboard.workspace_pool()
        && let Some(workspace) = pool.get(workspace_id).await
        && let Some(db) = workspace.db.as_ref()
    {
        let db = std::sync::Arc::clone(db);
        let workspace_id_for_task = workspace_id.to_string();
        let configs = LanguageConfigs::load_embedded();
        let options = report_options(workspace_id, fresh);
        return tokio::task::spawn_blocking(move || {
            let db = db.lock().map_err(|error| {
                tracing::error!("Early warning report database lock poisoned: {error}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            generate_early_warning_report(&db, &configs, options).map_err(|error| {
                tracing::error!(
                    "Early warning report generation failed for {workspace_id_for_task}: {error:#}"
                );
                StatusCode::INTERNAL_SERVER_ERROR
            })
        })
        .await
        .map_err(|error| {
            tracing::error!("Early warning report task failed for {workspace_id}: {error:#}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

    let db = open_workspace_db(state, workspace_id)?;
    let configs = LanguageConfigs::load_embedded();
    let options = report_options(workspace_id, fresh);
    generate_early_warning_report(&db, &configs, options).map_err(|error| {
        tracing::error!("Early warning report generation failed for {workspace_id}: {error:#}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

fn report_options(workspace_id: &str, fresh: bool) -> EarlyWarningReportOptions {
    EarlyWarningReportOptions {
        workspace_id: workspace_id.to_string(),
        file_pattern: None,
        fresh,
        limit_per_section: Some(100),
    }
}
