//! Invariant test: every relationship emitted by a data-format extractor must
//! carry a `to_symbol_id` that refers to a real symbol present in the same
//! `ExtractionResults`. Unresolvable targets must not produce an edge at all.
//!
//! Formats covered:
//!   - YAML  : alias *foo resolves to anchor &foo symbol
//!   - Markdown : [link](#anchor) resolves to heading symbol
//!   - JSON  : $ref relationships not yet emitted; see capability_gaps
//!   - TOML  : inter-key relationships not yet emitted; see capability_gaps

use crate::base::{Relationship, Symbol};
use crate::markdown::MarkdownExtractor;
use crate::yaml::YamlExtractor;
use std::collections::HashSet;
use std::path::PathBuf;
use tree_sitter::Parser;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_yaml_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let workspace_root = PathBuf::from("/tmp/test");
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_yaml::LANGUAGE.into())
        .expect("Error loading YAML grammar");
    let tree = parser.parse(code, None).expect("Failed to parse YAML");
    let mut extractor = YamlExtractor::new(
        "yaml".to_string(),
        "test.yaml".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);
    (symbols, relationships)
}

fn extract_markdown_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
    let workspace_root = PathBuf::from("/tmp/test");
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_md::LANGUAGE.into())
        .expect("Error loading Markdown grammar");
    let tree = parser.parse(code, None).expect("Failed to parse Markdown");
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

/// Assert that every relationship's `to_symbol_id` is the `id` of some symbol
/// in `symbols`. An empty relationships slice causes this helper to fail with
/// a clear message so the caller's fixture cannot silently become vacuous.
fn assert_non_vacuous_and_targets_are_real(
    symbols: &[Symbol],
    relationships: &[Relationship],
    format: &str,
) {
    assert!(
        !relationships.is_empty(),
        "{format}: fixture produced no relationships — the invariant assertion would be vacuously \
         true; fix the fixture so it emits at least one edge"
    );

    let symbol_ids: HashSet<&str> = symbols.iter().map(|s| s.id.as_str()).collect();

    for rel in relationships {
        assert!(
            symbol_ids.contains(rel.to_symbol_id.as_str()),
            "{format}: relationship to_symbol_id '{}' is not a real symbol ID in the same \
             extraction results. Valid IDs: {:?}. Relationship: {:?}",
            rel.to_symbol_id,
            symbol_ids,
            rel,
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant test
// ---------------------------------------------------------------------------

#[test]
fn test_data_format_relationships_have_exact_targets_or_no_edge() {
    // --- YAML: alias *defaults resolves to anchor &defaults symbol ---
    // The contract: if the alias target cannot be resolved to a symbol in the
    // same results, the extractor must NOT emit the edge.
    {
        // `server` key holds an alias *defaults; `defaults` has anchor &defaults.
        let yaml = "defaults: &defaults\n  port: 80\n\nserver: *defaults\n";
        let (symbols, relationships) = extract_yaml_relationships(yaml);
        assert_non_vacuous_and_targets_are_real(&symbols, &relationships, "YAML");
    }

    // --- Markdown: [link](#anchor) resolves to the heading symbol ---
    // The contract: internal heading links must resolve to a real heading
    // symbol extracted in the same pass; unresolvable links produce no edge.
    {
        let markdown = "# Intro\n\n## Usage\n\nSee [the intro](#intro) before continuing.\n";
        let (symbols, relationships) = extract_markdown_relationships(markdown);
        assert_non_vacuous_and_targets_are_real(&symbols, &relationships, "Markdown");
    }

    // JSON: $ref resolution relationships not yet emitted by the JSON extractor;
    // see capability_gaps for the planned implementation.

    // TOML: inter-key reference relationships not yet emitted by the TOML extractor;
    // see capability_gaps for the planned implementation.
}
