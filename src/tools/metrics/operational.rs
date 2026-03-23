//! Operational metrics formatting for session and history views.

use super::session::{SessionMetrics, ToolKind};
use crate::database::HistorySummary;
use std::sync::atomic::Ordering;

/// Format bytes into human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1_048_576 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{:.1}GB", bytes as f64 / 1_073_741_824.0)
    }
}

/// Compute p95 from a mutable slice of durations.
pub fn percentile_95(durations: &mut [f64]) -> f64 {
    if durations.is_empty() {
        return 0.0;
    }
    durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((durations.len() as f64) * 0.95).ceil() as usize;
    let idx = idx.min(durations.len()) - 1;
    durations[idx]
}

/// Format uptime duration into human-readable string.
fn format_uptime(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format session metrics from in-memory atomic counters.
pub fn format_session_from_metrics(metrics: &SessionMetrics) -> String {
    let uptime = metrics.session_start.elapsed();
    let total_calls = metrics.total_calls.load(Ordering::Relaxed);
    let total_duration_us = metrics.total_duration_us.load(Ordering::Relaxed);
    let total_source = metrics.total_source_bytes.load(Ordering::Relaxed);
    let total_output = metrics.total_output_bytes.load(Ordering::Relaxed);

    let mut lines = Vec::new();
    lines.push(format!(
        "Session Metrics (uptime: {})\n",
        format_uptime(uptime)
    ));

    // Per-tool breakdown
    lines.push("Tool Usage:".to_string());
    let mut any_tool = false;
    for i in 0..ToolKind::COUNT {
        let counters = &metrics.per_tool[i];
        let calls = counters.calls.load(Ordering::Relaxed);
        if calls == 0 {
            continue;
        }
        any_tool = true;
        let kind = match i {
            0 => ToolKind::FastSearch,
            1 => ToolKind::FastRefs,
            2 => ToolKind::GetSymbols,
            3 => ToolKind::DeepDive,
            4 => ToolKind::GetContext,
            5 => ToolKind::RenameSymbol,
            6 => ToolKind::ManageWorkspace,
            7 => ToolKind::QueryMetrics,
            _ => unreachable!(),
        };
        let dur_us = counters.duration_us.load(Ordering::Relaxed);
        let avg_ms = if calls > 0 {
            (dur_us as f64 / calls as f64) / 1000.0
        } else {
            0.0
        };
        let out = counters.output_bytes.load(Ordering::Relaxed);
        lines.push(format!(
            "  {:<20} {:>3} calls   avg {:.1}ms   output: {}",
            kind.name(),
            calls,
            avg_ms,
            format_bytes(out)
        ));
    }
    if !any_tool {
        lines.push("  (no tool calls yet)".to_string());
    }

    // Totals
    let avg_ms = if total_calls > 0 {
        (total_duration_us as f64 / total_calls as f64) / 1000.0
    } else {
        0.0
    };
    lines.push(format!(
        "\nTotals: {} calls | avg {:.1}ms",
        total_calls, avg_ms
    ));

    // Context efficiency
    lines.push("\nContext Efficiency:".to_string());
    lines.push(format!(
        "  Source files examined: {}",
        format_bytes(total_source)
    ));
    lines.push(format!("  Output returned: {}", format_bytes(total_output)));
    let not_injected = total_source.saturating_sub(total_output);
    lines.push(format!(
        "  NOT injected into context: {}",
        format_bytes(not_injected)
    ));

    lines.join("\n")
}

/// Format history summary from database aggregation.
pub fn format_history_output(history: &HistorySummary) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "Historical Metrics ({} sessions, {} total calls)\n",
        history.session_count, history.total_calls
    ));

    lines.push("Tool Performance:".to_string());
    for tool in &history.per_tool {
        let mut durs = history
            .durations_by_tool
            .get(&tool.tool_name)
            .cloned()
            .unwrap_or_default();
        let p95 = percentile_95(&mut durs);
        lines.push(format!(
            "  {:<20} {:>4} calls   avg {:.1}ms   p95 {:.1}ms",
            tool.tool_name, tool.call_count, tool.avg_duration_ms, p95
        ));
    }

    lines.push("\nContext Efficiency (cumulative):".to_string());
    lines.push(format!(
        "  Source examined: {}",
        format_bytes(history.total_source_bytes)
    ));
    lines.push(format!(
        "  Output returned: {}",
        format_bytes(history.total_output_bytes)
    ));
    let not_injected = history
        .total_source_bytes
        .saturating_sub(history.total_output_bytes);
    lines.push(format!(
        "  NOT injected into context: {}",
        format_bytes(not_injected)
    ));

    lines.join("\n")
}
