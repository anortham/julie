//! Phase 4a.elixir — Elixir emits `StructuredPendingRelationship` for cross-
//! module calls (`alias Phoenix.Router; Router.match()`).

use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_elixir_emits_structured_pending_for_cross_module_call() {
    let source = include_str!("../../../../../fixtures/extraction/elixir/cross_file/source.ex");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.ex", source, workspace_root)
        .expect("canonical Elixir extraction must succeed");

    let m = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "match")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-module match; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(m.pending.line_number > 0);
    assert_eq!(m.pending.file_path, "source.ex");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-module local_helper must not appear as structured pending"
    );
}

#[test]
fn test_elixir_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/elixir/cross_file/source.ex");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.ex", source, workspace_root)
        .expect("canonical Elixir extraction must succeed");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-module local_helper leaked into pending"
    );
}
