//! TDD tests for extract_symbol_to_file operation
//!
//! These tests verify that symbols can be extracted from one file to another
//! with proper import/use statements added.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::StructuredContentExt;
use crate::tools::refactoring::SmartRefactorTool;

fn extract_text(result: &crate::mcp_compat::CallToolResult) -> String {
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
    if let Some(structured) = result.structured_content() {
        return serde_json::to_string_pretty(&structured).unwrap_or_default();
    }

    String::new()
}

#[tokio::test]
async fn test_extract_typescript_function_to_new_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("large.ts");
    let target_file = temp_dir.path().join("utils.ts");

    // SOURCE: Large file with function we want to extract
    let source = r#"
export function processData(data: string): string {
    return data.trim().toLowerCase();
}

export function getUserData(userId: number): Promise<User> {
    // This function should be extracted
    const url = `https://api.example.com/users/${userId}`;
    return fetch(url).then(res => res.json());
}

export function saveData(data: any): void {
    localStorage.setItem('data', JSON.stringify(data));
}
"#;
    fs::write(&source_file, source)?;

    // Expected CONTROL for source file after extraction (getUserData removed, import added)
    let expected_source = r#"
import { getUserData } from './utils';

export function processData(data: string): string {
    return data.trim().toLowerCase();
}

export function saveData(data: any): void {
    localStorage.setItem('data', JSON.stringify(data));
}
"#;

    // Expected CONTROL for target file (getUserData extracted)
    let expected_target = r#"
export function getUserData(userId: number): Promise<User> {
    // This function should be extracted
    const url = `https://api.example.com/users/${userId}`;
    return fetch(url).then(res => res.json());
}
"#;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "getUserData",
            "update_imports": true
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);

    // This would FAIL if operation returned "coming soon"
    assert!(
        response.contains("Successfully extracted") || response.contains("success"),
        "Expected success message, got: {}",
        response
    );

    // Verify source file was modified correctly
    let actual_source = fs::read_to_string(&source_file)?;
    assert_eq!(
        actual_source.trim(),
        expected_source.trim(),
        "Source file not modified correctly"
    );

    // Verify target file was created with extracted symbol
    let actual_target = fs::read_to_string(&target_file)?;
    assert_eq!(
        actual_target.trim(),
        expected_target.trim(),
        "Target file not created correctly"
    );

    Ok(())
}

#[tokio::test]
async fn test_extract_python_class_to_new_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("models.py");
    let target_file = temp_dir.path().join("user_model.py");

    // SOURCE
    let source = r#"
class BaseModel:
    def save(self):
        pass

class UserModel(BaseModel):
    def __init__(self, name, email):
        self.name = name
        self.email = email

    def get_display_name(self):
        return f"{self.name} <{self.email}>"

class ProductModel(BaseModel):
    def __init__(self, title, price):
        self.title = title
        self.price = price
"#;
    fs::write(&source_file, source)?;

    // Expected source after extraction
    let expected_source = r#"
from user_model import UserModel

class BaseModel:
    def save(self):
        pass

class ProductModel(BaseModel):
    def __init__(self, title, price):
        self.title = title
        self.price = price
"#;

    // Expected target file
    let expected_target = r#"
class UserModel(BaseModel):
    def __init__(self, name, email):
        self.name = name
        self.email = email

    def get_display_name(self):
        return f"{self.name} <{self.email}>"
"#;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "UserModel",
            "update_imports": true
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(response.contains("Successfully extracted") || response.contains("success"));

    let actual_source = fs::read_to_string(&source_file)?;
    assert_eq!(actual_source.trim(), expected_source.trim());

    let actual_target = fs::read_to_string(&target_file)?;
    assert_eq!(actual_target.trim(), expected_target.trim());

    Ok(())
}

#[tokio::test]
async fn test_extract_rust_function_to_new_file() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("lib.rs");
    let target_file = temp_dir.path().join("helpers.rs");

    // SOURCE
    let source = r#"
pub fn main_logic() -> Result<()> {
    let result = calculate_sum(&[1, 2, 3]);
    println!("{}", result);
    Ok(())
}

pub fn calculate_sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}
"#;
    fs::write(&source_file, source)?;

    // Expected source after extraction
    let expected_source = r#"
use crate::helpers::calculate_sum;

pub fn main_logic() -> Result<()> {
    let result = calculate_sum(&[1, 2, 3]);
    println!("{}", result);
    Ok(())
}
"#;

    // Expected target file
    let expected_target = r#"
pub fn calculate_sum(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}
"#;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "calculate_sum",
            "update_imports": true
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(response.contains("Successfully extracted") || response.contains("success"));

    let actual_source = fs::read_to_string(&source_file)?;
    assert_eq!(actual_source.trim(), expected_source.trim());

    let actual_target = fs::read_to_string(&target_file)?;
    assert_eq!(actual_target.trim(), expected_target.trim());

    Ok(())
}

#[tokio::test]
async fn test_extract_to_existing_file_appends() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("main.ts");
    let target_file = temp_dir.path().join("utils.ts");

    // SOURCE
    let source = r#"
export function processData(data: string): string {
    return data.trim();
}

export function validateData(data: string): boolean {
    return data.length > 0;
}
"#;
    fs::write(&source_file, source)?;

    // Target file already exists
    let existing_target = r#"
export function helperFunction(): void {
    console.log('existing');
}
"#;
    fs::write(&target_file, existing_target)?;

    // Expected target after extraction (appended, not replaced)
    let expected_target = r#"
export function helperFunction(): void {
    console.log('existing');
}

export function validateData(data: string): boolean {
    return data.length > 0;
}
"#;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "validateData",
            "update_imports": false  // Don't update imports in this test
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let response = extract_text(&result);
    assert!(response.contains("Successfully extracted") || response.contains("success"));

    let actual_target = fs::read_to_string(&target_file)?;
    assert_eq!(actual_target.trim(), expected_target.trim());

    Ok(())
}

#[tokio::test]
async fn test_extract_symbol_to_file_dry_run() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("test.js");
    let target_file = temp_dir.path().join("extracted.js");

    let source = "function test() { return 1; }";
    fs::write(&source_file, source)?;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "test",
            "update_imports": false
        })
        .to_string(),
        dry_run: true,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    // Verify dry_run mode in structured_content
    assert!(result.structured_content().is_some(), "Should have structured content");
    let structured = result.structured_content().unwrap();
    assert!(
        structured.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false),
        "Should indicate dry run mode"
    );

    // Source file should be unchanged
    let actual = fs::read_to_string(&source_file)?;
    assert_eq!(actual, source);

    // Target file should not be created
    assert!(!target_file.exists());

    Ok(())
}

#[tokio::test]
async fn test_extract_missing_symbol() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("test.js");
    let target_file = temp_dir.path().join("extracted.js");

    fs::write(&source_file, "function foo() {}")?;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "nonexistent",
            "update_imports": false
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
async fn test_extract_missing_parameters() -> Result<()> {
    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": "test.js",
            // Missing target_file and symbol_name
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
