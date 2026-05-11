//! Phase 4a.scala — Scala emits `StructuredPendingRelationship` for cross-
//! package object references (`import other.Thing; Thing.apply`).

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_scala_emits_structured_pending_for_cross_package_call() {
    let source = include_str!("../../../../../fixtures/extraction/scala/cross_file/source.scala");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.scala", source, workspace_root)
        .expect("canonical Scala extraction must succeed");

    let thing_pending = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Thing" || p.target.terminal_name == "apply")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-package Thing/apply; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(thing_pending.pending.line_number > 0);
    assert_eq!(thing_pending.pending.file_path, "source.scala");
    assert!(thing_pending.caller_scope_symbol_id.is_some());

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "localHelper"),
        "intra-class localHelper must not appear as structured pending"
    );
}

#[test]
fn test_scala_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/scala/cross_file/source.scala");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.scala", source, workspace_root)
        .expect("canonical Scala extraction must succeed");

    let id = result
        .symbols
        .iter()
        .find(|s| {
            s.name == "localHelper"
                && (s.kind == SymbolKind::Method || s.kind == SymbolKind::Function)
        })
        .map(|s| s.id.clone())
        .expect("localHelper symbol must exist");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "localHelper"
                && p.target.terminal_name != "localHelper"),
        "intra-class localHelper leaked into pending"
    );
    assert!(!id.is_empty());
}
