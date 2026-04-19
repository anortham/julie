use anyhow::Result;
use std::fs;
use tempfile::TempDir;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::workspace::ManageWorkspaceTool;

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|c| c.as_text())
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

async fn setup_workspace(files: &[(&str, &str)]) -> Result<(TempDir, JulieServerHandler)> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    for (relative_path, content) in files {
        let absolute_path = workspace_path.join(relative_path);
        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&absolute_path, content)?;
    }

    let handler = JulieServerHandler::new(workspace_path.clone()).await?;
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        workspace_id: None,
        path: Some(workspace_path.to_string_lossy().to_string()),
        name: None,
        force: Some(false),
        detailed: None,
    };
    index_tool.call_tool(&handler).await?;

    Ok((temp_dir, handler))
}

// ── Python ───────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_python_replace_body_indented_suite() -> Result<()> {
    let source = "def greet(name: str) -> str:\n    return f\"Hello, {name}\"\n";
    let (temp_dir, handler) = setup_workspace(&[("greet.py", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "    return \"Fixed\"\n".to_string(),
        file_path: Some("greet.py".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Python replace_body should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("greet.py"))?;
    assert!(
        on_disk.contains("Fixed"),
        "Python body should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("def greet"),
        "Python signature should be preserved, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_python_replace_signature() -> Result<()> {
    let source = "def greet(name: str) -> str:\n    return f\"Hello, {name}\"\n";
    let (temp_dir, handler) = setup_workspace(&[("greet.py", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "def greet(name: str, greeting: str = \"Hello\") -> str:".to_string(),
        file_path: Some("greet.py".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Python replace_signature should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("greet.py"))?;
    assert!(
        on_disk.contains("greeting"),
        "Python signature should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("return"),
        "Python body should be preserved, got: {on_disk}"
    );

    Ok(())
}

// ── Java ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_java_replace_body_brace_delimited() -> Result<()> {
    let source = "public class Greeter {\n    public String greet(String name) {\n        return \"Hello, \" + name;\n    }\n}\n";
    let (temp_dir, handler) = setup_workspace(&[("Greeter.java", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n        return \"Fixed\";\n    }".to_string(),
        file_path: Some("Greeter.java".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Java replace_body should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("Greeter.java"))?;
    assert!(
        on_disk.contains("Fixed"),
        "Java method body should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("greet"),
        "Java method name should be preserved, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_java_replace_body_dry_run_shows_braces_in_old_content() -> Result<()> {
    // Verifies Task 3 integration: dry-run preview shows brace-inclusive old content.
    let source = "public class Greeter {\n    public String greet(String name) {\n        return \"Hello, \" + name;\n    }\n}\n";
    let (_temp_dir, handler) = setup_workspace(&[("Greeter.java", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n        return \"Fixed\";\n    }".to_string(),
        file_path: Some("Greeter.java".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("--- Old content ---"),
        "Java dry-run should show old content section, got: {text}"
    );
    assert!(
        text.contains('{') && text.contains('}'),
        "Java dry-run old content should include enclosing braces, got: {text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_java_interface_method_replace_signature_explicit_error() -> Result<()> {
    // Java interface method declarations have no body. replace_signature must error.
    let source = "public interface Greetable {\n    String greet(String name);\n}\n";
    let (temp_dir, handler) = setup_workspace(&[("Greetable.java", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "String greet(String name, String greeting)".to_string(),
        file_path: Some("Greetable.java".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("Error:"),
        "Java interface method replace_signature should return an error, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("Greetable.java"))?;
    assert_eq!(
        on_disk, source,
        "Java interface file must be unchanged after error"
    );

    Ok(())
}

// ── Ruby ─────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_ruby_replace_body_def_end() -> Result<()> {
    let source = "def greet(name)\n  \"Hello, #{name}\"\nend\n";
    let (temp_dir, handler) = setup_workspace(&[("greet.rb", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "  \"Fixed\"\n".to_string(),
        file_path: Some("greet.rb".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Ruby replace_body should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("greet.rb"))?;
    assert!(
        on_disk.contains("Fixed"),
        "Ruby method body should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("def greet"),
        "Ruby method signature should be preserved, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ruby_replace_signature() -> Result<()> {
    let source = "def greet(name)\n  \"Hello, #{name}\"\nend\n";
    let (temp_dir, handler) = setup_workspace(&[("greet.rb", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "def greet(name, greeting = \"Hello\")".to_string(),
        file_path: Some("greet.rb".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Ruby replace_signature should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("greet.rb"))?;
    assert!(
        on_disk.contains("greeting"),
        "Ruby signature should be updated, got: {on_disk}"
    );

    Ok(())
}

// ── Go ───────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_go_replace_signature() -> Result<()> {
    let source =
        "package main\n\nfunc greet(name string) string {\n\treturn \"Hello, \" + name\n}\n";
    let (temp_dir, handler) = setup_workspace(&[("greet.go", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "func greet(name string, greeting string) string".to_string(),
        file_path: Some("greet.go".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Go replace_signature should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("greet.go"))?;
    assert!(
        on_disk.contains("greeting"),
        "Go signature should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("return"),
        "Go body should be preserved, got: {on_disk}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_go_replace_body() -> Result<()> {
    let source =
        "package main\n\nfunc greet(name string) string {\n\treturn \"Hello, \" + name\n}\n";
    let (temp_dir, handler) = setup_workspace(&[("greet.go", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{\n\treturn \"Fixed\"\n}".to_string(),
        file_path: Some("greet.go".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        !text.contains("Error:"),
        "Go replace_body should succeed, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("greet.go"))?;
    assert!(
        on_disk.contains("Fixed"),
        "Go body should be updated, got: {on_disk}"
    );
    assert!(
        on_disk.contains("func greet"),
        "Go signature should be preserved, got: {on_disk}"
    );

    Ok(())
}

// ── Rust trait (regression from the dogfood incident) ────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_rust_trait_method_replace_signature_explicit_error_and_file_unchanged() -> Result<()>
{
    // This is the primary regression test for the silent-clobber bug.
    // Before the fix, replace_signature on a trait method would silently
    // overwrite the entire symbol span. After the fix it must error.
    let source = "pub trait Greetable {\n    fn greet(&self) -> String;\n}\n";
    let (temp_dir, handler) = setup_workspace(&[("src/greet.rs", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_signature".to_string(),
        content: "fn greet(&self, name: &str) -> String".to_string(),
        file_path: Some("src/greet.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("replace_signature is not supported"),
        "Must return explicit error for Rust trait method with no body, got: {text}"
    );
    assert!(
        text.contains("greet"),
        "Error should name the offending symbol, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("src/greet.rs"))?;
    assert_eq!(
        on_disk, source,
        "File must be byte-for-byte identical after failed replace_signature"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_rust_trait_method_replace_body_error_with_field_names() -> Result<()> {
    let source = "pub trait Greetable {\n    fn greet(&self) -> String;\n}\n";
    let (temp_dir, handler) = setup_workspace(&[("src/greet.rs", source)]).await?;

    let tool = crate::tools::editing::rewrite_symbol::RewriteSymbolTool {
        symbol: "greet".to_string(),
        operation: "replace_body".to_string(),
        content: "{ String::from(\"hello\") }".to_string(),
        file_path: Some("src/greet.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: false,
    };

    let result = tool.call_tool(&handler).await?;
    let text = extract_text(&result);
    assert!(
        text.contains("node has fields:"),
        "Error should list actual node field names, got: {text}"
    );
    assert!(
        text.contains("no 'body' field"),
        "Error should identify the missing body field, got: {text}"
    );

    let on_disk = fs::read_to_string(temp_dir.path().join("src/greet.rs"))?;
    assert_eq!(
        on_disk, source,
        "File must be unchanged after failed replace_body"
    );

    Ok(())
}
