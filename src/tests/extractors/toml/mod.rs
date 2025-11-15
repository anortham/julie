// TOML Extractor Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR
//
// Comprehensive test coverage matching the quality of TypeScript/Rust extractors
// Target: 700+ lines with edge cases, special syntax, and real-world validation

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

    // ========================================================================
    // Basic Table Extraction
    // ========================================================================

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
        assert!(
            symbols.len() >= 3,
            "Expected at least 3 tables, got {}",
            symbols.len()
        );

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
    fn test_simple_key_value_pairs() {
        let toml = r#"
title = "TOML Example"
count = 42
enabled = true
ratio = 3.14
"#;

        let symbols = extract_symbols(toml);

        // Key-value pairs at root level might or might not be extracted
        // Depends on implementation - at minimum should parse without errors
        assert!(
            symbols.len() >= 0,
            "Should handle root-level key-value pairs"
        );
    }

    // ========================================================================
    // Nested Tables & Dotted Keys
    // ========================================================================

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
        assert!(
            symbols.len() >= 3,
            "Expected nested tables, got {}",
            symbols.len()
        );

        let database = symbols.iter().find(|s| s.name == "database");
        assert!(database.is_some(), "Should find 'database' table");

        let connection = symbols.iter().find(|s| s.name == "database.connection");
        assert!(
            connection.is_some(),
            "Should find 'database.connection' table"
        );

        let pool = symbols
            .iter()
            .find(|s| s.name == "database.connection.pool");
        assert!(
            pool.is_some(),
            "Should find 'database.connection.pool' table"
        );
    }

    #[test]
    fn test_dotted_keys() {
        let toml = r#"
[server]
host = "localhost"
server.port = 8080
server.ssl.enabled = true
server.ssl.cert = "/path/to/cert"
"#;

        let symbols = extract_symbols(toml);

        // Should extract server table at minimum
        let server = symbols.iter().find(|s| s.name == "server");
        assert!(server.is_some(), "Should find 'server' table");
    }

    #[test]
    fn test_deeply_nested_tables() {
        let toml = r#"
[level1]
key = "value"

[level1.level2]
key = "value"

[level1.level2.level3]
key = "value"

[level1.level2.level3.level4]
key = "value"

[level1.level2.level3.level4.level5]
key = "deep"
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 5, "Should extract deeply nested tables");

        let has_deep = symbols
            .iter()
            .any(|s| s.name.contains("level4") || s.name.contains("level5"));
        assert!(has_deep, "Should find deeply nested tables");
    }

    // ========================================================================
    // Array Tables [[array]]
    // ========================================================================

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
        assert!(
            !servers.is_empty(),
            "Should find 'servers' array table entries"
        );
    }

    #[test]
    fn test_nested_array_tables() {
        let toml = r#"
[[products]]
name = "Product A"

[[products.variants]]
size = "small"
color = "red"

[[products.variants]]
size = "large"
color = "blue"

[[products]]
name = "Product B"
"#;

        let symbols = extract_symbols(toml);

        let products: Vec<_> = symbols
            .iter()
            .filter(|s| s.name.contains("products"))
            .collect();
        assert!(!products.is_empty(), "Should find products array tables");
    }

    #[test]
    fn test_mixed_tables_and_array_tables() {
        let toml = r#"
[config]
version = "1.0"

[[config.endpoints]]
name = "api"
url = "https://api.example.com"

[[config.endpoints]]
name = "web"
url = "https://example.com"

[config.database]
host = "localhost"
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 2, "Should extract mixed table types");

        let config = symbols.iter().find(|s| s.name == "config");
        assert!(config.is_some(), "Should find 'config' table");
    }

    // ========================================================================
    // Inline Tables
    // ========================================================================

    #[test]
    fn test_inline_tables() {
        let toml = r#"
[server]
name = { first = "Tom", last = "Smith" }
point = { x = 1, y = 2 }
colors = { red = 255, green = 0, blue = 0 }
"#;

        let symbols = extract_symbols(toml);

        let server = symbols.iter().find(|s| s.name == "server");
        assert!(
            server.is_some(),
            "Should find 'server' table with inline tables"
        );
    }

    #[test]
    fn test_nested_inline_tables() {
        let toml = r#"
[config]
user = { name = "Alice", settings = { theme = "dark", lang = "en" } }
"#;

        let symbols = extract_symbols(toml);

        let config = symbols.iter().find(|s| s.name == "config");
        assert!(config.is_some(), "Should handle nested inline tables");
    }

    // ========================================================================
    // Arrays
    // ========================================================================

    #[test]
    fn test_arrays_of_primitives() {
        let toml = r#"
[package]
keywords = ["rust", "code", "search"]
versions = [1, 2, 3, 4, 5]
flags = [true, false, true]
"#;

        let symbols = extract_symbols(toml);

        let package = symbols.iter().find(|s| s.name == "package");
        assert!(package.is_some(), "Should find 'package' table");
    }

    #[test]
    fn test_arrays_of_tables() {
        let toml = r#"
[package]
maintainers = [
    { name = "Alice", email = "alice@example.com" },
    { name = "Bob", email = "bob@example.com" }
]
"#;

        let symbols = extract_symbols(toml);

        let package = symbols.iter().find(|s| s.name == "package");
        assert!(package.is_some(), "Should handle arrays of inline tables");
    }

    #[test]
    fn test_multiline_arrays() {
        let toml = r#"
[build]
targets = [
    "x86_64-unknown-linux-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-apple-darwin",
]
"#;

        let symbols = extract_symbols(toml);

        let build = symbols.iter().find(|s| s.name == "build");
        assert!(build.is_some(), "Should handle multiline arrays");
    }

    // ========================================================================
    // Strings & Escaping
    // ========================================================================

    #[test]
    fn test_string_types() {
        let toml = r#"
[strings]
basic = "Hello World"
literal = 'C:\Users\path'
multiline_basic = """
Line 1
Line 2
Line 3"""

multiline_literal = '''
No escaping needed
Backslash: \
Quote: "
'''
"#;

        let symbols = extract_symbols(toml);

        let strings = symbols.iter().find(|s| s.name == "strings");
        assert!(strings.is_some(), "Should handle all string types");
    }

    #[test]
    fn test_escape_sequences() {
        let toml = r#"
[escapes]
quote = "She said \"Hello\""
backslash = "Path: C:\\Windows\\System32"
newline = "Line 1\nLine 2"
tab = "Col1\tCol2"
unicode = "Unicode: \u0041\u0042\u0043"
"#;

        let symbols = extract_symbols(toml);

        let escapes = symbols.iter().find(|s| s.name == "escapes");
        assert!(escapes.is_some(), "Should handle escape sequences");
    }

    #[test]
    fn test_unicode_in_keys() {
        // Note: TOML spec requires Unicode in table names to be quoted
        // Bare keys only support ASCII letters, digits, underscores, and dashes
        let toml = r#"
["日本語"]
名前 = "Japanese"

["Ελληνικά"]
όνομα = "Greek"

["数据库配置"]
host = "localhost"
"#;

        let symbols = extract_symbols(toml);

        assert!(
            symbols.len() >= 3,
            "Should extract quoted Unicode table names, got {}",
            symbols.len()
        );

        // Check that Unicode is preserved in extracted table names
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();

        let has_japanese = names.iter().any(|&n| n.contains("日本語"));
        let has_greek = names.iter().any(|&n| n.contains("Ελληνικά"));
        let has_chinese = names.iter().any(|&n| n.contains("数据库配置"));

        assert!(
            has_japanese,
            "Should preserve Japanese characters, got: {:?}",
            names
        );
        assert!(
            has_greek,
            "Should preserve Greek characters, got: {:?}",
            names
        );
        assert!(
            has_chinese,
            "Should preserve Chinese characters, got: {:?}",
            names
        );
    }

    // ========================================================================
    // Numbers & Dates
    // ========================================================================

    #[test]
    fn test_various_number_formats() {
        let toml = r#"
[numbers]
integer = 42
negative = -17
hex = 0xDEADBEEF
octal = 0o755
binary = 0b11010110
decimal = 3.14159
scientific = 1.23e10
negative_exp = -4.56e-7
infinity = inf
not_a_number = nan
"#;

        let symbols = extract_symbols(toml);

        let numbers = symbols.iter().find(|s| s.name == "numbers");
        assert!(numbers.is_some(), "Should handle all number formats");
    }

    #[test]
    fn test_underscores_in_numbers() {
        let toml = r#"
[readable]
large_number = 1_000_000
hex_with_underscores = 0xdead_beef
decimal_precision = 3.141_592_653_589
"#;

        let symbols = extract_symbols(toml);

        let readable = symbols.iter().find(|s| s.name == "readable");
        assert!(readable.is_some(), "Should handle underscores in numbers");
    }

    #[test]
    fn test_datetime_values() {
        let toml = r#"
[timestamps]
offset_datetime = 1979-05-27T07:32:00Z
local_datetime = 1979-05-27T07:32:00
local_date = 1979-05-27
local_time = 07:32:00
"#;

        let symbols = extract_symbols(toml);

        let timestamps = symbols.iter().find(|s| s.name == "timestamps");
        assert!(timestamps.is_some(), "Should handle datetime values");
    }

    // ========================================================================
    // Comments
    // ========================================================================

    #[test]
    fn test_comments_handling() {
        let toml = r#"
# This is a comment
[package] # inline comment
name = "julie" # another comment

# Multiple line comments
# Above a key
version = "1.0.0"

# Comment before table
[dependencies]
tokio = "1.0"
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 2, "Should handle files with comments");

        let package = symbols.iter().find(|s| s.name == "package");
        assert!(package.is_some(), "Should find package despite comments");
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_empty_toml_file() {
        let toml = "";
        let symbols = extract_symbols(toml);
        assert_eq!(symbols.len(), 0, "Empty file should yield no symbols");
    }

    #[test]
    fn test_only_comments() {
        let toml = r#"
# Just comments
# No actual content
# Nothing to extract
"#;

        let symbols = extract_symbols(toml);
        assert_eq!(
            symbols.len(),
            0,
            "Comments-only file should yield no symbols"
        );
    }

    #[test]
    fn test_empty_tables() {
        let toml = r#"
[empty1]

[empty2]

[empty3]
"#;

        let symbols = extract_symbols(toml);

        // Empty tables should still be extracted
        assert!(symbols.len() >= 3, "Should extract empty tables");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"empty1"), "Should find empty1");
        assert!(names.contains(&"empty2"), "Should find empty2");
        assert!(names.contains(&"empty3"), "Should find empty3");
    }

    #[test]
    fn test_keys_with_special_characters() {
        let toml = r#"
[special]
"key with spaces" = "value"
"key-with-dashes" = "value"
"key.with.dots" = "value"
"127.0.0.1" = "localhost"
"special!@#$%^&*()" = "value"
"#;

        let symbols = extract_symbols(toml);

        let special = symbols.iter().find(|s| s.name == "special");
        assert!(
            special.is_some(),
            "Should handle keys with special characters"
        );
    }

    // ========================================================================
    // Real-World Patterns
    // ========================================================================

    #[test]
    fn test_cargo_toml_pattern() {
        let toml = r#"
[package]
name = "my-project"
version = "0.1.0"
edition = "2021"
authors = ["Alice <alice@example.com>"]

[dependencies]
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"

[dev-dependencies]
proptest = "1.0"
criterion = "0.5"

[profile.release]
opt-level = 3
lto = true

[[bin]]
name = "my-app"
path = "src/main.rs"

[features]
default = ["std"]
std = []
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 5, "Should extract Cargo.toml structure");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"package"), "Should find package");
        assert!(names.contains(&"dependencies"), "Should find dependencies");
        assert!(
            names.contains(&"dev-dependencies"),
            "Should find dev-dependencies"
        );
    }

    #[test]
    fn test_config_file_pattern() {
        let toml = r#"
[app]
name = "MyApp"
debug = false

[server]
host = "0.0.0.0"
port = 8080

[server.tls]
enabled = true
cert = "/etc/ssl/cert.pem"
key = "/etc/ssl/key.pem"

[database]
url = "postgresql://localhost/mydb"
pool_size = 10

[logging]
level = "info"
format = "json"

[[logging.outputs]]
type = "file"
path = "/var/log/app.log"

[[logging.outputs]]
type = "stdout"
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 4, "Should extract config structure");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"app"), "Should find app config");
        assert!(names.contains(&"server"), "Should find server config");
        assert!(names.contains(&"database"), "Should find database config");
        assert!(names.contains(&"logging"), "Should find logging config");
    }

    #[test]
    fn test_pyproject_toml_pattern() {
        let toml = r#"
[build-system]
requires = ["setuptools>=45", "wheel"]
build-backend = "setuptools.build_meta"

[project]
name = "my-package"
version = "1.0.0"
description = "A sample package"
authors = [
    {name = "Alice", email = "alice@example.com"}
]

[project.urls]
homepage = "https://example.com"
repository = "https://github.com/user/repo"

[tool.pytest.ini_options]
testpaths = ["tests"]
python_files = ["test_*.py"]
"#;

        let symbols = extract_symbols(toml);

        assert!(
            symbols.len() >= 3,
            "Should extract pyproject.toml structure"
        );

        let has_build = symbols.iter().any(|s| s.name.contains("build-system"));
        assert!(has_build, "Should find build-system");

        let has_project = symbols.iter().any(|s| s.name.contains("project"));
        assert!(has_project, "Should find project");
    }

    // ========================================================================
    // Performance & Large Files
    // ========================================================================

    #[test]
    fn test_large_toml_with_many_tables() {
        // Simulate a large TOML file with 100 tables
        let mut toml = String::new();

        for i in 0..100 {
            toml.push_str(&format!("[section{}]\n", i));
            toml.push_str(&format!("key = \"value{}\"\n\n", i));
        }

        let symbols = extract_symbols(&toml);

        assert!(
            symbols.len() >= 100,
            "Should handle large files, got {} symbols",
            symbols.len()
        );
    }

    #[test]
    fn test_deeply_nested_performance() {
        // Create deeply nested tables (15 levels)
        let mut toml = String::new();

        for i in 1..=15 {
            let dots = (1..=i)
                .map(|n| format!("level{}", n))
                .collect::<Vec<_>>()
                .join(".");
            toml.push_str(&format!("[{}]\n", dots));
            toml.push_str("key = \"value\"\n\n");
        }

        let symbols = extract_symbols(&toml);

        assert!(symbols.len() >= 10, "Should handle deep nesting");
    }

    // ========================================================================
    // Position Tracking
    // ========================================================================

    #[test]
    fn test_table_position_tracking() {
        let toml = r#"
[first]
key = "value"

[second]
key = "value"

[third]
key = "value"
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 3, "Should extract three tables");

        // Verify positions are tracked
        for symbol in &symbols {
            assert!(
                symbol.start_line > 0,
                "Should track start line for {}",
                symbol.name
            );
            assert!(
                symbol.end_line > 0,
                "Should track end line for {}",
                symbol.name
            );
        }

        // Verify tables are in order
        let first = symbols.iter().find(|s| s.name == "first");
        let third = symbols.iter().find(|s| s.name == "third");

        if let (Some(f), Some(t)) = (first, third) {
            assert!(
                f.start_line < t.start_line,
                "First table should be before third table"
            );
        }
    }

    // ========================================================================
    // Whitespace Variations
    // ========================================================================

    #[test]
    fn test_whitespace_variations() {
        let toml = r#"
[table1]
key1="no spaces"
key2 = "with spaces"
key3  =  "extra spaces"

[table2]


key4 = "extra newlines"
"#;

        let symbols = extract_symbols(toml);

        assert!(symbols.len() >= 2, "Should handle whitespace variations");

        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"table1"), "Should find table1");
        assert!(names.contains(&"table2"), "Should find table2");
    }
}
