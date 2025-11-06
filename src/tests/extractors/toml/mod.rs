// TOML Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod toml_extractor_tests {
    #![allow(unused_imports)]
    #![allow(unused_variables)]

    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::extractors::toml::TomlExtractor;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    fn init_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_toml_ng::LANGUAGE.into())
            .expect("Error loading TOML grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = init_parser();
        let tree = parser.parse(code, None).expect("Failed to parse code");
        let mut extractor = TomlExtractor::new(
            "toml".to_string(),
            "test.toml".to_string(),
            code.to_string(),
            &workspace_root,
        );
        extractor.extract_symbols(&tree)
    }

    #[test]
    fn test_extract_toml_tables() {
        let toml = r#"
[package]
name = "julie"
version = "1.0.0"

[dependencies]
tokio = "1.0"
serde = "1.0"

[dev-dependencies]
proptest = "1.0"
"#;

        let symbols = extract_symbols(toml);

        // Should extract tables as modules
        assert!(symbols.len() >= 3, "Expected at least 3 tables, got {}", symbols.len());

        let package = symbols.iter().find(|s| s.name == "package");
        assert!(package.is_some(), "Should find 'package' table");
        assert_eq!(package.unwrap().kind, SymbolKind::Module);

        let deps = symbols.iter().find(|s| s.name == "dependencies");
        assert!(deps.is_some(), "Should find 'dependencies' table");
        assert_eq!(deps.unwrap().kind, SymbolKind::Module);

        let dev_deps = symbols.iter().find(|s| s.name == "dev-dependencies");
        assert!(dev_deps.is_some(), "Should find 'dev-dependencies' table");
        assert_eq!(dev_deps.unwrap().kind, SymbolKind::Module);
    }

    #[test]
    fn test_extract_toml_nested_tables() {
        let toml = r#"
[database]
host = "localhost"
port = 5432

[database.connection]
timeout = 30
retry = 3

[database.connection.pool]
size = 10
"#;

        let symbols = extract_symbols(toml);

        // Should have database table and nested tables
        assert!(symbols.len() >= 3, "Expected nested tables, got {}", symbols.len());

        let database = symbols.iter().find(|s| s.name == "database");
        assert!(database.is_some(), "Should find 'database' table");

        let connection = symbols.iter().find(|s| s.name == "database.connection");
        assert!(connection.is_some(), "Should find 'database.connection' table");

        let pool = symbols.iter().find(|s| s.name == "database.connection.pool");
        assert!(pool.is_some(), "Should find 'database.connection.pool' table");
    }

    #[test]
    fn test_extract_toml_array_tables() {
        let toml = r#"
[[servers]]
name = "alpha"
ip = "10.0.0.1"

[[servers]]
name = "beta"
ip = "10.0.0.2"
"#;

        let symbols = extract_symbols(toml);

        // Array tables should be extracted
        let servers: Vec<_> = symbols.iter().filter(|s| s.name == "servers").collect();
        assert!(!servers.is_empty(), "Should find 'servers' array table entries");
    }
}
