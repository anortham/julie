//! Task 12 deliverable: replay the captured content zero-hit queries
//! through the full `FastSearchTool::execute_with_trace` pipeline (not
//! `SearchIndex::search_content` directly like Task 3) so the replay
//! exercises promotion, hint formatting, and zero-hit attribution
//! end-to-end. Counts are asserted against the acceptance ceilings
//! defined by the plan and appended to the Task 3 diagnosis markdown.
//!
//! This test is **ignored by default**. It is slow (it indexes the
//! julie source tree into a throwaway `TempDir`) and requires the
//! canonical fixture. Run it manually:
//!
//! ```bash
//! cargo nextest run --lib acceptance_replay_against_captured_zero_hits -- --ignored
//! ```
//!
//! Inputs:
//!
//! * `fixtures/search-quality/zero-hit-replay-task3.json` — 47 captured
//!   zero-hit entries. Shared with Task 3 for continuity.
//!
//! Outputs:
//!
//! * A Task 12 section appended to
//!   `docs/plans/2026-04-21-search-quality-hardening-diagnosis.md` with
//!   per-reason counts, the raw/without-recourse rates, and the limit
//!   clamp count.
//! * Two assertions: raw zero-hit rate ≤ 20% and without-recourse rate
//!   ≤ 8%.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tempfile::TempDir;
use tokio::time::{Duration, sleep};

use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::{HintKind, ZeroHitReason};
use crate::tools::workspace::ManageWorkspaceTool;

/// Raw zero-hit rate ceiling (proportion of replayed queries that
/// return zero hits through the full pipeline). Plan target.
const RAW_ZERO_HIT_CEILING: f64 = 0.20;

/// "Without recourse" rate ceiling (proportion of replayed queries that
/// return zero hits AND carry no actionable hint). Plan target.
const WITHOUT_RECOURSE_CEILING: f64 = 0.08;

/// Upper bound on `limit` enforced by the `FastSearchTool` schema
/// (per the tool description: "range: 1-500"). Used to count fixture
/// entries that would be clamped if they were replayed verbatim today.
const FAST_SEARCH_LIMIT_UPPER: u32 = 500;

#[derive(Debug, Deserialize, Clone)]
struct ZeroHitEntry {
    #[allow(dead_code)]
    workspace_id: String,
    #[allow(dead_code)]
    timestamp: u64,
    query: String,
    file_pattern: Option<String>,
    language: Option<String>,
    exclude_tests: Option<bool>,
    limit_param: Option<u32>,
    search_target: Option<String>,
}

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_path() -> PathBuf {
    project_root()
        .join("fixtures")
        .join("search-quality")
        .join("zero-hit-replay-task3.json")
}

fn report_path() -> PathBuf {
    project_root()
        .join("docs")
        .join("plans")
        .join("2026-04-21-search-quality-hardening-diagnosis.md")
}

/// Convert a `ZeroHitReason` into its `snake_case` label via a serde
/// roundtrip so the label tracks the enum's serialization contract. If
/// the enum ever grows a variant or renames one, this stays correct.
fn reason_label(reason: &ZeroHitReason) -> String {
    serde_json::to_value(reason)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Convert a `HintKind` into its `snake_case` label the same way.
fn hint_label(hint: &HintKind) -> String {
    serde_json::to_value(hint)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

/// Bootstrap a throwaway handler pointed at the julie source tree and
/// run a full index through `ManageWorkspaceTool`. Uses `new_for_test`
/// so handler state lives in a `TempDir` and never contaminates the
/// user's `~/.julie/`. Returns the `TempDir` so the caller can keep it
/// alive for the duration of the replay.
async fn bootstrap_handler() -> Result<(TempDir, JulieServerHandler)> {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    // Keep a handle on the handler's TempDir scaffolding for the
    // lifetime of the test. The second `TempDir` below is the source
    // tree we ask the handler to index (here: the julie repo itself).
    let repo_root = project_root();
    let temp_dir = TempDir::new().context("creating throwaway indexing tempdir")?;

    let handler = JulieServerHandler::new_for_test()
        .await
        .context("JulieServerHandler::new_for_test")?;
    handler
        .initialize_workspace_with_force(Some(repo_root.to_string_lossy().to_string()), true)
        .await
        .context("initialize_workspace_with_force")?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(repo_root.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .context("ManageWorkspaceTool index")?;

    // Let background readiness settle; mirrors the pattern in
    // `seed_workspace` from Task 4a's integration tests.
    sleep(Duration::from_millis(500)).await;
    mark_index_ready(&handler).await;

    Ok((temp_dir, handler))
}

#[derive(Debug)]
struct ReplayCounts {
    total: usize,
    still_zero: usize,
    without_recourse: usize,
    limit_clamped: usize,
    per_reason: BTreeMap<String, usize>,
    per_hint_on_zero: BTreeMap<String, usize>,
}

fn write_diagnosis_section(counts: &ReplayCounts) -> Result<()> {
    let report = report_path();
    let mut md = String::new();
    md.push_str("\n\n## Task 12 — Acceptance replay (FastSearchTool end-to-end)\n\n");
    md.push_str(&format!(
        "_Replay harness: `cargo nextest run --lib acceptance_replay_against_captured_zero_hits -- --ignored`_\n\n"
    ));
    md.push_str(&format!(
        "* Fixture: `fixtures/search-quality/zero-hit-replay-task3.json`\n"
    ));
    md.push_str(&format!("* Entries replayed: {}\n", counts.total));
    md.push_str(&format!(
        "* Still zero hits after full pipeline: **{}** ({:.1}%) — ceiling {:.0}%\n",
        counts.still_zero,
        100.0 * counts.still_zero as f64 / counts.total.max(1) as f64,
        100.0 * RAW_ZERO_HIT_CEILING,
    ));
    md.push_str(&format!(
        "* Zero hits without an actionable hint (without-recourse): **{}** ({:.1}%) — ceiling {:.0}%\n",
        counts.without_recourse,
        100.0 * counts.without_recourse as f64 / counts.total.max(1) as f64,
        100.0 * WITHOUT_RECOURSE_CEILING,
    ));
    md.push_str(&format!(
        "* Fixture entries with `limit_param > {}` (would hit the tool clamp): **{}**\n",
        FAST_SEARCH_LIMIT_UPPER, counts.limit_clamped,
    ));
    md.push_str(&format!(
        "* Multi-token zero-hit hints: **{}**\n\n",
        counts.per_hint_on_zero.get("multi_token_hint").copied().unwrap_or(0),
    ));

    md.push_str("### Zero-hit reason distribution\n\n");
    md.push_str("| reason | count |\n| --- | ---: |\n");
    for (label, c) in &counts.per_reason {
        md.push_str(&format!("| `{label}` | {c} |\n"));
    }
    md.push_str("\n");

    md.push_str("### Hint distribution on zero-hit results\n\n");
    md.push_str("| hint | count |\n| --- | ---: |\n");
    for (label, c) in &counts.per_hint_on_zero {
        md.push_str(&format!("| `{label}` | {c} |\n"));
    }
    md.push_str("\n");

    // Idempotent: if a prior Task 12 section already exists, replace
    // it in place; otherwise append. Task 3's section stays intact
    // either way because we key on the Task 12 header literal.
    const SECTION_HEADER: &str = "\n## Task 12 — Acceptance replay (FastSearchTool end-to-end)\n";
    let existing = fs::read_to_string(&report).unwrap_or_default();
    let combined = if existing.is_empty() {
        md
    } else if let Some(header_pos) = existing.find(SECTION_HEADER) {
        // Strip the old Task 12 section (from the header to EOF or the
        // next top-level `\n## ` header that isn't Task 12's own) and
        // splice the fresh one in its place.
        let prefix = &existing[..header_pos];
        let after_header = &existing[header_pos + 1..]; // skip leading newline
        let next_h2 = after_header[SECTION_HEADER.len() - 1..]
            .find("\n## ")
            .map(|rel| header_pos + 1 + (SECTION_HEADER.len() - 1) + rel);
        let suffix = match next_h2 {
            Some(pos) => &existing[pos..],
            None => "",
        };
        format!("{}{}{}", prefix.trim_end(), md, suffix)
    } else {
        format!("{}{}", existing.trim_end(), md)
    };
    fs::write(&report, combined).with_context(|| format!("writing {}", report.display()))?;
    Ok(())
}

/// Task 12's acceptance assertion. Replays every captured zero-hit
/// through `FastSearchTool::execute_with_trace`, counts per-reason
/// outcomes, appends a diagnosis section, and asserts the two plan
/// ceilings. `#[ignore]`-gated — slow, requires indexing the julie
/// repo into a `TempDir`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "acceptance replay; indexes the julie workspace into a TempDir (slow)"]
async fn acceptance_replay_against_captured_zero_hits() -> Result<()> {
    let fixture = fixture_path();
    let raw = fs::read_to_string(&fixture)
        .with_context(|| format!("reading fixture {}", fixture.display()))?;
    let entries: Vec<ZeroHitEntry> =
        serde_json::from_str(&raw).context("parsing zero-hit fixture")?;
    assert!(!entries.is_empty(), "fixture must have at least one entry");

    let (_indexed_dir, handler) = bootstrap_handler().await?;

    let mut counts = ReplayCounts {
        total: entries.len(),
        still_zero: 0,
        without_recourse: 0,
        limit_clamped: 0,
        per_reason: BTreeMap::new(),
        per_hint_on_zero: BTreeMap::new(),
    };

    for entry in &entries {
        let limit_raw = entry.limit_param.unwrap_or(10).max(1);
        if limit_raw > FAST_SEARCH_LIMIT_UPPER {
            counts.limit_clamped += 1;
        }
        let limit = limit_raw.min(FAST_SEARCH_LIMIT_UPPER);

        let tool = FastSearchTool {
            query: entry.query.clone(),
            search_target: entry
                .search_target
                .clone()
                .unwrap_or_else(|| "content".to_string()),
            language: entry.language.clone(),
            file_pattern: entry.file_pattern.clone(),
            limit,
            context_lines: None,
            exclude_tests: entry.exclude_tests,
            ..FastSearchTool::default()
        };

        let run = tool
            .execute_with_trace(&handler)
            .await
            .with_context(|| format!("execute_with_trace query={:?}", entry.query))?;

        let execution = run.execution.unwrap_or_else(|| {
            panic!(
                "FastSearchTool::execute_with_trace returned execution=None for query {:?}; \
                 acceptance replay requires every call to carry a SearchExecutionResult. \
                 Investigate the readiness gate / early-return path before continuing.",
                entry.query
            )
        });

        let hit_count = execution.trace.result_count;
        if hit_count == 0 {
            counts.still_zero += 1;
            let has_hint = execution.trace.hint_kind.is_some();
            if !has_hint {
                counts.without_recourse += 1;
            }
            let reason_key = execution
                .trace
                .zero_hit_reason
                .as_ref()
                .map(reason_label)
                .unwrap_or_else(|| "unattributed".to_string());
            *counts.per_reason.entry(reason_key).or_insert(0) += 1;
            let hint_key = execution
                .trace
                .hint_kind
                .as_ref()
                .map(hint_label)
                .unwrap_or_else(|| "none".to_string());
            *counts.per_hint_on_zero.entry(hint_key).or_insert(0) += 1;
        }
    }

    write_diagnosis_section(&counts)?;

    let total = counts.total.max(1) as f64;
    let raw_rate = counts.still_zero as f64 / total;
    let wr_rate = counts.without_recourse as f64 / total;

    // Echo to stderr for --nocapture runs so the report summary is
    // visible without opening the markdown file.
    eprintln!();
    eprintln!("=== Zero-hit acceptance replay (Task 12) ===");
    eprintln!("entries        = {}", counts.total);
    eprintln!(
        "still zero     = {} ({:.1}%) [ceiling {:.0}%]",
        counts.still_zero,
        100.0 * raw_rate,
        100.0 * RAW_ZERO_HIT_CEILING
    );
    eprintln!(
        "without recourse = {} ({:.1}%) [ceiling {:.0}%]",
        counts.without_recourse,
        100.0 * wr_rate,
        100.0 * WITHOUT_RECOURSE_CEILING
    );
    eprintln!(
        "multi-token hints = {}",
        counts.per_hint_on_zero.get("multi_token_hint").copied().unwrap_or(0)
    );
    eprintln!("limit clamped  = {}", counts.limit_clamped);
    for (label, c) in &counts.per_reason {
        eprintln!("  reason: {label:<28} {c}");
    }
    for (label, c) in &counts.per_hint_on_zero {
        eprintln!("  hint:   {label:<28} {c}");
    }
    eprintln!("report: {}", report_path().display());

    assert!(
        raw_rate <= RAW_ZERO_HIT_CEILING,
        "raw zero-hit rate {:.3} exceeds ceiling {:.3} ({}/{})",
        raw_rate,
        RAW_ZERO_HIT_CEILING,
        counts.still_zero,
        counts.total
    );
    assert!(
        wr_rate <= WITHOUT_RECOURSE_CEILING,
        "without-recourse rate {:.3} exceeds ceiling {:.3} ({}/{})",
        wr_rate,
        WITHOUT_RECOURSE_CEILING,
        counts.without_recourse,
        counts.total
    );

    Ok(())
}
