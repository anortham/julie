use super::comparison::{
    QueryResultDelta, RankJump, SearchHitSnapshot, SearchSnapshot, SnapshotDiff, TopResultChange,
    capture_fixture_snapshot_from_file, diff_snapshots, load_query_set, write_snapshot_to_path,
};
use std::path::PathBuf;

const QUERY_SET_RELATIVE_PATH: &str = "fixtures/search-quality/tantivy-upgrade-queries.json";
const BASELINE_RELATIVE_PATH: &str = "target/search-quality/tantivy-0.26-baseline.json";
const MIN_RANK_JUMP: usize = 3;

fn project_path(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}

fn render_comparison_report(
    baseline: &SearchSnapshot,
    current: &SearchSnapshot,
    diff: &SnapshotDiff,
    min_rank_jump: usize,
) -> String {
    let (classification, rationale) = classify_drift(diff);
    let additions_count = count_delta_entries(&diff.additions);
    let removals_count = count_delta_entries(&diff.removals);

    let mut lines = vec![
        "# Tantivy 0.26 comparison report".to_string(),
        String::new(),
        "## Snapshot metadata".to_string(),
        format!("- Baseline snapshot: `{}`", BASELINE_RELATIVE_PATH),
        format!(
            "- Baseline suite: `{}` (version {})",
            baseline.metadata.suite, baseline.metadata.version
        ),
        format!(
            "- Current capture source: `{}`",
            current.metadata.capture_source
        ),
        format!("- Compared queries: {}", diff.compared_queries),
        format!("- Changed top results: {}", diff.changed_top_results.len()),
        format!(
            "- Additions: {} queries, {} entries",
            diff.additions.len(),
            additions_count
        ),
        format!(
            "- Removals: {} queries, {} entries",
            diff.removals.len(),
            removals_count
        ),
        format!(
            "- Large rank jumps (|delta| >= {}): {}",
            min_rank_jump,
            diff.rank_jumps.len()
        ),
        String::new(),
        "## Drift classification".to_string(),
        format!("- Classification: **{}**", classification),
        format!("- Rationale: {}", rationale),
        String::new(),
    ];

    lines.extend(render_changed_top_results(&diff.changed_top_results));
    lines.extend(render_result_deltas("## Additions", &diff.additions));
    lines.extend(render_result_deltas("## Removals", &diff.removals));
    lines.extend(render_rank_jumps(&diff.rank_jumps, min_rank_jump));

    lines.join("\n")
}

fn classify_drift(diff: &SnapshotDiff) -> (&'static str, &'static str) {
    let additions_count = count_delta_entries(&diff.additions);
    let removals_count = count_delta_entries(&diff.removals);

    if diff.changed_top_results.is_empty()
        && diff.rank_jumps.is_empty()
        && additions_count == 0
        && removals_count == 0
    {
        return (
            "neutral",
            "No top-N drift was observed between baseline and current snapshots.",
        );
    }

    if diff.changed_top_results.is_empty()
        && diff.rank_jumps.is_empty()
        && additions_count == removals_count
    {
        return (
            "neutral",
            "Observed drift is limited to balanced tail substitutions with no top-result or large-rank movement.",
        );
    }

    if diff.changed_top_results.is_empty() && diff.removals.is_empty() && additions_count > 0 {
        return (
            "better",
            "Only additions were observed, with no top-result swaps, removals, or large rank jumps.",
        );
    }

    (
        "regression",
        "Top-result swaps, removals, or large rank jumps were observed and need manual review.",
    )
}

fn count_delta_entries(deltas: &[QueryResultDelta]) -> usize {
    deltas
        .iter()
        .map(|delta| delta.entries.len())
        .sum::<usize>()
}

fn render_changed_top_results(changes: &[TopResultChange]) -> Vec<String> {
    let mut lines = vec!["## Changed top results".to_string()];

    if changes.is_empty() {
        lines.push("- None".to_string());
        lines.push(String::new());
        return lines;
    }

    for change in changes {
        let before = change
            .before
            .as_ref()
            .map(format_hit)
            .unwrap_or_else(|| "<none>".to_string());
        let after = change
            .after
            .as_ref()
            .map(format_hit)
            .unwrap_or_else(|| "<none>".to_string());
        lines.push(format!("- `{}`", change.query_id));
        lines.push(format!("  - before: {}", before));
        lines.push(format!("  - after: {}", after));
    }

    lines.push(String::new());
    lines
}

fn render_result_deltas(title: &str, deltas: &[QueryResultDelta]) -> Vec<String> {
    let mut lines = vec![title.to_string()];

    if deltas.is_empty() {
        lines.push("- None".to_string());
        lines.push(String::new());
        return lines;
    }

    for delta in deltas {
        lines.push(format!(
            "- `{}` ({} entries)",
            delta.query_id,
            delta.entries.len()
        ));
        for hit in &delta.entries {
            lines.push(format!("  - {}", format_hit(hit)));
        }
    }

    lines.push(String::new());
    lines
}

fn render_rank_jumps(rank_jumps: &[RankJump], min_rank_jump: usize) -> Vec<String> {
    let mut lines = vec![format!(
        "## Large rank jumps (|delta| >= {})",
        min_rank_jump
    )];

    if rank_jumps.is_empty() {
        lines.push("- None".to_string());
        lines.push(String::new());
        return lines;
    }

    for jump in rank_jumps {
        let symbol = jump.symbol_name.as_deref().unwrap_or("<none>");
        lines.push(format!(
            "- `{}` `{}` in `{}`: {} -> {} (delta {:+})",
            jump.query_id, symbol, jump.file_path, jump.before_rank, jump.after_rank, jump.delta
        ));
    }

    lines.push(String::new());
    lines
}

fn format_hit(hit: &SearchHitSnapshot) -> String {
    let symbol = hit.symbol_name.as_deref().unwrap_or("<none>");
    format!(
        "`{}` `{}` ({}, line {})",
        hit.file_path, symbol, hit.kind, hit.start_line
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tantivy_upgrade_harness_fixture_snapshot_and_diff() {
    let query_set_path = project_path(QUERY_SET_RELATIVE_PATH);
    let query_set = load_query_set(&query_set_path).expect("Should load Tantivy upgrade query set");

    let snapshot = capture_fixture_snapshot_from_file(&query_set_path)
        .await
        .expect("Should capture fixture-backed snapshot");

    assert_eq!(
        snapshot.queries.len(),
        query_set.queries.len(),
        "Snapshot should have one entry per query"
    );

    let or_fallback_queries = snapshot
        .queries
        .iter()
        .filter(|query| query.category == "or_fallback")
        .collect::<Vec<_>>();
    assert!(
        !or_fallback_queries.is_empty(),
        "Query fixture should include at least one or_fallback query"
    );
    assert!(
        or_fallback_queries.iter().any(|query| query.relaxed),
        "Expected at least one or_fallback query with relaxed=true, got:\n{}",
        or_fallback_queries
            .iter()
            .map(|query| format!(
                "{} => relaxed={} query={}",
                query.query_id, query.relaxed, query.query
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let diff = diff_snapshots(&snapshot, &snapshot, MIN_RANK_JUMP);
    assert!(
        diff.changed_top_results.is_empty(),
        "Self-diff must have no changed top results"
    );
    assert!(
        diff.additions.is_empty() && diff.removals.is_empty() && diff.rank_jumps.is_empty(),
        "Self-diff must not report additions, removals, or rank jumps"
    );

    if std::env::var("JULIE_WRITE_TANTIVY_026_BASELINE").as_deref() == Ok("1") {
        let baseline_path = project_path(BASELINE_RELATIVE_PATH);
        write_snapshot_to_path(&snapshot, &baseline_path)
            .expect("Should write Tantivy 0.26 baseline snapshot JSON");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tantivy_upgrade_baseline_comparison_report_generation() {
    let query_set_path = project_path(QUERY_SET_RELATIVE_PATH);
    let current = capture_fixture_snapshot_from_file(&query_set_path)
        .await
        .expect("Should capture current Tantivy 0.26 snapshot");
    let mut baseline = current.clone();
    baseline.metadata.capture_source = "synthetic_test_baseline".to_string();
    baseline
        .queries
        .first_mut()
        .expect("fixture should include at least one query")
        .top_results
        .reverse();
    let diff = diff_snapshots(&baseline, &current, MIN_RANK_JUMP);

    let report_markdown = render_comparison_report(&baseline, &current, &diff, MIN_RANK_JUMP);

    assert!(
        report_markdown.contains("## Changed top results"),
        "Report should include changed top result section"
    );
    assert!(
        report_markdown.contains("## Additions"),
        "Report should include additions section"
    );
    assert!(
        report_markdown.contains("## Removals"),
        "Report should include removals section"
    );
    assert!(
        report_markdown.contains("## Large rank jumps"),
        "Report should include rank jump section"
    );
    assert!(
        report_markdown.contains("## Drift classification"),
        "Report should include drift classification section"
    );

    let temp_dir = tempfile::TempDir::new().expect("Should create report temp dir");
    let report_path = temp_dir.path().join("tantivy-0.26-comparison-report.md");
    std::fs::write(&report_path, &report_markdown)
        .expect("Should write comparison report markdown");
    let written =
        std::fs::read_to_string(&report_path).expect("Should read written report markdown");
    assert_eq!(
        written, report_markdown,
        "Written report should match generated markdown"
    );
}
