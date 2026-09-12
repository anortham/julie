//! Tests for get_symbols tool with relative Unix-style path storage.

use anyhow::Result;
use julie_context::WorkspaceTarget;
use julie_test_support::{FakeToolContext, SnapshotFixture};
use std::fs;
use tempfile::TempDir;

use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::symbols::GetSymbolsTool;
use crate::workspace::registry::generate_workspace_id;

#[tokio::test]
async fn test_get_symbols_with_relative_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir)?;
    fs::write(
        src_dir.join("test_unique_file.rs"),
        r#"
        pub fn get_user_data(id: u32) -> String {
            format!("User {}", id)
        }

        pub struct UserService {
            pub name: String,
        }
    "#,
    )?;
    let handler = snapshot_context(temp_dir.path())?;

    let tool = GetSymbolsTool {
        file_path: "src/test_unique_file.rs".to_string(),
        max_depth: 1,
        mode: None,
        limit: None,
        offset: 0,
        target: None,
        workspace: None,
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    assert!(
        result_text.contains("get_user_data") || result_text.contains("UserService"),
        "Should find symbols with relative path input, got: {}",
        result_text
    );
    assert!(
        !result_text.contains("No symbols found"),
        "Should not return 'No symbols found' for valid relative path"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_with_absolute_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let src_dir = temp_dir.path().join("src");
    fs::create_dir(&src_dir)?;
    let test_file = src_dir.join("test_unique_lib.rs");
    fs::write(
        &test_file,
        r#"
        pub fn calculate_score(points: i32) -> i32 {
            points * 2
        }
    "#,
    )?;
    let handler = snapshot_context(temp_dir.path())?;

    let tool = GetSymbolsTool {
        file_path: test_file.to_string_lossy().to_string(),
        max_depth: 1,
        mode: None,
        limit: None,
        offset: 0,
        target: None,
        workspace: None,
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    assert!(
        result_text.contains("calculate_score"),
        "Should find symbols with absolute path input (converted to relative), got: {}",
        result_text
    );
    Ok(())
}

fn rebound_roots(temp_dir: &TempDir, rebound_source: &str) -> Result<std::path::PathBuf> {
    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("old.rs"),
        "fn old_root_only() {}\n",
    )?;
    fs::write(rebound_root.join("src").join("rebound.rs"), rebound_source)?;
    Ok(rebound_root.canonicalize()?)
}

#[tokio::test]
async fn test_get_symbols_relative_path_uses_rebound_current_primary_root() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rebound_path = rebound_roots(&temp_dir, "pub fn rebound_symbol() {}\n")?;
    let rebound_id = generate_workspace_id(&rebound_path.to_string_lossy())?;
    let fixture = SnapshotFixture::from_tree(&rebound_path)?;
    let handler = FakeToolContext::new()
        .with_workspace_id(rebound_id.clone())
        .with_primary_root(rebound_path)
        .with_resolved_target(WorkspaceTarget::Target(rebound_id.clone()))
        .with_snapshot_fixture(fixture);

    let tool = GetSymbolsTool {
        file_path: "src/rebound.rs".to_string(),
        max_depth: 1,
        mode: None,
        limit: None,
        offset: 0,
        target: None,
        workspace: Some(rebound_id),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    assert!(
        result_text.contains("rebound_symbol"),
        "relative get_symbols should resolve against rebound current primary root, not stale loaded root: {}",
        result_text
    );
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_primary_uses_rebound_current_primary_root() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let rebound_path = rebound_roots(&temp_dir, "pub fn rebound_primary_symbol() {}\n")?;
    let handler = snapshot_context(&rebound_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/rebound.rs".to_string(),
        max_depth: 1,
        mode: None,
        limit: None,
        offset: 0,
        target: None,
        workspace: Some("primary".to_string()),
        body_offset: 0,
        body_limit: None,
        source_hash: None,
    };
    let result = tool.call_tool(&handler).await?;
    let result_text = format!("{:?}", result);

    assert!(
        result_text.contains("rebound_primary_symbol"),
        "primary get_symbols should resolve against rebound current primary root, not stale loaded root: {}",
        result_text
    );
    Ok(())
}

#[tokio::test]
async fn test_database_stores_relative_unix_paths() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let tools_dir = temp_dir.path().join("src").join("tools");
    fs::create_dir_all(&tools_dir)?;
    fs::write(
        tools_dir.join("search.rs"),
        r#"
        pub fn search_code() {}
    "#,
    )?;
    let fixture = SnapshotFixture::from_tree(temp_dir.path())?;
    let snapshot = fixture.snapshot();
    let graph = snapshot.graph();

    let paths: Vec<&str> = graph.paths().iter().map(String::as_str).collect();
    assert!(!paths.is_empty(), "Should have indexed symbols");
    for path in &paths {
        assert!(
            !path.starts_with('/'),
            "Path should be relative, not absolute: {}",
            path
        );
        assert!(
            !path.contains('\\'),
            "Path should use Unix-style separators, not backslashes: {}",
            path
        );
    }
    assert!(
        !graph.symbols_in_path("src/tools/search.rs").is_empty(),
        "Should find symbols with relative path 'src/tools/search.rs'"
    );
    Ok(())
}
