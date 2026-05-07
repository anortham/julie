use crate::base::{
    ExtractionResults, Identifier, IdentifierKind, Relationship, RelationshipKind,
    StructuredPendingRelationship, Symbol, SymbolKind, TypeInfo, UnresolvedTarget,
};
use crate::pipeline::extract_canonical;
use md5;
use std::collections::HashMap;
use std::path::PathBuf;

fn expected_id(
    file_path: &str,
    name: &str,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
) -> String {
    let input = format!(
        "{file_path}:{name}:{start_line}:{start_column}:{end_line}:{end_column}:{start_byte}:{end_byte}"
    );
    format!("{:x}", md5::compute(input.as_bytes()))
}

fn expected_symbol_id(symbol: &Symbol) -> String {
    expected_id(
        symbol.file_path.as_str(),
        symbol.name.as_str(),
        symbol.start_line,
        symbol.start_column,
        symbol.end_line,
        symbol.end_column,
        symbol.start_byte,
        symbol.end_byte,
    )
}

fn expected_identifier_id(identifier: &Identifier) -> String {
    expected_id(
        identifier.file_path.as_str(),
        identifier.name.as_str(),
        identifier.start_line,
        identifier.start_column,
        identifier.end_line,
        identifier.end_column,
        identifier.start_byte,
        identifier.end_byte,
    )
}

fn expected_relationship_id(
    from_symbol_id: &str,
    to_symbol_id: &str,
    kind: &RelationshipKind,
    line_number: u32,
    previous_id: &str,
) -> String {
    let previous_digest = format!("{:x}", md5::compute(previous_id.as_bytes()));
    format!("{from_symbol_id}_{to_symbol_id}_{kind:?}_{line_number}_{previous_digest}")
}

fn test_type_info(symbol_id: &str, resolved_type: &str) -> TypeInfo {
    TypeInfo {
        symbol_id: symbol_id.to_string(),
        resolved_type: resolved_type.to_string(),
        generic_params: None,
        constraints: None,
        is_inferred: false,
        language: "rust".to_string(),
        metadata: None,
    }
}

fn test_symbol(
    id: &str,
    name: &str,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Variable,
        language: "rust".to_string(),
        file_path: "fixtures/path_identity.rs".to_string(),
        start_line,
        start_column,
        end_line,
        end_column,
        start_byte,
        end_byte,
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
    }
}

fn test_identifier(
    id: &str,
    name: &str,
    start_line: u32,
    start_column: u32,
    end_line: u32,
    end_column: u32,
    start_byte: u32,
    end_byte: u32,
) -> Identifier {
    Identifier {
        id: id.to_string(),
        name: name.to_string(),
        kind: IdentifierKind::VariableRef,
        language: "rust".to_string(),
        file_path: "fixtures/path_identity.rs".to_string(),
        start_line,
        start_column,
        end_line,
        end_column,
        start_byte,
        end_byte,
        containing_symbol_id: None,
        target_symbol_id: None,
        confidence: 1.0,
        code_context: None,
    }
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
        expected_symbol_id(second),
        "symbol IDs should hash the normalized stored location"
    );
}

#[test]
fn test_rekey_normalized_locations_rekeys_type_map_keys() {
    let old_symbol_id = expected_id("fixtures/events.jsonl", "type", 1, 1, 1, 7, 0, 6);
    let new_symbol_id = expected_id("fixtures/events.jsonl", "type", 2, 1, 2, 7, 35, 41);

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
        parse_diagnostics: Vec::new(),
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
    let old_from_symbol_id = expected_id("fixtures/events.jsonl", "caller", 1, 0, 1, 6, 0, 6);
    let old_to_symbol_id = expected_id("fixtures/events.jsonl", "callee", 1, 10, 1, 16, 10, 16);
    let new_from_symbol_id = expected_id("fixtures/events.jsonl", "caller", 2, 0, 2, 6, 20, 26);
    let new_to_symbol_id = expected_id("fixtures/events.jsonl", "callee", 2, 10, 2, 16, 30, 36);
    let old_relationship_id = "old-caller-callee-calls-relationship";

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
            id: old_relationship_id.to_string(),
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
        parse_diagnostics: Vec::new(),
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
        expected_relationship_id(
            relationship.from_symbol_id.as_str(),
            relationship.to_symbol_id.as_str(),
            &relationship.kind,
            relationship.line_number,
            old_relationship_id,
        ),
        "relationship ID should be regenerated from normalized endpoints"
    );
}

#[test]
fn test_rekey_normalized_locations_keeps_same_line_relationship_ids_distinct() {
    let old_from_symbol_id = expected_id("fixtures/events.jsonl", "caller", 1, 0, 1, 6, 0, 6);
    let old_to_symbol_id = expected_id("fixtures/events.jsonl", "callee", 1, 10, 1, 16, 10, 16);
    let first_old_relationship_id = "old-caller-callee-call-first";
    let second_old_relationship_id = "old-caller-callee-call-second";

    let mut results = ExtractionResults {
        symbols: vec![
            test_symbol(old_from_symbol_id.as_str(), "caller", 2, 0, 2, 6, 20, 26),
            test_symbol(old_to_symbol_id.as_str(), "callee", 2, 10, 2, 16, 30, 36),
        ],
        relationships: vec![
            Relationship {
                id: first_old_relationship_id.to_string(),
                from_symbol_id: old_from_symbol_id.clone(),
                to_symbol_id: old_to_symbol_id.clone(),
                kind: RelationshipKind::Calls,
                file_path: "fixtures/events.jsonl".to_string(),
                line_number: 2,
                confidence: 1.0,
                metadata: None,
            },
            Relationship {
                id: second_old_relationship_id.to_string(),
                from_symbol_id: old_from_symbol_id,
                to_symbol_id: old_to_symbol_id,
                kind: RelationshipKind::Calls,
                file_path: "fixtures/events.jsonl".to_string(),
                line_number: 2,
                confidence: 1.0,
                metadata: None,
            },
        ],
        pending_relationships: Vec::new(),
        structured_pending_relationships: Vec::new(),
        identifiers: Vec::new(),
        types: HashMap::new(),
        parse_diagnostics: Vec::new(),
    };

    results.rekey_normalized_locations();

    assert_ne!(
        results.relationships[0].id, results.relationships[1].id,
        "rekeying should not collapse same-line relationships with the same endpoints"
    );
}

#[test]
fn test_symbol_ids_do_not_collide_for_same_row_column_different_spans() {
    let mut short = test_symbol("old-short", "value", 1, 0, 1, 5, 0, 5);
    let mut long = test_symbol("old-long", "value", 1, 0, 1, 10, 0, 10);

    short.refresh_id();
    long.refresh_id();

    assert_ne!(
        short.id, long.id,
        "symbol IDs should include enough span entropy to distinguish same-start symbols"
    );
}

#[test]
fn test_identifier_ids_do_not_collide_for_same_row_column_different_spans() {
    let mut short = test_identifier("old-short", "value", 1, 0, 1, 5, 0, 5);
    let mut long = test_identifier("old-long", "value", 1, 0, 1, 10, 0, 10);

    short.refresh_id();
    long.refresh_id();

    assert_eq!(short.id, expected_identifier_id(&short));
    assert_eq!(long.id, expected_identifier_id(&long));
    assert_ne!(
        short.id, long.id,
        "identifier IDs should include enough span entropy to distinguish same-start identifiers"
    );
}

#[test]
fn test_rekey_normalized_locations_preserves_distinct_same_start_symbols_and_type_rows() {
    let short_old_id = "old-short";
    let long_old_id = "old-long";
    let mut results = ExtractionResults {
        symbols: vec![
            test_symbol(short_old_id, "value", 1, 0, 1, 5, 0, 5),
            test_symbol(long_old_id, "value", 1, 0, 1, 10, 0, 10),
        ],
        relationships: Vec::new(),
        pending_relationships: Vec::new(),
        structured_pending_relationships: Vec::new(),
        identifiers: Vec::new(),
        types: HashMap::from([
            (
                short_old_id.to_string(),
                test_type_info(short_old_id, "Short"),
            ),
            (long_old_id.to_string(), test_type_info(long_old_id, "Long")),
        ]),
        parse_diagnostics: Vec::new(),
    };

    results.rekey_normalized_locations();

    assert_ne!(
        results.symbols[0].id, results.symbols[1].id,
        "rekeying should preserve distinct symbol IDs for same-start spans"
    );
    assert_eq!(
        results.types.len(),
        2,
        "rekeying should not collapse type rows for same-start symbols"
    );

    for symbol in &results.symbols {
        let type_info = results
            .types
            .get(symbol.id.as_str())
            .expect("each rekeyed symbol should retain a type row");
        assert_eq!(type_info.symbol_id, symbol.id);
    }
}

#[test]
fn test_extraction_results_extend_does_not_silently_overwrite_typeinfo() {
    let mut results = ExtractionResults::empty();
    results.types.insert(
        "symbol-id".to_string(),
        test_type_info("symbol-id", "String"),
    );

    let mut other = ExtractionResults::empty();
    other.types.insert(
        "symbol-id".to_string(),
        test_type_info("symbol-id", "Number"),
    );

    results.extend(other);

    assert_eq!(
        results
            .types
            .get("symbol-id")
            .expect("type info should still exist")
            .resolved_type,
        "String",
        "conflicting type info should not overwrite the existing row"
    );
}

#[test]
fn test_rekey_normalized_locations_preserves_structured_target_identity() {
    let old_caller_id = expected_id("fixtures/events.jsonl", "caller", 1, 0, 1, 6, 0, 6);
    let old_scope_id = expected_id("fixtures/events.jsonl", "scope", 1, 10, 1, 15, 10, 15);

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
        parse_diagnostics: Vec::new(),
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
