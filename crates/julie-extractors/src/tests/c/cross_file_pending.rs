//! Phase 4a.c — C emits `StructuredPendingRelationship` for cross-
//! translation-unit calls (`extern int other_func(void); other_func();`).

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_c_emits_structured_pending_for_extern_call() {
    let source = include_str!("../../../../../fixtures/extraction/c/cross_file/source.c");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.c", source, workspace_root)
        .expect("canonical C extraction must succeed");

    let other = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "other_func")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for extern other_func; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(other.pending.line_number > 0);
    assert_eq!(other.pending.file_path, "source.c");
    assert!(other.caller_scope_symbol_id.is_some());

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper must not appear as structured pending"
    );
}

#[test]
fn test_c_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/c/cross_file/source.c");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.c", source, workspace_root)
        .expect("canonical C extraction must succeed");

    let id = result
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
        "intra-file local_helper leaked into pending"
    );
    assert!(!id.is_empty());
}
