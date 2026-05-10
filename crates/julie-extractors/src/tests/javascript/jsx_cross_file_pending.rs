//! Phase 4a.jsx — JSX (JavaScriptExtractor) emits `StructuredPendingRelationship`
//! for cross-file class imports referenced via `new` and JSX elements.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_jsx_emits_structured_pending_for_cross_file_class() {
    let source = include_str!("../../../../../fixtures/extraction/jsx/cross_file/source.jsx");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.jsx", source, workspace_root)
        .expect("canonical JSX extraction must succeed");

    let foo = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Foo")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-file Foo; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(foo.pending.line_number > 0);
    assert_eq!(foo.pending.file_path, "source.jsx");
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
fn test_jsx_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/jsx/cross_file/source.jsx");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.jsx", source, workspace_root)
        .expect("canonical JSX extraction must succeed");

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
