//! Phase 4a.zig — Zig emits `StructuredPendingRelationship` for cross-module
//! calls (`const m = @import("other.zig"); m.func()`).
//!
//! Source under test: `fixtures/extraction/zig/cross_file/source.zig`.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_zig_emits_structured_pending_for_cross_module_call() {
    let source = include_str!("../../../../../fixtures/extraction/zig/cross_file/source.zig");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.zig", source, workspace_root)
        .expect("canonical Zig extraction must succeed");

    let func = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "func")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-module func; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(
        func.pending.line_number > 0,
        "pending.line_number must reflect call site, not 0"
    );
    assert_eq!(
        func.pending.file_path, "source.zig",
        "pending.file_path must match the file under extraction"
    );
    assert!(
        func.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at enclosing fn"
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
fn test_zig_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/zig/cross_file/source.zig");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.zig", source, workspace_root)
        .expect("canonical Zig extraction must succeed");

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
