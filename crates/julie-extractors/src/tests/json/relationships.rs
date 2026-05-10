//! Phase 3.2 — JSON Schema `$ref` relationship extraction.
//!
//! Pre-Phase-3.2, JSON was wired through `define_data_only_extractors!`
//! (no relationships, no pending). This module proves three shapes:
//!
//! - **Local `$ref`** (`#/$defs/Address`) → concrete `Relationship` from the
//!   containing object's parent symbol (e.g. `billing`) to the resolved
//!   target (e.g. `Address`), kind `References`.
//! - **External `$ref`** (`external.json#/$defs/Address`) →
//!   `StructuredPendingRelationship` with
//!   `target.import_context = Some("external.json")`,
//!   `target.terminal_name = "Address"`,
//!   `target.namespace_path = ["$defs"]`.
//! - **Malformed local `$ref`** (`#/nonexistent/Path`) → no concrete
//!   relationship and no structured pending edge naming the missing
//!   segments. The fragment is malformed, not "deferred to another file".

use crate::base::{RelationshipKind, SymbolKind};
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_json_emits_relationship_for_local_ref() {
    let source = r##"{
        "$defs": {
            "Address": { "type": "object" }
        },
        "properties": {
            "billing": { "$ref": "#/$defs/Address" }
        }
    }"##;
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("schema.json", source, workspace_root)
        .expect("canonical JSON extraction must succeed");

    let billing_id = &result
        .symbols
        .iter()
        .find(|s| s.name == "billing")
        .expect("billing symbol must exist")
        .id;
    let address_id = &result
        .symbols
        .iter()
        .find(|s| s.name == "Address" && s.kind == SymbolKind::Module)
        .expect("Address symbol must exist as a Module (object container)")
        .id;
    let billing_to_address = result
        .relationships
        .iter()
        .find(|r| &r.from_symbol_id == billing_id && &r.to_symbol_id == address_id)
        .unwrap_or_else(|| {
            panic!(
                "expected billing → Address References relationship from local $ref; got {} relationships: {:#?}",
                result.relationships.len(),
                result.relationships
            )
        });

    assert!(
        matches!(billing_to_address.kind, RelationshipKind::References),
        "local $ref must produce References, got {:?}",
        billing_to_address.kind
    );
    assert!(
        billing_to_address.line_number > 0,
        "line_number must be the $ref pair's line, not 0"
    );
}

#[test]
fn test_json_emits_structured_pending_for_external_ref() {
    let source = r##"{
        "properties": {
            "billing": { "$ref": "external.json#/$defs/Address" }
        }
    }"##;
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("schema.json", source, workspace_root)
        .expect("canonical JSON extraction must succeed");

    let pending = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Address")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for external $ref `Address`; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert_eq!(
        pending.target.import_context.as_deref(),
        Some("external.json"),
        "import_context must carry the foreign filename"
    );
    assert_eq!(
        pending.target.namespace_path,
        vec!["$defs".to_string()],
        "namespace_path must reflect the JSON-pointer segments before the terminal"
    );
    assert_eq!(
        pending.target.display_name, "external.json#/$defs/Address",
        "display_name preserves the original $ref text"
    );
    assert!(
        pending.target.receiver.is_none(),
        "JSON $ref has no receiver — it's a pointer, not a method call"
    );
    assert_eq!(pending.pending.kind, RelationshipKind::References);
    assert!(
        pending.pending.line_number > 0,
        "pending.line_number must be the $ref pair's line, not 0"
    );
    assert_eq!(
        pending.pending.file_path, "schema.json",
        "pending.file_path must be the file under extraction"
    );
    assert!(
        pending.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at the containing object's parent symbol so the resolver can scope the reference"
    );

    // Negative: external $ref must not also emit a concrete relationship.
    // There is no local `Address` symbol, so any Address-bound concrete edge
    // would be wrong. We assert no relationships at all rather than chasing
    // hashed symbol ids.
    assert!(
        result.relationships.is_empty(),
        "external $ref must not collapse to a concrete relationship; got: {:#?}",
        result.relationships
    );
}

#[test]
fn test_json_no_relationship_for_malformed_ref() {
    // Negative case: $ref pointing at a non-existent local path produces
    // neither a concrete relationship nor a structured pending edge naming
    // the missing segments. The fragment is malformed, not deferred.
    let source = r##"{
        "$defs": {
            "Address": { "type": "object" }
        },
        "properties": {
            "broken": { "$ref": "#/nonexistent/Path" }
        }
    }"##;
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("schema.json", source, workspace_root)
        .expect("canonical JSON extraction must succeed");

    assert!(
        result.relationships.is_empty(),
        "malformed local $ref must not produce a concrete relationship; got: {:#?}",
        result.relationships
    );
    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "Path"
                && p.target.terminal_name != "nonexistent"),
        "malformed local $ref must not produce a structured pending edge; got: {:#?}",
        result.structured_pending_relationships
    );
}
