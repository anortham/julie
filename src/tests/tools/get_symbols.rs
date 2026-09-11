//! Tests for GetSymbolsTool - verify path normalization and symbol retrieval

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::extractors::{Symbol, SymbolKind};
use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context;
use crate::tools::GetSymbolsTool;
use crate::tools::symbols::formatting::format_symbol_response;

#[tokio::test]
async fn test_get_symbols_with_relative_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("example.rs"),
        r#"
pub struct User {
    pub id: String,
    pub name: String,
}

pub fn get_user(id: &str) -> User {
    User {
        id: id.to_string(),
        name: "Test".to_string(),
    }
}

pub const MAX_USERS: usize = 100;
"#,
    )?;
    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 2,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result = tool.call_tool(&handler).await?;
    let text_content = call_tool_result_text(&result);

    assert!(
        !text_content.contains("No symbols found"),
        "Expected to find symbols but got: {}",
        text_content
    );
    assert!(
        text_content.contains("User"),
        "Should find User struct in symbols"
    );
    assert!(
        text_content.contains("get_user"),
        "Should find get_user function in symbols"
    );
    assert!(
        text_content.contains("MAX_USERS"),
        "Should find MAX_USERS constant in symbols"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_with_absolute_path() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    let test_file = src_dir.join("example.rs");
    fs::write(
        &test_file,
        r#"
pub fn process_data(input: &str) -> String {
    input.to_uppercase()
}
"#,
    )?;
    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: test_file.to_string_lossy().to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result = tool.call_tool(&handler).await?;
    let text_content = call_tool_result_text(&result);

    assert!(
        !text_content.contains("No symbols found"),
        "Should find symbols with absolute path"
    );
    assert!(
        text_content.contains("process_data"),
        "Should find process_data function"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_normalizes_various_path_formats() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("utils.rs"), "pub fn helper() -> i32 { 42 }")?;
    let handler = snapshot_context(&workspace_path)?;

    for path_variant in ["src/utils.rs", "./src/utils.rs", "src/../src/utils.rs"] {
        let tool = GetSymbolsTool {
            file_path: path_variant.to_string(),
            max_depth: 1,
            target: None,
            limit: None,
            offset: 0,
            mode: None,
            workspace: None,
        };
        let result = tool.call_tool(&handler).await?;
        let text_content = call_tool_result_text(&result);

        assert!(
            !text_content.contains("No symbols found"),
            "Path variant '{}' should work but got: {}",
            path_variant,
            text_content
        );
        assert!(
            text_content.contains("helper"),
            "Path variant '{}' should find helper function",
            path_variant
        );
    }
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_with_limit_parameter() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    let mut content = String::new();
    for i in 1..=20 {
        content.push_str(&format!("pub fn function_{}() -> i32 {{ {} }}\n\n", i, i));
    }
    fs::write(src_dir.join("many_symbols.rs"), content)?;
    let handler = snapshot_context(&workspace_path)?;

    let tool_no_limit = GetSymbolsTool {
        file_path: "src/many_symbols.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result_no_limit = tool_no_limit.call_tool(&handler).await?;
    let text_no_limit = call_tool_result_text(&result_no_limit);

    assert!(
        text_no_limit.contains("20 symbols"),
        "Should list all 20 symbols in the header, got: {}",
        text_no_limit
    );
    let symbol_lines = text_no_limit
        .lines()
        .filter(|line| line.contains("function"))
        .count();
    assert_eq!(symbol_lines, 20, "Should list all 20 function symbols");

    let tool_with_limit = GetSymbolsTool {
        file_path: "src/many_symbols.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: Some(5),
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result_with_limit = tool_with_limit.call_tool(&handler).await?;
    let text_with_limit = call_tool_result_text(&result_with_limit);

    let limited_symbol_lines = text_with_limit
        .lines()
        .filter(|line| line.contains("function"))
        .count();
    assert_eq!(limited_symbol_lines, 5, "Should return exactly 5 symbols");
    assert!(
        !text_with_limit.contains("function_6"),
        "function_6 should not appear with limit=5"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_file_not_found_error() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("exists.rs"), "pub fn test() -> i32 { 42 }")?;
    fs::write(src_dir.join("empty.rs"), "")?;
    let handler = snapshot_context(&workspace_path)?;

    let tool_not_found = GetSymbolsTool {
        file_path: "src/does_not_exist.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result_not_found = tool_not_found.call_tool(&handler).await;

    assert!(result_not_found.is_err(), "missing file should fail");
    let err_text = result_not_found
        .err()
        .expect("error should exist")
        .to_string();
    assert!(
        err_text.contains("File not found"),
        "Should say 'File not found' for non-existent files, got: {}",
        err_text
    );
    assert!(
        err_text.contains("❌"),
        "Should include error emoji for visibility"
    );

    let tool_exists = GetSymbolsTool {
        file_path: "src/exists.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result_exists = tool_exists.call_tool(&handler).await?;
    let text_exists = call_tool_result_text(&result_exists);

    assert!(
        !text_exists.contains("File not found"),
        "Existing file should not trigger 'File not found'"
    );
    assert!(
        !text_exists.contains("No symbols found"),
        "Should find symbols in existing file"
    );
    assert!(
        text_exists.contains("test"),
        "Should find test function in symbols"
    );

    let tool_empty = GetSymbolsTool {
        file_path: "src/empty.rs".to_string(),
        max_depth: 1,
        target: None,
        limit: None,
        offset: 0,
        mode: None,
        workspace: None,
    };
    let result_empty = tool_empty.call_tool(&handler).await?;
    let text_empty = call_tool_result_text(&result_empty);

    assert!(
        !text_empty.contains("File not found"),
        "Empty file exists, should not say 'File not found'"
    );
    assert!(
        text_empty.contains("No symbols found"),
        "Empty file should say 'No symbols found', got: {}",
        text_empty
    );
    Ok(())
}

#[tokio::test]
async fn test_get_symbols_minimal_mode_code_format() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(
        src_dir.join("example.rs"),
        r#"pub struct User {
    pub id: String,
    pub name: String,
}

pub fn get_user(id: &str) -> User {
    User {
        id: id.to_string(),
        name: "Test".to_string(),
    }
}
"#,
    )?;
    let handler = snapshot_context(&workspace_path)?;

    let tool = GetSymbolsTool {
        file_path: "src/example.rs".to_string(),
        max_depth: 2,
        target: None,
        limit: None,
        offset: 0,
        mode: Some("minimal".to_string()),
        workspace: None,
    };
    let result = tool.call_tool(&handler).await?;
    let text_content = call_tool_result_text(&result);

    assert!(
        text_content.contains("pub struct User"),
        "Should contain the actual struct definition, got: {}",
        text_content
    );
    assert!(
        text_content.contains("pub fn get_user"),
        "Should contain the actual function definition, got: {}",
        text_content
    );
    assert!(
        !text_content.contains("\"id\":"),
        "Should not contain JSON field markers, got: {}",
        text_content
    );
    assert!(
        !text_content.contains("\"file_path\":"),
        "Should not contain JSON file_path field, got: {}",
        text_content
    );
    assert!(
        !text_content.contains("total_symbols"),
        "Should not contain metadata wrapper fields, got: {}",
        text_content
    );
    assert!(
        !text_content.contains("returned_symbols"),
        "Should not contain metadata wrapper fields, got: {}",
        text_content
    );
    assert!(
        text_content.lines().count() < 50,
        "Raw code output should be concise, got {} lines",
        text_content.lines().count()
    );
    Ok(())
}

#[test]
fn test_get_symbols_default_mode_is_structure() {
    let json = r#"{"file_path": "src/foo.rs"}"#;
    let tool: GetSymbolsTool = serde_json::from_str(json).expect("should deserialize");
    assert_eq!(
        tool.mode.as_deref(),
        Some("structure"),
        "default mode must be 'structure', got: {:?}",
        tool.mode
    );
}

#[test]
fn test_lean_format_skips_redundant_kind_prefix() {
    let struct_sym = Symbol {
        extracted: julie_extractors::Symbol {
            id: "s1".to_string(),
            name: "Foo".to_string(),
            kind: SymbolKind::Struct,
            language: "rust".to_string(),
            file_path: "src/foo.rs".to_string(),
            start_line: 10,
            start_column: 0,
            end_line: 25,
            end_column: 0,
            start_byte: 0,
            end_byte: 200,
            signature: Some("pub struct Foo".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
    };
    let fn_sym = Symbol {
        extracted: julie_extractors::Symbol {
            id: "s2".to_string(),
            name: "process".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "src/foo.rs".to_string(),
            start_line: 17,
            start_column: 0,
            end_line: 24,
            end_column: 0,
            start_byte: 300,
            end_byte: 500,
            signature: Some("pub fn process(&self)".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
    };

    let result = format_symbol_response("src/foo.rs", vec![struct_sym, fn_sym], None, None)
        .expect("format_symbol_response should not fail");
    let text = call_tool_result_text(&result);

    assert!(
        !text.contains("struct pub struct"),
        "output must not contain 'struct pub struct', got:\n{}",
        text
    );
    assert!(
        !text.contains("function pub fn"),
        "output must not contain 'function pub fn', got:\n{}",
        text
    );
    assert!(
        text.contains("pub struct Foo"),
        "output must contain 'pub struct Foo', got:\n{}",
        text
    );
    assert!(
        text.contains("pub fn process(&self)"),
        "output must contain 'pub fn process(&self)', got:\n{}",
        text
    );
}
