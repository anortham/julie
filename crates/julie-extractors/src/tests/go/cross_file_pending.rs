//! Phase 4a.go — Go emits `StructuredPendingRelationship` for cross-package
//! calls (`import "example/other"; other.DoIt()`).
//!
//! Source under test: `fixtures/extraction/go/cross_file/source.go`.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_go_emits_structured_pending_for_cross_package_call() {
    let source = include_str!("../../../../../fixtures/extraction/go/cross_file/source.go");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.go", source, workspace_root)
        .expect("canonical Go extraction must succeed");

    let do_it = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "DoIt")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-package DoIt; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(
        do_it.pending.line_number > 0,
        "pending.line_number must reflect call site, not 0"
    );
    assert_eq!(
        do_it.pending.file_path, "source.go",
        "pending.file_path must match the file under extraction"
    );
    assert!(
        do_it.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at enclosing func"
    );

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper call must not appear as structured pending; got: {:#?}",
        result.structured_pending_relationships
    );
}

#[test]
fn test_go_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/go/cross_file/source.go");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.go", source, workspace_root)
        .expect("canonical Go extraction must succeed");

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
    assert!(!local_helper_id.is_empty());
}
