use std::collections::HashMap;

use julie_core::database::SymbolDatabase;
use julie_core::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use julie_extractors::RelationshipKind;
use julie_extractors::base::StructuralFact;
use julie_test_support::db::{file_info_builder, relationship_builder, symbol_builder};
use tempfile::TempDir;

#[test]
fn merged_relationship_and_fact_text_reserves_separator_byte() {
    let temp = TempDir::new().unwrap();
    let mut db = SymbolDatabase::new(&temp.path().join("merge-cap.db")).unwrap();
    let file = file_info_builder("src/capped.ts")
        .language("typescript")
        .build();
    let focal = symbol_builder("sym-capped", "boundedMergeCanary", "src/capped.ts").build();
    let partner_name = "é".repeat(255);
    let partner = symbol_builder("sym-partner", &partner_name, "src/capped.ts").build();
    let relationship = relationship_builder("rel-capped", &focal.id, &partner.id)
        .kind(RelationshipKind::Calls)
        .file_path("src/capped.ts")
        .build();
    let fact = StructuralFact {
        id: "fact-capped".to_string(),
        file_path: "src/capped.ts".to_string(),
        language: "typescript".to_string(),
        pattern_id: "http.client_request.v1".to_string(),
        capture_name: "request".to_string(),
        node_kind: "call_expression".to_string(),
        containing_symbol_id: Some(focal.id.clone()),
        start_line: 5,
        start_column: 0,
        end_line: 5,
        end_column: 20,
        start_byte: 50,
        end_byte: 70,
        confidence: 1.0,
        metadata: Some(HashMap::from([(
            "target_path".to_string(),
            serde_json::json!("/utf8/canary"),
        )])),
    };
    let symbols = vec![focal, partner];
    let write_set = CanonicalWriteSet {
        files: std::slice::from_ref(&file),
        symbols: &symbols,
        relationships: std::slice::from_ref(&relationship),
        structural_facts: std::slice::from_ref(&fact),
        ..Default::default()
    };
    db.incremental_update_atomic_with_metadata(
        std::slice::from_ref(&file.path),
        &write_set,
        "merge-cap-test",
        AtomicPersistenceMetadata::default(),
    )
    .unwrap();

    let merged = super::load_enriched_relationship_text(&db, std::slice::from_ref(&symbols[0].id))
        .unwrap()
        .remove(&symbols[0].id)
        .unwrap();

    assert!(merged.len() <= 512, "{}", merged.len());
    assert!(merged.is_char_boundary(merged.len()));
}
