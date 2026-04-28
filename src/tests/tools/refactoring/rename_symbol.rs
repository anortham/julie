//! Unit tests for RenameSymbolTool
//!
//! Tests verify workspace-wide symbol renaming with flat parameters

use crate::handler::JulieServerHandler;
use crate::tools::refactoring::RenameSymbolTool;
use crate::tools::workspace::ManageWorkspaceTool;
use anyhow::Result;
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_rename_symbol_basic() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    // Setup: Create temp workspace with a Rust file
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn getUserData() { println!(\"test\"); }")?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // Index workspace for symbol lookup
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    // NEW API: Flat parameters (no JSON string)
    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Debug: Print result
    eprintln!("Rename result: {:?}", result);

    // Verify rename occurred
    let content = fs::read_to_string(&test_file)?;
    eprintln!("File content after rename: {}", content);
    assert!(
        content.contains("fetchUserData"),
        "Symbol should be renamed, but file contains: {}",
        content
    );
    assert!(
        !content.contains("getUserData"),
        "Old symbol should be gone"
    );

    // Verify result text confirms the rename
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("applied") && result_text.contains("change"),
        "Result should confirm applied changes, got: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_validation_same_name() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // ERROR CASE: old_name == new_name
    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "getUserData".to_string(), // Same name!
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;

    // Should return Ok with error message
    assert!(result.is_ok(), "Should return Ok with error message");
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("same") || result_text.contains("identical"),
        "Should reject same old/new names: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_validation_empty_names() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // ERROR CASE: Empty old_name
    let tool = RenameSymbolTool {
        old_name: "".to_string(), // Empty!
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;
    assert!(result.is_ok(), "Should return Ok with error message");
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("empty") || result_text.contains("required"),
        "Should reject empty old_name: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_validation_invalid_new_name() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetch user data".to_string(),
        scope: None,
        dry_run: true,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    assert!(
        result_text.contains("invalid identifier"),
        "Should reject a new_name that cannot be a code identifier: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_dry_run() -> Result<()> {
    // Dry run should preview changes without modifying files
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn getUserData() {}")?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // Index workspace for symbol lookup
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: true, // DRY RUN
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    // Should show preview
    let result_text_lower = result_text.to_lowercase();
    assert!(
        result_text_lower.contains("preview")
            || result_text_lower.contains("would")
            || result_text_lower.contains("dry"),
        "Expected preview indicator in result: {}",
        result_text
    );

    // File should NOT be modified
    let content = fs::read_to_string(&test_file)?;
    assert!(
        content.contains("getUserData"),
        "Dry run should not modify file"
    );
    assert!(
        !content.contains("fetchUserData"),
        "Dry run should not modify file"
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_file_scope_accepts_absolute_path() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(&test_file, "fn getUserData() { getUserData(); }")?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: Some(format!("file:{}", test_file.to_string_lossy())),
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);
    let content = fs::read_to_string(&test_file)?;

    assert!(
        content.contains("fetchUserData"),
        "Absolute file scope should rename the indexed file; result: {}",
        result_text
    );
    assert!(
        !content.contains("getUserData"),
        "Old symbol should be gone from scoped file"
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_qualified_method_renames_matching_parent_definition_only() -> Result<()>
{
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("main.rs");
    fs::write(
        &test_file,
        r#"
struct Target;
impl Target {
    fn getUserData(&self) {}
}

struct Other;
impl Other {
    fn getUserData(&self) {}
}

fn caller(target: Target, other: Other) {
    target.getUserData();
    other.getUserData();
}
"#,
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = RenameSymbolTool {
        old_name: "Target::getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);
    let content = fs::read_to_string(&test_file)?;

    assert!(
        content.contains("fn fetchUserData(&self) {}"),
        "Target method definition should be renamed; result: {}",
        result_text
    );
    assert!(
        content.contains("impl Other {\n    fn getUserData(&self) {}"),
        "Other method definition should not be renamed"
    );
    assert!(
        content.contains("other.getUserData();"),
        "Other method call should not be renamed"
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_multiple_files() -> Result<()> {
    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    // Verify workspace-wide rename across multiple files
    let temp_dir = TempDir::new()?;
    let file1 = temp_dir.path().join("main.rs");
    let file2 = temp_dir.path().join("lib.rs");

    fs::write(&file1, "fn getUserData() { /* main */ }")?;
    fs::write(
        &file2,
        "use crate::getUserData; fn test() { getUserData(); }",
    )?;

    let handler = JulieServerHandler::new_for_test().await?;
    handler
        .initialize_workspace_with_force(Some(temp_dir.path().to_string_lossy().to_string()), true)
        .await?;

    // Index workspace for symbol lookup
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(temp_dir.path().to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    let tool = RenameSymbolTool {
        old_name: "getUserData".to_string(),
        new_name: "fetchUserData".to_string(),
        scope: None,
        dry_run: false,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;

    // Verify both files modified
    let content1 = fs::read_to_string(&file1)?;
    let content2 = fs::read_to_string(&file2)?;

    assert!(
        content1.contains("fetchUserData"),
        "File 1 should be renamed"
    );
    assert!(
        content2.contains("fetchUserData"),
        "File 2 should be renamed"
    );

    // Verify result reports multiple files
    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("2") || result_text.contains("files"),
        "Should report multiple files changed: {}",
        result_text
    );

    Ok(())
}

#[tokio::test]
async fn test_rename_symbol_primary_resolves_rebound_current_primary_root() -> Result<()> {
    use crate::daemon::database::DaemonDatabase;
    use crate::daemon::workspace_pool::WorkspacePool;
    use crate::tools::refactoring::resolve_workspace_root;
    use crate::workspace::registry::generate_workspace_id;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(&original_root)?;
    fs::create_dir_all(&rebound_root)?;
    fs::write(original_root.join("main.rs"), "fn original_only() {}\n")?;
    fs::write(
        rebound_root.join("main.rs"),
        "fn rebound_name() { rebound_name(); }\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_id = generate_workspace_id(&original_path.to_string_lossy())?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_id = generate_workspace_id(&rebound_path.to_string_lossy())?;
    daemon_db.upsert_workspace(&original_id, &original_path.to_string_lossy(), "ready")?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path.to_string_lossy(), "ready")?;

    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
    let seed_handler = JulieServerHandler::new_with_shared_workspace(
        rebound_ws,
        rebound_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(rebound_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(rebound_path.to_string_lossy().to_string()),
        force: Some(true),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&seed_handler)
    .await?;

    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());
    handler
        .activate_workspace_with_root(&rebound_id, rebound_path.clone())
        .await?;

    let resolved_root = resolve_workspace_root(Some("primary"), &handler).await?;
    assert_eq!(resolved_root.canonicalize()?, rebound_path.canonicalize()?);

    Ok(())
}
