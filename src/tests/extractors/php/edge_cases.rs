//! Edge Case Tests for PHP Extractor
//!
//! Tests for handling edge cases and special PHP features:
//! - Malformed syntax (graceful error handling)
//! - Unicode characters in identifiers
//! - Heredoc and Nowdoc syntax
//! - Dynamic features (magic methods, variable functions)

use crate::extractors::php::PhpExtractor;
use crate::extractors::base::Symbol;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("Error loading PHP grammar");
    parser
}

// Helper function to extract symbols from PHP code
fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();

    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor =
        PhpExtractor::new("php".to_string(), "test.php".to_string(), code.to_string(), &workspace_root);

    extractor.extract_symbols(&tree)
}

#[test]
fn test_php_malformed_syntax() {
    let php_code = r#"<?php
class Test {
    public function method() {
        // Missing closing brace
        if (true) {
            echo "test";
        // Missing closing parenthesis and brace
        function broken( {
            return "broken";
        }
    }
"#;

    let symbols = extract_symbols(php_code);

    // Should handle malformed PHP gracefully
    assert!(!symbols.is_empty());
}

#[test]
fn test_php_unicode_and_special_chars() {
    let php_code = r#"<?php
class Café {
    public function método() {
        $variable = "tëst";
        $emoji = "🚀";
        return $variable . $emoji;
    }
}

function función_ñ() {
    return "español";
}
"#;

    let symbols = extract_symbols(php_code);

    // Should handle Unicode characters in identifiers
    assert!(!symbols.is_empty());
}

#[test]
fn test_php_heredoc_and_nowdoc() {
    let php_code = r#"<?php
class Template {
    public function getHeredoc(): string {
        return <<<HTML
<div class="content">
    <h1>Title</h1>
    <p>Content with "quotes" and 'apostrophes'</p>
</div>
HTML;
    }

    public function getNowdoc(): string {
        return <<<'SQL'
SELECT * FROM users
WHERE active = 1
AND name LIKE '%test%'
SQL;
    }
}
"#;

    let symbols = extract_symbols(php_code);

    // Should handle heredoc and nowdoc syntax
    assert!(!symbols.is_empty());
}

#[test]
fn test_php_dynamic_features() {
    let php_code = r#"<?php
class Dynamic {
    public function __call($method, $args) {
        return "Called: $method";
    }

    public static function __callStatic($method, $args) {
        return "Static called: $method";
    }

    public function __get($property) {
        return "Getting: $property";
    }

    public function __set($property, $value) {
        $this->$property = $value;
    }
}

function variable_function() {
    return "variable function result";
}

$func = 'variable_function';
$result = $func();
"#;

    let symbols = extract_symbols(php_code);

    // Should handle dynamic PHP features
    assert!(!symbols.is_empty());
}
