use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::search_matrix::SearchMatrixBaselineReport;
use crate::search_matrix_mine::SearchMatrixSeedReport;

pub fn write_seed_report(report: &SearchMatrixSeedReport, out_path: &Path) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(out_path, serde_json::to_string_pretty(report)?)?;
    fs::write(markdown_path(out_path), render_seed_report_markdown(report))?;
    Ok(())
}

pub fn write_baseline_report(report: &SearchMatrixBaselineReport, out_path: &Path) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(out_path, serde_json::to_string_pretty(report)?)?;
    fs::write(markdown_path(out_path), render_baseline_report_markdown(report))?;
    Ok(())
}

fn markdown_path(out_path: &Path) -> PathBuf {
    out_path.with_extension("md")
}

fn render_seed_report_markdown(report: &SearchMatrixSeedReport) -> String {
    let mut output = String::new();
    output.push_str("# Search Matrix Seed Report\n\n");
    output.push_str(&format!(
        "- Window days: `{}`\n- Candidate count: `{}`\n- Cluster count: `{}`\n\n",
        report.window_days,
        report.candidates.len(),
        report.clusters.len()
    ));
    output.push_str("| family | zero_hit_reason | file_pattern_diagnostic | hint_kind | count |\n");
    output.push_str("| --- | --- | --- | --- | ---: |\n");
    for cluster in &report.clusters {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | {} |\n",
            cluster.family,
            cluster.zero_hit_reason.as_deref().unwrap_or("∅"),
            cluster.file_pattern_diagnostic.as_deref().unwrap_or("∅"),
            cluster.hint_kind.as_deref().unwrap_or("∅"),
            cluster.count
        ));
    }
    output
}

fn render_baseline_report_markdown(report: &SearchMatrixBaselineReport) -> String {
    let mut output = String::new();
    output.push_str("# Search Matrix Baseline Report\n\n");
    output.push_str(&format!(
        "- Profile: `{}`\n- Executions: `{}`\n- Skipped repos: `{}`\n\n",
        report.profile,
        report.executions.len(),
        report.skipped_repos.len()
    ));
    output.push_str("| repo | case_id | hit_count | zero_hit_reason | file_pattern_diagnostic | hint_kind |\n");
    output.push_str("| --- | --- | ---: | --- | --- | --- |\n");
    for execution in &report.executions {
        let hit_count = if execution.hit_count_is_lower_bound {
            format!("{}+", execution.hit_count)
        } else {
            execution.hit_count.to_string()
        };
        output.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` | `{}` | `{}` |\n",
            execution.repo_name,
            execution.case_id,
            hit_count,
            execution.zero_hit_reason.as_deref().unwrap_or("∅"),
            execution.file_pattern_diagnostic.as_deref().unwrap_or("∅"),
            execution.hint_kind.as_deref().unwrap_or("∅")
        ));
    }
    if !report.skipped_repos.is_empty() {
        output.push_str("\n## Skipped Repos\n\n");
        for repo in &report.skipped_repos {
            output.push_str(&format!("- `{}`: {}\n", repo.repo_name, repo.reason));
        }
    }
    if !report.summary_flags.is_empty() {
        output.push_str("\n## Summary Flags\n\n");
        for flag in &report.summary_flags {
            output.push_str(&format!("- `{}`\n", flag));
        }
    }
    output
}
