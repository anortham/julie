use axum::extract::{Form, Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::dashboard::search_compare::{latest_compare_view, run_compare};

#[derive(Debug, Deserialize, Default)]
pub struct SearchCompareParams {
    pub days: Option<u32>,
    pub run_id: Option<i64>,
}

pub async fn index(
    State(state): State<AppState>,
    Query(params): Query<SearchCompareParams>,
) -> Result<Html<String>, StatusCode> {
    render_compare(&state, params.days.unwrap_or(7), params.run_id).await
}

pub async fn run(
    State(state): State<AppState>,
    Form(params): Form<SearchCompareParams>,
) -> Result<Html<String>, StatusCode> {
    let days = params.days.unwrap_or(7).max(1);
    let view = run_compare(&state, days)
        .await
        .or_else(|_| latest_compare_view(&state, 10, params.run_id))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("days", &days);
    context.insert("compare_runs", &view.runs);
    context.insert("selected_run", &view.selected_run);
    context.insert("compare_cases", &view.cases);

    render_template(&state, "search_compare.html", context).await
}

async fn render_compare(
    state: &AppState,
    days: u32,
    run_id: Option<i64>,
) -> Result<Html<String>, StatusCode> {
    let view =
        latest_compare_view(state, 10, run_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("days", &days);
    context.insert("compare_runs", &view.runs);
    context.insert("selected_run", &view.selected_run);
    context.insert("compare_cases", &view.cases);

    render_template(state, "search_compare.html", context).await
}
