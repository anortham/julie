//! Phase 4b.qml — QML emits `StructuredPendingRelationship` for
//! cross-module calls (`import "OtherModule"; external_helper()`).

use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_qml_emits_structured_pending_for_cross_module_call() {
    let source = include_str!("../../../../../fixtures/extraction/qml/cross_file/source.qml");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.qml", source, workspace_root)
        .expect("canonical QML extraction must succeed");

    let external = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "external_helper")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-module external_helper; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(external.pending.line_number > 0);
    assert_eq!(external.pending.file_path, "source.qml");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper must not appear as structured pending"
    );
}

#[test]
fn test_qml_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/qml/cross_file/source.qml");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.qml", source, workspace_root)
        .expect("canonical QML extraction must succeed");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-file local_helper leaked into pending"
    );
}
