use crate::base::{Relationship, RelationshipKind, Symbol};
use crate::css::CSSExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

pub fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_css::LANGUAGE.into())
        .expect("Error loading CSS grammar");
    parser
}

pub fn extract_symbols(code: &str) -> Vec<Symbol> {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSSExtractor::new(
        "css".to_string(),
        "test.css".to_string(),
        code.to_string(),
        &workspace_root,
    );
    extractor.extract_symbols(&tree)
}

pub fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = CSSExtractor::new(
        "css".to_string(),
        "test.css".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

#[test]
fn css_relationships_resolve_custom_properties_and_keyframes() {
    let css = r#"
:root {
  --brand: #0f766e;
}

.card {
  color: var(--brand);
  animation: spin 1s linear;
}

@keyframes spin {
  from { opacity: 0; }
  to { opacity: 1; }
}
"#;

    let (symbols, relationships) = extract_symbols_and_relationships(css);

    let brand = symbols
        .iter()
        .find(|symbol| symbol.name == "--brand")
        .expect("custom property should be extracted");
    let spin = symbols
        .iter()
        .find(|symbol| symbol.name == "@keyframes spin")
        .expect("keyframes rule should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.to_symbol_id == brand.id
        }),
        "var(--brand) should reference the local custom property, got: {:?}",
        relationships
    );
    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.to_symbol_id == spin.id
        }),
        "animation: spin should reference the local keyframes rule, got: {:?}",
        relationships
    );
}

pub mod advanced;
pub mod animations;
pub mod at_rules;
pub mod basic;
pub mod custom;
pub mod doc_comments;
pub mod identifier_extraction;
pub mod media_queries;
pub mod modern;
pub mod pseudo_elements;
pub mod responsive;
pub mod utilities;
