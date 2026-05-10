//! Phase 4b.vue — Vue emits `StructuredPendingRelationship` for cross-file
//! imports from `<script setup>` blocks (`import { foo } from './other'; foo()`).

use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_vue_emits_structured_pending_for_script_setup_import() {
    let source = include_str!("../../../../../fixtures/extraction/vue/cross_file/source.vue");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.vue", source, workspace_root)
        .expect("canonical Vue extraction must succeed");

    let foo = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "foo")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for script-setup foo import; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(foo.pending.line_number > 0);
    assert_eq!(foo.pending.file_path, "source.vue");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "local_helper"),
        "intra-file local_helper must not appear as structured pending"
    );
}

#[test]
fn test_vue_negative_local_helper_not_emitted_as_pending() {
    let source = include_str!("../../../../../fixtures/extraction/vue/cross_file/source.vue");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.vue", source, workspace_root)
        .expect("canonical Vue extraction must succeed");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "local_helper"
                && p.target.terminal_name != "local_helper"),
        "intra-file local_helper leaked into pending"
    );
}
