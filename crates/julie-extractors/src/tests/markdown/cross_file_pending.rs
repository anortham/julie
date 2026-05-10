//! Phase 4d.markdown — Recipe B closure. Markdown links and footnotes
//! resolve within the document or are opaque URL/path strings; there
//! is no symbol-level cross-file reference construct. Structured
//! pending is intentionally empty.

use crate::extract_canonical;
use std::path::Path;

#[test]
fn markdown_pending_relationships_intra_document_only() {
    let source = include_str!("../../../../../fixtures/extraction/markdown/cross_file/source.md");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.md", source, workspace_root)
        .expect("canonical Markdown extraction must succeed");

    assert!(
        result.structured_pending_relationships.is_empty(),
        "Markdown must not emit structured pending — links are opaque URLs/paths, \
         not symbol references. Got {} entries: {:#?}",
        result.structured_pending_relationships.len(),
        result.structured_pending_relationships
    );
    assert!(
        result.pending_relationships.is_empty(),
        "Markdown must not emit legacy pending either. Got {} entries: {:#?}",
        result.pending_relationships.len(),
        result.pending_relationships
    );
}
