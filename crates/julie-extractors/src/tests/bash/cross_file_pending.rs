//! Phase 4a.bash — Bash emits `StructuredPendingRelationship` for cross-
//! script calls (`source ./other.sh; other_fn args`).

use crate::base::SymbolKind;
use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_bash_emits_structured_pending_for_cross_script_call() {
    let source = include_str!("../../../../../fixtures/extraction/bash/cross_file/source.sh");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.sh", source, workspace_root)
        .expect("canonical Bash extraction must succeed");

    let other = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "other_fn")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-script other_fn; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(other.pending.line_number > 0);
    assert_eq!(other.pending.file_path, "source.sh");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-script local_helper must not appear as structured pending"
    );
}

#[test]
fn test_bash_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/bash/cross_file/source.sh");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.sh", source, workspace_root)
        .expect("canonical Bash extraction must succeed");

    let id = result
        .symbols
        .iter()
        .find(|s| {
            s.name == "local_helper"
                && (s.kind == SymbolKind::Function || s.kind == SymbolKind::Method)
        })
        .map(|s| s.id.clone())
        .expect("local_helper symbol must exist");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-script local_helper leaked into pending"
    );
    assert!(!id.is_empty());
}
