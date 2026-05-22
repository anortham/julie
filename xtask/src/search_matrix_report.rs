use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::search_matrix::{SearchMatrixBaselineReport, SearchMatrixBaselineExecution};
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
    fs::write(
        markdown_path(out_path),
        render_baseline_report_markdown(report),
    )?;
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
    // Determine ablation label from first execution (all executions in a single
    // report share the same ablation label).
    let ablation_label = report
        .executions
        .first()
        .map(|e| e.ablation_label.as_str())
        .unwrap_or("");
    let ablation_display = if ablation_label.is_empty() {
        "baseline".to_string()
    } else {
        format!("ablation:{ablation_label}")
    };
    output.push_str(&format!(
        "- Profile: `{}`\n- Ablation: `{ablation_display}`\n- Executions: `{}`\n- Skipped repos: `{}`\n\n",
        report.profile,
        report.executions.len(),
        report.skipped_repos.len()
    ));
    output.push_str(
        "| repo | case_id | ablation | hit_count | zero_hit_reason | file_pattern_diagnostic | hint_kind |\n",
    );
    output.push_str("| --- | --- | --- | ---: | --- | --- | --- |\n");
    for execution in &report.executions {
        let hit_count = if execution.hit_count_is_lower_bound {
            format!("{}+", execution.hit_count)
        } else {
            execution.hit_count.to_string()
        };
        let ablation = if execution.ablation_label.is_empty() {
            "baseline"
        } else {
            &execution.ablation_label
        };
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} | `{}` | `{}` | `{}` |\n",
            execution.repo_name,
            execution.case_id,
            ablation,
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

/// Print a side-by-side per-case top-rank diff between two baseline reports.
///
/// For each `(repo_name, case_id)` pair that appears in either report, emits a
/// row showing the top hit name from each report and whether the rank changed.
/// Rows where both reports agree (same top-hit name or both empty) are shown
/// with a `=` marker; rows that differ get a `!` marker.
///
/// # Usage
///
/// ```text
/// cargo xtask search-matrix diff --left baseline.json --right no-stemming.json
/// ```
///
/// The diff helper is also called internally by the bakeoff post-processor to
/// produce the Markdown comparison table written to `<out>.diff.md`.
pub fn diff_baseline_reports(
    left: &SearchMatrixBaselineReport,
    right: &SearchMatrixBaselineReport,
    out: &mut dyn Write,
) -> Result<()> {
    // Index left executions by (repo_name, case_id) → execution
    let left_index: BTreeMap<(&str, &str), &SearchMatrixBaselineExecution> = left
        .executions
        .iter()
        .map(|e| ((e.repo_name.as_str(), e.case_id.as_str()), e))
        .collect();

    // Index right the same way
    let right_index: BTreeMap<(&str, &str), &SearchMatrixBaselineExecution> = right
        .executions
        .iter()
        .map(|e| ((e.repo_name.as_str(), e.case_id.as_str()), e))
        .collect();

    // Union of all keys, sorted
    let mut all_keys: Vec<(&str, &str)> = left_index
        .keys()
        .chain(right_index.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    all_keys.dedup();

    let left_label = left
        .executions
        .first()
        .map(|e| {
            if e.ablation_label.is_empty() {
                "baseline"
            } else {
                &e.ablation_label
            }
        })
        .unwrap_or("left");
    let right_label = right
        .executions
        .first()
        .map(|e| {
            if e.ablation_label.is_empty() {
                "baseline"
            } else {
                &e.ablation_label
            }
        })
        .unwrap_or("right");

    writeln!(out, "| diff | repo | case_id | {left_label} top-1 | {right_label} top-1 | {left_label} hits | {right_label} hits |")?;
    writeln!(out, "| --- | --- | --- | --- | --- | ---: | ---: |")?;

    for (repo, case_id) in &all_keys {
        let left_exec = left_index.get(&(*repo, *case_id));
        let right_exec = right_index.get(&(*repo, *case_id));

        let left_top = left_exec
            .and_then(|e| e.top_hits.first())
            .map(|h| h.name.as_str())
            .unwrap_or("∅");
        let right_top = right_exec
            .and_then(|e| e.top_hits.first())
            .map(|h| h.name.as_str())
            .unwrap_or("∅");
        let left_hits = left_exec
            .map(|e| {
                if e.hit_count_is_lower_bound {
                    format!("{}+", e.hit_count)
                } else {
                    e.hit_count.to_string()
                }
            })
            .unwrap_or_else(|| "—".to_string());
        let right_hits = right_exec
            .map(|e| {
                if e.hit_count_is_lower_bound {
                    format!("{}+", e.hit_count)
                } else {
                    e.hit_count.to_string()
                }
            })
            .unwrap_or_else(|| "—".to_string());

        let marker = if left_top == right_top { "=" } else { "!" };
        writeln!(
            out,
            "| {marker} | `{repo}` | `{case_id}` | `{left_top}` | `{right_top}` | {left_hits} | {right_hits} |"
        )?;
    }

    Ok(())
}
