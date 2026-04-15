use std::collections::HashMap;

use crate::base::{
    ExtractionResults, PendingRelationship, RecordOffset, RelationshipKind,
    StructuredPendingRelationship, Symbol, SymbolKind, UnresolvedTarget,
};

#[test]
fn test_structured_pending_relationship_retains_member_call_context() {
    let target = UnresolvedTarget {
        display_name: "service.process".to_string(),
        terminal_name: "process".to_string(),
        receiver: Some("service".to_string()),
        namespace_path: vec!["billing".to_string()],
        import_context: Some("import { process as serviceProcess } from './service'".to_string()),
    };

    let pending = StructuredPendingRelationship::new(
        "caller-id".to_string(),
        target.clone(),
        Some("caller-scope".to_string()),
        RelationshipKind::Calls,
        "src/app.ts".to_string(),
        7,
        0.75,
    );

    assert_eq!(pending.pending.callee_name, "service.process");
    assert_eq!(pending.target, target);
    assert_eq!(
        pending.caller_scope_symbol_id.as_deref(),
        Some("caller-scope")
    );
}

#[test]
fn test_structured_pending_relationship_distinguishes_duplicate_terminal_names() {
    let service_render = StructuredPendingRelationship::new(
        "caller-a".to_string(),
        UnresolvedTarget {
            display_name: "service.render".to_string(),
            terminal_name: "render".to_string(),
            receiver: Some("service".to_string()),
            namespace_path: vec!["ui".to_string()],
            import_context: None,
        },
        Some("scope-a".to_string()),
        RelationshipKind::Calls,
        "src/ui.ts".to_string(),
        11,
        0.8,
    );
    let template_render = StructuredPendingRelationship::new(
        "caller-b".to_string(),
        UnresolvedTarget {
            display_name: "template.render".to_string(),
            terminal_name: "render".to_string(),
            receiver: Some("template".to_string()),
            namespace_path: vec!["templates".to_string()],
            import_context: Some(
                "import { render as templateRender } from './template'".to_string(),
            ),
        },
        Some("scope-b".to_string()),
        RelationshipKind::Calls,
        "src/ui.ts".to_string(),
        12,
        0.8,
    );

    assert_eq!(
        service_render.target.terminal_name,
        template_render.target.terminal_name
    );
    assert_ne!(service_render.target, template_render.target);
    assert_ne!(
        service_render.caller_scope_symbol_id,
        template_render.caller_scope_symbol_id
    );
}

#[test]
fn test_structured_pending_relationships_survive_extend_offset_and_rekey() {
    let old_caller_id = "old-caller".to_string();
    let old_scope_id = "old-scope".to_string();
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
        "fixtures/render.ts".to_string(),
        11,
        0.8,
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
        "fixtures/render.ts".to_string(),
        12,
        0.8,
    );

    let mut combined = ExtractionResults::empty();
    combined.extend(ExtractionResults {
        symbols: vec![
            Symbol {
                id: old_caller_id.clone(),
                name: "caller".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "fixtures/render.ts".to_string(),
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
            },
            Symbol {
                id: old_scope_id.clone(),
                name: "scope".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "fixtures/render.ts".to_string(),
                start_line: 4,
                start_column: 0,
                end_line: 4,
                end_column: 5,
                start_byte: 40,
                end_byte: 45,
                signature: None,
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
            },
        ],
        relationships: Vec::new(),
        pending_relationships: vec![
            service_render.clone().into_pending_relationship(),
            template_render.clone().into_pending_relationship(),
        ],
        structured_pending_relationships: vec![service_render.clone(), template_render.clone()],
        identifiers: Vec::new(),
        types: HashMap::new(),
    });

    assert_eq!(combined.structured_pending_relationships.len(), 2);
    assert_eq!(
        combined.pending_relationships,
        vec![
            service_render.clone().into_pending_relationship(),
            template_render.clone().into_pending_relationship(),
        ],
        "extend should preserve the compatibility payload alongside structured entries"
    );

    combined.apply_record_offset(RecordOffset {
        line_delta: 4,
        byte_delta: 0,
    });

    assert_eq!(combined.pending_relationships[0].line_number, 15);
    assert_eq!(combined.pending_relationships[1].line_number, 16);
    assert_eq!(
        combined.structured_pending_relationships[0]
            .pending
            .line_number,
        15
    );
    assert_eq!(
        combined.structured_pending_relationships[1]
            .pending
            .line_number,
        16
    );

    combined.rekey_normalized_locations();

    let caller_id = combined
        .symbols
        .iter()
        .find(|symbol| symbol.name == "caller")
        .expect("caller symbol should exist")
        .id
        .clone();
    let scope_id = combined
        .symbols
        .iter()
        .find(|symbol| symbol.name == "scope")
        .expect("scope symbol should exist")
        .id
        .clone();

    for pending in &combined.pending_relationships {
        assert_eq!(pending.from_symbol_id, caller_id);
    }

    for pending in &combined.structured_pending_relationships {
        assert_eq!(pending.pending.from_symbol_id, caller_id);
        assert_eq!(
            pending.caller_scope_symbol_id.as_deref(),
            Some(scope_id.as_str())
        );
    }

    assert_eq!(
        combined.structured_pending_relationships[0]
            .target
            .terminal_name,
        combined.structured_pending_relationships[1]
            .target
            .terminal_name
    );
    assert_ne!(
        combined.structured_pending_relationships[0].target,
        combined.structured_pending_relationships[1].target,
        "rekeying should not collapse distinct structured targets that share a terminal name"
    );
}

#[test]
fn test_pending_relationship_legacy_constructor_populates_compatibility_target() {
    let pending = PendingRelationship::legacy(
        "caller-id".to_string(),
        "external_helper".to_string(),
        RelationshipKind::Calls,
        "src/app.ts".to_string(),
        9,
        1.0,
    );

    assert_eq!(pending.callee_name, "external_helper");
    assert_eq!(pending.file_path, "src/app.ts");
    assert_eq!(pending.line_number, 9);
}

#[test]
fn test_structured_pending_relationship_can_degrade_to_legacy_pending_relationship() {
    let structured = StructuredPendingRelationship::new(
        "caller-id".to_string(),
        UnresolvedTarget {
            display_name: "api.render".to_string(),
            terminal_name: "render".to_string(),
            receiver: Some("api".to_string()),
            namespace_path: vec!["ui".to_string()],
            import_context: Some("import { render as apiRender } from './api'".to_string()),
        },
        Some("scope-id".to_string()),
        RelationshipKind::Calls,
        "src/app.ts".to_string(),
        15,
        0.6,
    );

    let pending = structured.clone().into_pending_relationship();
    assert_eq!(pending.callee_name, "api.render");
    assert_eq!(pending.from_symbol_id, "caller-id");
    assert_eq!(pending.line_number, 15);
    assert_eq!(
        structured.caller_scope_symbol_id.as_deref(),
        Some("scope-id")
    );
}
