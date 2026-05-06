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
  animation-name: spin;
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
        "animation-name: spin should reference the local keyframes rule, got: {:?}",
        relationships
    );
}

#[test]
fn css_relationships_do_not_link_animation_shorthand_var_function_name() {
    let css = r#"
:root {
  --anim: spin;
}

.card {
  animation: var(--anim) 1s linear;
}

@keyframes var {
  from { opacity: 0; }
  to { opacity: 1; }
}
"#;

    let (symbols, relationships) = extract_symbols_and_relationships(css);
    let var_keyframes = symbols
        .iter()
        .find(|symbol| symbol.name == "@keyframes var")
        .expect("keyframes rule should be extracted");

    assert!(
        relationships.iter().all(|relationship| {
            !(relationship.kind == RelationshipKind::References
                && relationship.to_symbol_id == var_keyframes.id)
        }),
        "animation shorthand using var() must not create a keyframes relationship to @keyframes var, got: {:?}",
        relationships
    );
}

#[test]
fn css_relationships_ignore_animation_name_inside_comments() {
    let css = r#"
.card {
  /* animation-name: var; */
  color: red;
}

@keyframes var {
  from { opacity: 0; }
  to { opacity: 1; }
}
"#;

    let (symbols, relationships) = extract_symbols_and_relationships(css);
    let var_keyframes = symbols
        .iter()
        .find(|symbol| symbol.name == "@keyframes var")
        .expect("keyframes rule should be extracted");

    assert!(
        relationships.iter().all(|relationship| {
            !(relationship.kind == RelationshipKind::References
                && relationship.to_symbol_id == var_keyframes.id)
        }),
        "commented animation-name must not create keyframes relationships, got: {:?}",
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
