//! Phase 4a.vbnet — VB.NET emits `StructuredPendingRelationship` for
//! cross-namespace references (`Imports OtherNs; Dim x As New OtherClass()`).
//!
//! Source under test: `fixtures/extraction/vbnet/cross_file/source.vb`.

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_vbnet_emits_structured_pending_for_cross_namespace_class() {
    let source = include_str!("../../../../../fixtures/extraction/vbnet/cross_file/source.vb");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.vb", source, workspace_root)
        .expect("canonical VB.NET extraction must succeed");

    let other = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "OtherClass")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-namespace OtherClass; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(
        other.pending.line_number > 0,
        "pending.line_number must reflect the call site, not 0"
    );
    assert_eq!(
        other.pending.file_path, "source.vb",
        "pending.file_path must be the file under extraction"
    );
    assert!(
        other.caller_scope_symbol_id.is_some(),
        "caller_scope_symbol_id must point at enclosing function so the resolver can scope the reference"
    );

    // Negative: Helper() is defined inside the same class. It must NOT appear
    // as a structured pending entry.
    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "Helper"),
        "intra-class Helper call must not be emitted as structured pending; got: {:#?}",
        result.structured_pending_relationships
    );
}

#[test]
fn test_vbnet_negative_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/vbnet/cross_file/source.vb");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.vb", source, workspace_root)
        .expect("canonical VB.NET extraction must succeed");

    let helper_id = result
        .symbols
        .iter()
        .find(|s| s.name == "Helper" && s.kind == SymbolKind::Method)
        .map(|s| s.id.clone())
        .expect("Helper method symbol must exist");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "Helper" && p.target.terminal_name != "Helper"),
        "intra-class Helper call leaked into structured pending"
    );

    assert!(!helper_id.is_empty());
}
