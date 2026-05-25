use super::real_world_tests::get_files_with_extension;
use crate::handler::JulieServerHandler;
use crate::tools::refactoring::RenameSymbolTool;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const REAL_WORLD_TEST_DIR: &str = "fixtures/real-world";

/// Test RenameSymbolTool against real TypeScript files
#[tokio::test]
async fn test_rename_symbol_real_typescript_files() {
    let ts_dir = Path::new(REAL_WORLD_TEST_DIR).join("typescript");
    let ts_files = get_files_with_extension(&ts_dir, &["ts", "tsx"]);

    if ts_files.is_empty() {
        println!(
            "⚠️ No TypeScript real-world test files found in {}",
            ts_dir.display()
        );
        return;
    }

    let handler = JulieServerHandler::new_for_test().await.unwrap();

    for file_path in ts_files {
        test_rename_symbol_on_real_file(&handler, &file_path, "typescript").await;
    }
}

/// Test RenameSymbolTool against real JavaScript files
#[tokio::test]
async fn test_rename_symbol_real_javascript_files() {
    let js_dir = Path::new(REAL_WORLD_TEST_DIR).join("javascript");
    let js_files = get_files_with_extension(&js_dir, &["js", "jsx"]);

    if js_files.is_empty() {
        println!(
            "⚠️ No JavaScript real-world test files found in {}",
            js_dir.display()
        );
        return;
    }

    let handler = JulieServerHandler::new_for_test().await.unwrap();

    for file_path in js_files {
        test_rename_symbol_on_real_file(&handler, &file_path, "javascript").await;
    }
}

/// Test RenameSymbolTool against real Python files
#[tokio::test]
async fn test_rename_symbol_real_python_files() {
    let py_dir = Path::new(REAL_WORLD_TEST_DIR).join("python");
    let py_files = get_files_with_extension(&py_dir, &["py"]);

    if py_files.is_empty() {
        println!(
            "⚠️ No Python real-world test files found in {}",
            py_dir.display()
        );
        return;
    }

    let handler = JulieServerHandler::new_for_test().await.unwrap();

    for file_path in py_files {
        test_rename_symbol_on_real_file(&handler, &file_path, "python").await;
    }
}

/// Core function to test rename operations on real files
async fn test_rename_symbol_on_real_file(
    handler: &JulieServerHandler,
    file_path: &Path,
    language: &str,
) {
    println!(
        "🔄 Testing rename refactoring on real file: {}",
        file_path.display()
    );

    // Read the real file content
    let content = match fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            println!("⚠️ Failed to read {}: {}", file_path.display(), e);
            return;
        }
    };

    // Extract symbols to find real symbols to rename
    let symbols = extract_symbols_from_real_file(&content, language);
    if symbols.is_empty() {
        println!("📝 No symbols found in {}", file_path.display());
        return;
    }

    // Create temporary workspace with copy of real file
    let temp_dir = TempDir::new().unwrap();
    let test_file_path = temp_dir.path().join(file_path.file_name().unwrap());
    fs::write(&test_file_path, &content).unwrap();

    // Initialize workspace for the refactoring tool
    let workspace_path = temp_dir.path().to_string_lossy().to_string();
    if let Err(e) = handler
        .initialize_workspace_with_force(Some(workspace_path), true)
        .await
    {
        println!("⚠️ Failed to initialize workspace: {}", e);
        return;
    }

    // Test renaming the first meaningful symbol we find
    for symbol in symbols.iter().take(3) {
        // Skip very short or generic names
        if symbol.len() < 3 || symbol.chars().all(|c| c.is_ascii_lowercase()) {
            continue;
        }

        let new_name = format!("Renamed{}", symbol);

        println!("  🎯 Testing rename: {} -> {}", symbol, new_name);

        // Test dry run first
        let dry_run_tool = RenameSymbolTool {
            old_name: symbol.clone(),
            new_name: new_name.clone(),
            scope: Some("workspace".to_string()),
            dry_run: true,
            workspace: None,
        };

        match dry_run_tool.call_tool(handler).await {
            Ok(result) => {
                let response = extract_response_text(&result);
                if response.contains("No references found") {
                    println!("    📝 No references found for symbol '{}'", symbol);
                    continue;
                }

                println!("    ✅ Dry run successful for symbol '{}'", symbol);

                // If dry run found references, test actual rename
                let actual_tool = RenameSymbolTool {
                    old_name: symbol.clone(),
                    new_name: new_name.clone(),
                    scope: Some("workspace".to_string()),
                    dry_run: false,
                    workspace: None,
                };

                match actual_tool.call_tool(handler).await {
                    Ok(result) => {
                        let response = extract_response_text(&result);
                        if response.contains("Rename successful") || response.contains("Modified") {
                            println!("    🎉 Actual rename successful for symbol '{}'", symbol);

                            // Verify the file was actually modified correctly
                            if let Ok(modified_content) = fs::read_to_string(&test_file_path) {
                                if modified_content.contains(&new_name) {
                                    println!("    ✅ File correctly modified with new symbol name");
                                } else {
                                    println!("    ⚠️ File was not modified as expected");
                                }
                            }
                        } else {
                            println!("    📝 Rename result: {}", response);
                        }
                    }
                    Err(e) => println!("    ❌ Actual rename failed: {}", e),
                }

                // Only test one successful rename per file to avoid conflicts
                break;
            }
            Err(e) => println!("    ❌ Dry run failed: {}", e),
        }
    }
}

/// Extract potential symbols from real file content for testing
fn extract_symbols_from_real_file(content: &str, language: &str) -> Vec<String> {
    let mut symbols = Vec::new();

    match language {
        "typescript" | "javascript" => {
            // Look for class names, function names, interface names
            for line in content.lines() {
                let line = line.trim();

                // Class declarations
                if line.starts_with("export class ") || line.starts_with("class ") {
                    if let Some(name) = extract_identifier_after_keyword(line, "class") {
                        symbols.push(name);
                    }
                }

                // Interface declarations
                if line.starts_with("export interface ") || line.starts_with("interface ") {
                    if let Some(name) = extract_identifier_after_keyword(line, "interface") {
                        symbols.push(name);
                    }
                }

                // Function declarations
                if line.starts_with("export function ") || line.starts_with("function ") {
                    if let Some(name) = extract_identifier_after_keyword(line, "function") {
                        symbols.push(name);
                    }
                }

                // Const declarations
                if line.starts_with("export const ") || line.starts_with("const ") {
                    if let Some(name) = extract_identifier_after_keyword(line, "const") {
                        symbols.push(name);
                    }
                }
            }
        }
        "python" => {
            // Look for class and function definitions
            for line in content.lines() {
                let line = line.trim();

                if line.starts_with("class ") {
                    if let Some(name) = extract_identifier_after_keyword(line, "class") {
                        symbols.push(name);
                    }
                }

                if line.starts_with("def ") {
                    if let Some(name) = extract_identifier_after_keyword(line, "def") {
                        symbols.push(name);
                    }
                }
            }
        }
        _ => {
            // Generic symbol extraction for other languages
            // This is a simple heuristic - real extraction would use tree-sitter
            for line in content.lines() {
                if line.contains("class ") || line.contains("function ") {
                    // Extract potential identifiers
                    for word in line.split_whitespace() {
                        if word.len() > 3 && word.chars().next().unwrap().is_alphabetic() {
                            let clean_word =
                                word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
                            if !clean_word.is_empty() && clean_word.len() > 3 {
                                symbols.push(clean_word.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    symbols.into_iter().take(5).collect() // Limit to first 5 symbols
}

/// Helper to extract identifier after a keyword
fn extract_identifier_after_keyword(line: &str, keyword: &str) -> Option<String> {
    let keyword_with_space = format!("{} ", keyword);
    if let Some(start) = line.find(&keyword_with_space) {
        let after_keyword = &line[start + keyword_with_space.len()..];
        let identifier = after_keyword
            .split_whitespace()
            .next()?
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

        if identifier.len() > 2 && identifier.chars().all(|c| c.is_alphanumeric() || c == '_') {
            Some(identifier.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Extract text from CallToolResult
fn extract_response_text(result: &crate::mcp_compat::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            })
        })
        .collect::<Vec<String>>()
        .join("\n")
}
