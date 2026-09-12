#[test]
fn scan_workspace_files_reports_missing_root() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");

    let error = crate::workspace_scan::scan_workspace_files(&missing).unwrap_err();

    assert!(error.to_string().contains("missing"));
}
