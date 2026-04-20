use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::dashboard::search_analysis::{
    aggregate_problems, analyze_tool_calls, episode_stats, extract_reformulation_pairs,
};

#[derive(Debug, Deserialize, Default)]
pub struct SearchAnalysisParams {
    pub days: Option<u32>,
    pub show_all: Option<bool>,
}

pub async fn index(
    State(state): State<AppState>,
    Query(params): Query<SearchAnalysisParams>,
) -> Result<Html<String>, StatusCode> {
    let days = params.days.unwrap_or(7).max(1);
    let show_all = params.show_all.unwrap_or(false);
    let episodes = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_tool_calls_for_search_analysis(days).ok())
        .map(|rows| analyze_tool_calls(&rows))
        .unwrap_or_default();
    let stats = episode_stats(&episodes);
    let problems = aggregate_problems(&episodes);
    let reformulations = extract_reformulation_pairs(&episodes);

    let total_episode_count = episodes.len();
    let filtered_episodes: Vec<_> = if show_all {
        episodes
    } else {
        episodes.into_iter().filter(|e| e.suspicious).collect()
    };

    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("days", &days);
    context.insert("show_all", &show_all);
    context.insert("episodes", &filtered_episodes);
    context.insert("total_episode_count", &total_episode_count);
    context.insert("episode_stats", &stats);
    context.insert("problems", &problems);
    context.insert("reformulations", &reformulations);

    render_template(&state, "search_analysis.html", context).await
}
