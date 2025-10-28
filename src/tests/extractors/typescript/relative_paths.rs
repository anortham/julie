// TypeScript Extractor - Relative Path Storage Tests
//
// Tests for Phase 2: Relative Unix-Style Path Storage
// Verifies that extracted symbols store relative Unix-style paths instead of absolute paths

use crate::extractors::typescript::TypeScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

/// Initialize JavaScript parser for TypeScript files
fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("Error loading JavaScript grammar");
    parser
}

#[cfg(test)]
mod relative_path_tests {
    use super::*;

    #[test]
    fn test_typescript_extractor_stores_relative_unix_paths() {
        // TDD RED: This test WILL FAIL because TypeScriptExtractor::new()
        // doesn't accept workspace_root parameter yet

        let workspace_root = PathBuf::from("/Users/murphy/source/julie");
        let file_path = workspace_root.join("src/tools/search.rs");

        let code = r#"
        function getUserData(id) {
          return fetch(`/api/users/${id}`).then(r => r.json());
        }

        class UserService {
          constructor() {
            this.cache = new Map();
          }

          async fetchUser(id) {
            return this.cache.get(id) || await getUserData(id);
          }
        }
        "#;

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        // NEW: Pass workspace_root as 4th parameter
        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            file_path.to_string_lossy().to_string(),
            code.to_string(),
            &workspace_root, // NEW: workspace_root parameter
        );

        let symbols = extractor.extract_symbols(&tree);

        // CONTRACT VERIFICATION: Symbols must store relative Unix-style paths
        assert!(symbols.len() >= 2, "Should extract function and class");

        for symbol in &symbols {
            // 1. Path must be relative (no leading / or drive letter)
            assert!(
                !symbol.file_path.starts_with('/'),
                "Path should be relative, not absolute: {}",
                symbol.file_path
            );

            assert!(
                !symbol.file_path.contains(":\\"),
                "Path should not contain Windows drive letter: {}",
                symbol.file_path
            );

            // 2. Path must use Unix separators (/)
            assert!(
                !symbol.file_path.contains('\\'),
                "Path should use Unix separators, not backslashes: {}",
                symbol.file_path
            );

            // 3. Path should be the expected relative path
            assert_eq!(
                symbol.file_path, "src/tools/search.rs",
                "Path should be relative to workspace root"
            );
        }

        // Verify specific symbols
        let get_user_data = symbols.iter().find(|s| s.name == "getUserData");
        assert!(get_user_data.is_some(), "Should find getUserData function");
        assert_eq!(
            get_user_data.unwrap().file_path,
            "src/tools/search.rs",
            "Function should have relative path"
        );

        let user_service = symbols.iter().find(|s| s.name == "UserService");
        assert!(user_service.is_some(), "Should find UserService class");
        assert_eq!(
            user_service.unwrap().file_path,
            "src/tools/search.rs",
            "Class should have relative path"
        );
    }

    #[test]
    fn test_root_level_file_has_no_directory_separator() {
        // Test file at workspace root (e.g., README.md)
        let workspace_root = PathBuf::from("/Users/murphy/source/julie");
        let file_path = workspace_root.join("index.ts");

        let code = "export const VERSION = '1.0.0';";

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            file_path.to_string_lossy().to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        if let Some(symbol) = symbols.first() {
            assert_eq!(
                symbol.file_path, "index.ts",
                "Root-level file should have no directory separator"
            );
            assert!(
                !symbol.file_path.contains('/'),
                "Root-level file path should not contain /"
            );
        }
    }

    #[test]
    fn test_nested_directory_uses_unix_separators() {
        // Test deeply nested file
        let workspace_root = PathBuf::from("/Users/murphy/source/julie");
        let file_path = workspace_root.join("src/extractors/typescript/symbols.ts");

        let code = "export function extractSymbols() {}";

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            file_path.to_string_lossy().to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        if let Some(symbol) = symbols.first() {
            assert_eq!(
                symbol.file_path, "src/extractors/typescript/symbols.ts",
                "Nested path should use Unix separators"
            );

            // Count separators (should be 3 for src/extractors/typescript/symbols.ts)
            let separator_count = symbol.file_path.matches('/').count();
            assert_eq!(
                separator_count, 3,
                "Should have 3 directory separators"
            );
        }
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_windows_unc_paths_converted_to_relative() {
        // Test Windows UNC path conversion
        // This will only run on Windows where UNC paths exist
        use std::path::Path;

        let workspace_root = PathBuf::from(r"\\?\C:\Users\murphy\source\julie");
        let file_path = workspace_root.join("src\\main.rs");

        let code = "function main() {}";

        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = TypeScriptExtractor::new(
            "typescript".to_string(),
            file_path.to_string_lossy().to_string(),
            code.to_string(),
            &workspace_root,
        );

        let symbols = extractor.extract_symbols(&tree);

        if let Some(symbol) = symbols.first() {
            // Verify NO Windows UNC prefix
            assert!(
                !symbol.file_path.contains(r"\\?\"),
                "Should not contain Windows UNC prefix"
            );

            // Verify NO backslashes
            assert!(
                !symbol.file_path.contains('\\'),
                "Should not contain backslashes"
            );

            // Verify Unix-style relative path
            assert_eq!(
                symbol.file_path, "src/main.rs",
                "Should be Unix-style relative path"
            );
        }
    }
}
