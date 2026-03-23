// Tests extracted from src/extractors/base.rs
// These were previously inline tests that have been moved to follow project standards

use crate::base::*;

#[test]
fn test_context_extraction_edge_cases() {
    // Test case 1: Symbol at the beginning of file (not enough lines before)
    let content = "line 1\nline 2\nfunction test() {\nreturn 42;\n}\nline 6\nline 7\nline 8";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let context = extractor.extract_code_context(2, 4); // function on line 3-5 (0-indexed: 2-4)
    assert!(context.is_some());
    let context_str = context.unwrap();

    // Should show lines 1-7 (with function highlighted on 3-5)
    assert!(context_str.contains("    1: line 1"));
    assert!(context_str.contains("    2: line 2"));
    assert!(context_str.contains("  ➤   3: function test() {"));
    assert!(context_str.contains("  ➤   4: return 42;"));
    assert!(context_str.contains("  ➤   5: }"));
    assert!(context_str.contains("    6: line 6"));

    // Test case 2: Symbol at the end of file (not enough lines after)
    let content = "line 1\nline 2\nline 3\nfunction test() {\nreturn 42;\n}";
    extractor.content = content.to_string();

    let context = extractor.extract_code_context(3, 5); // function on lines 4-6 (0-indexed: 3-5)
    assert!(context.is_some());
    let context_str = context.unwrap();

    // Should show lines 1-6 (all available lines)
    assert!(context_str.contains("    1: line 1"));
    assert!(context_str.contains("  ➤   4: function test() {"));
    assert!(context_str.contains("  ➤   6: }"));

    // Test case 3: Empty file
    extractor.content = "".to_string();
    let context = extractor.extract_code_context(0, 0);
    assert!(context.is_none());

    // Test case 4: Single line file
    extractor.content = "single line".to_string();
    let context = extractor.extract_code_context(0, 0);
    assert!(context.is_some());
    let context_str = context.unwrap();
    assert!(context_str.contains("  ➤   1: single line"));
}

#[test]
fn test_context_configuration() {
    let content =
        "line 1\nline 2\nline 3\nfunction test() {\nreturn 42;\n}\nline 7\nline 8\nline 9\nline 10";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    // Test custom context config (1 line before, 2 lines after)
    let custom_config = ContextConfig {
        lines_before: 1,
        lines_after: 2,
        max_line_length: 120,
        show_line_numbers: true,
    };
    extractor.set_context_config(custom_config);

    let context = extractor.extract_code_context(3, 5); // function on lines 4-6 (0-indexed: 3-5)
    assert!(context.is_some());
    let context_str = context.unwrap();

    // Should show lines 3-8 (1 before + symbol + 2 after)
    assert!(context_str.contains("    3: line 3"));
    assert!(context_str.contains("  ➤   4: function test() {"));
    assert!(context_str.contains("  ➤   6: }"));
    assert!(context_str.contains("    7: line 7"));
    assert!(context_str.contains("    8: line 8"));

    // Should NOT contain lines 1, 2, or 10
    assert!(!context_str.contains("line 1"));
    assert!(!context_str.contains("line 2"));
    assert!(!context_str.contains("line 10"));
}

#[test]
fn test_line_truncation() {
    let very_long_line = "a".repeat(150); // 150 character line
    let content = format!("line 1\nline 2\n{}\nline 4", very_long_line);
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    // Set config with short max line length
    let config = ContextConfig {
        lines_before: 3,
        lines_after: 3,
        max_line_length: 10,
        show_line_numbers: true,
    };
    extractor.set_context_config(config);

    let context = extractor.extract_code_context(2, 2); // long line (0-indexed: 2)
    assert!(context.is_some());
    let context_str = context.unwrap();

    // Long line should be truncated with "..."
    assert!(context_str.contains("aaaaaaa..."));
    assert!(!context_str.contains(&very_long_line)); // Full line should not appear
}

#[test]
fn test_context_without_line_numbers() {
    let content = "line 1\nline 2\nfunction test() {\nreturn 42;\n}\nline 6";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    // Disable line numbers
    let config = ContextConfig {
        lines_before: 2,
        lines_after: 1,
        max_line_length: 120,
        show_line_numbers: false,
    };
    extractor.set_context_config(config);

    let context = extractor.extract_code_context(2, 4); // function on lines 3-5 (0-indexed: 2-4)
    assert!(context.is_some());
    let context_str = context.unwrap();

    // Should show content without line numbers
    assert!(context_str.contains("    line 1"));
    assert!(context_str.contains("  ➤ function test() {"));
    assert!(context_str.contains("  ➤ }"));

    // Should NOT contain line numbers
    assert!(!context_str.contains("1:"));
    assert!(!context_str.contains("3:"));
    assert!(!context_str.contains("5:"));
}

#[test]
fn test_symbol_creation() {
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        "function test() {}".to_string(),
        &workspace_root,
    );

    // This will be tested with actual tree-sitter nodes in integration tests
    // For now, just test that the basic structure works
    assert_eq!(extractor.language, "javascript");
    // Note: file_path gets canonicalized, so we test by checking it ends with test.js
    assert!(extractor.file_path.ends_with("test.js"));
    assert!(!extractor.content.is_empty());
}

#[test]
fn test_id_generation() {
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "src/lib.rs".to_string(),
        "fn test() {}".to_string(),
        &workspace_root,
    );

    let id1 = extractor.generate_id("test", 1, 0);
    let id2 = extractor.generate_id("test", 1, 0);
    let id3 = extractor.generate_id("test", 2, 0);

    assert_eq!(id1, id2); // Same inputs should give same ID
    assert_ne!(id1, id3); // Different inputs should give different IDs
    assert_eq!(id1.len(), 32); // MD5 hash is 32 chars
}

#[test]
fn test_relative_path_canonicalization() {
    // BUG FIX TEST: Verify that relative paths are correctly canonicalized
    // This test reproduces the reference workspace indexing scenario where
    // relative paths like "COA.CodeSearch.McpServer/Services/FileIndexingService.cs"
    // were failing canonicalization because we tried to canonicalize them directly
    // instead of joining to workspace_root first.

    // Create a real temporary workspace directory
    let temp_dir = std::env::temp_dir().join("julie_test_relative_path");
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Create nested directories mimicking a real project structure
    let subdir = temp_dir.join("Services").join("Indexing");
    std::fs::create_dir_all(&subdir).unwrap();

    // Create a real file
    let file_path = subdir.join("TestService.cs");
    std::fs::write(&file_path, "class TestService { }").unwrap();

    // TEST CASE 1: Relative path (the bug scenario)
    let relative_path = "Services/Indexing/TestService.cs".to_string();
    let extractor = BaseExtractor::new(
        "csharp".to_string(),
        relative_path.clone(),
        "class TestService { }".to_string(),
        &temp_dir,
    );

    // Verify the extractor was created successfully (no panic from canonicalization)
    assert_eq!(extractor.language, "csharp");

    // Verify the path is stored in relative Unix-style format
    assert!(
        extractor.file_path.contains('/'),
        "Path should use Unix-style separators"
    );
    assert!(
        !extractor.file_path.contains('\\'),
        "Path should not contain Windows separators"
    );
    assert!(
        extractor.file_path.contains("Services/Indexing"),
        "Path should contain the directory structure"
    );
    assert!(
        extractor.file_path.ends_with("TestService.cs"),
        "Path should end with the filename"
    );

    // TEST CASE 2: Absolute path (should still work)
    let extractor_abs = BaseExtractor::new(
        "csharp".to_string(),
        file_path.to_string_lossy().to_string(),
        "class TestService { }".to_string(),
        &temp_dir,
    );

    assert_eq!(extractor_abs.language, "csharp");
    assert!(
        extractor_abs.file_path.contains('/'),
        "Absolute path should also be converted to Unix-style"
    );

    // Cleanup
    std::fs::remove_dir_all(&temp_dir).unwrap();
}

// ---------------------------------------------------------------------------
// get_node_text tests
// ---------------------------------------------------------------------------

/// Helper: parse Rust source and return the tree + a parser (caller owns the tree)
fn parse_rust(content: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser.parse(content, None).unwrap()
}

#[test]
fn test_get_node_text_normal_function_name() {
    // Parse a simple Rust function and extract the function name identifier node
    let content = "fn hello() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    // root -> function_item -> name (identifier)
    let func_item = root.child(0).expect("should have function_item");
    assert_eq!(func_item.kind(), "function_item");

    // Find the identifier child (the function name "hello")
    let mut name_node = None;
    for i in 0..func_item.child_count() {
        let child = func_item.child(i).unwrap();
        if child.kind() == "identifier" {
            name_node = Some(child);
            break;
        }
    }
    let name_node = name_node.expect("function_item should have an identifier child");

    let text = extractor.get_node_text(&name_node);
    assert_eq!(text, "hello");
}

#[test]
fn test_get_node_text_entire_function() {
    // get_node_text should return the full text for a broader node
    let content = "fn add(a: i32) -> i32 { a + 1 }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    let text = extractor.get_node_text(&func_item);
    assert_eq!(text, content);
}

#[test]
fn test_get_node_text_empty_content() {
    // Extractor with empty content — any node's byte range will be out of bounds
    // We parse "fn x() {}" to get a real node, but the extractor holds ""
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        "".to_string(), // empty content
        &workspace_root,
    );

    let tree = parse_rust("fn x() {}");
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    // Node's byte range [0..10) exceeds empty content → should return ""
    let text = extractor.get_node_text(&func_item);
    assert_eq!(text, "");
}

#[test]
fn test_get_node_text_unicode_content() {
    // Rust supports Unicode identifiers in raw identifiers, but let's use a
    // string literal to guarantee Unicode bytes in the content, then test
    // that get_node_text handles multi-byte characters correctly.
    let content = "fn café() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    // tree-sitter-rust may parse "café" as an ERROR node or identifier depending
    // on version. Either way, get_node_text should return the bytes without panic.
    // Let's just get the root text — that's the whole content.
    let text = extractor.get_node_text(&root);
    assert!(
        text.contains("café"),
        "Should contain the Unicode identifier: got '{}'",
        text
    );
}

#[test]
fn test_get_node_text_out_of_bounds_node() {
    // Simulate a node whose byte range exceeds the extractor's content.
    // We parse a longer string to get a node with large byte offsets,
    // then use an extractor with shorter content.
    let short_content = "fn a() {}";
    let long_content = "fn a_very_long_function_name_that_goes_way_beyond() { let _ = 42; }";

    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        short_content.to_string(),
        &workspace_root,
    );

    // Parse the long content to get a node with end_byte > short_content.len()
    let tree = parse_rust(long_content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    // func_item spans [0..67) but extractor content is only 9 bytes
    assert!(func_item.end_byte() > short_content.len());

    let text = extractor.get_node_text(&func_item);
    assert_eq!(text, "", "Out-of-bounds node should return empty string");
}

// ---------------------------------------------------------------------------
// Free-standing find_child_by_type / find_child_by_types tests
// ---------------------------------------------------------------------------

#[test]
fn test_free_find_child_by_type_finds_match() {
    let content = "fn hello(x: i32) -> bool { true }";
    let tree = parse_rust(content);
    let root = tree.root_node();
    let func = root.child(0).unwrap();
    assert_eq!(func.kind(), "function_item");

    let params = crate::base::find_child_by_type(&func, "parameters");
    assert!(params.is_some(), "should find parameters child");
    assert_eq!(params.unwrap().kind(), "parameters");
}

#[test]
fn test_free_find_child_by_type_returns_none_for_missing() {
    let content = "fn hello() {}";
    let tree = parse_rust(content);
    let root = tree.root_node();
    let func = root.child(0).unwrap();

    let result = crate::base::find_child_by_type(&func, "class_definition");
    assert!(
        result.is_none(),
        "should return None for nonexistent child type"
    );
}

#[test]
fn test_free_find_child_by_types_finds_first_match() {
    let content = "fn hello(x: i32) -> bool { true }";
    let tree = parse_rust(content);
    let root = tree.root_node();
    let func = root.child(0).unwrap();

    let result = crate::base::find_child_by_types(&func, &["class_definition", "parameters"]);
    assert!(result.is_some());
    assert_eq!(result.unwrap().kind(), "parameters");
}

#[test]
fn test_free_find_child_by_types_returns_none_when_no_match() {
    let content = "fn hello() {}";
    let tree = parse_rust(content);
    let root = tree.root_node();
    let func = root.child(0).unwrap();

    let result = crate::base::find_child_by_types(&func, &["class_definition", "import"]);
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// create_symbol tests (also indirectly tests find_doc_comment)
// ---------------------------------------------------------------------------

#[test]
fn test_create_symbol_basic_function() {
    let content = "fn hello() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).expect("should have function_item");

    let symbol = extractor.create_symbol(
        &func_item,
        "hello".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert_eq!(symbol.name, "hello");
    assert_eq!(symbol.kind, SymbolKind::Function);
    assert_eq!(symbol.language, "rust");
    assert!(symbol.file_path.ends_with("test.rs"));
    assert_eq!(symbol.start_line, 1); // 1-based
    assert_eq!(symbol.end_line, 1);
    assert_eq!(symbol.id.len(), 32); // MD5 hash
    assert!(symbol.code_context.is_some());
    assert!(symbol.content_type.is_none()); // Not markdown
    assert!(symbol.doc_comment.is_none()); // No doc comment above
}

#[test]
fn test_create_symbol_with_visibility() {
    let content = "pub fn visible() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    let options = SymbolOptions {
        visibility: Some(Visibility::Public),
        ..Default::default()
    };

    let symbol = extractor.create_symbol(
        &func_item,
        "visible".to_string(),
        SymbolKind::Function,
        options,
    );

    assert_eq!(symbol.visibility, Some(Visibility::Public));
}

#[test]
fn test_create_symbol_with_parent_id() {
    let content = "fn child() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    let options = SymbolOptions {
        parent_id: Some("parent_sym_id_abc".to_string()),
        ..Default::default()
    };

    let symbol =
        extractor.create_symbol(&func_item, "child".to_string(), SymbolKind::Method, options);

    assert_eq!(symbol.parent_id, Some("parent_sym_id_abc".to_string()));
    assert_eq!(symbol.kind, SymbolKind::Method);
}

#[test]
fn test_create_symbol_with_signature() {
    let content = "fn greet(name: &str) -> String { name.to_string() }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    let options = SymbolOptions {
        signature: Some("fn greet(name: &str) -> String".to_string()),
        ..Default::default()
    };

    let symbol = extractor.create_symbol(
        &func_item,
        "greet".to_string(),
        SymbolKind::Function,
        options,
    );

    assert_eq!(
        symbol.signature,
        Some("fn greet(name: &str) -> String".to_string())
    );
}

#[test]
fn test_create_symbol_markdown_content_type() {
    // Markdown extractors set language="markdown" → content_type should be "documentation"
    let content = "# Hello World";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "markdown".to_string(),
        "README.md".to_string(),
        content.to_string(),
        &workspace_root,
    );

    // Parse as markdown to get a real node
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .expect("Error loading Markdown grammar");
    let tree = parser.parse(content, None).unwrap();
    let root = tree.root_node();
    // Use the root node (section or document)
    let first_child = root.child(0).unwrap_or(root);

    let symbol = extractor.create_symbol(
        &first_child,
        "Hello World".to_string(),
        SymbolKind::Module,
        SymbolOptions::default(),
    );

    assert_eq!(symbol.content_type, Some("documentation".to_string()));
}

#[test]
fn test_create_symbol_inserted_into_symbol_map() {
    let content = "fn mapped() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    assert!(extractor.symbol_map.is_empty());

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    let symbol = extractor.create_symbol(
        &func_item,
        "mapped".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert_eq!(extractor.symbol_map.len(), 1);
    assert_eq!(extractor.symbol_map.get(&symbol.id).unwrap().name, "mapped");
}

// ---------------------------------------------------------------------------
// find_doc_comment tests (indirectly through create_symbol)
// ---------------------------------------------------------------------------

#[test]
fn test_find_doc_comment_rust_triple_slash() {
    // Rust /// doc comment above a function should be captured
    let content = "/// This is a doc comment\nfn documented() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    // Find the function_item node (it should be the last named child after the comment)
    let mut func_node = None;
    for i in 0..root.named_child_count() {
        let child = root.named_child(i).unwrap();
        if child.kind() == "function_item" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("should find function_item");

    let symbol = extractor.create_symbol(
        &func_node,
        "documented".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert!(
        symbol.doc_comment.is_some(),
        "Should capture /// doc comment"
    );
    let doc = symbol.doc_comment.unwrap();
    assert!(
        doc.contains("This is a doc comment"),
        "Doc comment should contain the text, got: '{}'",
        doc
    );
}

#[test]
fn test_find_doc_comment_none_when_absent() {
    // No doc comment → doc_comment should be None
    let content = "fn undocumented() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_node = root.child(0).unwrap();

    let symbol = extractor.create_symbol(
        &func_node,
        "undocumented".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert!(
        symbol.doc_comment.is_none(),
        "Should have no doc comment, got: {:?}",
        symbol.doc_comment
    );
}

#[test]
fn test_find_doc_comment_multiline() {
    // Multiple /// lines should be joined
    let content = "/// First line\n/// Second line\n/// Third line\nfn multi_doc() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    let mut func_node = None;
    for i in 0..root.named_child_count() {
        let child = root.named_child(i).unwrap();
        if child.kind() == "function_item" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("should find function_item");

    let symbol = extractor.create_symbol(
        &func_node,
        "multi_doc".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert!(
        symbol.doc_comment.is_some(),
        "Should capture multi-line doc comments"
    );
    let doc = symbol.doc_comment.unwrap();
    assert!(
        doc.contains("First line"),
        "Should contain first line, got: '{}'",
        doc
    );
    assert!(
        doc.contains("Second line"),
        "Should contain second line, got: '{}'",
        doc
    );
    assert!(
        doc.contains("Third line"),
        "Should contain third line, got: '{}'",
        doc
    );
}

#[test]
fn test_find_doc_comment_explicit_option_overrides_auto_detection() {
    // If SymbolOptions provides a doc_comment, it should be used instead of auto-detection
    let content = "/// Auto-detected comment\nfn overridden() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    let mut func_node = None;
    for i in 0..root.named_child_count() {
        let child = root.named_child(i).unwrap();
        if child.kind() == "function_item" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("should find function_item");

    let options = SymbolOptions {
        doc_comment: Some("Manually provided doc".to_string()),
        ..Default::default()
    };

    let symbol = extractor.create_symbol(
        &func_node,
        "overridden".to_string(),
        SymbolKind::Function,
        options,
    );

    assert_eq!(
        symbol.doc_comment,
        Some("Manually provided doc".to_string()),
        "Explicit doc_comment in options should override auto-detection"
    );
}

// ---------------------------------------------------------------------------
// create_identifier tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_identifier_basic_call() {
    // Parse `fn foo() { bar(); }` and create an identifier for the `bar` call
    let content = "fn foo() { bar(); }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    // Navigate: source_file > function_item > block > expression_statement > call_expression > identifier("bar")
    let func_item = root.child(0).expect("should have function_item");
    let block = func_item
        .child_by_field_name("body")
        .expect("function should have body");

    // Find the call expression's identifier node ("bar")
    let mut bar_node = None;
    let mut cursor = block.walk();
    for child in block.children(&mut cursor) {
        // Walk into expression_statement -> call_expression -> function identifier
        if child.kind() == "expression_statement" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "call_expression" {
                    // The function being called is the first named child
                    if let Some(callee) = inner.named_child(0) {
                        if callee.kind() == "identifier" {
                            bar_node = Some(callee);
                        }
                    }
                }
            }
        }
    }
    let bar_node = bar_node.expect("should find 'bar' identifier node in call");

    let identifier =
        extractor.create_identifier(&bar_node, "bar".to_string(), IdentifierKind::Call, None);

    assert_eq!(identifier.name, "bar");
    assert_eq!(identifier.kind, IdentifierKind::Call);
    assert_eq!(identifier.language, "rust");
    assert!(identifier.file_path.ends_with("test.rs"));
    assert_eq!(identifier.start_line, 1); // 1-based
    assert_eq!(identifier.containing_symbol_id, None);
    assert_eq!(identifier.target_symbol_id, None); // Unresolved
    assert_eq!(identifier.confidence, 1.0);
    assert_eq!(identifier.id.len(), 32); // MD5 hash
    assert!(identifier.code_context.is_some());
}

#[test]
fn test_create_identifier_with_containing_symbol_id() {
    let content = "fn foo() { bar(); }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();
    let block = func_item.child_by_field_name("body").unwrap();

    // Find `bar` identifier
    let mut bar_node = None;
    let mut cursor = block.walk();
    for child in block.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            let mut inner_cursor = child.walk();
            for inner in child.children(&mut inner_cursor) {
                if inner.kind() == "call_expression" {
                    if let Some(callee) = inner.named_child(0) {
                        if callee.kind() == "identifier" {
                            bar_node = Some(callee);
                        }
                    }
                }
            }
        }
    }
    let bar_node = bar_node.unwrap();

    let parent_id = Some("parent_sym_abc123".to_string());
    let identifier = extractor.create_identifier(
        &bar_node,
        "bar".to_string(),
        IdentifierKind::Call,
        parent_id.clone(),
    );

    assert_eq!(
        identifier.containing_symbol_id, parent_id,
        "Parent linkage should be preserved"
    );
}

#[test]
fn test_create_identifier_without_containing_symbol_id() {
    let content = "fn foo() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    // Use the function name node as the identifier target (just to test None path)
    let mut name_node = None;
    for i in 0..func_item.child_count() {
        let child = func_item.child(i).unwrap();
        if child.kind() == "identifier" {
            name_node = Some(child);
            break;
        }
    }
    let name_node = name_node.unwrap();

    let identifier = extractor.create_identifier(
        &name_node,
        "foo".to_string(),
        IdentifierKind::VariableRef,
        None,
    );

    assert_eq!(
        identifier.containing_symbol_id, None,
        "None should be preserved when no containing symbol"
    );
}

#[test]
fn test_create_identifier_type_usage_kind() {
    let content = "fn foo() -> String { String::new() }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    // Find the return type identifier ("String" in -> String)
    let mut type_node = None;
    let mut cursor = func_item.walk();
    for child in func_item.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            type_node = Some(child);
            break;
        }
    }
    let type_node = type_node.expect("should find type_identifier for return type");

    let identifier = extractor.create_identifier(
        &type_node,
        "String".to_string(),
        IdentifierKind::TypeUsage,
        None,
    );

    assert_eq!(identifier.kind, IdentifierKind::TypeUsage);
    assert_eq!(identifier.name, "String");
}

#[test]
fn test_create_identifier_stored_in_identifiers_vec() {
    let content = "fn foo() { bar(); baz(); }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    assert!(
        extractor.identifiers.is_empty(),
        "identifiers should start empty"
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).unwrap();

    // Use the function name node to create two identifiers
    let mut name_node = None;
    for i in 0..func_item.child_count() {
        let child = func_item.child(i).unwrap();
        if child.kind() == "identifier" {
            name_node = Some(child);
            break;
        }
    }
    let name_node = name_node.unwrap();

    extractor.create_identifier(&name_node, "bar".to_string(), IdentifierKind::Call, None);
    assert_eq!(extractor.identifiers.len(), 1);

    extractor.create_identifier(&name_node, "baz".to_string(), IdentifierKind::Call, None);
    assert_eq!(
        extractor.identifiers.len(),
        2,
        "Each create_identifier call should push to the vec"
    );

    assert_eq!(extractor.identifiers[0].name, "bar");
    assert_eq!(extractor.identifiers[1].name, "baz");
}

// ---------------------------------------------------------------------------
// find_doc_comment edge case tests (indirectly through create_symbol)
// ---------------------------------------------------------------------------

#[test]
fn test_find_doc_comment_block_comment() {
    // Rust /** block doc comment */ should be captured via block_comment node
    let content = "/** block doc comment */\nfn foo() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    // Find the function_item node
    let mut func_node = None;
    for i in 0..root.named_child_count() {
        let child = root.named_child(i).unwrap();
        if child.kind() == "function_item" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("should find function_item");

    let symbol = extractor.create_symbol(
        &func_node,
        "foo".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert!(
        symbol.doc_comment.is_some(),
        "Should capture /** block comment */"
    );
    let doc = symbol.doc_comment.unwrap();
    assert!(
        doc.contains("block doc comment"),
        "Block comment text should be captured, got: '{}'",
        doc
    );
}

#[test]
fn test_find_doc_comment_attribute_doc_not_captured() {
    // #[doc = "..."] is parsed as attribute_item (not a comment node), so
    // find_doc_comment won't capture it — the loop stops at non-comment siblings
    let content = "#[doc = \"attr doc\"]\nfn bar() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    let mut func_node = None;
    for i in 0..root.named_child_count() {
        let child = root.named_child(i).unwrap();
        if child.kind() == "function_item" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("should find function_item");

    let symbol = extractor.create_symbol(
        &func_node,
        "bar".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    // attribute_item is NOT a comment kind, so find_doc_comment skips it
    assert!(
        symbol.doc_comment.is_none(),
        "#[doc = ...] attribute should not be captured by find_doc_comment, got: {:?}",
        symbol.doc_comment
    );
}

#[test]
fn test_find_doc_comment_blank_line_separated_still_captured() {
    // In tree-sitter Rust, a `///` comment separated by a blank line from the function
    // is still the prev_named_sibling. find_doc_comment walks siblings, not lines,
    // so it WILL capture the comment. This test documents that behavior.
    let content = "/// Separated comment\n\nfn separated() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    let mut func_node = None;
    for i in 0..root.named_child_count() {
        let child = root.named_child(i).unwrap();
        if child.kind() == "function_item" {
            func_node = Some(child);
            break;
        }
    }
    let func_node = func_node.expect("should find function_item");

    let symbol = extractor.create_symbol(
        &func_node,
        "separated".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    // find_doc_comment walks prev_named_sibling, which IS the comment even with blank line
    assert!(
        symbol.doc_comment.is_some(),
        "Blank-line-separated comment is still prev_named_sibling in tree-sitter, so it's captured"
    );
    let doc = symbol.doc_comment.unwrap();
    assert!(
        doc.contains("Separated comment"),
        "Should contain the comment text, got: '{}'",
        doc
    );
}

#[test]
fn test_find_doc_comment_function_first_in_file() {
    // A function at the very start of the file has no preceding siblings
    let content = "fn first() { }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_node = root.child(0).expect("should have function_item");

    let symbol = extractor.create_symbol(
        &func_node,
        "first".to_string(),
        SymbolKind::Function,
        SymbolOptions::default(),
    );

    assert!(
        symbol.doc_comment.is_none(),
        "Function with no preceding siblings should have no doc comment, got: {:?}",
        symbol.doc_comment
    );
}

// ---------------------------------------------------------------------------
// find_containing_symbol tests
// ---------------------------------------------------------------------------

/// Helper: build a minimal Symbol with the position fields needed for containment checks.
fn make_symbol(
    name: &str,
    kind: SymbolKind,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
) -> Symbol {
    Symbol {
        id: format!("sym_{}", name),
        name: name.to_string(),
        kind,
        language: "rust".to_string(),
        file_path: "test.rs".to_string(),
        start_line,
        start_column,
        end_line,
        end_column,
        start_byte,
        end_byte,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }
}

#[test]
fn test_find_containing_symbol_node_inside_function() {
    // Content: a function with a call inside it
    //          0         1         2
    // line 1:  fn outer() { call(); }
    let content = "fn outer() { call(); }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    // Navigate to the call_expression's identifier ("call") inside the function body
    let func_item = root.child(0).expect("function_item");
    let block = func_item.child_by_field_name("body").expect("body");
    let mut call_node = None;
    let mut cursor = block.walk();
    for child in block.children(&mut cursor) {
        if child.kind() == "expression_statement" {
            let mut inner = child.walk();
            for c in child.children(&mut inner) {
                if c.kind() == "call_expression" {
                    if let Some(callee) = c.named_child(0) {
                        call_node = Some(callee);
                    }
                }
            }
        }
    }
    let call_node = call_node.expect("should find call identifier");

    // Build a symbol spanning the entire function: line 1, col 0 -> line 1, col 22
    // (tree-sitter rows are 0-based, but Symbol uses 1-based lines)
    let symbols = vec![make_symbol(
        "outer",
        SymbolKind::Function,
        1,  // start_line (1-based)
        0,  // start_column
        1,  // end_line
        22, // end_column (length of the line)
        0,  // start_byte
        22, // end_byte
    )];

    let result = extractor.find_containing_symbol(&call_node, &symbols);
    assert!(result.is_some(), "call node should be inside outer()");
    assert_eq!(result.unwrap().name, "outer");
}

#[test]
fn test_find_containing_symbol_nested_returns_innermost() {
    // Content:
    // line 1: fn outer() {
    // line 2:     fn inner() {
    // line 3:         let _ = 42;
    // line 4:     }
    // line 5: }
    let content = "fn outer() {\n    fn inner() {\n        let _ = 42;\n    }\n}";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    // Find the `let _ = 42;` statement (on line 3)
    // Navigate: source_file > function_item(outer) > block > function_item(inner) > block > let_declaration
    let outer_fn = root.child(0).expect("outer function_item");
    let outer_block = outer_fn.child_by_field_name("body").expect("outer body");
    let mut inner_fn = None;
    let mut cursor = outer_block.walk();
    for child in outer_block.children(&mut cursor) {
        if child.kind() == "function_item" {
            inner_fn = Some(child);
        }
    }
    let inner_fn = inner_fn.expect("should find inner function_item");
    let inner_block = inner_fn.child_by_field_name("body").expect("inner body");
    let mut let_node = None;
    let mut cursor2 = inner_block.walk();
    for child in inner_block.children(&mut cursor2) {
        if child.kind() == "let_declaration" {
            let_node = Some(child);
        }
    }
    let let_node = let_node.expect("should find let_declaration");

    // Build symbols for outer (lines 1-5) and inner (lines 2-4)
    let symbols = vec![
        make_symbol(
            "outer",
            SymbolKind::Function,
            1,
            0,
            5,
            1,
            0,
            content.len() as u32,
        ),
        make_symbol(
            "inner",
            SymbolKind::Function,
            2,
            4,
            4,
            5,
            // inner starts at "    fn inner..." which is offset 13 in the content
            13,
            // inner ends at "    }" which is at offset 52
            52,
        ),
    ];

    let result = extractor.find_containing_symbol(&let_node, &symbols);
    assert!(result.is_some(), "let node should be inside inner()");
    assert_eq!(
        result.unwrap().name,
        "inner",
        "Should return the innermost (narrowest) containing symbol"
    );
}

#[test]
fn test_find_containing_symbol_node_at_top_level() {
    // Content: just a let statement at the top level
    let content = "let _ = 42;";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    // Get the first child node (the let statement or error node)
    let node = root.child(0).expect("should have a child node");

    // No symbols at all — the node is at the top level
    let symbols = vec![make_symbol(
        "elsewhere",
        SymbolKind::Function,
        10, // far away from line 1
        0,
        15,
        1,
        200,
        300,
    )];

    let result = extractor.find_containing_symbol(&node, &symbols);
    assert!(
        result.is_none(),
        "Node at top level (not within any symbol's range) should return None"
    );
}

#[test]
fn test_find_containing_symbol_empty_symbols_list() {
    let content = "fn hello() {}";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();
    let func_item = root.child(0).expect("function_item");

    let symbols: Vec<Symbol> = vec![];
    let result = extractor.find_containing_symbol(&func_item, &symbols);
    assert!(result.is_none(), "Empty symbols list should return None");
}

#[test]
fn test_find_containing_symbol_single_line_column_checks() {
    // Content: fn a() { fn b() {} }
    // Both functions are on a single line. The "b" function starts at col 9 and ends at col 19.
    // A node at col 12 should be inside b, not a.
    let content = "fn a() { fn b() {} }";
    let workspace_root = std::path::PathBuf::from("/tmp/test");
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
        &workspace_root,
    );

    let tree = parse_rust(content);
    let root = tree.root_node();

    // Navigate to inner function "b"
    let outer_fn = root.child(0).expect("outer function_item");
    let outer_block = outer_fn.child_by_field_name("body").expect("body");
    let mut inner_fn = None;
    let mut cursor = outer_block.walk();
    for child in outer_block.children(&mut cursor) {
        if child.kind() == "function_item" {
            inner_fn = Some(child);
        }
    }
    let inner_fn = inner_fn.expect("should find inner function_item b");
    let inner_block = inner_fn.child_by_field_name("body").expect("inner body");

    // The inner block `{}` is empty but exists — use the block node itself as our target.
    // It sits inside b's span.

    // Both symbols are on line 1. a spans col 0..20, b spans col 9..18.
    let symbols = vec![
        make_symbol(
            "a",
            SymbolKind::Function,
            1,
            0,
            1,
            20,
            0,  // start_byte
            20, // end_byte
        ),
        make_symbol(
            "b",
            SymbolKind::Function,
            1,
            9,
            1,
            18,
            9,  // start_byte
            18, // end_byte
        ),
    ];

    let result = extractor.find_containing_symbol(&inner_block, &symbols);
    assert!(result.is_some(), "inner block should be contained");
    assert_eq!(
        result.unwrap().name,
        "b",
        "Should return b (narrower) not a, since both are on line 1 but b has smaller byte range"
    );
}
