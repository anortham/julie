//! Tests for two overfetch/filter-parity bugs in `run_unified_pass` (Phase 2
//! unified execution). Codex adversarial-review findings #1 and #2.
//!
//! Finding #1 (HIGH, confidence 0.94): run_unified_pass asks unified_search_hits
//! for only `limit` raw hits then applies file_pattern and exclude_tests filters.
//! When all `limit` raw hits are out-of-scope, valid in-scope hits beyond that
//! window are silently dropped.
//!
//! Finding #2 (HIGH, confidence 0.93): the exclude_tests path only checks
//! is_test_path(file_path).  Inline #[test] functions in production files
//! carry role=="test" in the search document (set by the projection layer from
//! extractor metadata) but their file path is not a test path, so they leak.

use anyhow::Result;
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;

async fn mark_search_ready(handler: &JulieServerHandler) {
    handler
        .indexing_status
        .search_ready
        .store(true, Ordering::Relaxed);
    *handler.is_indexed.write().await = true;
}

async fn index_workspace(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    mark_search_ready(&handler).await;
    Ok(handler)
}

/// Finding #1: run_unified_pass fetches exactly `limit` raw hits then applies
/// file_pattern.  When the first `limit` ranked hits are all outside the
/// file_pattern scope, a valid in-scope hit beyond that window is dropped.
///
/// Corpus: 5 source files in `lib/` each export a function named exactly with
/// the query term (exact-name match → top reranker slots, outside file_pattern)
/// plus 1 file in `src/scope/` with a lower-scoring doc-comment match (inside
/// file_pattern scope).
///
/// NOTE: the directory is `src/scope/` not `src/target/` — "target" is in
/// BLACKLISTED_DIRECTORIES (Rust build artifact dir) so the walker skips it.
///
/// Before fix: limit=5 raw hits → all lib/ hits → file_pattern drops all →
/// scope-rescue fires → scope_relaxed=true.
/// After fix: overfetch brings the src/scope/ hit into the pool → scoped result
/// returned directly → scope_relaxed=false, hit is in src/scope/.
#[tokio::test]
async fn unified_pass_overfetch_surfaces_valid_hit_when_limit_hits_all_filtered() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // 5 noise files in lib/ — function named exactly with the query term so
    // they consistently land in top-5 after reranking (exact-name match +
    // source role, no vendor/test penalty).
    fs::create_dir_all(workspace_path.join("lib"))?;
    for name in &["a", "b", "c", "d", "e"] {
        fs::write(
            workspace_path.join(format!("lib/{name}.rs")),
            "pub fn ovftest_probe_u7k9() -> bool { true }\n",
        )?;
    }

    // 1 target file in src/scope/ — the query term appears only in the
    // doc-comment, not as the function name, so it ranks below the 5 noise
    // exact-name matches.  "scope" is not in BLACKLISTED_DIRECTORIES.
    fs::create_dir_all(workspace_path.join("src/scope"))?;
    fs::write(
        workspace_path.join("src/scope/core.rs"),
        "/// ovftest_probe_u7k9 usage\npub fn scope_core_fn() -> bool { false }\n",
    )?;

    let handler = index_workspace(workspace_path).await?;

    // limit=5: the 5 noise functions have exact-name matches and occupy the
    // top-5 reranked positions.  Without overfetch, those 5 fill the raw pool,
    // file_pattern removes all of them, and scope-rescue fires (scope_relaxed=true).
    let run = FastSearchTool {
        query: "ovftest_probe_u7k9".to_string(),
        limit: 5,
        file_pattern: Some("src/scope/**".to_string()),
        exclude_tests: Some(false),
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .expect("unified search should populate execution trace");

    // After the fix: the scoped run must succeed (no scope-rescue).
    assert!(
        !execution.trace.scope_relaxed,
        "scope-rescue fired — overfetch bug: the 5 lib/ noise hits filled the \
         limit=5 raw pool; the valid src/scope/ hit was never seen \
         (original_fp={:?}, original_reason={:?})",
        execution.trace.original_file_pattern,
        execution.trace.original_zero_hit_reason,
    );
    assert!(
        !execution.hits.is_empty(),
        "expected at least one hit from src/scope/ but got none"
    );
    assert!(
        execution.hits.iter().all(|h| h.file.contains("src/scope")),
        "expected all hits to be from src/scope/, got: {:?}",
        execution
            .hits
            .iter()
            .map(|h| h.file.as_str())
            .collect::<Vec<_>>(),
    );

    Ok(())
}

/// Finding #2: run_unified_pass's exclude_tests filter only calls
/// is_test_path(file_path).  Inline #[test] functions in production-looking
/// source files have role=="test" in the search document (set by the Rust
/// extractor metadata → projection layer) but their file path is NOT a test
/// path, so is_test_path returns false and they are not excluded.
///
/// Before fix: is_test_path("src/main_logic.rs") == false → inline test
/// function returned despite exclude_tests=true.
/// After fix: role=="test" check also applied → function filtered out.
#[tokio::test]
async fn unified_pass_inline_test_role_excluded_when_exclude_tests_true() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Production file in src/ (NOT a test path segment).  Contains an inline
    // #[test] function — the Rust extractor tags it with is_test=true metadata
    // and the projection layer writes role="test" into the search document.
    fs::create_dir_all(workspace_path.join("src"))?;
    fs::write(
        workspace_path.join("src/main_logic.rs"),
        "\
pub fn production_fn() -> bool { true }

#[test]
fn inline_test_u7k9_hidden_in_production() {
    assert!(production_fn());
}
",
    )?;

    let handler = index_workspace(workspace_path).await?;

    // Searching for the inline test function with exclude_tests=true.
    // is_test_path("src/main_logic.rs") returns false (no test-path segment),
    // so without the role check the function leaks through.
    let run = FastSearchTool {
        query: "inline_test_u7k9_hidden_in_production".to_string(),
        limit: 10,
        exclude_tests: Some(true),
        workspace: Some("primary".to_string()),
        ..Default::default()
    }
    .execute_with_trace(&handler)
    .await?;

    let execution = run
        .execution
        .expect("unified search should populate execution trace");

    // The key assertion: the #[test]-annotated function must NOT appear.
    // Other hits from the same file (e.g. production_fn, file docs) may appear
    // because the tokenized query contains terms like "production" that match
    // them — that is correct behaviour.  Only the test-role function must be
    // excluded by the role=="test" filter.
    let test_fn_in_results = execution
        .hits
        .iter()
        .any(|h| h.name == "inline_test_u7k9_hidden_in_production");

    assert!(
        !test_fn_in_results,
        "expected inline #[test] function to be excluded with exclude_tests=true \
         (role==\"test\" check missing — path-only check not sufficient), \
         but it appeared in results: {:?}",
        execution
            .hits
            .iter()
            .map(|h| (h.name.as_str(), h.file.as_str()))
            .collect::<Vec<_>>(),
    );

    Ok(())
}
