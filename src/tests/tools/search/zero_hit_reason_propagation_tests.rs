//! Task 4b: `SearchExecutionResult.trace.zero_hit_reason` propagation from
//! `line_mode_matches` through `execute_content_search`.
//!
//! Teammate-b's Task 4a populated `LineModeSearchResult.zero_hit_reason`
//! at the line_mode layer. This file pins that the execution layer
//! copies that reason onto the public `SearchTrace` so MCP callers,
//! telemetry, and the dashboard see the same attribution.
//!
//! Scope is deliberately narrow: one variant exercised end-to-end
//! (FilePatternFiltered) plus the "non-empty run leaves reason None"
//! invariant. Full per-variant coverage at the pipeline layer already
//! lives in `zero_hit_reason_tests.rs` (teammate-b).

use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;
use tokio::time::{Duration, sleep};

use crate::handler::JulieServerHandler;
use crate::tools::search::trace::{FilePatternDiagnostic, HintKind, ZeroHitReason};
use crate::tools::{FastSearchTool, ManageWorkspaceTool};

async fn mark_index_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn seed_workspace(files: &[(&str, &str)]) -> (TempDir, JulieServerHandler) {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();

    for (rel_path, content) in files {
        let full = workspace_path.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(full, content).expect("write file");
    }

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("handler init");
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .expect("workspace init");

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await.expect("index");
    sleep(Duration::from_millis(500)).await;
    mark_index_ready(&handler).await;

    (temp_dir, handler)
}

fn content_search(query: &str, file_pattern: Option<&str>) -> FastSearchTool {
    FastSearchTool {
        query: query.to_string(),
        language: None,
        file_pattern: file_pattern.map(|s| s.to_string()),
        limit: 10,
        search_target: "content".to_string(),
        context_lines: Some(0),
        exclude_tests: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    }
}

fn extract_text_from_result(result: &crate::mcp_compat::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The content token `marker_pattern` exists only in files outside
/// `src/ui/**`. line_mode's per-file loop drops every candidate on the
/// `file_pattern` stage and reports `FilePatternFiltered`. Task 4b must
/// copy that attribution onto `SearchExecutionResult.trace.zero_hit_reason`
/// so the public trace reflects the same verdict.
#[tokio::test(flavor = "multi_thread")]
async fn trace_zero_hit_reason_propagates_file_pattern_filtered() {
    let (_dir, handler) = seed_workspace(&[
        ("src/core.rs", "fn core() { let marker scope = 1; }\n"),
        (
            "crates/other/misc.rs",
            "fn misc() { let marker scope = 2; }\n",
        ),
    ])
    .await;

    let execution = content_search("marker_scope", Some("src/ui/**"))
        .execute_with_trace(&handler)
        .await
        .expect("search should not error")
        .execution
        .expect("execute_with_trace populates execution for content search");

    assert!(
        execution.hits.is_empty(),
        "file_pattern should drop every candidate; got {} hits: {:?}",
        execution.hits.len(),
        execution.hits.iter().map(|h| &h.file).collect::<Vec<_>>(),
    );

    assert_eq!(
        execution.trace.zero_hit_reason,
        Some(ZeroHitReason::FilePatternFiltered),
        "execute_content_search must copy line_mode's zero_hit_reason onto \
         trace.zero_hit_reason; got {:?}",
        execution.trace.zero_hit_reason,
    );
}

/// Counter-example: a search that finds hits MUST leave
/// `trace.zero_hit_reason` as `None`. This guards against a sloppy
/// propagation that stamps the line_mode reason onto the trace
/// unconditionally (line_mode sets it to `None` on non-empty runs,
/// but the execution layer could accidentally re-attribute after
/// aggregation).
#[tokio::test(flavor = "multi_thread")]
async fn trace_zero_hit_reason_stays_none_on_non_empty_run() {
    let (_dir, handler) =
        seed_workspace(&[("src/core.rs", "fn core() { let hit_me = 1; }\n")]).await;

    let execution = content_search("hit_me", None)
        .execute_with_trace(&handler)
        .await
        .expect("search should not error")
        .execution
        .expect("execute_with_trace populates execution for content search");

    assert!(
        !execution.hits.is_empty(),
        "fixture should yield at least one hit for 'hit_me'",
    );
    assert_eq!(
        execution.trace.zero_hit_reason, None,
        "non-empty runs must leave zero_hit_reason None; got {:?}",
        execution.trace.zero_hit_reason,
    );
}

/// Task 2: when the scoped miss is a real out-of-scope request rather than
/// starvation, the execution layer must copy `file_pattern_diagnostic` onto the
/// public trace the same way it already does for `zero_hit_reason`.
#[tokio::test(flavor = "multi_thread")]
async fn trace_file_pattern_diagnostic_propagates_no_in_scope_candidates() {
    let (_dir, handler) = seed_workspace(&[
        ("src/core.rs", "fn core() { let marker scope = 1; }\n"),
        (
            "crates/other/misc.rs",
            "fn misc() { let marker scope = 2; }\n",
        ),
    ])
    .await;

    let execution = content_search("marker_scope", Some("src/ui/**"))
        .execute_with_trace(&handler)
        .await
        .expect("search should not error")
        .execution
        .expect("execute_with_trace populates execution for content search");

    assert!(execution.hits.is_empty(), "scoped miss should stay empty");
    assert_eq!(
        execution.trace.zero_hit_reason,
        Some(ZeroHitReason::FilePatternFiltered),
    );
    assert_eq!(
        execution.trace.file_pattern_diagnostic,
        Some(FilePatternDiagnostic::NoInScopeCandidates),
        "execute_content_search must copy line_mode's file_pattern_diagnostic onto trace; got {:?}",
        execution.trace.file_pattern_diagnostic,
    );
}

/// Task 4: once live telemetry showed `NoInScopeCandidates` was a real bucket,
/// content zero-hits in that bucket should prepend the dedicated out-of-scope
/// hint and persist `hint_kind` on the public trace. This must beat the older
/// multi-token hint for queries like `marker scope`.
#[tokio::test(flavor = "multi_thread")]
async fn trace_hint_kind_prefers_out_of_scope_for_no_in_scope_candidates() {
    let (_dir, handler) = seed_workspace(&[
        ("src/core.rs", "fn core() { let marker scope = 1; }\n"),
        (
            "crates/other/misc.rs",
            "fn misc() { let marker scope = 2; }\n",
        ),
    ])
    .await;

    let run = content_search("marker_scope", Some("src/ui/**"))
        .execute_with_trace(&handler)
        .await
        .expect("search should not error");
    let execution = run
        .execution
        .expect("execute_with_trace populates execution for content search");
    let text = extract_text_from_result(&run.result);

    assert!(execution.hits.is_empty(), "scoped miss should stay empty");
    assert_eq!(
        execution.trace.file_pattern_diagnostic,
        Some(FilePatternDiagnostic::NoInScopeCandidates),
    );
    assert_eq!(
        execution.trace.hint_kind,
        Some(HintKind::OutOfScopeContentHint),
        "no-in-scope content miss should prefer the dedicated out-of-scope hint; got {:?}",
        execution.trace.hint_kind,
    );
    assert!(
        text.contains("found no candidate files inside file_pattern=src/ui/**"),
        "expected out-of-scope hint text, got: {}",
        text,
    );
    assert!(
        !text.contains("Tokens: ["),
        "out-of-scope hint should beat multi-token hint, got: {}",
        text,
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_scope_rescue_labels_out_of_scope_hits() {
    let (_dir, handler) = seed_workspace(&[
        ("src/core.rs", "fn core() { let marker_scope = 1; }\n"),
        (
            "crates/other/misc.rs",
            "fn misc() { let marker_scope = 2; }\n",
        ),
    ])
    .await;

    let run = content_search("marker_scope", Some("src/ui/**"))
        .execute_with_trace(&handler)
        .await
        .expect("search should not error");
    let execution = run
        .execution
        .expect("execute_with_trace populates execution for content search");
    let text = extract_text_from_result(&run.result);

    assert_eq!(
        execution.hits.len(),
        2,
        "scope rescue should return the out-of-scope hits",
    );
    assert!(execution.trace.scope_relaxed);
    assert_eq!(execution.trace.scope_rescue_count, 1);
    assert_eq!(
        execution.trace.original_file_pattern,
        Some("src/ui/**".to_string()),
    );
    assert_eq!(
        execution.trace.original_zero_hit_reason,
        Some(ZeroHitReason::FilePatternFiltered),
    );
    assert_eq!(execution.trace.zero_hit_reason, None);
    assert_eq!(execution.trace.file_pattern_diagnostic, None);
    assert!(
        text.starts_with(
            "NOTE: 0 matches within file_pattern=src/ui/**. Showing 2 results from the full codebase (outside requested scope).",
        ),
        "rescued output should lead with the scope label, got: {}",
        text,
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_scope_rescue_single_file_hint_mentions_get_symbols() {
    let (_dir, handler) =
        seed_workspace(&[("src/core.rs", "fn core() { let marker_single_file = 1; }\n")]).await;

    let run = content_search("marker_single_file", Some("src/ui/view.rs"))
        .execute_with_trace(&handler)
        .await
        .expect("search should not error");
    let execution = run
        .execution
        .expect("execute_with_trace populates execution for content search");
    let text = extract_text_from_result(&run.result);

    assert_eq!(execution.hits.len(), 1);
    assert!(execution.trace.scope_relaxed);
    assert!(text.contains(
        "Hint: for symbol structure within a specific file, use get_symbols(file_path=src/ui/view.rs).",
    ));
    assert!(text.contains("file_pattern is valid for text search within a known file.",));
}

#[tokio::test(flavor = "multi_thread")]
async fn trace_or_disjunction_detected_flows_from_execute_content_search() {
    let (_dir, handler) = seed_workspace(&[(
        "src/logging.py",
        "logging.basicConfig(format='%(asctime)s', datefmt='%Y-%m-%d')\n",
    )])
    .await;

    let execution = content_search("logging.basicConfig OR datefmt", None)
        .execute_with_trace(&handler)
        .await
        .expect("search should not error")
        .execution
        .expect("execute_with_trace populates execution for content search");

    assert!(
        !execution.hits.is_empty(),
        "clean OR disjunction should still return line hits",
    );
    assert!(
        execution.trace.or_disjunction_detected,
        "execute_content_search should stamp clean OR disjunction detection on the trace",
    );
}
