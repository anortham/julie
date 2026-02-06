//! TDD tests for insert_relative_to_symbol operation
//!
//! These tests verify that code can be inserted before/after symbols
//! with correct indentation across multiple languages.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::tools::refactoring::SmartRefactorTool;

fn extract_text(result: &crate::mcp_compat::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| {
            serde_json::to_value(block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn test_insert_before_typescript_function() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.ts");

    // SOURCE
    let source = r#"
export function processUser(userId: number) {
    return api.getUser(userId);
}
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL after inserting comment before function
    let expected = r#"
// TODO: Add error handling for network failures
export function processUser(userId: number) {
    return api.getUser(userId);
}
"#;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "processUser",
            "position": "before",
            "content": "// TODO: Add error handling for network failures"
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(
        response.contains("applied") && response.contains("change"),
        "Expected applied confirmation, got: {}",
        response
    );

    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual.trim(), expected.trim());

    Ok(())
}

#[tokio::test]
async fn test_insert_after_python_function() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.py");

    // SOURCE
    let source = r#"
def calculate_sum(numbers):
    return sum(numbers)
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL after inserting new function after existing one
    let expected = r#"
def calculate_sum(numbers):
    return sum(numbers)

def calculate_average(numbers):
    return sum(numbers) / len(numbers) if numbers else 0
"#;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "calculate_sum",
            "position": "after",
            "content": r#"
def calculate_average(numbers):
    return sum(numbers) / len(numbers) if numbers else 0"#
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(
        response.contains("applied") && response.contains("change"),
        "Expected applied confirmation, got: {}",
        response
    );

    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual.trim(), expected.trim());

    Ok(())
}

#[tokio::test]
async fn test_insert_after_rust_struct() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.rs");

    // SOURCE
    let source = r#"
pub struct User {
    pub id: u64,
    pub name: String,
}
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL after inserting impl block
    let expected = r#"
pub struct User {
    pub id: u64,
    pub name: String,
}

impl User {
    pub fn new(id: u64, name: String) -> Self {
        User { id, name }
    }
}
"#;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "User",
            "position": "after",
            "content": r#"
impl User {
    pub fn new(id: u64, name: String) -> Self {
        User { id, name }
    }
}"#
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(
        response.contains("applied") && response.contains("change"),
        "Expected applied confirmation, got: {}",
        response
    );

    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual.trim(), expected.trim());

    Ok(())
}

#[tokio::test]
async fn test_insert_preserves_indentation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.ts");

    // SOURCE with indented class method
    let source = r#"
class Service {
    process() {
        return true;
    }
}
"#;
    fs::write(&file_path, source)?;

    // Expected CONTROL - comment should match method indentation
    let expected = r#"
class Service {
    // This method needs refactoring
    process() {
        return true;
    }
}
"#;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "process",
            "position": "before",
            "content": "// This method needs refactoring"
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(
        response.contains("applied") && response.contains("change"),
        "Expected applied confirmation, got: {}",
        response
    );

    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual.trim(), expected.trim());

    Ok(())
}

#[tokio::test]
async fn test_insert_dry_run() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.js");

    let source = "function test() {}";
    fs::write(&file_path, source)?;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "test",
            "position": "before",
            "content": "// comment"
        })
        .to_string(),
        dry_run: true,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    // Verify dry_run mode in text content
    let response = extract_text(&result);
    assert!(
        response.contains("dry run"),
        "Should indicate dry run mode, got: {}",
        response
    );

    // File should be unchanged
    let actual = fs::read_to_string(&file_path)?;
    assert_eq!(actual, source);

    Ok(())
}

#[tokio::test]
async fn test_insert_invalid_position() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.js");

    fs::write(&file_path, "function test() {}")?;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "test",
            "position": "invalid",  // Should be "before" or "after"
            "content": "// comment"
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
            .contains("must be 'before' or 'after'")
    );

    Ok(())
}

#[tokio::test]
async fn test_insert_missing_symbol() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.js");

    fs::write(&file_path, "function foo() {}")?;

    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": file_path.to_string_lossy(),
            "target_symbol": "nonexistent",
            "position": "before",
            "content": "// comment"
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
async fn test_insert_missing_parameters() -> Result<()> {
    let tool = SmartRefactorTool {
        operation: "insert_relative_to_symbol".to_string(),
        params: serde_json::json!({
            "file": "test.js",
            // Missing target_symbol and content
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
