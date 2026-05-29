use std::collections::HashMap;

use crate::base::{
    ExtractionResults, LocalTargetResolution, PendingRelationship, RecordOffset, RelationshipKind,
    ScopedSymbolIndex, StructuredPendingRelationship, Symbol, SymbolKind, UnresolvedTarget,
};

fn symbol(
    id: &str,
    name: &str,
    kind: SymbolKind,
    parent_id: Option<&str>,
    start_line: u32,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: "test".to_string(),
        file_path: "src/app.test".to_string(),
        start_line,
        start_column: 0,
        end_line: start_line,
        end_column: 1,
        start_byte: start_line * 10,
        end_byte: start_line * 10 + 1,
        body_span: None,
        body_hash: None,
        signature: None,
        doc_comment: None,
        visibility: None,
        parent_id: parent_id.map(str::to_string),
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn symbol_with_is_definition(id: &str, name: &str, is_definition: bool, start_line: u32) -> Symbol {
    let mut symbol = symbol(id, name, SymbolKind::Function, None, start_line);
    symbol.metadata = Some(HashMap::from([(
        "isDefinition".to_string(),
        serde_json::Value::String(is_definition.to_string()),
    )]));
    symbol
}

#[test]
fn test_scoped_symbol_index_resolves_self_receiver_to_same_parent_method() {
    let class_a = symbol("class-a", "A", SymbolKind::Class, None, 1);
    let class_b = symbol("class-b", "B", SymbolKind::Class, None, 10);
    let caller = symbol("caller", "caller", SymbolKind::Method, Some("class-a"), 3);
    let a_render = symbol("a-render", "render", SymbolKind::Method, Some("class-a"), 4);
    let b_render = symbol(
        "b-render",
        "render",
        SymbolKind::Method,
        Some("class-b"),
        12,
    );
    let symbols = vec![class_a, class_b, caller.clone(), a_render.clone(), b_render];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("render", Some(&caller), Some("self"));

    assert_eq!(
        resolution.as_symbol().map(|symbol| symbol.id.as_str()),
        Some(a_render.id.as_str())
    );
}

#[test]
fn test_scoped_symbol_index_super_receiver_does_not_resolve_to_child_override() {
    let superclass = symbol("class-a", "Base", SymbolKind::Class, None, 1);
    let subclass = symbol("class-b", "Child", SymbolKind::Class, None, 10);
    let base_render = symbol(
        "base-render",
        "render",
        SymbolKind::Method,
        Some("class-a"),
        3,
    );
    let caller = symbol(
        "child-render",
        "render",
        SymbolKind::Method,
        Some("class-b"),
        12,
    );
    let symbols = vec![superclass, subclass, base_render, caller.clone()];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("render", Some(&caller), Some("super"));

    assert!(
        matches!(resolution, LocalTargetResolution::ReceiverQualified),
        "super calls need inheritance-aware resolution, not same-parent self resolution"
    );
}

#[test]
fn test_scoped_symbol_index_this_receiver_without_parent_scope_stays_unresolved() {
    let caller = symbol("caller", "caller", SymbolKind::Function, None, 3);
    let local_render = symbol("render", "render", SymbolKind::Function, None, 4);
    let symbols = vec![caller.clone(), local_render];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("render", Some(&caller), Some("this"));

    assert!(matches!(resolution, LocalTargetResolution::Missing));
}

#[test]
fn test_scoped_symbol_index_refuses_foreign_receiver_when_local_method_shares_name() {
    let class_a = symbol("class-a", "A", SymbolKind::Class, None, 1);
    let caller = symbol("caller", "caller", SymbolKind::Method, Some("class-a"), 3);
    let local_render = symbol("a-render", "render", SymbolKind::Method, Some("class-a"), 4);
    let symbols = vec![class_a, caller.clone(), local_render];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("render", Some(&caller), Some("service"));

    assert!(matches!(
        resolution,
        LocalTargetResolution::ReceiverQualified
    ));
}

#[test]
fn test_scoped_symbol_index_marks_duplicate_unqualified_calls_ambiguous() {
    let first = symbol("first", "render", SymbolKind::Function, None, 1);
    let second = symbol("second", "render", SymbolKind::Function, None, 5);
    let caller = symbol("caller", "caller", SymbolKind::Function, None, 9);
    let symbols = vec![first, second, caller.clone()];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("render", Some(&caller), None);

    assert!(matches!(resolution, LocalTargetResolution::Ambiguous));
}

#[test]
fn test_scoped_symbol_index_prefers_unique_definition_over_declaration() {
    let declaration = symbol_with_is_definition("helper-decl", "helper", false, 5);
    let definition = symbol_with_is_definition("helper-def", "helper", true, 11);
    let caller = symbol("caller", "worker_run", SymbolKind::Function, None, 7);
    let symbols = vec![declaration, definition.clone(), caller.clone()];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("helper", Some(&caller), None);

    assert_eq!(
        resolution.as_symbol().map(|symbol| symbol.id.as_str()),
        Some(definition.id.as_str())
    );
}

#[test]
fn test_scoped_symbol_index_marks_same_scope_overloads_ambiguous_for_self_receiver() {
    let class_a = symbol("class-a", "A", SymbolKind::Class, None, 1);
    let caller = symbol("caller", "render", SymbolKind::Method, Some("class-a"), 3);
    let overload_one = symbol("render-1", "render", SymbolKind::Method, Some("class-a"), 4);
    let overload_two = symbol("render-2", "render", SymbolKind::Method, Some("class-a"), 5);
    let symbols = vec![class_a, caller.clone(), overload_one, overload_two];

    let index = ScopedSymbolIndex::new(&symbols);
    let resolution = index.resolve_call_target("render", Some(&caller), Some("self"));

    assert!(matches!(resolution, LocalTargetResolution::Ambiguous));
}

#[test]
fn test_scoped_symbol_index_unique_map_drops_duplicate_names() {
    let first = symbol("first", "render", SymbolKind::Function, None, 1);
    let second = symbol("second", "render", SymbolKind::Function, None, 5);
    let unique = symbol("unique", "caller", SymbolKind::Function, None, 9);
    let symbols = vec![first, second, unique];

    let unique_map = ScopedSymbolIndex::unique_symbol_map(&symbols);

    assert!(
        !unique_map.contains_key("render"),
        "ambiguous duplicate names must not survive in legacy lookup maps"
    );
    assert_eq!(
        unique_map.get("caller").map(|symbol| symbol.id.as_str()),
        Some("unique")
    );
}

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
                body_span: None,
                body_hash: None,
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
                language: "typescript".to_string(),
                file_path: "fixtures/render.ts".to_string(),
                start_line: 4,
                start_column: 0,
                end_line: 4,
                end_column: 5,
                start_byte: 40,
                end_byte: 45,
                body_span: None,
                body_hash: None,
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
        structured_pending_relationships: vec![service_render.clone(), template_render.clone()],
        identifiers: Vec::new(),
        types: HashMap::new(),
        type_argument_usages: Vec::new(),
        literals: Vec::new(),
        parse_diagnostics: Vec::new(),
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
