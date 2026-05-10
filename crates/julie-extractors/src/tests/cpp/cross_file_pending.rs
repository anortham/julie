//! Phase 4a.cpp — C++ emits `StructuredPendingRelationship` for cross-
//! namespace calls (`#include "other.h"; other_ns::do_thing();`).

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_cpp_emits_structured_pending_for_cross_namespace_call() {
    let source = include_str!("../../../../../fixtures/extraction/cpp/cross_file/source.cpp");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.cpp", source, workspace_root)
        .expect("canonical C++ extraction must succeed");

    let do_thing = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "do_thing")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-namespace do_thing; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(do_thing.pending.line_number > 0);
    assert_eq!(do_thing.pending.file_path, "source.cpp");
    assert!(do_thing.caller_scope_symbol_id.is_some());

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper must not appear as structured pending"
    );
}

#[test]
fn test_cpp_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/cpp/cross_file/source.cpp");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.cpp", source, workspace_root)
        .expect("canonical C++ extraction must succeed");

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
