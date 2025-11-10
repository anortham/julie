// Tests for memory SQL views and indexes
// Following TDD: Write tests first, then implement

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_memories_view_exists() -> Result<()> {
    // Setup: Create database and workspace
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();
    let db_path = workspace_root.join(".julie/indexes/test_workspace/db/symbols.db");
    fs::create_dir_all(db_path.parent().unwrap())?;

    // Create database with schema
    let db = crate::database::SymbolDatabase::new(&db_path)?;

    // Verify memories view exists
    let view_exists: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name='memories'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(view_exists, 1, "memories view should exist");

    Ok(())
}

#[test]
fn test_memories_view_queries_files_table() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();
    let db_path = workspace_root.join(".julie/indexes/test_workspace/db/symbols.db");
    fs::create_dir_all(db_path.parent().unwrap())?;

    // Create memory file
    let memory = crate::tools::memory::Memory::new(
        "mem_test_123".to_string(),
        1234567890,
        "checkpoint".to_string(),
    )
    .with_extra(serde_json::json!({
        "description": "Test memory for SQL view",
        "tags": ["test", "sql"]
    }));

    let file_path = crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Create database
    let mut db = crate::database::SymbolDatabase::new(&db_path)?;

    // Index the memory file (simulating tree-sitter indexing)
    let content = fs::read_to_string(&file_path)?;
    let relative_path = file_path.strip_prefix(&workspace_root).unwrap();
    // Normalize to Unix-style paths (Julie's standard)
    let normalized_path = relative_path.to_string_lossy().replace('\\', "/");

    db.conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            normalized_path,
            "json",
            "test_hash",
            content.len() as i64,
            1234567890i64,
            content,
        ],
    )?;

    // Query memories view
    let count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM memories",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(count, 1, "Should find one memory in view");

    // Query specific fields
    let (id, timestamp, memory_type): (String, i64, String) = db.conn.query_row(
        "SELECT id, timestamp, type FROM memories",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    assert_eq!(id, "mem_test_123");
    assert_eq!(timestamp, 1234567890);
    assert_eq!(memory_type, "checkpoint");

    Ok(())
}

#[test]
fn test_memories_view_filters_json_files() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();
    let db_path = workspace_root.join(".julie/indexes/test_workspace/db/symbols.db");
    fs::create_dir_all(db_path.parent().unwrap())?;

    let mut db = crate::database::SymbolDatabase::new(&db_path)?;

    // Insert non-memory JSON file
    db.conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            "package.json",
            "json",
            "hash1",
            100i64,
            1234567890i64,
            r#"{"name": "test", "version": "1.0.0"}"#,
        ],
    )?;

    // Insert memory JSON file
    let memory_content = serde_json::to_string_pretty(&crate::tools::memory::Memory::new(
        "mem_test".to_string(),
        1234567890,
        "checkpoint".to_string(),
    ))?;

    db.conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            ".memories/2009-02-13/181553_a8f2.json",
            "json",
            "hash2",
            memory_content.len() as i64,
            1234567890i64,
            memory_content,
        ],
    )?;

    // Query memories view - should only see memory files
    let count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM memories",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(count, 1, "Should only find memory files, not other JSON files");

    Ok(())
}

#[test]
fn test_memories_view_handles_missing_optional_fields() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();
    let db_path = workspace_root.join(".julie/indexes/test_workspace/db/symbols.db");
    fs::create_dir_all(db_path.parent().unwrap())?;

    let mut db = crate::database::SymbolDatabase::new(&db_path)?;

    // Insert minimal memory (no description, no tags, no git)
    let memory_content = serde_json::to_string_pretty(&crate::tools::memory::Memory::new(
        "mem_minimal".to_string(),
        1234567890,
        "checkpoint".to_string(),
    ))?;

    db.conn.execute(
        "INSERT INTO files (path, language, hash, size, last_modified, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            ".memories/2009-02-13/181553_b7c3.json",
            "json",
            "hash3",
            memory_content.len() as i64,
            1234567890i64,
            memory_content,
        ],
    )?;

    // Query should work even with missing optional fields
    let (id, description, git_branch): (String, Option<String>, Option<String>) = db.conn.query_row(
        "SELECT id, description, git_branch FROM memories",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;

    assert_eq!(id, "mem_minimal");
    assert!(description.is_none(), "Optional description should be NULL");
    assert!(git_branch.is_none(), "Optional git_branch should be NULL");

    Ok(())
}

#[test]
fn test_memories_timestamp_index_exists() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();
    let db_path = workspace_root.join(".julie/indexes/test_workspace/db/symbols.db");
    fs::create_dir_all(db_path.parent().unwrap())?;

    let db = crate::database::SymbolDatabase::new(&db_path)?;

    // Verify timestamp index exists
    let index_exists: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_memories_timestamp'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(index_exists, 1, "idx_memories_timestamp index should exist");

    Ok(())
}

#[test]
fn test_memories_type_index_exists() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();
    let db_path = workspace_root.join(".julie/indexes/test_workspace/db/symbols.db");
    fs::create_dir_all(db_path.parent().unwrap())?;

    let db = crate::database::SymbolDatabase::new(&db_path)?;

    // Verify type index exists
    let index_exists: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_memories_type'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(index_exists, 1, "idx_memories_type index should exist");

    Ok(())
}
