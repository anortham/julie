//! Metrics page route handlers.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;

#[derive(Deserialize)]
pub struct MetricsParams {
    #[serde(default = "default_days")]
    pub days: u32,
    pub workspace: Option<String>,
}

fn default_days() -> u32 {
    7
}

pub async fn index(
    State(state): State<AppState>,
    Query(params): Query<MetricsParams>,
) -> Result<Html<String>, StatusCode> {
    let db = match state.dashboard.daemon_db() {
        Some(db) => db,
        None => {
            let mut context = Context::new();
            context.insert("active_page", "metrics");
            context.insert("no_data", &true);
            return render_template(&state, "metrics.html", context).await;
        }
    };

    let workspaces = db.list_workspaces().unwrap_or_default();
    let workspace_id = params.workspace.as_deref().unwrap_or("");

    // Query tool call history
    let history = if workspace_id.is_empty() {
        // Aggregate across all workspaces
        let mut total = crate::database::HistorySummary::default();
        for ws in &workspaces {
            if let Ok(h) = db.query_tool_call_history(&ws.workspace_id, params.days) {
                total.session_count += h.session_count;
                total.total_calls += h.total_calls;
                total.total_source_bytes += h.total_source_bytes;
                total.total_output_bytes += h.total_output_bytes;
                for tool in h.per_tool {
                    if let Some(existing) = total
                        .per_tool
                        .iter_mut()
                        .find(|t| t.tool_name == tool.tool_name)
                    {
                        let prev_count = existing.call_count;
                        existing.call_count += tool.call_count;
                        existing.avg_duration_ms = (existing.avg_duration_ms
                            * prev_count as f64
                            + tool.avg_duration_ms * tool.call_count as f64)
                            / existing.call_count as f64;
                    } else {
                        total.per_tool.push(tool);
                    }
                }
                for (name, durations) in h.durations_by_tool {
                    total
                        .durations_by_tool
                        .entry(name)
                        .or_default()
                        .extend(durations);
                }
            }
        }
        total
    } else {
        db.query_tool_call_history(workspace_id, params.days)
            .unwrap_or_default()
    };

    // Sort by call count descending
    let mut tools = history.per_tool.clone();
    tools.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    let max_calls = tools.first().map(|t| t.call_count).unwrap_or(1);

    // Compute p95 per tool
    let mut p95_by_tool = std::collections::HashMap::<String, f64>::new();
    for (name, durations) in &history.durations_by_tool {
        let mut sorted: Vec<f64> = durations.clone();
        sorted.sort_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !sorted.is_empty() {
            let idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
            p95_by_tool.insert(name.clone(), sorted[idx]);
        }
    }

    // Context efficiency: how much data Julie kept out of the LLM context
    let source_bytes = history.total_source_bytes;
    let output_bytes = history.total_output_bytes;
    let saved_bytes = source_bytes.saturating_sub(output_bytes);

    // Tool success rate
    let (success_total, success_ok) = if workspace_id.is_empty() {
        let mut total = 0i64;
        let mut ok = 0i64;
        for ws in &workspaces {
            if let Ok((t, o)) = db.get_tool_success_rate(&ws.workspace_id, params.days) {
                total += t;
                ok += o;
            }
        }
        (total, ok)
    } else {
        db.get_tool_success_rate(workspace_id, params.days).unwrap_or((0, 0))
    };

    let success_rate = if success_total > 0 {
        (success_ok as f64 / success_total as f64) * 100.0
    } else {
        100.0
    };

    let mut context = Context::new();
    context.insert("active_page", "metrics");
    context.insert("no_data", &false);
    context.insert("days", &params.days);
    context.insert("selected_workspace", &workspace_id);
    context.insert("workspaces", &workspaces);
    context.insert("total_calls", &history.total_calls);
    context.insert("session_count", &history.session_count);
    context.insert("tools", &tools);
    context.insert("max_calls", &max_calls);
    context.insert("p95_by_tool", &p95_by_tool);
    context.insert("source_bytes", &source_bytes);
    context.insert("output_bytes", &output_bytes);
    context.insert("saved_bytes", &saved_bytes);
    context.insert("success_rate", &success_rate);
    context.insert("success_total", &success_total);

    render_template(&state, "metrics.html", context).await
}

pub async fn table(
    State(state): State<AppState>,
    Query(params): Query<MetricsParams>,
) -> Result<Html<String>, StatusCode> {
    // Render just the table partial for htmx partial swaps
    let db = match state.dashboard.daemon_db() {
        Some(db) => db,
        None => return Ok(Html("<p>No data</p>".to_string())),
    };

    let workspace_id = params.workspace.as_deref().unwrap_or("");
    let history = if workspace_id.is_empty() {
        let mut total = crate::database::HistorySummary::default();
        for ws in db.list_workspaces().unwrap_or_default() {
            if let Ok(h) = db.query_tool_call_history(&ws.workspace_id, params.days) {
                total.total_calls += h.total_calls;
                for tool in h.per_tool {
                    if let Some(existing) = total.per_tool.iter_mut().find(|t| t.tool_name == tool.tool_name) {
                        let prev = existing.call_count;
                        existing.call_count += tool.call_count;
                        existing.avg_duration_ms = (existing.avg_duration_ms * prev as f64
                            + tool.avg_duration_ms * tool.call_count as f64) / existing.call_count as f64;
                    } else {
                        total.per_tool.push(tool);
                    }
                }
                for (name, durations) in h.durations_by_tool {
                    total.durations_by_tool.entry(name).or_default().extend(durations);
                }
            }
        }
        total
    } else {
        db.query_tool_call_history(workspace_id, params.days).unwrap_or_default()
    };

    let mut tools = history.per_tool;
    tools.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    let max_calls = tools.first().map(|t| t.call_count).unwrap_or(1);

    let mut p95_by_tool = std::collections::HashMap::<String, f64>::new();
    for (name, durations) in &history.durations_by_tool {
        let mut sorted = durations.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !sorted.is_empty() {
            let idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
            p95_by_tool.insert(name.clone(), sorted[idx]);
        }
    }

    let mut context = tera::Context::new();
    context.insert("tools", &tools);
    context.insert("max_calls", &max_calls);
    context.insert("p95_by_tool", &p95_by_tool);

    render_template(&state, "partials/metrics_table.html", context).await
}
