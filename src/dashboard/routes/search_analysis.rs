use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::dashboard::search_analysis::{
    aggregate_problems, analyze_tool_calls, episode_stats, extract_reformulation_pairs,
    has_trace_data,
};

#[derive(Debug, Deserialize, Default)]
pub struct SearchAnalysisParams {
    pub days: Option<u32>,
    pub hours: Option<u32>,
    pub show_all: Option<bool>,
}

pub async fn index(
    State(state): State<AppState>,
    Query(params): Query<SearchAnalysisParams>,
) -> Result<Html<String>, StatusCode> {
    let (window_secs, window_label) = if let Some(h) = params.hours {
        let h = h.max(1);
        (h as i64 * 3600, format!("{h}h"))
    } else {
        let d = params.days.unwrap_or(7).max(1);
        (d as i64 * 86400, format!("{d}d"))
    };
    let show_all = params.show_all.unwrap_or(false);

    let episodes = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_tool_calls_for_search_analysis(window_secs).ok())
        .map(|rows| analyze_tool_calls(&rows))
        .unwrap_or_default();

    let traced_episodes: Vec<_> = episodes.into_iter().filter(|e| has_trace_data(e)).collect();

    let stats = episode_stats(&traced_episodes);
    let problems = aggregate_problems(&traced_episodes);
    let reformulations = extract_reformulation_pairs(&traced_episodes);

    let total_episode_count = traced_episodes.len();
    let filtered_episodes: Vec<_> = if show_all {
        traced_episodes
    } else {
        traced_episodes
            .into_iter()
            .filter(|e| e.suspicious)
            .collect()
    };

    let window_param = if params.hours.is_some() {
        format!("hours={}", params.hours.unwrap())
    } else {
        format!("days={}", params.days.unwrap_or(7))
    };

    let mut context = Context::new();
    context.insert("active_page", "search");
    context.insert("window_label", &window_label);
    context.insert("window_param", &window_param);
    context.insert("show_all", &show_all);
    context.insert("episodes", &filtered_episodes);
    context.insert("total_episode_count", &total_episode_count);
    context.insert("episode_stats", &stats);
    context.insert("problems", &problems);
    context.insert("reformulations", &reformulations);

    render_template(&state, "search_analysis.html", context).await
}
