use super::*;

/// Test 2: Stale index - file modified after last index
/// Given: File is modified AFTER database was last updated
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_stale_index_file_modified_after_db() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index a test file
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Sleep to ensure mtime changes
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Modify the file AFTER indexing
    fs::write(&test_file, "fn hello() { println!(\"world\"); }")?;

    // Verify: Indexing IS needed (file is newer than database)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "Modified file should trigger re-indexing");

    Ok(())
}

#[tokio::test]
async fn test_primary_workspace_repair_plan_reports_stale_files_reason() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    fs::write(&test_file, "fn hello() { println!(\"world\"); }")?;

    let repair_plan = crate::startup::plan_primary_workspace_repair(&handler)
        .await?
        .expect("stale workspace should produce a repair plan");

    assert!(
        repair_plan
            .reasons
            .contains(&IndexingRepairReason::StaleFiles),
        "stale workspace should surface the stale-files repair reason"
    );

    Ok(())
}

/// Test 3: New file added that isn't in database
/// Given: A new file exists that wasn't indexed
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_new_file_not_in_database() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index first file
    let first_file = workspace_path.join("first.rs");
    fs::write(&first_file, "fn first() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Add a NEW file that isn't indexed
    let new_file = workspace_path.join("second.rs");
    fs::write(&new_file, "fn second() {}")?;

    // Verify: Indexing IS needed (new file detected)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "New file should trigger re-indexing");

    Ok(())
}

#[tokio::test]
async fn test_primary_workspace_repair_plan_reports_new_files_reason() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let first_file = workspace_path.join("first.rs");
    fs::write(&first_file, "fn first() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let new_file = workspace_path.join("second.rs");
    fs::write(&new_file, "fn second() {}")?;

    let repair_plan = crate::startup::plan_primary_workspace_repair(&handler)
        .await?
        .expect("new file should produce a repair plan");

    assert!(
        repair_plan
            .reasons
            .contains(&IndexingRepairReason::NewFiles),
        "repair plan should report new files explicitly"
    );

    Ok(())
}

#[tokio::test]
async fn test_primary_workspace_repair_plan_reports_extractor_failure_reason() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}\n")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let snapshot = handler.primary_workspace_snapshot().await?;
    {
        let db_lock = snapshot
            .database
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        db_lock.conn.execute(
            "INSERT INTO indexing_repairs (path, reason, detail, updated_at)
             VALUES (?1, ?2, ?3, 0)",
            rusqlite::params!["test.rs", "extractor_failure", "seeded startup repair"],
        )?;
    }

    let repair_plan = crate::startup::plan_primary_workspace_repair(&handler)
        .await?
        .expect("persisted extractor repair should produce a repair plan");

    assert!(
        repair_plan
            .reasons
            .contains(&IndexingRepairReason::ExtractorFailure),
        "repair plan should surface persisted extractor-failure state"
    );

    Ok(())
}

/// Test 4: Empty database still triggers indexing (existing behavior)
/// Given: Database is completely empty
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_empty_database_needs_indexing() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create test file but DON'T index
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    let handler = create_test_handler(workspace_path).await?;

    // Verify: Indexing IS needed (database is empty)
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "Empty database should need indexing");

    Ok(())
}

/// Test 5: Multiple stale files trigger re-indexing
/// Given: Multiple files modified after last index
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (indexing needed)
#[tokio::test]
async fn test_multiple_stale_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create and index multiple files
    let file1 = workspace_path.join("file1.rs");
    let file2 = workspace_path.join("file2.rs");
    fs::write(&file1, "fn one() {}")?;
    fs::write(&file2, "fn two() {}")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Sleep to ensure mtime changes
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Modify BOTH files after indexing
    fs::write(&file1, "fn one() { println!(\"modified\"); }")?;
    fs::write(&file2, "fn two() { println!(\"modified\"); }")?;

    // Verify: Indexing IS needed
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(
        needs_indexing,
        "Multiple modified files should trigger re-indexing"
    );

    Ok(())
}
