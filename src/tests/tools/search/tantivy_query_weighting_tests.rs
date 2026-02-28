//! Tests for weighted query-builder term groups.

use tantivy::collector::Count;
use tantivy::schema::{Field, STRING, Schema, TEXT};
use tantivy::{Index, doc};

#[derive(Debug, Clone, Copy)]
struct TestFields {
    name: Field,
    signature: Field,
    doc_comment: Field,
    code_body: Field,
    doc_type: Field,
    language: Field,
    kind: Field,
}

fn make_test_fields() -> TestFields {
    let mut schema_builder = Schema::builder();
    let name = schema_builder.add_text_field("name", TEXT);
    let signature = schema_builder.add_text_field("signature", TEXT);
    let doc_comment = schema_builder.add_text_field("doc_comment", TEXT);
    let code_body = schema_builder.add_text_field("code_body", TEXT);
    let doc_type = schema_builder.add_text_field("doc_type", STRING);
    let language = schema_builder.add_text_field("language", STRING);
    let kind = schema_builder.add_text_field("kind", STRING);
    let _schema = schema_builder.build();

    TestFields {
        name,
        signature,
        doc_comment,
        code_body,
        doc_type,
        language,
        kind,
    }
}

fn assert_debug_contains(query_debug: &str, needle: &str) {
    assert!(
        query_debug.contains(needle),
        "Expected query debug output to contain '{needle}', got: {query_debug}"
    );
}

fn positions(haystack: &str, needle: &str) -> Vec<usize> {
    let enforce_word_boundaries = needle
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_');

    if !enforce_word_boundaries {
        return haystack.match_indices(needle).map(|(idx, _)| idx).collect();
    }

    haystack
        .match_indices(needle)
        .filter_map(|(idx, _)| {
            let start_ok = idx == 0
                || !haystack[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            let end_idx = idx + needle.len();
            let end_ok = end_idx == haystack.len()
                || !haystack[end_idx..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');

            if start_ok && end_ok { Some(idx) } else { None }
        })
        .collect()
}

fn has_association_within_distance(
    query_debug: &str,
    left: &str,
    right: &str,
    max_distance: usize,
) -> bool {
    let left_positions = positions(query_debug, left);
    let right_positions = positions(query_debug, right);

    left_positions.iter().any(|left_pos| {
        right_positions
            .iter()
            .any(|right_pos| left_pos.abs_diff(*right_pos) <= max_distance)
    })
}

fn assert_associated_any(
    query_debug: &str,
    left: &str,
    right_candidates: &[&str],
    max_distance: usize,
) {
    assert_debug_contains(query_debug, left);

    let associated = right_candidates.iter().any(|right| {
        query_debug.contains(right)
            && has_association_within_distance(query_debug, left, right, max_distance)
    });

    assert!(
        associated,
        "Expected '{left}' to be associated with one of {:?} (within {} chars), got: {query_debug}",
        right_candidates, max_distance
    );
}

fn assert_not_associated_any(
    query_debug: &str,
    left: &str,
    wrong_candidates: &[&str],
    max_distance: usize,
) {
    assert_debug_contains(query_debug, left);

    for wrong in wrong_candidates {
        assert!(
            !has_association_within_distance(query_debug, left, wrong, max_distance),
            "Expected '{left}' to not be associated with '{wrong}' (within {} chars), got: {query_debug}",
            max_distance
        );
    }
}

#[test]
fn test_weighted_symbol_query_includes_original_alias_and_normalized_groups() {
    let fields = make_test_fields();

    let original_terms = vec!["workspace".to_string()];
    let alias_terms = vec!["router".to_string()];
    let normalized_terms = vec!["rout".to_string()];

    let query = crate::search::query::build_symbol_query_weighted(
        &original_terms,
        &alias_terms,
        &normalized_terms,
        fields.name,
        fields.signature,
        fields.doc_comment,
        fields.code_body,
        fields.doc_type,
        fields.language,
        fields.kind,
        None,
        None,
        true,
    );

    let query_debug = format!("{query:?}");

    assert_debug_contains(&query_debug, "workspace");
    assert_debug_contains(&query_debug, "router");
    assert_debug_contains(&query_debug, "rout");

    // Verify each group maps to the intended weight in debug output.
    assert_associated_any(&query_debug, "workspace", &["boost=5"], 120);
    assert_associated_any(&query_debug, "router", &["boost=3.5"], 120);
    assert_associated_any(&query_debug, "rout", &["boost=2.5"], 120);

    // Guard against swapped/incorrect associations between alias/original/normalized groups.
    assert_not_associated_any(&query_debug, "workspace", &["boost=3.5", "boost=2.5"], 120);
    assert_not_associated_any(&query_debug, "router", &["boost=5", "boost=2.5"], 120);
    assert_not_associated_any(&query_debug, "rout", &["boost=5", "boost=3.5"], 120);
}

#[test]
fn test_weighted_symbol_query_preserves_doc_type_and_filters() {
    let fields = make_test_fields();

    let original_terms = vec!["workspace".to_string()];
    let alias_terms = vec!["router".to_string()];
    let normalized_terms = vec!["rout".to_string()];

    let query = crate::search::query::build_symbol_query_weighted(
        &original_terms,
        &alias_terms,
        &normalized_terms,
        fields.name,
        fields.signature,
        fields.doc_comment,
        fields.code_body,
        fields.doc_type,
        fields.language,
        fields.kind,
        Some("rust"),
        Some("function"),
        true,
    );

    let query_debug = format!("{query:?}");

    assert_debug_contains(&query_debug, "symbol");
    assert_debug_contains(&query_debug, "rust");
    assert_debug_contains(&query_debug, "function");

    // In the weighted builder, each filter must remain required (not optional).
    assert_associated_any(&query_debug, "symbol", &["Must"], 120);
    assert_associated_any(&query_debug, "rust", &["Must"], 120);
    assert_associated_any(&query_debug, "function", &["Must"], 120);
}

#[test]
fn test_weighted_symbol_query_and_mode_keeps_expansions_optional() {
    let mut schema_builder = Schema::builder();
    let name = schema_builder.add_text_field("name", TEXT);
    let signature = schema_builder.add_text_field("signature", TEXT);
    let doc_comment = schema_builder.add_text_field("doc_comment", TEXT);
    let code_body = schema_builder.add_text_field("code_body", TEXT);
    let doc_type = schema_builder.add_text_field("doc_type", STRING);
    let language = schema_builder.add_text_field("language", STRING);
    let kind = schema_builder.add_text_field("kind", STRING);
    let schema = schema_builder.build();

    let index = Index::create_in_ram(schema);
    let mut writer = index.writer(15_000_000).unwrap();
    writer
        .add_document(doc!(
            name => "workspace",
            signature => "",
            doc_comment => "",
            code_body => "",
            doc_type => "symbol",
            language => "rust",
            kind => "function",
        ))
        .unwrap();
    writer.commit().unwrap();

    let reader = index.reader().unwrap();
    reader.reload().unwrap();
    let searcher = reader.searcher();

    let query = crate::search::query::build_symbol_query_weighted(
        &["workspace".to_string()],
        &["router".to_string()],
        &["rout".to_string()],
        name,
        signature,
        doc_comment,
        code_body,
        doc_type,
        language,
        kind,
        None,
        None,
        true,
    );

    let matches = searcher.search(&query, &Count).unwrap();
    assert_eq!(
        matches, 1,
        "AND mode must require original terms only; alias/normalized groups are optional boosts"
    );
}

#[test]
fn test_weighted_content_query_and_mode_keeps_expansions_optional() {
    let mut schema_builder = Schema::builder();
    let content = schema_builder.add_text_field("content", TEXT);
    let doc_type = schema_builder.add_text_field("doc_type", STRING);
    let language = schema_builder.add_text_field("language", STRING);
    let schema = schema_builder.build();

    let index = Index::create_in_ram(schema);
    let mut writer = index.writer(15_000_000).unwrap();
    writer
        .add_document(doc!(
            content => "workspace",
            doc_type => "file",
            language => "rust",
        ))
        .unwrap();
    writer.commit().unwrap();

    let reader = index.reader().unwrap();
    reader.reload().unwrap();
    let searcher = reader.searcher();

    let query = crate::search::query::build_content_query_weighted(
        &["workspace".to_string()],
        &["router".to_string()],
        &["rout".to_string()],
        content,
        doc_type,
        language,
        None,
        true,
    );

    let matches = searcher.search(&query, &Count).unwrap();
    assert_eq!(
        matches, 1,
        "AND mode must require original content terms only; alias/normalized groups are optional boosts"
    );
}
