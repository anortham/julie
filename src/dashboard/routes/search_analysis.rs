use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::dashboard::search_analysis::{
    analyze_tool_calls, compute_flags, compute_summary, has_trace_data,
};

#[derive(Debug, Deserialize, Default)]
pub struct SearchAnalysisParams {
    pub days: Option<u32>,
    pub hours: Option<u32>,
    pub show_all: Option<bool>,
    pub flag: Option<String>,
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

    let mut episodes: Vec<_> = state
        .dashboard
        .daemon_db()
        .and_then(|db| db.list_tool_calls_for_search_analysis(window_secs).ok())
        .map(|rows| analyze_tool_calls(&rows))
        .unwrap_or_default()
        .into_iter()
        .filter(|e| has_trace_data(e))
        .collect();

    for ep in &mut episodes {
        compute_flags(ep);
    }

    let summary = compute_summary(&episodes);
    let total_episode_count = episodes.len();

    if let Some(ref flag) = params.flag {
        episodes.retain(|e| e.flags.contains(flag));
    }

    let filtered_episodes: Vec<_> = if show_all {
        episodes
    } else {
        episodes.into_iter().filter(|e| !e.flags.is_empty()).collect()
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
    context.insert("active_flag", &params.flag.as_deref().unwrap_or(""));
    context.insert("episodes", &filtered_episodes);
    context.insert("total_episode_count", &total_episode_count);
    context.insert("summary", &summary);

    render_template(&state, "search_analysis.html", context).await
}
