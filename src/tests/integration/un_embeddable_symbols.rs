//! TDD Tests for Un-embeddable Symbols SQL Filtering
//!
//! **Bug #3: Un-embeddable Symbols Queued Forever**
//!
//! Problem:
//! - `get_symbols_without_embeddings()` uses simple LEFT JOIN (search.rs:98)
//! - Doesn't filter symbols where `build_embedding_text()` returns empty string:
//!   * Markdown headings without doc comments
//!   * Memory JSON symbols other than "description"
//! - These symbols queued forever, wasting GPU/CPU on every reindex
//! - Background job spawns unnecessarily even when no embeddable symbols changed
//!
//! Solution:
//! - Add SQL filters to exclude un-embeddable symbols:
//!   * Filter: `NOT (language = 'markdown' AND (doc_comment IS NULL OR doc_comment = ''))`
//!   * Filter: `NOT (file_path LIKE '.memories/%' AND name != 'description')`
//!
//! Test Scenarios:
//! 1. Markdown headings without doc comments are excluded
//! 2. Memory JSON symbols (except "description") are excluded
//! 3. Embeddable symbols still appear in results

use crate::database::SymbolDatabase;
use crate::extractors::base::{Symbol, SymbolKind};
use anyhow::Result;

/// Helper to create test symbols with required fields
fn create_symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    language: &str,
    file_path: &str,
    doc_comment: Option<String>,
    signature: Option<String>,
    start_line: u32,
    code_context: Option<String>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: language.to_string(),
        file_path: file_path.to_string(),
        doc_comment,
        signature,
        start_line,
        end_line: start_line,
        start_column: 0,
        end_column: 20,
        start_byte: 0,
        end_byte: 100,
        parent_id: None,
        visibility: None,
        code_context,
        content_type: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
    }
}

/// Test 1: Markdown headings without doc comments should be excluded
///
/// Given: Database with markdown heading symbols (no doc_comment)
/// When: Call get_symbols_without_embeddings()
/// Expected: Markdown headings with empty/null doc_comment excluded
/// Actual (BUG): All markdown symbols returned, causing infinite retry
#[test]
#[ignore = "Failing test - reproduces Bug #3"]
fn test_markdown_headings_without_docs_excluded() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path)?;

    // Create markdown heading symbol WITH doc comment (embeddable)
    let heading_with_docs = create_symbol(
        "heading_with_docs",
        "Introduction",
        SymbolKind::Module, // markdown headings
        "markdown",
        "README.md",
        Some("This is a documented section".to_string()),
        None,
        1,
        None,
    );

    // Create markdown heading symbol WITHOUT doc comment (un-embeddable)
    let heading_without_docs = create_symbol(
        "heading_without_docs",
        "Overview",
        SymbolKind::Module, // markdown headings
        "markdown",
        "README.md",
        None, // Un-embeddable: build_embedding_text() returns empty
        None,
        3,
        None,
    );

    // Store both symbols (bulk_store_symbols satisfies FK constraints)
    db.bulk_store_symbols(
        &[heading_with_docs.clone(), heading_without_docs.clone()],
        "test_workspace",
    )?;

    // Get symbols without embeddings
    let symbols_needing_embeddings = db.get_symbols_without_embeddings()?;

    // BUG REPRODUCTION: This assertion WILL FAIL
    // The heading without docs should be EXCLUDED, but currently it's included
    assert_eq!(
        symbols_needing_embeddings.len(),
        1,
        "BUG: Should only have 1 embeddable symbol (heading WITH docs), but got {}",
        symbols_needing_embeddings.len()
    );

    assert_eq!(
        symbols_needing_embeddings[0].id, "heading_with_docs",
        "BUG: Only the heading WITH doc_comment should be embeddable"
    );

    Ok(())
}

/// Test 2: Memory JSON symbols (except "description") should be excluded
///
/// Given: Database with memory JSON symbols (id, timestamp, tags, description)
/// When: Call get_symbols_without_embeddings()
/// Expected: Only "description" symbol included, others excluded
/// Actual (BUG): All memory symbols returned
#[test]
#[ignore = "Failing test - reproduces Bug #3"]
fn test_memory_symbols_except_description_excluded() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path)?;

    // Create memory JSON symbols
    let memory_id = create_symbol(
        "memory_id",
        "id",
        SymbolKind::Field,
        "json",
        ".memories/2025-11-14/123456_abcd.json",
        None,
        None,
        2,
        Some("\"id\": \"milestone_123456_abcd\"".to_string()),
    );

    let memory_timestamp = create_symbol(
        "memory_timestamp",
        "timestamp",
        SymbolKind::Field,
        "json",
        ".memories/2025-11-14/123456_abcd.json",
        None,
        None,
        3,
        Some("\"timestamp\": 1700000000".to_string()),
    );

    let memory_description = create_symbol(
        "memory_description",
        "description", // ONLY this symbol should be embeddable
        SymbolKind::Field,
        "json",
        ".memories/2025-11-14/123456_abcd.json",
        None,
        None,
        5,
        Some("\"description\": \"Fixed bug #3 in semantic search\"".to_string()),
    );

    // Store all memory symbols (bulk_store_symbols satisfies FK constraints)
    db.bulk_store_symbols(
        &[memory_id, memory_timestamp, memory_description.clone()],
        "test_workspace",
    )?;

    // Get symbols without embeddings
    let symbols_needing_embeddings = db.get_symbols_without_embeddings()?;

    // BUG REPRODUCTION: This assertion WILL FAIL
    // Should only have "description" symbol, but all 3 are returned
    assert_eq!(
        symbols_needing_embeddings.len(),
        1,
        "BUG: Should only have 'description' symbol, but got {}",
        symbols_needing_embeddings.len()
    );

    assert_eq!(
        symbols_needing_embeddings[0].id, "memory_description",
        "BUG: Only 'description' field should be embeddable in memory files"
    );

    Ok(())
}

/// Test 3: Normal embeddable symbols should still appear
///
/// Given: Database with regular Rust function symbols
/// When: Call get_symbols_without_embeddings()
/// Expected: All functions returned (embeddable)
#[test]
fn test_embeddable_symbols_still_returned() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path)?;

    // Create regular Rust function (embeddable)
    let rust_function = create_symbol(
        "rust_fn",
        "getUserData",
        SymbolKind::Function,
        "rust",
        "src/user.rs",
        Some("Get user data from database".to_string()),
        Some("pub fn getUserData(id: u64) -> Option<User>".to_string()),
        10,
        None,
    );

    db.bulk_store_symbols(&[rust_function.clone()], "test_workspace")?;

    // Get symbols without embeddings
    let symbols_needing_embeddings = db.get_symbols_without_embeddings()?;

    // This should pass: Normal functions are embeddable
    assert_eq!(symbols_needing_embeddings.len(), 1);
    assert_eq!(symbols_needing_embeddings[0].id, "rust_fn");

    Ok(())
}
