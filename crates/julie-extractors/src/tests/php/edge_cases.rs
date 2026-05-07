//! Edge Case Tests for PHP Extractor
//!
//! Tests for handling edge cases and special PHP features:
//! - Malformed syntax (graceful error handling)
//! - Unicode characters in identifiers
//! - Heredoc and Nowdoc syntax
//! - Dynamic features (magic methods, variable functions)

use crate::base::{Symbol, SymbolKind, Visibility};
use crate::php::PhpExtractor;
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
    let mut extractor = PhpExtractor::new(
        "php".to_string(),
        "test.php".to_string(),
        code.to_string(),
        &workspace_root,
    );

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

#[test]
fn test_php_constructor_property_promotion_emits_property_symbols() {
    let php_code = r#"<?php
class UserProfile {
    public function __construct(
        public string $name,
        private readonly Foo $foo,
        protected ?int $id,
        int $notPromoted
    ) {}
}
"#;

    let symbols = extract_symbols(php_code);

    let class_symbol = symbols
        .iter()
        .find(|symbol| symbol.name == "UserProfile" && symbol.kind == SymbolKind::Class)
        .expect("expected class symbol");

    let constructor = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "__construct"
                && symbol.kind == SymbolKind::Constructor
                && symbol.parent_id.as_deref() == Some(class_symbol.id.as_str())
        })
        .expect("expected constructor symbol under class");
    assert!(
        constructor
            .signature
            .as_deref()
            .unwrap_or_default()
            .contains("public function __construct"),
        "constructor signature should be extracted"
    );

    let promoted_name = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "name"
                && symbol.kind == SymbolKind::Property
                && symbol.parent_id.as_deref() == Some(class_symbol.id.as_str())
        })
        .expect("expected promoted property for $name");
    assert_eq!(promoted_name.visibility, Some(Visibility::Public));
    assert!(
        promoted_name
            .signature
            .as_deref()
            .unwrap_or_default()
            .contains("public string $name")
    );
    assert_eq!(
        promoted_name
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("propertyType"))
            .and_then(|property_type| property_type.as_str()),
        Some("string")
    );

    let promoted_foo = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "foo"
                && symbol.kind == SymbolKind::Property
                && symbol.parent_id.as_deref() == Some(class_symbol.id.as_str())
        })
        .expect("expected promoted property for $foo");
    assert_eq!(promoted_foo.visibility, Some(Visibility::Private));
    assert!(
        promoted_foo
            .signature
            .as_deref()
            .unwrap_or_default()
            .contains("private readonly Foo $foo")
    );
    assert_eq!(
        promoted_foo
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("propertyType"))
            .and_then(|property_type| property_type.as_str()),
        Some("Foo")
    );

    let promoted_id = symbols
        .iter()
        .find(|symbol| {
            symbol.name == "id"
                && symbol.kind == SymbolKind::Property
                && symbol.parent_id.as_deref() == Some(class_symbol.id.as_str())
        })
        .expect("expected promoted property for $id");
    assert_eq!(promoted_id.visibility, Some(Visibility::Protected));
    assert!(
        promoted_id
            .signature
            .as_deref()
            .unwrap_or_default()
            .contains("protected ?int $id")
    );
    assert_eq!(
        promoted_id
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("propertyType"))
            .and_then(|property_type| property_type.as_str()),
        Some("?int")
    );

    assert!(
        !symbols.iter().any(|symbol| {
            symbol.name == "notPromoted"
                && symbol.kind == SymbolKind::Property
                && symbol.parent_id.as_deref() == Some(class_symbol.id.as_str())
        }),
        "regular constructor parameters must not be emitted as properties"
    );
}
