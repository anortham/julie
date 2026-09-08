//! Formatting for early warning signals report.
//!
//! Extracted from `output.rs` to maintain single responsibility and file line limits.

use crate::cli_tools::subcommands::OutputFormat;

/// Format an early warning signals report for CLI output.
pub fn format_signals_report(
    report: &crate::analysis::EarlyWarningReport,
    format: OutputFormat,
) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(report).unwrap_or_default(),
        OutputFormat::Text => format_signals_text(report),
        OutputFormat::Markdown => format_signals_markdown(report),
    }
}

fn format_signals_text(report: &crate::analysis::EarlyWarningReport) -> String {
    let mut out = String::new();
    let s = &report.summary;
    out.push_str(&format!(
        "Early Warning Signals  (entry_points: {}, auth_coverage_candidates: {}, review_markers: {}, scheduler: {}, ep_linkage_gaps: {}, centrality_gaps: {})\n",
        s.entry_points, s.auth_coverage_candidates, s.review_markers,
        s.scheduler_signals, s.entry_point_linkage_gaps, s.high_centrality_linkage_gaps
    ));
    if report.from_cache {
        out.push_str("  (from cache)\n");
    }
    out.push('\n');

    if !report.entry_points.is_empty() {
        out.push_str("Entry Points:\n");
        for ep in &report.entry_points {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                ep.symbol_name, ep.file_path, ep.start_line, ep.annotation
            ));
        }
        out.push('\n');
    }

    if !report.auth_coverage_candidates.is_empty() {
        out.push_str("Auth Coverage Candidates:\n");
        for ac in &report.auth_coverage_candidates {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                ac.symbol_name, ac.file_path, ac.start_line, ac.annotation
            ));
        }
        out.push('\n');
    }

    if !report.review_markers.is_empty() {
        out.push_str("Review Markers:\n");
        for rm in &report.review_markers {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                rm.symbol_name, rm.file_path, rm.start_line, rm.annotation
            ));
        }
        out.push('\n');
    }

    if !report.scheduler_signals.is_empty() {
        out.push_str("Scheduler Signals:\n");
        for ss in &report.scheduler_signals {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                ss.symbol_name, ss.file_path, ss.start_line, ss.annotation
            ));
        }
        out.push('\n');
    }

    if !report.entry_point_linkage_gaps.is_empty() {
        out.push_str("Entry Point Linkage Gaps:\n");
        for gap in &report.entry_point_linkage_gaps {
            out.push_str(&format!(
                "  {} ({}:{}) [{}]\n",
                gap.symbol_name, gap.file_path, gap.start_line, gap.entry_annotation
            ));
        }
        out.push('\n');
    }

    if !report.high_centrality_linkage_gaps.is_empty() {
        out.push_str("High Centrality Linkage Gaps:\n");
        for gap in &report.high_centrality_linkage_gaps {
            out.push_str(&format!(
                "  {} ({}:{}) score={:.2}\n",
                gap.symbol_name, gap.file_path, gap.start_line, gap.reference_score
            ));
        }
    }

    out
}

fn format_signals_markdown(report: &crate::analysis::EarlyWarningReport) -> String {
    let mut out = String::new();
    let s = &report.summary;
    out.push_str("# Early Warning Signals\n\n");
    out.push_str(&format!(
        "| Metric | Count |\n|--------|-------|\n| Entry Points | {} |\n| Auth Coverage Candidates | {} |\n| Review Markers | {} |\n| Scheduler Signals | {} |\n| Entry Point Linkage Gaps | {} |\n| High Centrality Linkage Gaps | {} |\n\n",
        s.entry_points, s.auth_coverage_candidates, s.review_markers,
        s.scheduler_signals, s.entry_point_linkage_gaps, s.high_centrality_linkage_gaps
    ));

    if !report.entry_points.is_empty() {
        out.push_str("## Entry Points\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for ep in &report.entry_points {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ep.symbol_name, ep.file_path, ep.start_line, ep.annotation
            ));
        }
        out.push('\n');
    }

    if !report.auth_coverage_candidates.is_empty() {
        out.push_str("## Auth Coverage Candidates\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for ac in &report.auth_coverage_candidates {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ac.symbol_name, ac.file_path, ac.start_line, ac.annotation
            ));
        }
        out.push('\n');
    }

    if !report.review_markers.is_empty() {
        out.push_str("## Review Markers\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for rm in &report.review_markers {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                rm.symbol_name, rm.file_path, rm.start_line, rm.annotation
            ));
        }
        out.push('\n');
    }

    if !report.scheduler_signals.is_empty() {
        out.push_str("## Scheduler Signals\n\n| Symbol | File | Line | Annotation |\n|--------|------|------|------------|\n");
        for ss in &report.scheduler_signals {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                ss.symbol_name, ss.file_path, ss.start_line, ss.annotation
            ));
        }
        out.push('\n');
    }

    if !report.entry_point_linkage_gaps.is_empty() {
        out.push_str("## Entry Point Linkage Gaps\n\n| Symbol | File | Line | Entry Annotation |\n|--------|------|------|------------------|\n");
        for gap in &report.entry_point_linkage_gaps {
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                gap.symbol_name, gap.file_path, gap.start_line, gap.entry_annotation
            ));
        }
        out.push('\n');
    }

    if !report.high_centrality_linkage_gaps.is_empty() {
        out.push_str("## High Centrality Linkage Gaps\n\n| Symbol | File | Line | Reference Score |\n|--------|------|------|-----------------|\n");
        for gap in &report.high_centrality_linkage_gaps {
            out.push_str(&format!(
                "| {} | {} | {} | {:.2} |\n",
                gap.symbol_name, gap.file_path, gap.start_line, gap.reference_score
            ));
        }
    }

    out
}
