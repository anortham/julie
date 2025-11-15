//! End-to-End Integration Tests for Documentation Indexing (RAG POC)
//!
//! These tests verify that markdown documentation files flow through the complete pipeline:
//! 1. File discovery (markdown files found)
//! 2. Symbol extraction (markdown extractor processes files)
//! 3. Documentation storage (symbols table with content_type='documentation')
//! 4. FTS5 sync (symbols_fts table auto-updated via triggers)
//! 5. Deduplication (file_hash prevents duplicate processing)

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::workspace::ManageWorkspaceTool;

/// Test 1: Basic documentation indexing
/// Given: Workspace with markdown files
/// When: Indexing is performed
/// Expected: Documentation symbols appear in symbols table WHERE content_type='documentation'
#[tokio::test]
async fn test_documentation_indexing_basic() -> Result<()> {
    // Skip embedding generation for faster test execution
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create markdown documentation files
    fs::write(
        workspace_path.join("README.md"),
        "# Test Project\n\nThis is a test project for documentation indexing.",
    )?;

    fs::write(
        workspace_path.join("ARCHITECTURE.md"),
        "# Architecture\n\nThis document describes the system architecture.",
    )?;

    let docs_dir = workspace_path.join("docs");
    fs::create_dir(&docs_dir)?;
    fs::write(
        docs_dir.join("GUIDE.md"),
        "# User Guide\n\nThis guide explains how to use the system.",
    )?;

    // Also create some non-documentation files (regular code symbols)
    fs::write(workspace_path.join("main.rs"), "fn main() {}")?;
    fs::write(workspace_path.join("config.json"), r#"{"version": "1.0"}"#)?;

    // Debug: List all files created
    println!("DEBUG: Created test files:");
    for entry in fs::read_dir(workspace_path)? {
        let entry = entry?;
        println!("  - {}", entry.path().display());
    }
    for entry in fs::read_dir(&docs_dir)? {
        let entry = entry?;
        println!("  - {}", entry.path().display());
    }

    // Initialize and index workspace
    let handler = create_test_handler(workspace_path).await?;

    // Enable debug logging for this test
    unsafe {
        std::env::set_var("RUST_LOG", "julie=debug");
    }

    println!("DEBUG: Starting indexing...");
    index_workspace(&handler, workspace_path).await?;
    println!("DEBUG: Indexing complete");

    // Debug: Check ALL symbols in database
    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    let db_arc = workspace
        .db
        .as_ref()
        .expect("Database should be initialized");
    let db = db_arc.lock().unwrap();

    let total_symbols: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .unwrap_or(0);
    println!("DEBUG: Total symbols in database: {}", total_symbols);

    if total_symbols > 0 {
        // Show some sample symbols
        let mut stmt = db
            .conn
            .prepare("SELECT name, language, content_type FROM symbols LIMIT 5")?;
        let symbols: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        println!("DEBUG: Sample symbols:");
        for (name, lang, ct) in symbols {
            println!(
                "  - name={}, language={}, content_type={:?}",
                name, lang, ct
            );
        }
    }
    drop(db);

    // Verify: Documentation symbols are in symbols table with content_type='documentation'
    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace should be initialized");

    let db_arc = workspace
        .db
        .as_ref()
        .expect("Database should be initialized");
    let db = db_arc.lock().unwrap();

    // Count total documentation entries
    let doc_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    assert!(
        doc_count >= 3,
        "Should have at least 3 documentation sections (README.md, ARCHITECTURE.md, GUIDE.md), found {}",
        doc_count
    );

    // Verify: Specific documentation files are present
    let readme_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%README.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to query README.md");

    assert_eq!(
        readme_count, 1,
        "Should find 1 README.md documentation entry"
    );

    let arch_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%ARCHITECTURE.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to query ARCHITECTURE.md");

    assert_eq!(
        arch_count, 1,
        "Should find 1 ARCHITECTURE.md documentation entry"
    );

    // Verify: Non-documentation files are NOT marked as documentation
    let code_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%main.rs' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to query main.rs");

    assert_eq!(
        code_count, 0,
        "main.rs should NOT have content_type='documentation' (it's code, not documentation)"
    );

    let json_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%config.json' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to query config.json");

    assert_eq!(
        json_count, 0,
        "config.json should NOT have content_type='documentation' (it's configuration, not documentation)"
    );

    Ok(())
}

/// Test 2: FTS5 full-text search sync
/// Given: Documentation indexed into symbols table with content_type='documentation'
/// When: Querying symbols_fts (FTS5 virtual table)
/// Expected: Documentation is searchable via full-text search (triggers auto-synced)
#[tokio::test]
async fn test_documentation_fts5_sync() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create markdown with searchable content
    fs::write(
        workspace_path.join("DEPLOYMENT.md"),
        "# Deployment Guide\n\nInstructions for deploying to production servers using Docker containers.",
    )?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    let db_arc = workspace
        .db
        .as_ref()
        .expect("Database should be initialized");
    let db = db_arc.lock().unwrap();

    // Debug: Check if data exists in symbols table
    let symbols_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to count symbols");

    println!(
        "DEBUG: symbols table has {} documentation entries",
        symbols_count
    );

    // Debug: Check if FTS5 table has any documentation data
    let fts_total: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE rowid IN (SELECT rowid FROM symbols WHERE content_type = 'documentation')",
            [],
            |row| row.get(0),
        )
        .expect("Failed to count symbols_fts");

    println!("DEBUG: symbols_fts has {} documentation entries", fts_total);

    // Debug: Check what content is actually in FTS5
    let actual_content: String = db
        .conn
        .query_row("SELECT doc_comment FROM symbols_fts LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("Failed to get FTS5 content");

    println!("DEBUG: FTS5 doc_comment = '{}'", actual_content);

    // Verify: FTS5 search finds documentation by keyword
    let docker_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols s
             JOIN symbols_fts fts ON s.rowid = fts.rowid
             WHERE fts.doc_comment MATCH 'docker' AND s.content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to search symbols_fts");

    assert_eq!(
        docker_count, 1,
        "FTS5 search should find 'docker' in DEPLOYMENT.md content"
    );

    let production_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols s
             JOIN symbols_fts fts ON s.rowid = fts.rowid
             WHERE fts.doc_comment MATCH 'production' AND s.content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to search symbols_fts");

    assert_eq!(
        production_count, 1,
        "FTS5 search should find 'production' in DEPLOYMENT.md content"
    );

    // Verify: FTS5 search for non-existent term returns zero
    let nonexistent_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols s
             JOIN symbols_fts fts ON s.rowid = fts.rowid
             WHERE fts.doc_comment MATCH 'nonexistent_keyword_xyz' AND s.content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to search symbols_fts");

    assert_eq!(
        nonexistent_count, 0,
        "FTS5 search should return 0 for non-existent keyword"
    );

    Ok(())
}

/// Test 3: File hash deduplication
/// Given: Documentation indexed, then re-indexed without changes
/// When: Second indexing occurs
/// Expected: Duplicate prevention via file_hash (INSERT OR REPLACE with same hash)
#[tokio::test]
async fn test_documentation_deduplication() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create markdown file
    fs::write(
        workspace_path.join("CHANGELOG.md"),
        "# Changelog\n\nVersion 1.0.0 - Initial release",
    )?;

    let handler = create_test_handler(workspace_path).await?;

    // First indexing
    index_workspace(&handler, workspace_path).await?;

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    let db_arc = workspace
        .db
        .as_ref()
        .expect("Database should be initialized");
    let db = db_arc.lock().unwrap();

    // Count after first indexing
    let count_after_first: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%CHANGELOG.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to count after first indexing");

    assert_eq!(
        count_after_first, 1,
        "Should have 1 entry after first indexing"
    );

    drop(db); // Release lock before re-indexing

    // Second indexing (same content - should not create duplicate)
    index_workspace(&handler, workspace_path).await?;

    let db = db_arc.lock().unwrap();

    // Count after second indexing
    let count_after_second: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%CHANGELOG.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to count after second indexing");

    assert_eq!(
        count_after_second, 1,
        "Should still have 1 entry after re-indexing (no duplicate)"
    );

    Ok(())
}

/// Test 4: Modified documentation updates existing entry
/// Given: Documentation indexed, then content modified
/// When: Re-indexing occurs
/// Expected: Existing entry updated (not duplicated) and doc_comment reflects new content
#[tokio::test]
async fn test_documentation_update_on_change() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let doc_file = workspace_path.join("API.md");

    // Initial content
    fs::write(&doc_file, "# API Reference\n\nVersion 1.0 API")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    let db_arc = workspace
        .db
        .as_ref()
        .expect("Database should be initialized");
    let db = db_arc.lock().unwrap();

    // Get initial doc_comment content
    let initial_content: String = db
        .conn
        .query_row(
            "SELECT doc_comment FROM symbols WHERE file_path LIKE '%API.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to get initial content");

    assert!(
        initial_content.contains("Version 1.0 API"),
        "Initial content should contain 'Version 1.0 API'"
    );

    drop(db); // Release lock

    // Modify content
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    fs::write(
        &doc_file,
        "# API Reference\n\nVersion 2.0 API with new endpoints",
    )?;

    // Re-index
    index_workspace(&handler, workspace_path).await?;

    let db = db_arc.lock().unwrap();

    // Get updated doc_comment content
    let updated_content: String = db
        .conn
        .query_row(
            "SELECT doc_comment FROM symbols WHERE file_path LIKE '%API.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to get updated content");

    // Verify: Content changed (reflecting modified file)
    assert_ne!(
        initial_content, updated_content,
        "Content should change when documentation is modified"
    );

    assert!(
        updated_content.contains("Version 2.0 API"),
        "Updated content should contain 'Version 2.0 API'"
    );

    // Verify: Still only one entry (replaced, not duplicated)
    let count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%API.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to count entries");

    assert_eq!(
        count, 1,
        "Should still have exactly 1 entry (replaced, not duplicated)"
    );

    Ok(())
}

/// Test 5: Multiple markdown sections from single file
/// Given: Markdown file with multiple heading sections
/// When: Indexing occurs
/// Expected: Multiple symbol entries in symbols table, one per section (markdown extractor behavior)
#[tokio::test]
async fn test_multiple_sections_from_single_file() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create markdown with multiple sections
    fs::write(
        workspace_path.join("MULTIPART.md"),
        "# Introduction\n\nOverview section.\n\n# Installation\n\nSetup instructions.\n\n# Configuration\n\nConfig details.",
    )?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    let db_arc = workspace
        .db
        .as_ref()
        .expect("Database should be initialized");
    let db = db_arc.lock().unwrap();

    // Count sections from this file
    let section_count: i64 = db
        .conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE file_path LIKE '%MULTIPART.md' AND content_type = 'documentation'",
            [],
            |row| row.get(0),
        )
        .expect("Failed to count sections");

    assert!(
        section_count >= 3,
        "Should have at least 3 sections (Introduction, Installation, Configuration), found {}",
        section_count
    );

    Ok(())
}

// ============================================================================
// Test Helpers
// ============================================================================

async fn create_test_handler(workspace_path: &std::path::Path) -> Result<JulieServerHandler> {
    let handler = JulieServerHandler::new().await?;
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await?;
    Ok(handler)
}

async fn index_workspace(
    handler: &JulieServerHandler,
    workspace_path: &std::path::Path,
) -> Result<()> {
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };

    index_tool.call_tool(handler).await?;
    Ok(())
}
