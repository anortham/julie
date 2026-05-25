use super::*;

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_fresh_index_no_reindex_needed() -> Result<()> {
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create a simple test file
    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}")?;

    // Initialize workspace and index
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Seed mtime explicitly to a fixed old timestamp so db_mtime is
    // unambiguously newer even when indexing is slow under integration load.
    let backdated = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    File::options()
        .write(true)
        .open(&test_file)?
        .set_modified(backdated)?;

    // Verify: No indexing needed (database is fresh)
    let repair_plan = crate::startup::plan_primary_workspace_repair(&handler).await?;
    assert!(
        repair_plan.is_none(),
        "Fresh index should not need re-indexing; repair plan: {repair_plan:?}"
    );

    Ok(())
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_primary_workspace_repair_plan_reports_semantic_version_changed() -> Result<()> {
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let test_file = workspace_path.join("test.rs");
    fs::write(&test_file, "fn hello() {}\n")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let backdated = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    File::options()
        .write(true)
        .open(&test_file)?
        .set_modified(backdated)?;

    let workspace_id = handler.require_primary_workspace_identity()?;
    let db = handler.primary_pooled_database().await?;
    db.set_index_engine_version(
        &workspace_id,
        SEMANTIC_INDEX_ENGINE_COMPONENT,
        "stale-test-version",
    )?;

    let repair_plan = crate::startup::plan_primary_workspace_repair(&handler)
        .await?
        .expect("stale semantic engine version should produce a repair plan");

    assert!(
        repair_plan
            .reasons
            .contains(&IndexingRepairReason::SemanticVersionChanged),
        "repair plan should report semantic-version drift explicitly: {repair_plan:?}"
    );
    assert!(
        !repair_plan
            .reasons
            .contains(&IndexingRepairReason::StaleFiles),
        "backdated source file should keep this regression focused on semantic-version drift"
    );

    Ok(())
}

/// Finding #3 regression: an indexed workspace containing extensionless text
/// files (Dockerfile, Makefile, etc.) must not flag phantom "deleted file"
/// signals on reconnection. Before the fix, the indexer accepted those files
/// via `is_likely_text_file` but `scan_workspace_files` rejected them (its
/// `is_code_file` filter requires a matching extension), so every freshness
/// check reported extensionless files as "indexed but missing" and forced an
/// unnecessary re-index on session reconnect.
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_fresh_index_with_extensionless_text_files_needs_no_reindex() -> Result<()> {
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // A regular code file and two extensionless text files the indexer accepts.
    let rust_file = workspace_path.join("main.rs");
    fs::write(&rust_file, "fn main() {}\n")?;

    let dockerfile = workspace_path.join("Dockerfile");
    fs::write(&dockerfile, "FROM alpine:latest\nRUN echo hello\n")?;

    let makefile = workspace_path.join("Makefile");
    fs::write(&makefile, ".PHONY: all\nall:\n\techo hello\n")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Backdate every file so `db_mtime > max(file_mtime)` regardless of FS
    // clock resolution or integration-test load.
    let backdated = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    for path in [&rust_file, &dockerfile, &makefile] {
        File::options()
            .write(true)
            .open(path)?
            .set_modified(backdated)?;
    }

    let repair_plan = crate::startup::plan_primary_workspace_repair(&handler).await?;
    assert!(
        repair_plan.is_none(),
        "Fresh index with Dockerfile + Makefile must not trigger re-indexing (scan/index asymmetry); repair plan: {repair_plan:?}"
    );

    Ok(())
}
