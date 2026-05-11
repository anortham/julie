//! Phase 4a.python — Python emits `StructuredPendingRelationship` for
//! cross-module calls (`from other import bar; bar()`).
//!
//! Source under test: `fixtures/extraction/python/cross_file/source.py`.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_python_emits_structured_pending_for_cross_module_call() {
    let source = include_str!("../../../../../fixtures/extraction/python/cross_file/source.py");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.py", source, workspace_root)
        .expect("canonical Python extraction must succeed");

    let bar = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "bar")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-module bar; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(
        bar.pending.line_number > 0,
        "pending.line_number must reflect call site, not 0"
    );
    assert_eq!(
        bar.pending.file_path, "source.py",
        "pending.file_path must match the file under extraction"
    );
    assert!(
        bar.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at enclosing function"
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
fn test_python_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/python/cross_file/source.py");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.py", source, workspace_root)
        .expect("canonical Python extraction must succeed");

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
