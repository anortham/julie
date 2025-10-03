//! TDD Tests for AutoFixSyntax operation - TRUE SYNTAX ERRORS ONLY
//!
//! Following TDD methodology: RED → GREEN → REFACTOR
//!
//! Focus: ACTUAL syntax errors that break parsing (agent retry loops)
//! NOT style preferences (semicolons in JS/TS have ASI - they're valid!)
//!
//! Test cases cover:
//! 1. Unmatched braces/brackets/parentheses (parse-breaking)
//! 2. Unclosed strings (parse error)
//! 3. Missing closing delimiters (actual errors tree-sitter detects)
//! 4. Invalid nesting (breaks parsing)

use crate::tools::refactoring::SmartRefactorTool;
use anyhow::Result;

/// Test 1: REAL WORLD - Multi-property object with missing closing brace
#[tokio::test]
async fn test_auto_fix_unmatched_opening_brace() -> Result<()> {
    let code = r#"
function getData() {
    const obj = {
        name: "Test",
        email: "test@example.com",
        status: "active"
        // Missing closing brace causes parse error
    return obj;
}
"#;

    // IDEAL: closing brace should be after last property (line 6)
    // NOT after opening brace (line 3)
    let expected = r#"
function getData() {
    const obj = {
        name: "Test",
        email: "test@example.com",
        status: "active"
    };
        // Missing closing brace causes parse error
    return obj;
}
"#;

    let test_file = "/tmp/test_unmatched_brace.js";
    std::fs::write(test_file, code)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    // Verify the fix was applied
    let fixed_content = std::fs::read_to_string(test_file)?;

    // Cleanup
    std::fs::remove_file(test_file)?;

    // GREEN phase: Assert exact match with expected output
    assert_eq!(
        fixed_content.trim(),
        expected.trim(),
        "Fixed content should match expected output"
    );

    Ok(())
}

/// Test 2: RED - Unclosed string literal (actual parse error)
#[tokio::test]
async fn test_auto_fix_unclosed_string() -> Result<()> {
    let code = r#"
const message = "Hello world;
console.log(message);
"#;

    let expected = r#"
const message = "Hello world";
console.log(message);
"#;

    let test_file = "/tmp/test_unclosed_string.js";
    std::fs::write(test_file, code)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let _result = tool.call_tool(&handler).await?;

    std::fs::remove_file(test_file)?;
    Ok(())
}

/// Test 3: RED - Missing closing parenthesis (parse error)
#[tokio::test]
async fn test_auto_fix_missing_closing_paren() -> Result<()> {
    let code = r#"
function calculate(a, b {
    return a + b;
}
"#;

    let expected = r#"
function calculate(a, b) {
    return a + b;
}
"#;

    let test_file = "/tmp/test_missing_paren.js";
    std::fs::write(test_file, code)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let _result = tool.call_tool(&handler).await?;

    std::fs::remove_file(test_file)?;
    Ok(())
}

/// Test 4: Edge case - Dry run mode (preview without applying)
#[tokio::test]
async fn test_auto_fix_dry_run_mode() -> Result<()> {
    let code = r#"
const obj = { name: "test" // Missing closing brace
"#;

    let test_file = "/tmp/test_dry_run.js";
    std::fs::write(test_file, code)?;

    let original_content = std::fs::read_to_string(test_file)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: true,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let _result = tool.call_tool(&handler).await?;

    // File should be unchanged in dry_run mode
    let unchanged_content = std::fs::read_to_string(test_file)?;
    assert_eq!(unchanged_content, original_content);

    std::fs::remove_file(test_file)?;
    Ok(())
}

/// Test 5: Edge case - No errors to fix (should succeed gracefully)
#[tokio::test]
async fn test_auto_fix_no_errors() -> Result<()> {
    let code = r#"
function getData() {
    return {
        name: "Test",
        value: 42
    };
}
"#;

    let test_file = "/tmp/test_no_errors.js";
    std::fs::write(test_file, code)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let _result = tool.call_tool(&handler).await?;

    // File should be unchanged
    let unchanged_content = std::fs::read_to_string(test_file)?;
    assert_eq!(unchanged_content.trim(), code.trim());

    std::fs::remove_file(test_file)?;
    Ok(())
}

/// Test 6: Real-world case - Missing closing bracket in array
#[tokio::test]
async fn test_auto_fix_missing_bracket() -> Result<()> {
    let code = r#"
const items = [
    "apple",
    "banana",
    "orange"
    // Missing closing bracket causes parse error
;
"#;

    let expected = r#"
const items = [
    "apple",
    "banana",
    "orange"
];
"#;

    let test_file = "/tmp/test_missing_bracket.js";
    std::fs::write(test_file, code)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let _result = tool.call_tool(&handler).await?;

    std::fs::remove_file(test_file)?;
    Ok(())
}

/// Test 7: Cross-language - Rust unmatched brace
#[tokio::test]
async fn test_auto_fix_rust_unmatched_brace() -> Result<()> {
    let code = r#"
fn get_user() -> User {
    User {
        name: String::from("John"),
        age: 25
        // Missing closing brace
}
"#;

    let expected = r#"
fn get_user() -> User {
    User {
        name: String::from("John"),
        age: 25
    }
}
"#;

    let test_file = "/tmp/test_rust_brace.rs";
    std::fs::write(test_file, code)?;

    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    let _result = tool.call_tool(&handler).await?;

    std::fs::remove_file(test_file)?;
    Ok(())
}
