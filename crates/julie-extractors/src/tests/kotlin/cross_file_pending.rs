//! Phase 4a.kotlin — Kotlin emits `StructuredPendingRelationship` for cross-
//! package class references (`import other.Thing; Thing()`).

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_kotlin_emits_structured_pending_for_cross_package_call() {
    let source = include_str!("../../../../../fixtures/extraction/kotlin/cross_file/source.kt");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.kt", source, workspace_root)
        .expect("canonical Kotlin extraction must succeed");

    let thing = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Thing")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-package Thing; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(thing.pending.line_number > 0);
    assert_eq!(thing.pending.file_path, "source.kt");
    assert!(thing.caller_scope_symbol_id.is_some());

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "localHelper"),
        "intra-class localHelper must not appear as structured pending"
    );
}

#[test]
fn test_kotlin_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/kotlin/cross_file/source.kt");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.kt", source, workspace_root)
        .expect("canonical Kotlin extraction must succeed");

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
