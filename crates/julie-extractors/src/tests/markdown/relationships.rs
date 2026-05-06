use crate::base::{Relationship, RelationshipKind, Symbol};
use crate::markdown::MarkdownExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .expect("Error loading Markdown grammar");
    parser
}

fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let workspace_root = PathBuf::from("/tmp/test");
    let mut parser = init_parser();
    let tree = parser.parse(code, None).expect("Failed to parse code");
    let mut extractor = MarkdownExtractor::new(
        "markdown".to_string(),
        "test.md".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

#[test]
fn markdown_relationships_resolve_local_heading_links() {
    let markdown = r#"# Intro

## Usage

See [the intro](#intro) before continuing.
"#;

    let (symbols, relationships) = extract_symbols_and_relationships(markdown);

    let intro = symbols
        .iter()
        .find(|symbol| symbol.name == "Intro")
        .expect("intro heading should be extracted");
    let usage = symbols
        .iter()
        .find(|symbol| symbol.name == "Usage")
        .expect("usage heading should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.from_symbol_id == usage.id
                && relationship.to_symbol_id == intro.id
        }),
        "local markdown link should reference the target heading, got: {:?}",
        relationships
    );
}

#[test]
fn markdown_relationships_do_not_emit_self_reference_edges() {
    let markdown = r#"# Self

[self](#self)
"#;

    let (symbols, relationships) = extract_symbols_and_relationships(markdown);
    let self_heading = symbols
        .iter()
        .find(|symbol| symbol.name == "Self")
        .expect("self heading should be extracted");

    assert!(
        !relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.from_symbol_id == self_heading.id
                && relationship.to_symbol_id == self_heading.id
        }),
        "self links should not create self edges, got: {:?}",
        relationships
    );
}

#[test]
fn markdown_relationships_duplicate_slugs_use_first_heading_deterministically() {
    let markdown = r#"# Overview

## First

# Overview

## Later

See [overview](#overview) before moving on.
"#;

    let (symbols, relationships) = extract_symbols_and_relationships(markdown);
    let mut overview_headings: Vec<&Symbol> = symbols
        .iter()
        .filter(|symbol| symbol.name == "Overview")
        .collect();
    overview_headings.sort_by_key(|symbol| symbol.start_line);

    assert_eq!(
        overview_headings.len(),
        2,
        "expected two duplicate overview headings, got: {:?}",
        symbols
    );

    let first_overview = overview_headings[0];
    let later_section = symbols
        .iter()
        .find(|symbol| symbol.name == "Later")
        .expect("later section heading should be extracted");

    assert!(
        relationships.iter().any(|relationship| {
            relationship.kind == RelationshipKind::References
                && relationship.from_symbol_id == later_section.id
                && relationship.to_symbol_id == first_overview.id
        }),
        "duplicate heading links should resolve to first heading deterministically, got: {:?}",
        relationships
    );
}
