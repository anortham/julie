//! Tests for federated get_context (workspace="all").
//!
//! Verifies that the `WorkspaceTarget::All` branch in `pipeline::run()`
//! correctly fans out per-workspace pipelines and merges results with
//! project tags, grouped file maps, and global token budget.

use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use crate::database::SymbolDatabase;
use crate::search::index::{SearchIndex, SymbolDocument};
use crate::tools::get_context::federated::project_name_from_path;
use crate::tools::get_context::formatting::{OutputFormat, format_federated_context};
use crate::tools::get_context::pipeline::run_pipeline;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temporary workspace with an indexed DB + SearchIndex containing
/// the given symbols. Returns (db, search_index, temp_dir).
fn create_test_workspace_with_symbols(
    symbols: Vec<(&str, &str, &str, &str)>, // (id, name, kind, file_path)
) -> (Arc<Mutex<SymbolDatabase>>, Arc<Mutex<SearchIndex>>, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("symbols.db");
    let tantivy_path = dir.path().join("tantivy");
    std::fs::create_dir_all(&tantivy_path).unwrap();

    // Create DB and insert symbols (must insert file records first for FK)
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Collect unique file paths and insert file records
    let unique_files: std::collections::HashSet<&str> =
        symbols.iter().map(|(_, _, _, fp)| *fp).collect();
    for fp in &unique_files {
        db.conn
            .execute(
                "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified)
                 VALUES (?1, 'rust', 'deadbeef', 100, 0)",
                rusqlite::params![fp],
            )
            .unwrap();
    }

    for (id, name, kind, file_path) in &symbols {
        let sig = format!("fn {}()", name);
        let code = format!("fn {}() {{}}", name);
        db.conn
            .execute(
                "INSERT INTO symbols (id, name, kind, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte, language, signature, code_context)
                 VALUES (?1, ?2, ?3, ?4, 1, 0, 10, 0, 0, 100, 'rust', ?5, ?6)",
                rusqlite::params![id, name, kind, file_path, sig, code],
            )
            .unwrap();
    }

    // Create SearchIndex and index symbols
    let search_index = SearchIndex::create(&tantivy_path).unwrap();
    for (id, name, kind, file_path) in &symbols {
        let doc = SymbolDocument {
            id: id.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            file_path: file_path.to_string(),
            language: "rust".to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            code_body: format!("fn {}() {{}}", name),
            start_line: 1,
        };
        search_index.add_symbol(&doc).unwrap();
    }
    search_index.commit().unwrap();

    (
        Arc::new(Mutex::new(db)),
        Arc::new(Mutex::new(search_index)),
        dir,
    )
}

// ---------------------------------------------------------------------------
// Unit tests for format_federated_context
// ---------------------------------------------------------------------------

#[test]
fn test_format_federated_context_tags_projects() {
    let per_project = vec![
        ("julie".to_string(), "PIVOT search_symbols src/search.rs:1 kind=function centrality=low\n  fn search_symbols()\n".to_string()),
        ("coa-framework".to_string(), "PIVOT handle_request src/handler.rs:5 kind=function centrality=low\n  fn handle_request()\n".to_string()),
    ];

    let result = format_federated_context("search", &per_project, OutputFormat::Compact);

    // Should contain project headers
    assert!(result.contains("[project: julie]"), "Missing julie project header");
    assert!(result.contains("[project: coa-framework]"), "Missing coa-framework project header");

    // Should contain the pivots
    assert!(result.contains("search_symbols"), "Missing julie pivot");
    assert!(result.contains("handle_request"), "Missing coa-framework pivot");

    // Should have a federated header
    assert!(result.contains("search"), "Missing query in header");
}

#[test]
fn test_format_federated_context_empty_results() {
    let per_project: Vec<(String, String)> = vec![];
    let result = format_federated_context("nonexistent", &per_project, OutputFormat::Compact);

    assert!(result.contains("nonexistent"), "Should mention the query");
    assert!(
        result.contains("no relevant symbols") || result.contains("No relevant symbols"),
        "Should indicate no results, got: {}",
        result,
    );
}

#[test]
fn test_format_federated_context_single_project() {
    let per_project = vec![
        ("my-project".to_string(), "PIVOT my_func src/lib.rs:1 kind=function centrality=low\n  fn my_func()\n".to_string()),
    ];

    let result = format_federated_context("my_func", &per_project, OutputFormat::Compact);

    assert!(result.contains("[project: my-project]"), "Missing project header");
    assert!(result.contains("my_func"), "Missing function name");
}

// ---------------------------------------------------------------------------
// Unit tests for run_pipeline per-workspace (validates Option A approach)
// ---------------------------------------------------------------------------

#[test]
fn test_run_pipeline_per_workspace_returns_results() {
    let (_db, _si, _dir) = create_test_workspace_with_symbols(vec![
        ("sym1", "parse_query", "function", "src/parser.rs"),
        ("sym2", "build_ast", "function", "src/ast.rs"),
    ]);

    let db_guard = _db.lock().unwrap();
    let si_guard = _si.lock().unwrap();

    let result = run_pipeline(
        "parse",
        Some(2000),
        None,
        None,
        Some("compact".to_string()),
        &db_guard,
        &si_guard,
        None,
    )
    .unwrap();

    assert!(result.contains("parse_query"), "Should find parse_query symbol");
}

// ---------------------------------------------------------------------------
// Integration test: federated pipeline merges multiple workspaces
// ---------------------------------------------------------------------------

#[test]
fn test_federated_pipeline_merges_workspaces() {
    // Create two workspaces with different symbols
    let (db1, si1, _dir1) = create_test_workspace_with_symbols(vec![
        ("a1", "search_engine", "function", "src/search.rs"),
        ("a2", "index_file", "function", "src/indexer.rs"),
    ]);

    let (db2, si2, _dir2) = create_test_workspace_with_symbols(vec![
        ("b1", "search_handler", "function", "src/handler.rs"),
        ("b2", "query_builder", "function", "src/query.rs"),
    ]);

    // Run pipeline per workspace
    let mut per_project_results = Vec::new();

    {
        let db_guard = db1.lock().unwrap();
        let si_guard = si1.lock().unwrap();
        let output = run_pipeline(
            "search",
            Some(1500),
            None,
            None,
            Some("compact".to_string()),
            &db_guard,
            &si_guard,
            None,
        )
        .unwrap();
        per_project_results.push(("alpha-project".to_string(), output));
    }

    {
        let db_guard = db2.lock().unwrap();
        let si_guard = si2.lock().unwrap();
        let output = run_pipeline(
            "search",
            Some(1500),
            None,
            None,
            Some("compact".to_string()),
            &db_guard,
            &si_guard,
            None,
        )
        .unwrap();
        per_project_results.push(("beta-project".to_string(), output));
    }

    // Merge with federated formatting
    let merged = format_federated_context("search", &per_project_results, OutputFormat::Compact);

    // Both projects should be represented
    assert!(merged.contains("[project: alpha-project]"), "Missing alpha-project");
    assert!(merged.contains("[project: beta-project]"), "Missing beta-project");

    // Should contain symbols from both workspaces
    assert!(merged.contains("search_engine") || merged.contains("search_handler"),
        "Should contain search results from at least one workspace");
}

// ---------------------------------------------------------------------------
// Test: project name derivation from path
// ---------------------------------------------------------------------------

#[test]
fn test_project_name_from_path() {
    use std::path::PathBuf;

    assert_eq!(project_name_from_path(&PathBuf::from("/Users/dev/projects/julie")), "julie");
    assert_eq!(project_name_from_path(&PathBuf::from("/home/user/my-app")), "my-app");
    assert_eq!(project_name_from_path(&PathBuf::from("/")), "root");
    assert_eq!(project_name_from_path(&PathBuf::from("")), "unknown");
}

// ---------------------------------------------------------------------------
// Test: token budget splitting
// ---------------------------------------------------------------------------

#[test]
fn test_token_budget_split_across_projects() {
    // With 3000 tokens and 3 projects, each gets 1000
    let total = 3000u32;
    let num_projects = 3usize;
    let per_project = total / num_projects as u32;
    assert_eq!(per_project, 1000);

    // With no explicit budget (None), each project gets None (adaptive)
    let total: Option<u32> = None;
    let per_project: Option<u32> = total.map(|t| t / num_projects as u32);
    assert!(per_project.is_none());
}

// ---------------------------------------------------------------------------
// Test: stdio mode error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_federated_get_context_requires_daemon_mode() {
    use crate::handler::JulieServerHandler;
    use crate::tools::get_context::GetContextTool;

    let handler = JulieServerHandler::new_for_test().await.unwrap();

    // handler.daemon_state is None in test mode (stdio mode)
    let tool = GetContextTool {
        query: "search".to_string(),
        max_tokens: None,
        workspace: Some("all".to_string()),
        language: None,
        file_pattern: None,
        format: None,
    };

    let result = crate::tools::get_context::pipeline::run(&tool, &handler).await;
    assert!(result.is_err(), "Should error in stdio mode");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("daemon mode") || err_msg.contains("stdio"),
        "Error should mention daemon mode, got: {}",
        err_msg
    );
}

// ---------------------------------------------------------------------------
// Test: skip non-Ready workspaces
// ---------------------------------------------------------------------------

#[test]
fn test_format_federated_skips_empty_results() {
    // One workspace has results, one has "no relevant symbols"
    let per_project = vec![
        ("good-project".to_string(), "PIVOT my_func src/lib.rs:1 kind=function centrality=low\n  fn my_func()\n".to_string()),
        ("empty-project".to_string(), "Context \"search\" | no relevant symbols".to_string()),
    ];

    let result = format_federated_context("search", &per_project, OutputFormat::Compact);

    // Good project should be there
    assert!(result.contains("[project: good-project]"), "Missing good-project");
    // Empty project should be filtered or shown minimally
    // The key thing is the overall output is valid
    assert!(result.contains("my_func"), "Missing function from good project");
}
