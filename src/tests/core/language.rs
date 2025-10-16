//! Inline tests extracted from src/language.rs
//!
//! This module contains all test cases for language support functionality.
//! Tests verify that all 26 supported languages are properly configured and
//! that language detection and AST node lookups work correctly.

use crate::language::*;

#[test]
fn test_all_26_languages_supported() {
    let languages = vec![
        "rust",
        "c",
        "cpp",
        "go",
        "zig",
        "typescript",
        "tsx",
        "javascript",
        "html",
        "css",
        "vue",
        "python",
        "java",
        "csharp",
        "php",
        "ruby",
        "swift",
        "kotlin",
        "dart",
        "lua",
        "bash",
        "powershell",
        "gdscript",
        "razor",
        "sql",
        "regex",
    ];

    for lang in &languages {
        assert!(
            get_tree_sitter_language(lang).is_ok(),
            "Language '{}' should be supported",
            lang
        );
    }

    assert_eq!(languages.len(), 26, "Should have exactly 26 languages");
}

#[test]
fn test_unsupported_language_fails() {
    assert!(get_tree_sitter_language("cobol").is_err());
    assert!(get_tree_sitter_language("fortran").is_err());
}

#[test]
fn test_extension_detection() {
    assert_eq!(detect_language_from_extension("rs"), Some("rust"));
    assert_eq!(detect_language_from_extension("ts"), Some("typescript"));
    assert_eq!(detect_language_from_extension("py"), Some("python"));
    assert_eq!(detect_language_from_extension("unknown"), None);
}
