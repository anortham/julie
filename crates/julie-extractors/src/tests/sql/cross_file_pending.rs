//! Phase 3.1 — SQL emits `StructuredPendingRelationship` for cross-schema
//! FK targets. Pre-Phase-3.1, SQL was in `define_no_pending_extractors!`
//! and dropped any FK whose target was not in the same file.
//!
//! Source under test: `fixtures/extraction/sql/cross_file/source.sql`.

use crate::extract_canonical;
use crate::base::{RelationshipKind, SymbolKind};
use std::path::Path;

/// CREATE TABLE orders has a FK to `other_schema.users(id)`. Because `users`
/// is not defined in this file, the canonical extractor must emit a
/// `StructuredPendingRelationship` whose target carries the schema-qualified
/// reference shape (`namespace_path=["other_schema"]`,
/// `terminal_name="users"`) and whose `caller_scope_symbol_id` points at the
/// `orders` table symbol.
#[test]
fn test_sql_emits_structured_pending_for_cross_file_fk() {
    let source = include_str!("../../../../../fixtures/extraction/sql/cross_file/source.sql");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.sql", source, workspace_root)
        .expect("canonical SQL extraction succeeds for cross_file fixture");

    let pendings = &result.structured_pending_relationships;
    let users_ref = pendings
        .iter()
        .find(|p| p.target.terminal_name == "users")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for `users` cross-schema reference; got {} entries: {:#?}",
                pendings.len(),
                pendings
            )
        });

    assert_eq!(
        users_ref.target.namespace_path,
        vec!["other_schema".to_string()],
        "target.namespace_path must reflect the qualifier path"
    );
    assert_eq!(
        users_ref.target.display_name, "other_schema.users",
        "target.display_name preserves the original qualified text"
    );
    assert!(
        users_ref.target.receiver.is_none(),
        "FK references have no receiver — it's a table reference, not a method call"
    );
    assert!(
        users_ref.target.import_context.is_none(),
        "SQL has no import_context shape; that's reserved for languages with `import` constructs"
    );
    assert_eq!(users_ref.pending.kind, RelationshipKind::References);
    assert!(
        users_ref.pending.line_number > 0,
        "pending.line_number must be the FK line, not 0 (root-node fallback)"
    );
    assert_eq!(
        users_ref.pending.file_path, "source.sql",
        "pending.file_path must be the file under extraction"
    );
    assert!(
        users_ref.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at the `orders` table symbol so the resolver can scope the reference"
    );

    // Negative: orders → audit_events FK is local; that emission must NOT
    // appear as structured pending. The `audit_events` table is in this file,
    // so its FK to `orders` resolves concretely. Cross-check by asserting no
    // structured pending entry has terminal_name="orders".
    assert!(
        pendings
            .iter()
            .all(|p| p.target.terminal_name != "orders"),
        "FK to `orders` should resolve to a concrete relationship, not a structured pending; got: {:#?}",
        pendings
    );

    // Sanity: the local FK still produces a concrete relationship.
    let audit_id = &result
        .symbols
        .iter()
        .find(|s| s.name == "audit_events" && s.kind == SymbolKind::Class)
        .expect("audit_events table symbol must exist")
        .id;
    let orders_id = &result
        .symbols
        .iter()
        .find(|s| s.name == "orders" && s.kind == SymbolKind::Class)
        .expect("orders table symbol must exist")
        .id;
    assert!(
        result
            .relationships
            .iter()
            .any(|r| &r.from_symbol_id == audit_id && &r.to_symbol_id == orders_id),
        "local FK audit_events → orders must produce a concrete relationship"
    );
}
