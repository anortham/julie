// Tests extracted from src/extractors/base.rs
// These were previously inline tests that have been moved to follow project standards

use crate::extractors::base::*;

#[test]
fn test_context_extraction_edge_cases() {
    // Test case 1: Symbol at the beginning of file (not enough lines before)
    let content = "line 1\nline 2\nfunction test() {\nreturn 42;\n}\nline 6\nline 7\nline 8";
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
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
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
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
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
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
    let mut extractor = BaseExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        content.to_string(),
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
    let extractor = BaseExtractor::new(
        "javascript".to_string(),
        "test.js".to_string(),
        "function test() {}".to_string(),
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
    let extractor = BaseExtractor::new(
        "rust".to_string(),
        "src/lib.rs".to_string(),
        "fn test() {}".to_string(),
    );

    let id1 = extractor.generate_id("test", 1, 0);
    let id2 = extractor.generate_id("test", 1, 0);
    let id3 = extractor.generate_id("test", 2, 0);

    assert_eq!(id1, id2); // Same inputs should give same ID
    assert_ne!(id1, id3); // Different inputs should give different IDs
    assert_eq!(id1.len(), 32); // MD5 hash is 32 chars
}
