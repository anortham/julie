use crate::extractors::{RelationshipKind, Symbol, SymbolKind};
use crate::tools::impact::formatting::format_blast_radius;
use crate::tools::impact::ranking::RankedImpact;
use crate::tools::impact::seed::SeedContext;
use crate::tools::spillover::SpilloverFormat;

fn make_symbol(name: &str, file_path: &str, line: u32) -> Symbol {
    Symbol {
        id: format!("{}_id", name),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        end_line: line + 1,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 16,
        parent_id: None,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: None,
        content_type: None,
    }
}

#[test]
fn test_format_blast_radius_includes_sections_and_overflow_marker() {
    let seed_context = SeedContext {
        seed_symbols: vec![make_symbol("run_pipeline", "src/worker.rs", 10)],
        changed_files: vec!["src/worker.rs".to_string()],
        deleted_files: vec!["src/legacy.rs".to_string()],
    };
    let impacts = vec![RankedImpact {
        symbol: make_symbol("handle_request", "src/api.rs", 20),
        distance: 1,
        relationship_kind: RelationshipKind::Calls,
        reference_score: 4.0,
        why: "direct caller, 1 hop, centrality=medium".to_string(),
    }];

    let text = format_blast_radius(
        &seed_context,
        &impacts,
        &["tests/request_tests.rs".to_string()],
        &seed_context.deleted_files,
        Some("br_123"),
        SpilloverFormat::Readable,
    );

    assert!(text.contains("Blast radius from 1 changed file, 1 seed symbol"));
    assert!(text.contains("High impact"));
    assert!(text.contains("handle_request  src/api.rs:20"));
    assert!(text.contains("Likely tests"));
    assert!(text.contains("tests/request_tests.rs"));
    assert!(text.contains("Deleted files"));
    assert!(text.contains("src/legacy.rs"));
    assert!(text.contains("More available: spillover_handle=br_123"));
}
