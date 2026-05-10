//! Phase 4a.powershell — PowerShell emits `StructuredPendingRelationship`
//! for cross-module calls (`Import-Module Other; Invoke-Other -arg`).

use crate::extract_canonical;
use std::path::Path;

#[test]
fn test_powershell_emits_structured_pending_for_cross_module_call() {
    let source =
        include_str!("../../../../../fixtures/extraction/powershell/cross_file/source.ps1");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.ps1", source, workspace_root)
        .expect("canonical PowerShell extraction must succeed");

    let invoke = result
        .structured_pending_relationships
        .iter()
        .find(|p| p.target.terminal_name == "Invoke-Other")
        .unwrap_or_else(|| {
            panic!(
                "expected structured pending for cross-module Invoke-Other; got {} entries: {:#?}",
                result.structured_pending_relationships.len(),
                result.structured_pending_relationships
            )
        });

    assert!(invoke.pending.line_number > 0);
    assert_eq!(invoke.pending.file_path, "source.ps1");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.target.terminal_name != "Local-Helper"),
        "intra-script Local-Helper must not appear as structured pending"
    );
}

#[test]
fn test_powershell_negative_local_helper_not_emitted_as_pending() {
    let source =
        include_str!("../../../../../fixtures/extraction/powershell/cross_file/source.ps1");
    let workspace_root = Path::new("/tmp/test");
    let result = extract_canonical("source.ps1", source, workspace_root)
        .expect("canonical PowerShell extraction must succeed");

    assert!(
        result
            .structured_pending_relationships
            .iter()
            .all(|p| p.pending.callee_name != "Local-Helper"
                && p.target.terminal_name != "Local-Helper"),
        "intra-script Local-Helper leaked into pending"
    );
}
