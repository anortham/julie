//! Phase 4a.javascript — JavaScript emits `StructuredPendingRelationship`
//! for cross-file CommonJS calls (`const { foo } = require('./other'); foo()`).
//!
//! Source under test: `fixtures/extraction/javascript/cross_file/source.js`.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_javascript_emits_structured_pending_for_cross_file_call() {
    let source = include_str!("../../../../../fixtures/extraction/javascript/cross_file/source.js");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.js", source, workspace_root)
        .expect("canonical JavaScript extraction must succeed");

    let foo = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "foo")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-file foo; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(foo.pending.line_number > 0);
    assert_eq!(foo.pending.file_path, "source.js");
    assert!(foo.caller_scope_symbol_id.is_some());

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper must not appear as structured pending"
    );
}

#[test]
fn test_javascript_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/javascript/cross_file/source.js");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.js", source, workspace_root)
        .expect("canonical JavaScript extraction must succeed");

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
