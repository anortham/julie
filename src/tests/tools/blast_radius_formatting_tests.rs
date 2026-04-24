use crate::extractors::{RelationshipKind, Symbol, SymbolKind};
use crate::tools::impact::LikelyTests;
use crate::tools::impact::formatting::{BlastRadiusHeader, format_blast_radius};
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
        annotations: Vec::new(),
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

    let likely_tests = LikelyTests {
        likely_test_paths: vec!["tests/request_tests.rs".to_string()],
        related_test_symbols: vec!["test_handle_request".to_string()],
        likely_test_paths_total: 1,
        related_test_symbols_total: 1,
    };
    let text = format_blast_radius(
        &seed_context,
        &impacts,
        &likely_tests,
        &seed_context.deleted_files,
        Some("br_123"),
        SpilloverFormat::Readable,
        BlastRadiusHeader::default(),
    );

    assert!(text.contains("Blast radius from 1 changed file, 1 seed symbol"));
    assert!(text.contains("High impact"));
    assert!(text.contains("handle_request  src/api.rs:20"));
    assert!(text.contains("Likely tests"));
    assert!(text.contains("tests/request_tests.rs"));
    assert!(text.contains("Related test symbols"));
    assert!(text.contains("test_handle_request"));
    assert!(text.contains("Deleted files"));
    assert!(text.contains("src/legacy.rs"));
    assert!(text.contains("More available: spillover_handle=br_123"));
}

#[test]
fn test_format_header_includes_revision_range_when_set() {
    let seed_context = SeedContext {
        seed_symbols: vec![],
        changed_files: vec!["src/a.rs".to_string(), "src/b.rs".to_string()],
        deleted_files: vec![],
    };
    let likely_tests = LikelyTests::default();
    let text = format_blast_radius(
        &seed_context,
        &[],
        &likely_tests,
        &[],
        None,
        SpilloverFormat::Compact,
        BlastRadiusHeader {
            revision_range: Some((42, 48)),
        },
    );

    assert!(
        text.contains("revs 42..48"),
        "header should echo revision range: {text}"
    );
    assert!(
        text.contains("2 changed files"),
        "header should still include file count: {text}"
    );
}

#[test]
fn test_likely_tests_overflow_marker_appears_when_truncated() {
    let seed_context = SeedContext {
        seed_symbols: vec![make_symbol("seed", "src/lib.rs", 1)],
        changed_files: vec![],
        deleted_files: vec![],
    };
    let mut paths = Vec::new();
    for i in 0..10 {
        paths.push(format!("tests/t{i}.rs"));
    }
    let likely_tests = LikelyTests {
        likely_test_paths: paths,
        related_test_symbols: vec![],
        // Pre-truncate total of 13 → 3 overflow
        likely_test_paths_total: 13,
        related_test_symbols_total: 0,
    };
    let text = format_blast_radius(
        &seed_context,
        &[],
        &likely_tests,
        &[],
        None,
        SpilloverFormat::Compact,
        BlastRadiusHeader::default(),
    );

    assert!(
        text.contains("…and 3 more"),
        "expected overflow marker for likely tests: {text}"
    );
}

#[test]
fn test_related_test_symbols_overflow_marker_independent_of_paths() {
    let seed_context = SeedContext {
        seed_symbols: vec![make_symbol("seed", "src/lib.rs", 1)],
        changed_files: vec![],
        deleted_files: vec![],
    };
    let mut related = Vec::new();
    for i in 0..10 {
        related.push(format!("test_case_{i}"));
    }
    let likely_tests = LikelyTests {
        likely_test_paths: vec!["tests/single.rs".to_string()],
        related_test_symbols: related,
        likely_test_paths_total: 1,     // no overflow on paths
        related_test_symbols_total: 17, // 7 overflow on names
    };
    let text = format_blast_radius(
        &seed_context,
        &[],
        &likely_tests,
        &[],
        None,
        SpilloverFormat::Compact,
        BlastRadiusHeader::default(),
    );

    assert!(
        text.contains("Related test symbols"),
        "related symbols heading must be present: {text}"
    );
    assert!(
        text.contains("…and 7 more"),
        "expected overflow marker for related symbols: {text}"
    );

    // Paths collection was NOT truncated, so no "more" marker for it.
    // Since both markers share a prefix, assert on total occurrences.
    let overflow_occurrences = text.matches("…and").count();
    assert_eq!(
        overflow_occurrences, 1,
        "only the related-symbols list overflowed: {text}"
    );
}

#[test]
fn test_spillover_format_parse_strict_rejects_unknown_value() {
    use crate::tools::spillover::SpilloverFormat;

    assert!(SpilloverFormat::parse_strict("readible").is_err());
    assert!(SpilloverFormat::parse_strict("").is_err());
    assert_eq!(
        SpilloverFormat::parse_strict("readable").unwrap(),
        SpilloverFormat::Readable
    );
    assert_eq!(
        SpilloverFormat::parse_strict("Compact").unwrap(),
        SpilloverFormat::Compact
    );
}
