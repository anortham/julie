use crate::base::{
    ExtractionResults, Relationship, RelationshipKind, StructuredPendingRelationship, Symbol,
    SymbolKind, TypeInfo, UnresolvedTarget,
};
use crate::pipeline::extract_canonical;
use md5;
use std::collections::HashMap;
use std::path::PathBuf;

fn expected_id(file_path: &str, name: &str, start_line: u32, start_column: u32) -> String {
    let input = format!("{file_path}:{name}:{start_line}:{start_column}");
    format!("{:x}", md5::compute(input.as_bytes()))
}

#[test]
fn test_symbol_ids_hash_normalized_path_and_stored_span() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file_path = "fixtures/path_identity.rs";
    let content = r#"
pub fn first() {}

pub fn second() {}
"#;

    let results = extract_canonical(file_path, content, &workspace_root)
        .expect("canonical extraction should succeed");

    let second = results
        .symbols
        .iter()
        .find(|symbol| symbol.name == "second")
        .expect("expected second function symbol");

    assert_eq!(second.file_path, file_path);
    assert_eq!(
        second.id,
        expected_id(
            second.file_path.as_str(),
            second.name.as_str(),
            second.start_line,
            second.start_column,
        ),
        "symbol IDs should hash the normalized stored location"
    );
}

#[test]
fn test_rekey_normalized_locations_rekeys_type_map_keys() {
    let old_symbol_id = expected_id("fixtures/events.jsonl", "type", 1, 1);
    let new_symbol_id = expected_id("fixtures/events.jsonl", "type", 2, 1);

    let mut results = ExtractionResults {
        symbols: vec![Symbol {
            id: old_symbol_id.clone(),
            name: "type".to_string(),
            kind: SymbolKind::Variable,
            language: "json".to_string(),
            file_path: "fixtures/events.jsonl".to_string(),
            start_line: 2,
            start_column: 1,
            end_line: 2,
            end_column: 7,
            start_byte: 35,
            end_byte: 41,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
        }],
        relationships: Vec::new(),
        pending_relationships: Vec::new(),
        structured_pending_relationships: Vec::new(),
        identifiers: Vec::new(),
        types: HashMap::from([(
            old_symbol_id.clone(),
            TypeInfo {
                symbol_id: old_symbol_id.clone(),
                resolved_type: "string".to_string(),
                generic_params: None,
                constraints: None,
                is_inferred: false,
                language: "json".to_string(),
                metadata: None,
            },
        )]),
    };

    results.rekey_normalized_locations();

    assert!(
        !results.types.contains_key(&old_symbol_id),
        "old type key should be removed after rekey"
    );
    let type_info = results
        .types
        .get(&new_symbol_id)
        .expect("type info should be rekeyed to normalized symbol ID");
    assert_eq!(type_info.symbol_id, new_symbol_id);
}

#[test]
fn test_rekey_normalized_locations_refreshes_relationship_ids() {
    let old_from_symbol_id = expected_id("fixtures/events.jsonl", "caller", 1, 0);
    let old_to_symbol_id = expected_id("fixtures/events.jsonl", "callee", 1, 10);
    let new_from_symbol_id = expected_id("fixtures/events.jsonl", "caller", 2, 0);
    let new_to_symbol_id = expected_id("fixtures/events.jsonl", "callee", 2, 10);

    let mut results = ExtractionResults {
        symbols: vec![
            Symbol {
                id: old_from_symbol_id.clone(),
                name: "caller".to_string(),
                kind: SymbolKind::Function,
                language: "json".to_string(),
                file_path: "fixtures/events.jsonl".to_string(),
                start_line: 2,
                start_column: 0,
                end_line: 2,
                end_column: 6,
                start_byte: 20,
                end_byte: 26,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
                annotations: Vec::new(),
            },
            Symbol {
                id: old_to_symbol_id.clone(),
                name: "callee".to_string(),
                kind: SymbolKind::Function,
                language: "json".to_string(),
                file_path: "fixtures/events.jsonl".to_string(),
                start_line: 2,
                start_column: 10,
                end_line: 2,
                end_column: 16,
                start_byte: 30,
                end_byte: 36,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
                annotations: Vec::new(),
            },
        ],
        relationships: vec![Relationship {
            id: format!(
                "{}_{}_{:?}_{}",
                old_from_symbol_id,
                old_to_symbol_id,
                RelationshipKind::Calls,
                1
            ),
            from_symbol_id: old_from_symbol_id.clone(),
            to_symbol_id: old_to_symbol_id.clone(),
            kind: RelationshipKind::Calls,
            file_path: "fixtures/events.jsonl".to_string(),
            line_number: 2,
            confidence: 1.0,
            metadata: None,
        }],
        pending_relationships: Vec::new(),
        structured_pending_relationships: Vec::new(),
        identifiers: Vec::new(),
        types: HashMap::new(),
    };

    results.rekey_normalized_locations();

    let relationship = results
        .relationships
        .first()
        .expect("relationship should still exist after rekey");
    assert_eq!(relationship.from_symbol_id, new_from_symbol_id);
    assert_eq!(relationship.to_symbol_id, new_to_symbol_id);
    assert_eq!(
        relationship.id,
        format!(
            "{}_{}_{:?}_{}",
            relationship.from_symbol_id,
            relationship.to_symbol_id,
            relationship.kind,
            relationship.line_number
        ),
        "relationship ID should be regenerated from normalized endpoints"
    );
}

#[test]
fn test_rekey_normalized_locations_preserves_structured_target_identity() {
    let old_caller_id = expected_id("fixtures/events.jsonl", "caller", 1, 0);
    let old_scope_id = expected_id("fixtures/events.jsonl", "scope", 1, 10);

    let service_render = StructuredPendingRelationship::new(
        old_caller_id.clone(),
        UnresolvedTarget {
            display_name: "service.render".to_string(),
            terminal_name: "render".to_string(),
            receiver: Some("service".to_string()),
            namespace_path: vec!["ui".to_string()],
            import_context: None,
        },
        Some(old_scope_id.clone()),
        RelationshipKind::Calls,
        "fixtures/events.jsonl".to_string(),
        2,
        1.0,
    );
    let template_render = StructuredPendingRelationship::new(
        old_caller_id.clone(),
        UnresolvedTarget {
            display_name: "template.render".to_string(),
            terminal_name: "render".to_string(),
            receiver: Some("template".to_string()),
            namespace_path: vec!["templates".to_string()],
            import_context: Some(
                "import { render as templateRender } from './template'".to_string(),
            ),
        },
        Some(old_scope_id.clone()),
        RelationshipKind::Calls,
        "fixtures/events.jsonl".to_string(),
        3,
        1.0,
    );

    let mut results = ExtractionResults {
        symbols: vec![
            Symbol {
                id: old_caller_id.clone(),
                name: "caller".to_string(),
                kind: SymbolKind::Function,
                language: "json".to_string(),
                file_path: "fixtures/events.jsonl".to_string(),
                start_line: 2,
                start_column: 0,
                end_line: 2,
                end_column: 6,
                start_byte: 20,
                end_byte: 26,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
                annotations: Vec::new(),
            },
            Symbol {
                id: old_scope_id.clone(),
                name: "scope".to_string(),
                kind: SymbolKind::Function,
                language: "json".to_string(),
                file_path: "fixtures/events.jsonl".to_string(),
                start_line: 2,
                start_column: 10,
                end_line: 2,
                end_column: 15,
                start_byte: 30,
                end_byte: 35,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
                annotations: Vec::new(),
            },
        ],
        relationships: Vec::new(),
        pending_relationships: vec![
            service_render.clone().into_pending_relationship(),
            template_render.clone().into_pending_relationship(),
        ],
        structured_pending_relationships: vec![service_render, template_render],
        identifiers: Vec::new(),
        types: HashMap::new(),
    };

    results.rekey_normalized_locations();

    let caller_id = results
        .symbols
        .iter()
        .find(|symbol| symbol.name == "caller")
        .expect("caller symbol should exist")
        .id
        .clone();
    let scope_id = results
        .symbols
        .iter()
        .find(|symbol| symbol.name == "scope")
        .expect("scope symbol should exist")
        .id
        .clone();

    for pending in &results.pending_relationships {
        assert_eq!(pending.from_symbol_id, caller_id);
    }

    for pending in &results.structured_pending_relationships {
        assert_eq!(pending.pending.from_symbol_id, caller_id);
        assert_eq!(
            pending.caller_scope_symbol_id.as_deref(),
            Some(scope_id.as_str())
        );
    }

    assert_eq!(
        results.structured_pending_relationships[0]
            .target
            .terminal_name,
        results.structured_pending_relationships[1]
            .target
            .terminal_name
    );
    assert_ne!(
        results.structured_pending_relationships[0].target,
        results.structured_pending_relationships[1].target,
        "rekeying should preserve structured target identity for colliding terminal names"
    );
}
