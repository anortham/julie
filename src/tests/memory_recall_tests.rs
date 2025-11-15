// Tests for recall tool (reading memories from disk)
// Following TDD: Write tests first, then implement

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_recall_empty_workspace() -> Result<()> {
    // Setup: Empty workspace with no memories
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Recall should return empty list, not error
    let memories = crate::tools::memory::recall_memories(&workspace_root, Default::default())?;

    assert_eq!(
        memories.len(),
        0,
        "Should return empty list for new workspace"
    );

    Ok(())
}

#[test]
fn test_recall_single_memory() -> Result<()> {
    // Setup: Create one memory
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let memory = crate::tools::memory::Memory::new(
        "mem_test_1".to_string(),
        1234567890,
        "checkpoint".to_string(),
    );

    crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Recall all memories
    let memories = crate::tools::memory::recall_memories(&workspace_root, Default::default())?;

    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].id, "mem_test_1");
    assert_eq!(memories[0].memory_type, "checkpoint");

    Ok(())
}

#[test]
fn test_recall_multiple_memories_chronological() -> Result<()> {
    // Setup: Create memories with different timestamps
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let mem1 = crate::tools::memory::Memory::new(
        "mem_oldest".to_string(),
        1234567890, // Oldest
        "checkpoint".to_string(),
    );

    let mem2 = crate::tools::memory::Memory::new(
        "mem_middle".to_string(),
        1234567900, // Middle
        "checkpoint".to_string(),
    );

    let mem3 = crate::tools::memory::Memory::new(
        "mem_newest".to_string(),
        1234567910, // Newest
        "checkpoint".to_string(),
    );

    // Save in random order
    crate::tools::memory::save_memory(&workspace_root, &mem2)?;
    crate::tools::memory::save_memory(&workspace_root, &mem1)?;
    crate::tools::memory::save_memory(&workspace_root, &mem3)?;

    // Recall should return in chronological order (oldest first)
    let memories = crate::tools::memory::recall_memories(&workspace_root, Default::default())?;

    assert_eq!(memories.len(), 3);
    assert_eq!(memories[0].id, "mem_oldest");
    assert_eq!(memories[1].id, "mem_middle");
    assert_eq!(memories[2].id, "mem_newest");

    Ok(())
}

#[test]
fn test_recall_filter_by_type() -> Result<()> {
    // Setup: Create memories of different types
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let checkpoint = crate::tools::memory::Memory::new(
        "mem_checkpoint".to_string(),
        1234567890,
        "checkpoint".to_string(),
    );

    let decision = crate::tools::memory::Memory::new(
        "mem_decision".to_string(),
        1234567891,
        "decision".to_string(),
    );

    let learning = crate::tools::memory::Memory::new(
        "mem_learning".to_string(),
        1234567892,
        "learning".to_string(),
    );

    crate::tools::memory::save_memory(&workspace_root, &checkpoint)?;
    crate::tools::memory::save_memory(&workspace_root, &decision)?;
    crate::tools::memory::save_memory(&workspace_root, &learning)?;

    // Filter by type
    let options = crate::tools::memory::RecallOptions {
        memory_type: Some("decision".to_string()),
        ..Default::default()
    };

    let memories = crate::tools::memory::recall_memories(&workspace_root, options)?;

    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].id, "mem_decision");
    assert_eq!(memories[0].memory_type, "decision");

    Ok(())
}

#[test]
fn test_recall_filter_by_date_range() -> Result<()> {
    // Setup: Create memories on different days
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Jan 1, 2009
    let mem1 = crate::tools::memory::Memory::new(
        "mem_jan_1".to_string(),
        1230768000, // 2009-01-01
        "checkpoint".to_string(),
    );

    // Jan 5, 2009
    let mem2 = crate::tools::memory::Memory::new(
        "mem_jan_5".to_string(),
        1231113600, // 2009-01-05
        "checkpoint".to_string(),
    );

    // Jan 10, 2009
    let mem3 = crate::tools::memory::Memory::new(
        "mem_jan_10".to_string(),
        1231545600, // 2009-01-10
        "checkpoint".to_string(),
    );

    crate::tools::memory::save_memory(&workspace_root, &mem1)?;
    crate::tools::memory::save_memory(&workspace_root, &mem2)?;
    crate::tools::memory::save_memory(&workspace_root, &mem3)?;

    // Recall only Jan 5-10
    let options = crate::tools::memory::RecallOptions {
        since: Some(1231113600), // Jan 5
        until: Some(1231632000), // Jan 11
        ..Default::default()
    };

    let memories = crate::tools::memory::recall_memories(&workspace_root, options)?;

    assert_eq!(memories.len(), 2);
    assert_eq!(memories[0].id, "mem_jan_5");
    assert_eq!(memories[1].id, "mem_jan_10");

    Ok(())
}

#[test]
fn test_recall_limit() -> Result<()> {
    // Setup: Create many memories
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    for i in 0..20 {
        let memory = crate::tools::memory::Memory::new(
            format!("mem_{}", i),
            1234567890 + i,
            "checkpoint".to_string(),
        );
        crate::tools::memory::save_memory(&workspace_root, &memory)?;
    }

    // Limit to 5 results
    let options = crate::tools::memory::RecallOptions {
        limit: Some(5),
        ..Default::default()
    };

    let memories = crate::tools::memory::recall_memories(&workspace_root, options)?;

    assert_eq!(memories.len(), 5, "Should respect limit");

    Ok(())
}

#[test]
fn test_recall_preserves_extra_fields() -> Result<()> {
    // Setup: Create memory with type-specific fields
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let memory = crate::tools::memory::Memory::new(
        "mem_with_extras".to_string(),
        1234567890,
        "checkpoint".to_string(),
    )
    .with_extra(serde_json::json!({
        "description": "Test memory with extras",
        "tags": ["test", "example"],
        "custom_field": 42
    }));

    crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Recall and verify extras are preserved
    let memories = crate::tools::memory::recall_memories(&workspace_root, Default::default())?;

    assert_eq!(memories.len(), 1);

    let extra = memories[0].extra.as_object().unwrap();
    assert_eq!(
        extra.get("description").unwrap().as_str().unwrap(),
        "Test memory with extras"
    );
    assert_eq!(extra.get("tags").unwrap().as_array().unwrap().len(), 2);
    assert_eq!(extra.get("custom_field").unwrap().as_i64().unwrap(), 42);

    Ok(())
}

#[test]
fn test_recall_handles_corrupted_json_gracefully() -> Result<()> {
    // Setup: Create a memory, then corrupt the JSON
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let memory = crate::tools::memory::Memory::new(
        "mem_good".to_string(),
        1234567890,
        "checkpoint".to_string(),
    );

    let file_path = crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Corrupt the JSON file
    let corrupted_path = file_path.parent().unwrap().join("corrupted.json");
    fs::write(&corrupted_path, "{ invalid json }")?;

    // Recall should skip corrupted file and return good one
    let memories = crate::tools::memory::recall_memories(&workspace_root, Default::default())?;

    assert_eq!(memories.len(), 1, "Should skip corrupted file");
    assert_eq!(memories[0].id, "mem_good");

    Ok(())
}
