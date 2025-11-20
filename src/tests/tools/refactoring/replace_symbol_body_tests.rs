//! TDD tests for replace_symbol_body operation
//!
//! These tests verify that function/method bodies are correctly replaced
//! across multiple languages using tree-sitter AST analysis.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::refactoring::SmartRefactorTool;

/// Extract text from CallToolResult
fn extract_text(result: &rust_mcp_sdk::schema::CallToolResult) -> String {
    // Try extracting from .content first (TOON mode)
    if !result.content.is_empty() {
        return result
            .content
            .iter()
            .filter_map(|block| {
                serde_json::to_value(block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                })
            })
            .collect::<Vec<_>>()
            .join("
");
    }

    // Fall back to .structured_content (JSON mode)
    if let Some(structured) = &result.structured_content {
        return serde_json::to_string_pretty(structured).unwrap_or_default();
    }

    String::new()
}

#[tokio::test]
async fn test_replace_typescript_function_body() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.ts");

    // SOURCE: Original TypeScript function
    let source = r#"
export function getUserData(userId: number): Promise<User> {
    // Old implementation with database call
    return database.query('SELECT * FROM users WHERE id = ?', [userId]);
}
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL after replacement
    let expected = r#"
export function getUserData(userId: number): Promise<User> {
    // New implementation with cache
    const cached = cache.get(userId);
    if (cached) return Promise.resolve(cached);
    return api.fetchUser(userId);
}
"#;

    // Execute replace_symbol_body operation
    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "symbol_name": "getUserData",
            "new_body": r#"    // New implementation with cache
    const cached = cache.get(userId);
    if (cached) return Promise.resolve(cached);
    return api.fetchUser(userId);"#
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);

    // This assertion would FAIL if the operation returned "coming soon"
    assert!(
        result.structured_content.as_ref().and_then(|s| s.get("success")).and_then(|v| v.as_bool()).unwrap_or(false),
        "Expected success message, got: {}",
        response
    );

    // Verify the actual file was modified correctly
    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "Body replacement did not produce expected result"
    );

    Ok(())
}

#[tokio::test]
async fn test_replace_python_method_body() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.py");

    // SOURCE: Original Python class method
    let source = r#"
class UserService:
    def get_user(self, user_id):
        # Old implementation
        return self.database.query(user_id)
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL after replacement
    let expected = r#"
class UserService:
    def get_user(self, user_id):
        # New implementation with caching
        cached = self.cache.get(user_id)
        if cached:
            return cached
        result = self.api.fetch_user(user_id)
        self.cache.set(user_id, result)
        return result
"#;

    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "symbol_name": "get_user",
            "new_body": r#"        # New implementation with caching
        cached = self.cache.get(user_id)
        if cached:
            return cached
        result = self.api.fetch_user(user_id)
        self.cache.set(user_id, result)
        return result"#
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(
        result.structured_content.as_ref().and_then(|s| s.get("success")).and_then(|v| v.as_bool()).unwrap_or(false),
        "Expected success, got: {}",
        response
    );

    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual.trim(), expected.trim());

    Ok(())
}

#[tokio::test]
async fn test_replace_rust_function_body() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.rs");

    // SOURCE: Original Rust function
    let source = r#"
pub fn calculate_total(items: &[Item]) -> f64 {
    let mut total = 0.0;
    for item in items {
        total += item.price;
    }
    total
}
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL after replacement
    let expected = r#"
pub fn calculate_total(items: &[Item]) -> f64 {
    items.iter().map(|item| item.price).sum()
}
"#;

    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "symbol_name": "calculate_total",
            "new_body": "    items.iter().map(|item| item.price).sum()"
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(
        result.structured_content.as_ref().and_then(|s| s.get("success")).and_then(|v| v.as_bool()).unwrap_or(false),
        "Expected success, got: {}",
        response
    );

    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual.trim(), expected.trim());

    Ok(())
}

#[tokio::test]
async fn test_replace_symbol_body_dry_run() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.js");

    let source = "function test() { return 1; }";
    fs::write(&file_path, source)?;

    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "symbol_name": "test",
            "new_body": "    return 2;"
        })
        .to_string(),
        dry_run: true,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    // Verify dry_run mode in structured_content
    assert!(result.structured_content.is_some(), "Should have structured content");
    let structured = result.structured_content.as_ref().unwrap();
    assert!(
        structured.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false),
        "Should indicate dry run mode"
    );

    // File should be unchanged
    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual, source);

    Ok(())
}

#[tokio::test]
async fn test_replace_symbol_body_missing_file() -> Result<()> {
    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": "/nonexistent/file.js",
            "symbol_name": "test",
            "new_body": "return 1;"
        })
        .to_string(),
        dry_run: true,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));

    Ok(())
}

#[tokio::test]
async fn test_replace_symbol_body_missing_symbol() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.js");

    fs::write(&file_path, "function foo() { return 1; }")?;

    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "symbol_name": "nonexistent",
            "new_body": "return 2;"
        })
        .to_string(),
        dry_run: true,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));

    Ok(())
}

#[tokio::test]
async fn test_replace_symbol_body_missing_parameters() -> Result<()> {
    let tool = SmartRefactorTool {
        operation: "replace_symbol_body".to_string(),
        params: serde_json::json!({
            "file": "test.js",
            // Missing symbol_name and new_body
        })
        .to_string(),
        dry_run: true,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await;

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Missing required parameter")
    );

    Ok(())
}
