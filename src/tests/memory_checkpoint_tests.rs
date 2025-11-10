// Tests for checkpoint tool (saving memories to disk)
// Following TDD: Write tests first, then implement

use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn test_checkpoint_creates_date_directory() -> Result<()> {
    // Setup: Create temp workspace
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create a checkpoint
    let memory = crate::tools::memory::Memory::new(
        "mem_test_123".to_string(),
        1234567890,
        "checkpoint".to_string(),
    );

    crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Verify directory structure exists
    let memories_dir = workspace_root.join(".memories");
    assert!(memories_dir.exists(), "memories directory should exist");

    // Should have created a date directory (format: YYYY-MM-DD)
    let date_dirs: Vec<_> = fs::read_dir(&memories_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    assert_eq!(date_dirs.len(), 1, "Should have exactly one date directory");

    let date_dir_name = date_dirs[0].file_name();
    let date_str = date_dir_name.to_string_lossy();

    // Verify date format (YYYY-MM-DD)
    assert_eq!(date_str.len(), 10, "Date directory should be 10 chars");
    assert_eq!(&date_str[4..5], "-", "Should have hyphen at position 4");
    assert_eq!(&date_str[7..8], "-", "Should have hyphen at position 7");

    Ok(())
}

#[test]
fn test_checkpoint_filename_format() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create checkpoint
    let memory = crate::tools::memory::Memory::new(
        "mem_test_456".to_string(),
        1234567890,
        "checkpoint".to_string(),
    );

    let file_path = crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Verify filename format: HHMMSS_xxxx.json
    let filename = file_path.file_name().unwrap().to_string_lossy();

    // Should be 15 characters: HHMMSS (6) + _ (1) + xxxx (4) + .json (5) = 16
    assert!(filename.len() == 16, "Filename should be 16 chars: {}", filename);
    assert!(filename.ends_with(".json"), "Should end with .json");

    // Should have underscore at position 6
    assert_eq!(&filename[6..7], "_", "Should have underscore at position 6");

    // Time part should be 6 digits
    let time_part = &filename[0..6];
    assert!(time_part.chars().all(|c| c.is_ascii_digit()),
            "Time part should be all digits: {}", time_part);

    // Random part should be 4 hex chars
    let random_part = &filename[7..11];
    assert!(random_part.chars().all(|c| c.is_ascii_hexdigit()),
            "Random part should be hex: {}", random_part);

    Ok(())
}

#[test]
fn test_checkpoint_multiple_same_second_no_collision() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create multiple checkpoints rapidly
    let mut file_paths = Vec::new();
    for i in 0..5 {
        let memory = crate::tools::memory::Memory::new(
            format!("mem_test_{}", i),
            1234567890 + i,
            "checkpoint".to_string(),
        );
        file_paths.push(crate::tools::memory::save_memory(&workspace_root, &memory)?);
    }

    // All filenames should be unique
    let filenames: Vec<String> = file_paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    let unique_count = filenames.iter().collect::<std::collections::HashSet<_>>().len();
    assert_eq!(unique_count, 5, "All filenames should be unique");

    Ok(())
}

#[test]
fn test_checkpoint_pretty_printed_json() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create checkpoint with some data
    let memory = crate::tools::memory::Memory::new(
        "mem_test_789".to_string(),
        1234567890,
        "checkpoint".to_string(),
    )
    .with_extra(serde_json::json!({
        "description": "Test checkpoint",
        "tags": ["test", "example"]
    }));

    let file_path = crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Read file and verify it's pretty-printed
    let content = fs::read_to_string(&file_path)?;

    // Should have newlines (pretty-printed)
    assert!(content.contains('\n'), "Should be pretty-printed with newlines");

    // Should have indentation
    assert!(content.contains("  "), "Should have indentation");

    // Should be valid JSON that roundtrips
    let parsed: crate::tools::memory::Memory = serde_json::from_str(&content)?;
    assert_eq!(parsed.id, "mem_test_789");
    assert_eq!(parsed.timestamp, 1234567890);

    Ok(())
}

#[test]
fn test_checkpoint_atomic_write() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create checkpoint
    let memory = crate::tools::memory::Memory::new(
        "mem_atomic_test".to_string(),
        1234567890,
        "checkpoint".to_string(),
    );

    let file_path = crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Verify file exists
    assert!(file_path.exists(), "File should exist");

    // Verify no temp files left behind
    let parent_dir = file_path.parent().unwrap();
    let temp_files: Vec<_> = fs::read_dir(parent_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".tmp") || e.file_name().to_string_lossy().ends_with(".temp")
        })
        .collect();

    assert_eq!(temp_files.len(), 0, "Should not have any temp files");

    Ok(())
}

#[test]
fn test_checkpoint_with_git_context() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create checkpoint with git context
    let memory = crate::tools::memory::Memory::new(
        "mem_git_test".to_string(),
        1234567890,
        "checkpoint".to_string(),
    )
    .with_git(crate::tools::memory::GitContext {
        branch: "main".to_string(),
        commit: "abc123".to_string(),
        dirty: false,
        files_changed: Some(vec!["src/main.rs".to_string()]),
    });

    let file_path = crate::tools::memory::save_memory(&workspace_root, &memory)?;

    // Read and verify git context is saved
    let content = fs::read_to_string(&file_path)?;
    let parsed: crate::tools::memory::Memory = serde_json::from_str(&content)?;

    let git = parsed.git.expect("Should have git context");
    assert_eq!(git.branch, "main");
    assert_eq!(git.commit, "abc123");
    assert_eq!(git.dirty, false);
    assert_eq!(git.files_changed.unwrap()[0], "src/main.rs");

    Ok(())
}

#[test]
fn test_checkpoint_different_memory_types() -> Result<()> {
    // Setup
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create different memory types
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

    // Save all
    let path1 = crate::tools::memory::save_memory(&workspace_root, &checkpoint)?;
    let path2 = crate::tools::memory::save_memory(&workspace_root, &decision)?;
    let path3 = crate::tools::memory::save_memory(&workspace_root, &learning)?;

    // All should exist
    assert!(path1.exists());
    assert!(path2.exists());
    assert!(path3.exists());

    // Verify types are preserved
    let content1 = fs::read_to_string(&path1)?;
    let parsed1: crate::tools::memory::Memory = serde_json::from_str(&content1)?;
    assert_eq!(parsed1.memory_type, "checkpoint");

    let content2 = fs::read_to_string(&path2)?;
    let parsed2: crate::tools::memory::Memory = serde_json::from_str(&content2)?;
    assert_eq!(parsed2.memory_type, "decision");

    Ok(())
}
