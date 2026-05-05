use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::regex::RegexExtractor;
use crate::tests::helpers::init_parser;
use std::path::PathBuf;

fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let workspace_root = PathBuf::from("/tmp/test");
    let tree = init_parser(code, "regex");
    let mut extractor = RegexExtractor::new(
        "regex".to_string(),
        "test.regex".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

#[test]
fn regex_relationships_resolve_named_backrefs() {
    let regex_code = r#"(?<word>\w+)-\k<word>"#;

    let (symbols, relationships) = extract_symbols_and_relationships(regex_code);

    let pattern = symbols
        .iter()
        .find(|symbol| symbol.kind == SymbolKind::Variable)
        .expect("top-level regex pattern should be extracted");
    let word_group = symbols
        .iter()
        .find(|symbol| {
            symbol
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("named"))
                .and_then(serde_json::Value::as_str)
                == Some("word")
        })
        .expect("named capture group should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.from_symbol_id == pattern.id
                && relationship.to_symbol_id == word_group.id
        }),
        "named backreference should create a pattern -> capture group References edge, got: {:?}",
        relationships
    );
}

#[test]
fn regex_relationships_resolve_numeric_backrefs() {
    let regex_code = r#"(foo)-(bar)-\2-\1"#;

    let (symbols, relationships) = extract_symbols_and_relationships(regex_code);

    let pattern = symbols
        .iter()
        .find(|symbol| symbol.kind == SymbolKind::Variable)
        .expect("top-level regex pattern should be extracted");
    let first_group = symbols
        .iter()
        .find(|symbol| {
            symbol
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("captureIndex"))
                .and_then(serde_json::Value::as_u64)
                == Some(1)
        })
        .expect("first referenced capture group should be extracted");
    let second_group = symbols
        .iter()
        .find(|symbol| {
            symbol
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("captureIndex"))
                .and_then(serde_json::Value::as_u64)
                == Some(2)
        })
        .expect("second referenced capture group should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.from_symbol_id == pattern.id
                && relationship.to_symbol_id == first_group.id
        }),
        "\\1 should reference the first capture group, got: {:?}",
        relationships
    );
    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.from_symbol_id == pattern.id
                && relationship.to_symbol_id == second_group.id
        }),
        "\\2 should reference the second capture group, got: {:?}",
        relationships
    );
}
