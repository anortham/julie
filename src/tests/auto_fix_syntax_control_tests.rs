//! SOURCE/CONTROL tests for AutoFixSyntax operation
//!
//! This module implements the professional SOURCE/CONTROL testing pattern for AutoFixSyntax.
//! SOURCE files contain broken code (never edited), CONTROL files show expected fixes.
//! Every fix is verified against control files using exact comparison.

use crate::tools::refactoring::SmartRefactorTool;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// AutoFixSyntax test case structure for SOURCE/CONTROL verification
#[derive(Debug)]
struct AutoFixTestCase {
    name: &'static str,
    source_file: &'static str,
    control_file: &'static str,
    description: &'static str,
}

/// All AutoFixSyntax test cases
const AUTO_FIX_TEST_CASES: &[AutoFixTestCase] = &[
    AutoFixTestCase {
        name: "multi_property_object_missing_brace",
        source_file: "broken/multi_property_object.js",
        control_file: "auto-fix/multi_property_object.js",
        description:
            "Multi-property object with missing closing brace - should fix after last property",
    },
    AutoFixTestCase {
        name: "multi_element_array_missing_bracket",
        source_file: "broken/multi_element_array.js",
        control_file: "auto-fix/multi_element_array.js",
        description:
            "Multi-element array with missing closing bracket - should fix after last element",
    },
    AutoFixTestCase {
        name: "nested_structures_missing_braces",
        source_file: "broken/nested_structures.js",
        control_file: "auto-fix/nested_structures.js",
        description: "Nested objects with multiple missing closing braces at different levels",
    },
    AutoFixTestCase {
        name: "rust_struct_missing_brace",
        source_file: "broken/rust_struct.rs",
        control_file: "auto-fix/rust_struct.rs",
        description: "Rust struct and function with missing closing braces",
    },
    AutoFixTestCase {
        name: "typescript_function_missing_paren",
        source_file: "broken/missing_paren.ts",
        control_file: "auto-fix/missing_paren.ts",
        description: "TypeScript function with missing closing parenthesis in parameter list",
    },
    AutoFixTestCase {
        name: "python_unclosed_string",
        source_file: "broken/unclosed_string.py",
        control_file: "auto-fix/unclosed_string.py",
        description: "Python function with unclosed string literal",
    },
];

/// Test helper to set up temp directories and files
fn setup_auto_fix_test_environment() -> Result<PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("julie_auto_fix_tests_{}", timestamp));

    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

/// Copy source file to test location (SOURCE files are never edited)
fn setup_auto_fix_test_file(
    source_file: &str,
    test_case_name: &str,
    temp_dir: &Path,
) -> Result<PathBuf> {
    let source_path = Path::new("tests/editing/sources").join(source_file);

    // Extract extension from source file
    let extension = Path::new(source_file)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("txt");

    let test_file_path = temp_dir.join(format!("{}_test.{}", test_case_name, extension));

    let source_content = fs::read_to_string(&source_path)
        .map_err(|e| anyhow::anyhow!("Failed to read source file {:?}: {}", source_path, e))?;

    fs::write(&test_file_path, source_content)?;
    Ok(test_file_path)
}

/// Load control file content
fn load_control_content(control_file: &str) -> Result<String> {
    let control_path = Path::new("tests/editing/controls").join(control_file);
    fs::read_to_string(&control_path)
        .map_err(|e| anyhow::anyhow!("Failed to read control file {:?}: {}", control_path, e))
}

/// Run a single SOURCE/CONTROL test case
async fn run_auto_fix_test_case(test_case: &AutoFixTestCase) -> Result<()> {
    // Setup test environment
    let temp_dir = setup_auto_fix_test_environment()?;
    let test_file = setup_auto_fix_test_file(test_case.source_file, test_case.name, &temp_dir)?;

    // Apply auto-fix
    let tool = SmartRefactorTool {
        operation: "auto_fix_syntax".to_string(),
        params: format!(r#"{{"file_path": "{}"}}"#, test_file.display()),
        dry_run: false,
    };

    let handler = crate::handler::JulieServerHandler::new().await?;
    tool.call_tool(&handler).await?;

    // Load fixed content and control content
    let fixed_content = fs::read_to_string(&test_file)?;
    let control_content = load_control_content(test_case.control_file)?;

    // Cleanup temp directory
    fs::remove_dir_all(&temp_dir)?;

    // Compare fixed content to control
    assert_eq!(
        fixed_content.trim(),
        control_content.trim(),
        "\n❌ MISMATCH for test case: {}\n\nExpected (CONTROL):\n{}\n\nActual (FIXED):\n{}\n",
        test_case.name,
        control_content.trim(),
        fixed_content.trim()
    );

    Ok(())
}

// Individual test functions for each case
#[tokio::test]
async fn test_multi_property_object_missing_brace() -> Result<()> {
    run_auto_fix_test_case(&AUTO_FIX_TEST_CASES[0]).await
}

#[tokio::test]
async fn test_multi_element_array_missing_bracket() -> Result<()> {
    run_auto_fix_test_case(&AUTO_FIX_TEST_CASES[1]).await
}

#[tokio::test]
async fn test_nested_structures_missing_braces() -> Result<()> {
    run_auto_fix_test_case(&AUTO_FIX_TEST_CASES[2]).await
}

#[tokio::test]
async fn test_rust_struct_missing_brace() -> Result<()> {
    run_auto_fix_test_case(&AUTO_FIX_TEST_CASES[3]).await
}

#[tokio::test]
async fn test_typescript_function_missing_paren() -> Result<()> {
    run_auto_fix_test_case(&AUTO_FIX_TEST_CASES[4]).await
}

#[tokio::test]
async fn test_python_unclosed_string() -> Result<()> {
    run_auto_fix_test_case(&AUTO_FIX_TEST_CASES[5]).await
}
