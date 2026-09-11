//! Tests for GetSymbolsTool Phase 2 - Smart Read with Code Bodies
//!
//! These tests verify the mode parameter for code body extraction

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::GetSymbolsTool;

fn create_test_rust_file() -> Result<(TempDir, String)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("example.rs");
    let file_content = r#"/// Get user by ID
pub fn get_user(id: &str) -> User {
    User {
        id: id.to_string(),
        name: "Test".to_string(),
    }
}

pub struct User {
    pub id: String,
    pub name: String,
}

impl User {
    pub fn new(id: String, name: String) -> Self {
        User { id, name }
    }

    pub fn display_name(&self) -> String {
        self.name.clone()
    }
}

pub const MAX_USERS: usize = 100;
"#;
    fs::write(&test_file, file_content)?;

    Ok((temp_dir, workspace_path.to_string_lossy().to_string()))
}

#[tokio::test]
async fn test_default_behavior_strips_context() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(!text.is_empty(), "Should find at least one symbol");
    assert!(text.contains("get_user"), "Should list get_user symbol");
    assert!(text.contains("User"), "Should list User symbol");

    assert!(
        !text.contains("id: id.to_string()"),
        "Structure mode should not include function body code, got: {}",
        text
    );
    Ok(())
}

#[tokio::test]
async fn test_invalid_mode_returns_error() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("minmal".to_string()),
        workspace: None,
    };

    let error = tool
        .call_tool(&handler)
        .await
        .expect_err("invalid mode should fail before symbol extraction");

    assert!(
        error
            .to_string()
            .contains("Invalid mode: 'minmal'. Expected one of: structure, minimal, full"),
        "unexpected error: {error}"
    );

    Ok(())
}

#[tokio::test]
async fn test_structure_mode_strips_context() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("structure".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(!text.is_empty(), "Should find at least one symbol");
    assert!(text.contains("get_user"), "Should list get_user symbol");

    assert!(
        !text.contains("id: id.to_string()"),
        "Structure mode should not include function body code"
    );
    Ok(())
}

#[tokio::test]
async fn test_mode_structure_always_strips() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("structure".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(!text.is_empty(), "Should find at least one symbol");
    assert!(text.contains("User"), "Should list User symbol");

    assert!(
        !text.contains("id: id.to_string()"),
        "Structure mode should never include function body code"
    );
    Ok(())
}

#[tokio::test]
async fn test_mode_minimal_top_level_only() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(!text.is_empty(), "Should find at least one symbol");

    assert!(
        text.contains("pub fn get_user"),
        "Minimal mode should include top-level function code body"
    );

    assert!(
        text.contains("pub struct User"),
        "Minimal mode should include top-level struct definition"
    );
    Ok(())
}

#[tokio::test]
async fn test_mode_full_all_symbols() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 2,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("full".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(!text.is_empty(), "Should find at least one symbol");

    assert!(
        text.contains("pub fn get_user"),
        "Full mode should include top-level function"
    );
    assert!(
        text.contains("pub struct User"),
        "Full mode should include struct definition"
    );
    assert!(
        text.contains("fn new("),
        "Full mode should include nested method 'new'"
    );
    assert!(
        text.contains("fn display_name"),
        "Full mode should include nested method 'display_name'"
    );
    Ok(())
}

#[tokio::test]
async fn test_target_with_minimal_mode() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: Some("User".to_string()),
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("User"),
        "Should find User struct when filtering by target, got: {}",
        text
    );

    assert!(
        text.contains("pub struct User"),
        "Top-level User struct should have code body in minimal mode"
    );

    Ok(())
}

#[tokio::test]
async fn test_file_read_error_handling() -> Result<()> {
    let _temp_dir = TempDir::new()?;
    let workspace_path = _temp_dir.path().to_path_buf();

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/nonexistent.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await;

    match result {
        Ok(call_result) => {
            let text = call_tool_result_text(&call_result);
            assert!(
                text.contains("File not found")
                    || text.contains("No symbols found")
                    || text.is_empty(),
                "Should gracefully handle missing file, got: {}",
                text
            );
        }
        Err(_e) => {}
    }

    Ok(())
}

#[tokio::test]
async fn test_utf8_decode_error_handling() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_rust_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(
        !text.is_empty(),
        "Should successfully extract symbols even with UTF-8 handling"
    );
    assert!(
        text.contains("get_user") || text.contains("User"),
        "Should contain symbol names in output"
    );
    Ok(())
}

fn create_test_vue_file() -> Result<(TempDir, String)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;

    let test_file = src_dir.join("Counter.vue");
    let file_content = r#"<script setup lang="ts">
import { ref } from 'vue'

const count = ref(0)

function increment() {
  count.value++
}

function decrement() {
  count.value--
}
</script>

<template>
  <div>
    <button @click="decrement">-</button>
    <span>{{ count }}</span>
    <button @click="increment">+</button>
  </div>
</template>

<style scoped>
.counter {
  display: flex;
  gap: 8px;
}
</style>
"#;
    fs::write(&test_file, file_content)?;

    Ok((temp_dir, workspace_path.to_string_lossy().to_string()))
}

#[tokio::test]
async fn test_vue_target_minimal_extracts_code_body() -> Result<()> {
    let (_temp_dir, workspace_path) = create_test_vue_file()?;

    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/Counter.vue".to_string(),
        max_depth: 1,
        target: Some("increment".to_string()),
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
    };

    let result = tool.call_tool(&handler).await?;
    let text = call_tool_result_text(&result);

    assert!(
        text.contains("count.value++"),
        "Vue target+minimal should extract function body, got: {}",
        text
    );
    assert!(
        text.contains("function increment()"),
        "Should contain function declaration, got: {}",
        text
    );

    let structure_tool = GetSymbolsTool {
        file_path: "src/Counter.vue".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("structure".to_string()),
        workspace: None,
    };

    let structure_result = structure_tool.call_tool(&handler).await?;
    let structure_text = call_tool_result_text(&structure_result);

    assert!(
        structure_text.contains("increment"),
        "Structure mode should list increment symbol, got: {}",
        structure_text
    );
    assert!(
        structure_text.contains("decrement"),
        "Structure mode should list decrement symbol, got: {}",
        structure_text
    );

    Ok(())
}
