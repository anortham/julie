//! Phase 4a.rust — Rust emits `StructuredPendingRelationship` for
//! cross-module calls (`use crate::other_module::Function; Function()`).
//!
//! Source under test: `fixtures/extraction/rust/cross_file/source.rs`.

use crate::base::{RelationshipKind, SymbolKind};
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_rust_emits_structured_pending_for_cross_module_call() {
    let source = include_str!("../../../../../fixtures/extraction/rust/cross_file/source.rs");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.rs", source, workspace_root)
        .expect("canonical Rust extraction must succeed");

    // The rust extractor emits TWO pending entries for this fixture: one
    // for the `use` (kind Imports, namespace_path=["crate","other_module"],
    // import_context carries the full use statement) and one for the call
    // site (kind Calls, terminal_name="Function"). Both are valid cross-
    // file evidence; we assert on the import-shape since it carries the
    // richest structure.
    let import_ref = result
        .structured_pending_relationships
        .iter()
        .find(|p| {
            p.target.terminal_name == "Function" && p.pending.kind == RelationshipKind::Imports
        })
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending Imports for `crate::other_module::Function`; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert_eq!(
        import_ref.target.namespace_path,
        vec!["crate".to_string(), "other_module".to_string()],
        "namespace_path must reflect the full use-path qualifier"
    );
    assert_eq!(
        import_ref.target.display_name, "crate::other_module::Function",
        "display_name preserves the original use-path text"
    );
    assert!(
        import_ref.target.import_context.is_some(),
        "import_context must carry the use statement text"
    );

    let call_ref = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Function" && p.pending.kind == RelationshipKind::Calls)
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending Calls for `Function()` call site; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(
        call_ref.pending.line_number > 0,
        "call-site pending.line_number must be > 0"
    );
    let function_ref = call_ref;
    assert!(
        function_ref.pending.line_number > 0,
        "pending.line_number must be the call site, not 0"
    );
    assert_eq!(
        function_ref.pending.file_path, "source.rs",
        "pending.file_path must be the file under extraction"
    );
    assert!(
        function_ref.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at the enclosing fn so the resolver can scope the call"
    );

    // Negative: local_helper() is defined in this file. It must NOT appear
    // as a structured pending entry; it must resolve to a concrete
    // relationship (or at minimum not be emitted as pending).
    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file `local_helper` call must not be emitted as structured pending; got: {:#?}",
        result.structured_pending_relationships
    );
}

#[test]
fn test_rust_negative_local_helper_not_emitted_as_pending() {
    // Locking test: confirms intra-file calls don't leak into the pending
    // queue. Phrased as a separate test so a regression here surfaces as
    // its own failure rather than burying inside the positive test.
    let source = include_str!("../../../../../fixtures/extraction/rust/cross_file/source.rs");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.rs", source, workspace_root)
        .expect("canonical Rust extraction must succeed");

    let local_helper_id = result
        .symbols
        .iter()
        .find(|s| s.name == "local_helper" && s.kind == SymbolKind::Function)
        .map(|s| s.id.clone())
        .expect("local_helper symbol must exist");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-file local_helper call leaked into structured pending"
    );

    // Confirm local_helper symbol exists (so the resolver can pin to it).
    assert!(!local_helper_id.is_empty());
}
