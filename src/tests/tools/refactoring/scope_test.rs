//! Test scope-awareness in find_any_symbol

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
async fn test_extract_outer_function_with_nested_same_name() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("scope.ts");
    let target_file = temp_dir.path().join("extracted.ts");

    // File with SHADOWED function names
    let source = r#"
export function processData(data: string): string {
    return data.trim();
}

export function main() {
    // Nested function with SAME NAME
    function processData(input: number): number {
        return input * 2;
    }

    return processData(42);
}
"#;
    fs::write(&source_file, source)?;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "processData",  // Which one will it find?
            "update_imports": false
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let text = extract_text(&result);
    println!("Result: {}", text);

    // Read what actually got extracted
    let target_content = fs::read_to_string(&target_file)?;
    println!("Extracted content:\n{}", target_content);

    // CRITICAL: Did it extract the TOP-LEVEL function or the NESTED one?
    // Expected: Should extract the top-level export function processData(data: string)
    // Bug scenario: Might extract the nested function processData(input: number)

    assert!(
        target_content.contains("data: string"),
        "Should extract the TOP-LEVEL function (data: string), not the nested one"
    );
    assert!(
        !target_content.contains("input: number"),
        "Should NOT extract the nested function (input: number)"
    );

    Ok(())
}

#[tokio::test]
async fn test_extract_specifies_ambiguous_symbol() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let source_file = temp_dir.path().join("ambiguous.rs");
    let target_file = temp_dir.path().join("extracted.rs");

    // Rust file with TWO structs named Config in different modules
    let source = r#"
pub struct Config {
    pub host: String,
}

mod nested {
    pub struct Config {
        pub port: u16,
    }
}
"#;
    fs::write(&source_file, source)?;

    let tool = SmartRefactorTool {
        operation: "extract_symbol_to_file".to_string(),
        params: serde_json::json!({
            "source_file": source_file.to_string_lossy(),
            "target_file": target_file.to_string_lossy(),
            "symbol_name": "Config",  // Ambiguous!
            "update_imports": false
        })
        .to_string(),
        dry_run: false,
    };

    let handler = JulieServerHandler::new().await?;
    let result = tool.call_tool(&handler).await?;

    let text = extract_text(&result);
    println!("Result: {}", text);

    // Read what got extracted
    let target_content = fs::read_to_string(&target_file)?;
    println!("Extracted content:\n{}", target_content);

    // CRITICAL: Which Config did it extract?
    // Expected behavior: Extract the FIRST one found (top-level)
    // The question is: Is this predictable and documented?

    assert!(
        target_content.contains("host: String"),
        "Should extract the top-level Config (host: String)"
    );

    Ok(())
}
