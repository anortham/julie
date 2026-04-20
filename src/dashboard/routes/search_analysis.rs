use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::dashboard::search_analysis::{analyze_tool_calls, episode_stats};

#[derive(Debug, Deserialize, Default)]
pub struct SearchAnalysisParams {
    pub days: Option<u32>,
}

pub async fn index(
    State(state): State<AppState>,
    Query(params): Query<SearchAnalysisParams>,
) -> Result<Html<String>, StatusCode> {
    let days = params.days.unwrap_or(7).max(1);
    let episodes = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_tool_calls_for_search_analysis(days).ok())
        .map(|rows| analyze_tool_calls(&rows))
        .unwrap_or_default();
    let stats = episode_stats(&episodes);

    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("days", &days);
    context.insert("episodes", &episodes);
    context.insert("episode_stats", &stats);

    render_template(&state, "search_analysis.html", context).await
}
