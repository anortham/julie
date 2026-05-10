//! Phase 4a.r — R emits `StructuredPendingRelationship` for cross-package
//! calls (`library(other); other::do_thing()`).

use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_r_emits_structured_pending_for_cross_package_call() {
    let source = include_str!("../../../../../fixtures/extraction/r/cross_file/source.R");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.R", source, workspace_root)
        .expect("canonical R extraction must succeed");

    let do_thing = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "do_thing")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-package do_thing; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(do_thing.pending.line_number > 0);
    assert_eq!(do_thing.pending.file_path, "source.R");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper must not appear as structured pending"
    );
}

#[test]
fn test_r_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/r/cross_file/source.R");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.R", source, workspace_root)
        .expect("canonical R extraction must succeed");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-file local_helper leaked into pending"
    );
}
