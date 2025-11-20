//! Test Helpers - Shared utilities for search quality tests

use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use anyhow::{Result, anyhow};
use rust_mcp_sdk::schema::{CallToolResult, ContentBlock};
use std::sync::atomic::Ordering;

/// Search Julie's codebase (file content search)
pub async fn search_content(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let tool = FastSearchTool {
        query: query.to_string(),
        search_method: "text".to_string(),
        limit,
        language: None,
        file_pattern: None,
        workspace: Some("primary".to_string()),
        output: None, // Use default (symbols mode)
        context_lines: None,
        search_target: "content".to_string(),
    };

    let result = tool.call_tool(handler).await?;
    parse_search_results(&result)
}

/// Search Julie's codebase (symbol definitions search)
pub async fn search_definitions(
    handler: &JulieServerHandler,
    query: &str,
    limit: u32,
) -> Result<Vec<Symbol>> {
    let tool = FastSearchTool {
        query: query.to_string(),
        search_method: "text".to_string(),
        limit,
        language: None,
        file_pattern: None,
        workspace: Some("primary".to_string()),
        output: None,
        context_lines: None,
        search_target: "definitions".to_string(),
    };

    let result = tool.call_tool(handler).await?;
    parse_search_results(&result)
}

/// Parse search results from MCP CallToolResult
fn parse_search_results(result: &CallToolResult) -> Result<Vec<Symbol>> {
    // Prefer lean text output (dense format) to avoid JSON bloat
    if let Some(ContentBlock::TextContent(text)) = result.content.first() {
        let dense = parse_dense_output(&text.text);
        if !dense.is_empty() {
            return Ok(dense);
        }
    }

    // Fallback: legacy structured_content parsing
    if let Some(structured) = &result.structured_content {
        if let Some(results_value) = structured.get("results") {
            let symbols: Vec<Symbol> = serde_json::from_value(results_value.clone())
                .map_err(|e| anyhow!("Failed to parse symbols from results: {}", e))?;
            return Ok(symbols);
        }
    }

    Ok(Vec::new())
}

/// Parse lean dense search output into Symbols for testing
pub fn parse_dense_output(text: &str) -> Vec<Symbol> {
    text.split("\n\n")
        .filter_map(|block| {
            let mut lines = block.lines();
            let header = lines.next()?.trim();
            if header.is_empty() {
                return None;
            }

            // Header format: <file_path>:<start>-<end> | <name> | <kind>
            let mut header_parts = header.split('|').map(|s| s.trim());

            let span_part = header_parts.next()?;
            let (file_path, span) = span_part.rsplit_once(':')?;
            let (start_str, end_str) = span.split_once('-')?;

            let start_line: u32 = start_str.trim().parse().ok()?;
            let end_line: u32 = end_str.trim().parse().ok()?;

            let name = header_parts.next().unwrap_or("").to_string();
            let kind_str = header_parts.next().unwrap_or("variable");
            let kind = SymbolKind::from_string(kind_str);

            let snippet = lines.collect::<Vec<_>>().join("\n").trim().to_string();

            Some(Symbol {
                id: format!("{}:{}-{}", file_path, start_line, end_line),
                name,
                kind,
                language: String::new(),
                file_path: file_path.to_string(),
                start_line,
                start_column: 0,
                end_line,
                end_column: 0,
                start_byte: 0,
                end_byte: 0,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: if snippet.is_empty() {
                    None
                } else {
                    Some(snippet)
                },
                content_type: None,
            })
        })
        .collect()
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
/// This eliminates live indexing entirely - all 16 tests run in <1s total!
pub async fn setup_handler_with_fixture() -> JulieServerHandler {
    use crate::handler::JulieServerHandler;
    use crate::tests::fixtures::julie_db::JulieTestFixture;
    use crate::workspace::JulieWorkspace;
    use std::fs;
    use std::path::PathBuf;

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

    // Get workspace ID for temp workspace
    use crate::workspace::registry::generate_workspace_id;
    let workspace_id = generate_workspace_id(&temp_root.to_string_lossy())
        .expect("Failed to generate workspace ID");
    let workspace_name = temp_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let full_workspace_id = format!("{}_{}", workspace_name, &workspace_id[..8]);

    // Create workspace index directory
    let workspace_dir = indexes_dir.join(&full_workspace_id);
    let db_dir = workspace_dir.join("db");
    fs::create_dir_all(&db_dir).expect("Failed to create workspace db dir");

    // Copy fixture database to temp workspace
    let fixture_db_src = fixture.db_path();
    let fixture_db_dest = db_dir.join("symbols.db");
    let src_size = fs::metadata(fixture_db_src)
        .expect("Failed to read fixture DB metadata")
        .len();
    fs::copy(fixture_db_src, &fixture_db_dest).expect("Failed to copy fixture database");
    let dest_size = fs::metadata(&fixture_db_dest)
        .expect("Failed to read copied DB metadata")
        .len();
    println!("✓ Fixture database copied: {} bytes", dest_size);
    assert_eq!(src_size, dest_size, "Database copy size mismatch!");

    // Create handler
    let handler = JulieServerHandler::new()
        .await
        .expect("Failed to create handler");

    // Now we need to load the fixture database directly
    // Open a connection to the fixture database directly (not through normal initialization)
    use crate::database::SymbolDatabase;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    // The copied database already has all the data - we just need to wrap it
    // Open a read-write connection directly to the fixture database
    let conn = Connection::open(&fixture_db_dest).expect("Failed to open fixture database");

    // Configure for search operations
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .expect("Failed to set busy timeout");
    conn.pragma_update(None, "wal_autocheckpoint", 2000)
        .expect("Failed to set WAL autocheckpoint");

    // Verify the database has data
    let symbol_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("Failed to count symbols");

    println!(
        "✓ Fixture database opened directly with {} symbols",
        symbol_count
    );

    // Check if FTS5 indexes are healthy before rebuilding (optimization)
    // Only rebuild if indexes are corrupted or non-functional
    let symbols_fts_ok = conn
        .query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE name MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .is_ok();

    let files_fts_ok = conn
        .query_row(
            "SELECT COUNT(*) FROM files_fts WHERE content MATCH 'test'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .is_ok();

    if !symbols_fts_ok || !files_fts_ok {
        // Rebuild FTS5 indexes if corrupted (sometimes copying causes corruption)
        println!("⏳ FTS5 indexes need rebuild, fixing...");
        if !symbols_fts_ok {
            if let Err(e) =
                conn.execute("INSERT INTO symbols_fts(symbols_fts) VALUES('rebuild')", [])
            {
                println!("⚠️  symbols_fts rebuild failed: {}", e);
            } else {
                println!("✓ symbols_fts rebuilt");
            }
        }
        if !files_fts_ok {
            if let Err(e) = conn.execute("INSERT INTO files_fts(files_fts) VALUES('rebuild')", []) {
                println!("⚠️  files_fts rebuild failed: {}", e);
            } else {
                println!("✓ files_fts rebuilt");
            }
        }
    } else {
        println!("✓ FTS5 indexes are healthy, skipping rebuild");
    }

    // Wrap the connection in the database struct
    let db_struct = SymbolDatabase {
        conn,
        file_path: fixture_db_dest.clone(),
    };

    // Create workspace configuration and registry
    {
        // Write julie.toml config
        let config = r#"version = "0.1.0"
languages = []
ignore_patterns = [
    "**/node_modules/**",
    "**/target/**",
    "**/build/**",
    "**/dist/**",
    "**/.git/**",
    "**/*.min.js",
    "**/*.bundle.js",
    "**/.julie/**",
]
max_file_size = 1048576
embedding_model = "bge-small"
incremental_updates = true
"#;
        fs::write(julie_dir.join("julie.toml"), config).expect("Failed to write julie.toml");

        // Create workspace_registry.json so workspace resolution works
        // The registry tracks primary and reference workspaces
        let registry_json = serde_json::json!({
            "version": "1.0",
            "last_updated": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "primary_workspace": {
                "id": full_workspace_id,
                "original_path": temp_root.to_string_lossy(),
                "directory_name": full_workspace_id,
                "display_name": workspace_name,
                "workspace_type": "Primary",
                "created_at": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "last_accessed": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                "expires_at": null,
                "symbol_count": fixture.metadata.symbol_count,
                "file_count": fixture.metadata.file_count,
                "index_size_bytes": dest_size,
                "status": "Active",
                "embedding_status": "NotStarted"
            },
            "reference_workspaces": {},
            "orphaned_indexes": {},
            "config": {
                "default_ttl_seconds": 604800,
                "max_total_size_bytes": 524288000,
                "auto_cleanup_enabled": true,
                "cleanup_interval_seconds": 3600
            },
            "statistics": {
                "total_workspaces": 1,
                "total_orphans": 0,
                "total_index_size_bytes": dest_size,
                "total_symbols": fixture.metadata.symbol_count,
                "total_files": fixture.metadata.file_count,
                "last_cleanup": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            }
        });

        fs::write(
            julie_dir.join("workspace_registry.json"),
            serde_json::to_string_pretty(&registry_json).expect("Failed to serialize registry"),
        )
        .expect("Failed to write workspace_registry.json");
    }

    // Create workspace structure manually with the fixture database
    let mut workspace = crate::workspace::JulieWorkspace {
        root: temp_root.clone(),
        julie_dir: julie_dir.clone(),
        db: Some(Arc::new(Mutex::new(db_struct))),
        embeddings: None,
        vector_store: None,
        watcher: None,
        config: crate::workspace::WorkspaceConfig::default(),
    };

    // Store the workspace in the handler
    {
        let mut workspace_guard = handler.workspace.write().await;
        *workspace_guard = Some(workspace);
    }

    // Mark indexing as complete so searches work immediately
    // We're loading from a pre-indexed fixture, so this status is accurate
    handler
        .indexing_status
        .sqlite_fts_ready
        .store(true, std::sync::atomic::Ordering::Relaxed);

    // Debug confirmation
    {
        let workspace_guard = handler.workspace.read().await;
        if let Some(ws) = workspace_guard.as_ref() {
            println!("✓ Workspace initialized at: {}", ws.root.display());
            if let Some(db_arc) = ws.db.as_ref() {
                if let Ok(db_lock) = db_arc.lock() {
                    match db_lock
                        .conn
                        .query_row("SELECT COUNT(*) FROM symbols", [], |row| {
                            row.get::<_, i64>(0)
                        }) {
                        Ok(count) => println!(
                            "✓ Workspace database has {} symbols ready for search",
                            count
                        ),
                        Err(e) => println!("✗ Error querying symbols: {}", e),
                    }
                }
            }
        }
    }

    // Keep temp directory alive for the handler's lifetime
    // SAFETY: We leak the TempDir to keep it alive - it will be cleaned up by the OS on process exit
    // This is acceptable for tests since they're short-lived
    let _ = Box::leak(Box::new(temp_dir));

    handler
}
