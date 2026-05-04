//! Metrics page route handlers.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Deserialize;
use serde::Serialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use crate::database::{HistorySummary, ToolCallSummary};

#[derive(Deserialize)]
pub struct MetricsParams {
    #[serde(default = "default_days")]
    pub days: u32,
    pub workspace: Option<String>,
}

fn default_days() -> u32 {
    7
}

#[derive(Serialize)]
struct ToolMetricView {
    tool_name: String,
    call_count: u64,
    avg_duration_ms: f64,
    total_input_bytes: u64,
    total_source_bytes: u64,
    total_output_bytes: u64,
    request_bytes_text: String,
    activity_bytes_text: String,
}

fn merge_history(total: &mut HistorySummary, history: HistorySummary) {
    total.session_count += history.session_count;
    total.total_calls += history.total_calls;
    total.total_input_bytes += history.total_input_bytes;
    total.total_source_bytes += history.total_source_bytes;
    total.total_output_bytes += history.total_output_bytes;

    for tool in history.per_tool {
        if let Some(existing) = total
            .per_tool
            .iter_mut()
            .find(|candidate| candidate.tool_name == tool.tool_name)
        {
            merge_tool_summary(existing, tool);
        } else {
            total.per_tool.push(tool);
        }
    }

    for (name, durations) in history.durations_by_tool {
        total
            .durations_by_tool
            .entry(name)
            .or_default()
            .extend(durations);
    }
}

fn merge_tool_summary(existing: &mut ToolCallSummary, tool: ToolCallSummary) {
    let previous_count = existing.call_count;
    existing.call_count += tool.call_count;
    existing.avg_duration_ms = (existing.avg_duration_ms * previous_count as f64
        + tool.avg_duration_ms * tool.call_count as f64)
        / existing.call_count as f64;
    existing.total_input_bytes += tool.total_input_bytes;
    existing.total_source_bytes += tool.total_source_bytes;
    existing.total_output_bytes += tool.total_output_bytes;
}

fn sorted_tool_views(history: &HistorySummary) -> Vec<ToolMetricView> {
    let mut tools = history.per_tool.clone();
    tools.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    tools.into_iter().map(tool_metric_view).collect()
}

fn tool_metric_view(tool: ToolCallSummary) -> ToolMetricView {
    let request_label = if is_edit_tool(&tool.tool_name) {
        "edit request"
    } else {
        "Julie request"
    };
    let request_bytes_text = if tool.total_input_bytes > 0 {
        format!("{} {request_label}", format_bytes(tool.total_input_bytes))
    } else {
        format!("{request_label} bytes not recorded")
    };
    let activity_bytes_text = match (tool.total_source_bytes, tool.total_output_bytes) {
        (0, 0) => "no source or output bytes recorded".to_string(),
        (0, output) => format!("{} returned", format_bytes(output)),
        (source, 0) => format!("{} examined", format_bytes(source)),
        (source, output) => {
            format!(
                "{} examined, {} returned",
                format_bytes(source),
                format_bytes(output)
            )
        }
    };

    ToolMetricView {
        tool_name: tool.tool_name,
        call_count: tool.call_count,
        avg_duration_ms: tool.avg_duration_ms,
        total_input_bytes: tool.total_input_bytes,
        total_source_bytes: tool.total_source_bytes,
        total_output_bytes: tool.total_output_bytes,
        request_bytes_text,
        activity_bytes_text,
    }
}

fn is_edit_tool(tool_name: &str) -> bool {
    matches!(tool_name, "edit_file" | "rewrite_symbol" | "rename_symbol")
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    let value = bytes as f64;

    if value >= MB {
        format!("{:.1} MB", value / MB)
    } else if value >= KB {
        format!("{:.1} KB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn p95_by_tool(history: &HistorySummary) -> std::collections::HashMap<String, f64> {
    let mut p95_by_tool = std::collections::HashMap::<String, f64>::new();
    for (name, durations) in &history.durations_by_tool {
        let mut sorted = durations.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if !sorted.is_empty() {
            let idx = ((sorted.len() as f64 * 0.95) as usize).min(sorted.len() - 1);
            p95_by_tool.insert(name.clone(), sorted[idx]);
        }
    }
    p95_by_tool
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
        let mut total = HistorySummary::default();
        for ws in &workspaces {
            if let Ok(h) = db.query_tool_call_history(&ws.workspace_id, params.days) {
                merge_history(&mut total, h);
            }
        }
        total
    } else {
        db.query_tool_call_history(workspace_id, params.days)
            .unwrap_or_default()
    };

    // Sort by call count descending
    let tools = sorted_tool_views(&history);
    let max_calls = tools.first().map(|t| t.call_count).unwrap_or(1);

    // Compute p95 per tool
    let p95_by_tool = p95_by_tool(&history);

    // Context efficiency: how much data Julie kept out of the LLM context
    let input_bytes = history.total_input_bytes;
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
        db.get_tool_success_rate(workspace_id, params.days)
            .unwrap_or((0, 0))
    };

    let success_rate = if success_total > 0 {
        (success_ok as f64 / success_total as f64) * 100.0
    } else {
        100.0
    };
    let failure_count = success_total.saturating_sub(success_ok);

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
    context.insert("input_bytes", &input_bytes);
    context.insert("output_bytes", &output_bytes);
    context.insert("saved_bytes", &saved_bytes);
    context.insert("success_rate", &success_rate);
    context.insert("success_total", &success_total);
    context.insert("success_ok", &success_ok);
    context.insert("failure_count", &failure_count);

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
        let mut total = HistorySummary::default();
        for ws in db.list_workspaces().unwrap_or_default() {
            if let Ok(h) = db.query_tool_call_history(&ws.workspace_id, params.days) {
                merge_history(&mut total, h);
            }
        }
        total
    } else {
        db.query_tool_call_history(workspace_id, params.days)
            .unwrap_or_default()
    };

    let tools = sorted_tool_views(&history);
    let max_calls = tools.first().map(|t| t.call_count).unwrap_or(1);

    let p95_by_tool = p95_by_tool(&history);

    let mut context = tera::Context::new();
    context.insert("tools", &tools);
    context.insert("max_calls", &max_calls);
    context.insert("p95_by_tool", &p95_by_tool);

    render_template(&state, "partials/metrics_table.html", context).await
}

/// Returns just the summary cards partial for htmx polling.
pub async fn summary(
    State(state): State<AppState>,
    Query(params): Query<MetricsParams>,
) -> Result<Html<String>, StatusCode> {
    let db = match state.dashboard.daemon_db() {
        Some(db) => db,
        None => return Ok(Html("<p>No data</p>".to_string())),
    };

    let workspaces = db.list_workspaces().unwrap_or_default();
    let workspace_id = params.workspace.as_deref().unwrap_or("");

    let history = if workspace_id.is_empty() {
        let mut total = HistorySummary::default();
        for ws in &workspaces {
            if let Ok(h) = db.query_tool_call_history(&ws.workspace_id, params.days) {
                merge_history(&mut total, h);
            }
        }
        total
    } else {
        db.query_tool_call_history(workspace_id, params.days)
            .unwrap_or_default()
    };

    let input_bytes = history.total_input_bytes;
    let source_bytes = history.total_source_bytes;
    let output_bytes = history.total_output_bytes;
    let saved_bytes = source_bytes.saturating_sub(output_bytes);

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
        db.get_tool_success_rate(workspace_id, params.days)
            .unwrap_or((0, 0))
    };

    let success_rate = if success_total > 0 {
        (success_ok as f64 / success_total as f64) * 100.0
    } else {
        100.0
    };
    let failure_count = success_total.saturating_sub(success_ok);
    let tools = sorted_tool_views(&history);

    let mut context = tera::Context::new();
    context.insert("total_calls", &history.total_calls);
    context.insert("session_count", &history.session_count);
    context.insert("tools", &tools);
    context.insert("input_bytes", &input_bytes);
    context.insert("source_bytes", &source_bytes);
    context.insert("output_bytes", &output_bytes);
    context.insert("saved_bytes", &saved_bytes);
    context.insert("success_rate", &success_rate);
    context.insert("success_total", &success_total);
    context.insert("success_ok", &success_ok);
    context.insert("failure_count", &failure_count);

    render_template(&state, "partials/metrics_summary.html", context).await
}
