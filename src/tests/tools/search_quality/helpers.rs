//! Test Helpers - Shared utilities for search quality tests
//!
//! These helpers call the search engine directly (not through the MCP tool layer)
//! to get structured Symbol results for quality assertions.

use crate::handler::JulieServerHandler;
use anyhow::{Result, bail};
use julie_core::Symbol;
use std::ops::Deref;

#[derive(Debug)]
pub struct SearchExecution {
    pub symbols: Vec<Symbol>,
    pub relaxed: bool,
    pub total_hits: usize,
}

/// Execute a search-quality query and return both hits and fallback metadata.
pub async fn search_with_metadata(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
    search_target: &str,
) -> Result<SearchExecution> {
    if !matches!(search_target, "content" | "definitions") {
        bail!(
            "Unsupported search_target '{}' in search quality helper",
            search_target
        );
    }

    let (symbols, relaxed, total_hits) = crate::tools::search::text_search::text_search_impl(
        query,
        &None,
        &None,
        limit,
        None,
        search_target,
        None,
        None,
        handler,
    )
    .await?;

    Ok(SearchExecution {
        symbols,
        relaxed,
        total_hits,
    })
}

/// Search Julie's codebase (file content search)
pub async fn search_content(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let run = search_with_metadata(handler, query, limit, "content").await?;
    Ok(run.symbols)
}

/// Search Julie's codebase (symbol definitions search)
pub async fn search_definitions(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let run = search_with_metadata(handler, query, limit, "definitions").await?;
    Ok(run.symbols)
}

/// Assert that results contain a file path matching the pattern
pub fn assert_contains_path(results: &[Symbol], path_pattern: &str) {
    let found = results.iter().any(|r| r.file_path.contains(path_pattern));
    assert!(
        found,
        "Expected results to contain path '{}', but found:\n{}",
        path_pattern,
        format_results(results)
    );
}

/// Assert that results do NOT contain a file path matching the pattern
#[allow(dead_code)]
pub fn assert_not_contains_path(results: &[Symbol], path_pattern: &str) {
    let found = results.iter().any(|r| r.file_path.contains(path_pattern));
    assert!(
        !found,
        "Expected results to NOT contain path '{}', but it was found in:\n{}",
        path_pattern,
        format_results(results)
    );
}

/// Assert minimum number of results
pub fn assert_min_results(results: &[Symbol], min: usize) {
    assert!(
        results.len() >= min,
        "Expected at least {} results, but got {}:\n{}",
        min,
        results.len(),
        format_results(results)
    );
}

/// Assert maximum number of results
pub fn assert_max_results(results: &[Symbol], max: usize) {
    assert!(
        results.len() <= max,
        "Expected at most {} results, but got {}:\n{}",
        max,
        results.len(),
        format_results(results)
    );
}

/// Assert exact number of results
#[allow(dead_code)]
pub fn assert_exact_count(results: &[Symbol], expected: usize) {
    assert_eq!(
        results.len(),
        expected,
        "Expected exactly {} results, but got {}:\n{}",
        expected,
        results.len(),
        format_results(results)
    );
}

/// Assert that a specific symbol kind is present
pub fn assert_contains_symbol_kind(results: &[Symbol], kind: &str) {
    let found = results.iter().any(|r| r.kind.to_string() == kind);
    assert!(
        found,
        "Expected results to contain symbol kind '{}', but found:\n{}",
        kind,
        format_results(results)
    );
}

/// Assert that first result matches criteria (for ranking tests)
#[allow(dead_code)]
pub fn assert_first_result(results: &[Symbol], path_pattern: &str, name_pattern: Option<&str>) {
    assert!(
        !results.is_empty(),
        "Expected at least one result, but got none"
    );

    let first = &results[0];
    assert!(
        first.file_path.contains(path_pattern),
        "Expected first result to be in '{}', but got '{}'\nAll results:\n{}",
        path_pattern,
        first.file_path,
        format_results(results)
    );

    if let Some(name) = name_pattern {
        assert!(
            first.name.contains(name),
            "Expected first result name to contain '{}', but got '{}'\nAll results:\n{}",
            name,
            first.name,
            format_results(results)
        );
    }
}

/// Format results for error messages
fn format_results(results: &[Symbol]) -> String {
    if results.is_empty() {
        return "  (no results)".to_string();
    }

    results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "  [{}] {} ({}:{})",
                i + 1,
                r.name,
                r.file_path,
                r.start_line
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Setup handler for dogfooding tests using pre-built fixture database
///
/// **PERFORMANCE:** <1s (uses pre-indexed fixture, no live indexing)
/// Previously: ~17s first test, ~1s subsequent tests (cached)
///
/// **How it works:**
/// 1. Load pre-built fixture database (5ms)
/// 2. Create temp workspace with pre-indexed database
/// 3. Initialize handler directly with temp workspace
/// 4. Mark indexing as complete to enable searches
///
/// The returned guard owns the temporary workspace and removes it on drop.
pub struct FixtureHandlerGuard {
    // Drop open database and index handles before TempDir removes the workspace.
    handler: JulieServerHandler,
    _temp_dir: tempfile::TempDir,
}

impl Deref for FixtureHandlerGuard {
    type Target = JulieServerHandler;

    fn deref(&self) -> &Self::Target {
        &self.handler
    }
}

impl AsRef<JulieServerHandler> for FixtureHandlerGuard {
    fn as_ref(&self) -> &JulieServerHandler {
        &self.handler
    }
}

pub async fn setup_handler_with_fixture() -> FixtureHandlerGuard {
    setup_handler_inner().await
}

async fn setup_handler_inner() -> FixtureHandlerGuard {
    use crate::handler::JulieServerHandler;
    use crate::tests::fixtures::julie_db::JulieTestFixture;
    use std::fs;

    // Load the pre-built fixture (5ms, no indexing)
    let fixture = JulieTestFixture::get_instance();

    // Create a temporary workspace directory
    let temp_dir = tempfile::TempDir::new().expect("Failed to create temp directory");
    let temp_root = temp_dir.path().to_path_buf();

    // Create the .julie folder structure in temp workspace
    let julie_dir = temp_root.join(".julie");
    fs::create_dir_all(&julie_dir).expect("Failed to create .julie dir");

    // Create indexes directory
    let indexes_dir = julie_dir.join("indexes");
    fs::create_dir_all(&indexes_dir).expect("Failed to create indexes dir");

    // Get the canonical workspace ID. `generate_workspace_id` already
    // returns the full `{safe_name}_{hash8}` form — use it directly so
    // the directory layout and registry entry match what the pooled-DB
    // lookup (e.g. fast_refs after A2.2c) expects. Previously this
    // function rebuilt the ID from `temp_root.file_name() + first-8-of-
    // full-id`, which produced a non-canonical name like
    // `.tmpbaRV03_ws__tmpb` and broke fast_refs identifier tests.
    use crate::workspace::registry::generate_workspace_id;
    let full_workspace_id = generate_workspace_id(&temp_root.to_string_lossy())
        .expect("Failed to generate workspace ID");
    let workspace_name = temp_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");

    let workspace_dir = indexes_dir.join(&full_workspace_id);
    fs::create_dir_all(&workspace_dir).expect("Failed to create workspace index dir");
    let fixture_src = fixture
        .db_path()
        .parent()
        .expect("fixture facts.sqlite has a parent");
    copy_dir(fixture_src, &workspace_dir).expect("Failed to copy fixture index");
    let dest_size = fs::metadata(workspace_dir.join("facts.sqlite"))
        .expect("Failed to read copied facts.sqlite")
        .len();
    println!("✓ Fixture index copied: {dest_size} bytes");

    let workspace = crate::workspace::JulieWorkspace::initialize_with_index_root(
        temp_root.clone(),
        workspace_dir.clone(),
    )
    .await
    .expect("Failed to open fixture workspace");
    println!(
        "✓ Fixture store opened with {} symbols",
        workspace.store.status().graph.symbols
    );

    let mut handler = JulieServerHandler::new_for_test()
        .await
        .expect("Failed to create handler");
    handler.in_process_index_root = Some(workspace_dir.clone());
    {
        let mut workspace_guard = handler.workspace.write().await;
        *workspace_guard = Some(workspace);
    }

    let workspace_id =
        crate::workspace::registry::generate_workspace_id(&temp_root.to_string_lossy())
            .expect("fixture workspace id should generate");
    *handler
        .workspace_id
        .write()
        .unwrap_or_else(|p| p.into_inner()) = Some(workspace_id.clone());
    handler.set_current_primary_binding(workspace_id, temp_root.clone());
    handler
        .indexing_status
        .search_ready
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = (workspace_name, dest_size, julie_dir, full_workspace_id);

    FixtureHandlerGuard {
        handler,
        _temp_dir: temp_dir,
    }
}

fn copy_dir(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}
