use super::*;

#[test]
fn test_upsert_workspace_path_conflict_updates_status() {
    let (db, _tmp) = create_test_db();

    // Insert with old workspace ID
    db.upsert_workspace("julie_316c0b08", "/Users/murphy/source/julie", "ready")
        .unwrap();

    // Upsert same path with different workspace ID -- should not crash.
    // "ready" must NOT be downgraded to "pending" (the whole point of the fix).
    db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "pending")
        .unwrap();

    // The row should still exist (workspace_id NOT changed, status preserved)
    let ws = db.get_workspace("julie_316c0b08").unwrap().unwrap();
    assert_eq!(ws.status, "ready");
    assert_eq!(ws.path, "/Users/murphy/source/julie");
}

#[test]
fn test_upsert_workspace_allows_upgrade_to_ready() {
    let (db, _tmp) = create_test_db();

    // Start as pending
    db.upsert_workspace("ws1", "/path", "pending").unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().status, "pending");

    // Upsert with "ready" should upgrade
    db.upsert_workspace("ws1", "/path", "ready").unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().status, "ready");

    // Upsert with "pending" should NOT downgrade
    db.upsert_workspace("ws1", "/path", "pending").unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().status, "ready");

    // Explicit status change via update_workspace_status still works
    db.update_workspace_status("ws1", "error").unwrap();
    assert_eq!(db.get_workspace("ws1").unwrap().unwrap().status, "error");
}

#[test]
fn test_normalize_workspace_paths_fixes_slashes_and_status() {
    let (db, _tmp) = create_test_db();

    // Insert workspace with forward-slash path and "pending" status
    // (simulates the adapter's old .replace('\\', "/") behavior)
    db.upsert_workspace("ws1", "//?/C:/source/project", "pending")
        .unwrap();
    db.update_workspace_stats("ws1", 500, 50, None, None, Some(1000))
        .unwrap();

    // Insert workspace with native path and "ready" status (should be untouched)
    db.upsert_workspace("ws2", "\\\\?\\C:\\source\\other", "ready")
        .unwrap();

    let count = db.normalize_workspace_paths().unwrap();

    if cfg!(windows) {
        // ws1: path normalized AND status restored (has symbols, was pending)
        assert_eq!(count, 1);
        let ws1 = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws1.path, "\\\\?\\C:\\source\\project");
        assert_eq!(ws1.status, "ready");

        // ws2: untouched (already native path and ready status)
        let ws2 = db.get_workspace("ws2").unwrap().unwrap();
        assert_eq!(ws2.path, "\\\\?\\C:\\source\\other");
        assert_eq!(ws2.status, "ready");
    } else {
        // On Unix, no path fix needed but status is still restored
        assert_eq!(count, 1); // ws1 has symbols and was pending
        let ws1 = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws1.status, "ready");
    }
}

#[test]
fn test_normalize_workspace_paths_skips_pending_without_symbols() {
    let (db, _tmp) = create_test_db();

    // Workspace with no symbols: status should stay "pending"
    db.upsert_workspace("ws1", "//?/C:/source/empty", "pending")
        .unwrap();

    let count = db.normalize_workspace_paths().unwrap();

    if cfg!(windows) {
        // Path normalized but status stays pending (no symbols)
        assert_eq!(count, 1);
        let ws = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.path, "\\\\?\\C:\\source\\empty");
        assert_eq!(ws.status, "pending");
    } else {
        // No path fix needed and no status fix (no symbols)
        assert_eq!(count, 0);
    }
}

#[test]
fn test_normalize_restores_ready_on_all_platforms() {
    let (db, _tmp) = create_test_db();

    // Workspace with symbols stuck at "pending" (simulates post-restart state)
    db.upsert_workspace("ws1", "/Users/test/project", "pending")
        .unwrap();
    db.update_workspace_stats("ws1", 500, 50, None, None, Some(1000))
        .unwrap();

    // Workspace with no symbols should stay "pending"
    db.upsert_workspace("ws2", "/Users/test/empty", "pending")
        .unwrap();

    // Workspace already "ready" should stay "ready"
    db.upsert_workspace("ws3", "/Users/test/other", "ready")
        .unwrap();
    db.update_workspace_stats("ws3", 100, 10, None, None, Some(500))
        .unwrap();

    let count = db.normalize_workspace_paths().unwrap();

    // ws1 should be restored to "ready" (has symbols, was stuck at pending)
    let ws1 = db.get_workspace("ws1").unwrap().unwrap();
    assert_eq!(
        ws1.status, "ready",
        "indexed workspace should be restored to ready"
    );

    // ws2 should stay "pending" (no symbols means it was never indexed)
    let ws2 = db.get_workspace("ws2").unwrap().unwrap();
    assert_eq!(
        ws2.status, "pending",
        "unindexed workspace should stay pending"
    );

    // ws3 should stay "ready" (was never stuck)
    let ws3 = db.get_workspace("ws3").unwrap().unwrap();
    assert_eq!(ws3.status, "ready", "ready workspace should stay ready");

    // At least ws1 should have been fixed
    assert!(count >= 1, "should have fixed at least one workspace");
}
