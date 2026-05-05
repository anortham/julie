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
