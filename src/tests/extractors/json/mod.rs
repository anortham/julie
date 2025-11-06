// JSON Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod json_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::extractors::json::JsonExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_json::LANGUAGE.into())
            .expect("Error loading JSON grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = JsonExtractor::new(
            "json".to_string(),
            "test.json".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    #[test]
    fn test_extract_json_object_keys() {
        let json = r#"{
  "name": "my-project",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.0.0",
    "lodash": "^4.17.21"
  },
  "scripts": {
    "build": "webpack",
    "test": "jest"
  }
}"#;

        let symbols = extract_symbols(json);

        // We should extract top-level keys as symbols
        assert!(symbols.len() >= 4, "Expected at least 4 top-level keys, got {}", symbols.len());

        // Check for top-level keys
        let name_key = symbols.iter().find(|s| s.name == "name");
        assert!(name_key.is_some(), "Should find 'name' key");
        assert_eq!(name_key.unwrap().kind, SymbolKind::Variable);

        let version_key = symbols.iter().find(|s| s.name == "version");
        assert!(version_key.is_some(), "Should find 'version' key");

        let deps_key = symbols.iter().find(|s| s.name == "dependencies");
        assert!(deps_key.is_some(), "Should find 'dependencies' key");
        // Objects should be treated as namespaces/modules
        assert_eq!(deps_key.unwrap().kind, SymbolKind::Module);

        let scripts_key = symbols.iter().find(|s| s.name == "scripts");
        assert!(scripts_key.is_some(), "Should find 'scripts' key");
        assert_eq!(scripts_key.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_extract_nested_json_keys() {
        let json = r#"{
  "config": {
    "database": {
      "host": "localhost",
      "port": 5432
    }
  }
}"#;

        let symbols = extract_symbols(json);

        // Should have config, database, host, port
        assert!(symbols.len() >= 4, "Expected nested keys, got {}", symbols.len());

        let config = symbols.iter().find(|s| s.name == "config");
        assert!(config.is_some(), "Should find 'config' key");

        let database = symbols.iter().find(|s| s.name == "database");
        assert!(database.is_some(), "Should find 'database' key");
    }
}
