//! Phase 4b.html — HTML emits `StructuredPendingRelationship` for external
//! `<script src="..."></script>` and `<link href="...">` references.

use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_html_emits_structured_pending_for_external_script_and_link() {
    let source = include_str!("../../../../../fixtures/extraction/html/cross_file/source.html");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.html", source, workspace_root)
        .expect("canonical HTML extraction must succeed");

    let script = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "./other.js")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for ./other.js; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });
    assert!(script.pending.line_number > 0);
    assert_eq!(script.pending.file_path, "source.html");

    let stylesheet = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "./other.css")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for ./other.css; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });
    assert!(stylesheet.pending.line_number > 0);
    assert_eq!(stylesheet.pending.file_path, "source.html");
}

#[test]
fn test_html_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/html/cross_file/source.html");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.html", source, workspace_root)
        .expect("canonical HTML extraction must succeed");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"
                && p.pending.callee_name != "local_helper"),
        "intra-document local_helper leaked into structured pending"
    );
}
