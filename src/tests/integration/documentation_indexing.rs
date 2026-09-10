//! End-to-End Integration Tests for Documentation Indexing (RAG POC)
//!
//! These tests verify that markdown documentation files flow through the complete pipeline:
//! 1. File discovery (markdown files found)
//! 2. Symbol extraction (markdown extractor processes files)
//! 3. Documentation storage (symbols table with content_type='documentation')
//! 4. Deduplication (file_hash prevents duplicate processing)

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
#[serial_test::serial(embedding_env)]
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

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    let symbols = graph_symbols(&workspace);
    println!("DEBUG: Total symbols in snapshot: {}", symbols.len());
    for symbol in symbols.iter().take(5) {
        println!(
            "  - name={}, language={}, content_type={:?}",
            symbol.name, symbol.language, symbol.content_type
        );
    }

    let doc_count = symbols
        .iter()
        .filter(|symbol| symbol.content_type.as_deref() == Some("documentation"))
        .count();
    assert!(
        doc_count >= 3,
        "Should have at least 3 documentation sections (README.md, ARCHITECTURE.md, GUIDE.md), found {}",
        doc_count
    );
    assert_eq!(
        documentation_in_path(&workspace, "README.md").len(),
        1,
        "Should find 1 README.md documentation entry"
    );
    assert_eq!(
        documentation_in_path(&workspace, "ARCHITECTURE.md").len(),
        1,
        "Should find 1 ARCHITECTURE.md documentation entry"
    );
    assert_eq!(
        documentation_in_path(&workspace, "main.rs").len(),
        0,
        "main.rs should NOT have content_type='documentation' (it's code, not documentation)"
    );
    assert_eq!(
        documentation_in_path(&workspace, "config.json").len(),
        0,
        "config.json should NOT have content_type='documentation' (it's configuration, not documentation)"
    );

    Ok(())
}

/// Test 3: File hash deduplication
/// Given: Documentation indexed, then re-indexed without changes
/// When: Second indexing occurs
/// Expected: Duplicate prevention via file_hash (INSERT OR REPLACE with same hash)
#[tokio::test]
#[serial_test::serial(embedding_env)]
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
    assert_eq!(
        documentation_in_path(&workspace, "CHANGELOG.md").len(),
        1,
        "Should have 1 entry after first indexing"
    );

    index_workspace(&handler, workspace_path).await?;

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace initialized");
    assert_eq!(
        documentation_in_path(&workspace, "CHANGELOG.md").len(),
        1,
        "Should still have 1 entry after re-indexing (no duplicate)"
    );

    Ok(())
}

/// Test 4: Modified documentation updates existing entry
/// Given: Documentation indexed, then content modified
/// When: Re-indexing occurs
/// Expected: Existing entry updated (not duplicated) and doc_comment reflects new content
#[tokio::test]
#[serial_test::serial(embedding_env)]
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
    let initial = documentation_in_path(&workspace, "API.md");
    let initial_content = initial[0]
        .doc_comment
        .clone()
        .unwrap_or_default();
    assert!(
        initial_content.contains("Version 1.0 API"),
        "Initial content should contain 'Version 1.0 API'"
    );

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    fs::write(
        &doc_file,
        "# API Reference\n\nVersion 2.0 API with new endpoints",
    )?;

    index_workspace(&handler, workspace_path).await?;

    let workspace = handler
        .get_workspace()
        .await?
        .expect("Workspace should still exist after re-index");
    let updated = documentation_in_path(&workspace, "API.md");
    assert_eq!(
        updated.len(),
        1,
        "Should still have exactly 1 entry (replaced, not duplicated)"
    );
    let updated_content = updated[0].doc_comment.clone().unwrap_or_default();
    assert_ne!(
        initial_content, updated_content,
        "Content should change when documentation is modified"
    );
    assert!(
        updated_content.contains("Version 2.0 API"),
        "Updated content should contain 'Version 2.0 API'"
    );

    Ok(())
}

/// Test 5: Multiple markdown sections from single file
/// Given: Markdown file with multiple heading sections
/// When: Indexing occurs
/// Expected: Multiple symbol entries in symbols table, one per section (markdown extractor behavior)
#[tokio::test]
#[serial_test::serial(embedding_env)]
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
    let section_count = documentation_in_path(&workspace, "MULTIPART.md").len();
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
    let handler = JulieServerHandler::new_for_test().await?;
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

fn graph_symbols(workspace: &crate::workspace::JulieWorkspace) -> Vec<julie_facts::rows::SymbolRow> {
    let snapshot = workspace.store.current();
    let graph = snapshot.graph();
    (0..graph.len())
        .map(|i| graph.symbol(julie_index::graph::SymbolId(i as u32)).clone())
        .collect()
}

fn documentation_in_path(
    workspace: &crate::workspace::JulieWorkspace,
    path_needle: &str,
) -> Vec<julie_facts::rows::SymbolRow> {
    graph_symbols(workspace)
        .into_iter()
        .filter(|symbol| {
            symbol.content_type.as_deref() == Some("documentation")
                && symbol.path.contains(path_needle)
        })
        .collect()
}
