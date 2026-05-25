use crate::database::SymbolDatabase;
use crate::extractors::{
    AnnotationMarker, IdentifierKind, RelationshipKind, SymbolKind, Visibility,
};

use super::{
    file_info_builder, identifier_builder, relationship_builder, set_symbol_reference_scores,
    symbol_builder,
};

#[test]
fn test_file_info_builder_sets_stable_defaults() {
    let file = file_info_builder("src/lib.rs").build();

    assert_eq!(file.path, "src/lib.rs");
    assert_eq!(file.language, "rust");
    assert_eq!(file.hash, "hash-src/lib.rs");
    assert_eq!(file.size, 0);
    assert_eq!(file.last_modified, 1);
    assert_eq!(file.last_indexed, 1);
    assert_eq!(file.symbol_count, 1);
    assert_eq!(file.line_count, 1);
    assert_eq!(file.content, None);
}

#[test]
fn test_file_info_builder_overrides_symbol_count() {
    let file = file_info_builder("src/lib.rs").symbol_count(0).build();

    assert_eq!(file.symbol_count, 0);
}

#[test]
fn test_set_symbol_reference_scores_updates_scores() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    db.store_file_info(&file_info_builder("src/lib.rs").build())
        .unwrap();
    db.store_symbols(&[
        symbol_builder("sym-1", "run", "src/lib.rs").build(),
        symbol_builder("sym-2", "helper", "src/lib.rs").build(),
    ])
    .unwrap();

    set_symbol_reference_scores(&db, &[("sym-1", 12.5), ("sym-2", 3.25)]).unwrap();

    let scores = db.get_reference_scores(&["sym-1", "sym-2"]).unwrap();
    assert_eq!(scores.get("sym-1"), Some(&12.5));
    assert_eq!(scores.get("sym-2"), Some(&3.25));
}

#[test]
fn test_set_symbol_reference_scores_rejects_missing_symbol() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    let error = set_symbol_reference_scores(&db, &[("missing", 1.0)]).unwrap_err();

    assert!(
        error.to_string().contains("missing symbol id `missing`"),
        "unexpected error: {error}"
    );
}

#[test]
fn test_symbol_builder_overrides_metadata_and_span() {
    let symbol = symbol_builder("sym-1", "run", "src/lib.rs")
        .kind(SymbolKind::Method)
        .language("typescript")
        .span(3, 4, 5, 6)
        .bytes(30, 60)
        .signature("run(): void")
        .visibility(Visibility::Public)
        .confidence(0.8)
        .build();

    assert_eq!(symbol.id, "sym-1");
    assert_eq!(symbol.name, "run");
    assert_eq!(symbol.kind, SymbolKind::Method);
    assert_eq!(symbol.language, "typescript");
    assert_eq!(symbol.file_path, "src/lib.rs");
    assert_eq!(symbol.start_line, 3);
    assert_eq!(symbol.start_column, 4);
    assert_eq!(symbol.end_line, 5);
    assert_eq!(symbol.end_column, 6);
    assert_eq!(symbol.start_byte, 30);
    assert_eq!(symbol.end_byte, 60);
    assert_eq!(symbol.signature.as_deref(), Some("run(): void"));
    assert_eq!(symbol.visibility, Some(Visibility::Public));
    assert_eq!(symbol.confidence, Some(0.8));
}

#[test]
fn test_symbol_builder_overrides_parent_context_and_annotations() {
    let symbol = symbol_builder("child", "health", "src/controller.rs")
        .parent_id("controller")
        .code_context("health() {}")
        .annotations(vec![AnnotationMarker {
            annotation: "HttpGet".to_string(),
            annotation_key: "httpget".to_string(),
            raw_text: Some("[HttpGet]".to_string()),
            carrier: None,
        }])
        .build();

    assert_eq!(symbol.parent_id.as_deref(), Some("controller"));
    assert_eq!(symbol.code_context.as_deref(), Some("health() {}"));
    assert_eq!(symbol.annotations.len(), 1);
    assert_eq!(symbol.annotations[0].annotation_key, "httpget");
}

#[test]
fn test_relationship_and_identifier_builders_cover_common_reference_rows() {
    let relationship = relationship_builder("rel-1", "caller", "callee")
        .kind(RelationshipKind::Instantiates)
        .file_path("src/main.rs")
        .line_number(9)
        .confidence(0.7)
        .build();

    assert_eq!(relationship.id, "rel-1");
    assert_eq!(relationship.from_symbol_id, "caller");
    assert_eq!(relationship.to_symbol_id, "callee");
    assert_eq!(relationship.kind, RelationshipKind::Instantiates);
    assert_eq!(relationship.file_path, "src/main.rs");
    assert_eq!(relationship.line_number, 9);
    assert_eq!(relationship.confidence, 0.7);

    let identifier = identifier_builder("ident-1", "Callee", "src/main.rs")
        .kind(IdentifierKind::TypeUsage)
        .language("typescript")
        .containing_symbol_id("caller")
        .target_symbol_id("callee")
        .line(11)
        .column(2, 8)
        .bytes(20, 26)
        .confidence(0.9)
        .build();

    assert_eq!(identifier.id, "ident-1");
    assert_eq!(identifier.name, "Callee");
    assert_eq!(identifier.kind, IdentifierKind::TypeUsage);
    assert_eq!(identifier.language, "typescript");
    assert_eq!(identifier.file_path, "src/main.rs");
    assert_eq!(identifier.containing_symbol_id.as_deref(), Some("caller"));
    assert_eq!(identifier.target_symbol_id.as_deref(), Some("callee"));
    assert_eq!(identifier.start_line, 11);
    assert_eq!(identifier.end_line, 11);
    assert_eq!(identifier.start_column, 2);
    assert_eq!(identifier.end_column, 8);
    assert_eq!(identifier.start_byte, 20);
    assert_eq!(identifier.end_byte, 26);
    assert_eq!(identifier.confidence, 0.9);
}
