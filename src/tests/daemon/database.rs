#[cfg(test)]
mod tests {
    use crate::daemon::database::DaemonDatabase;
    use tempfile::TempDir;

    fn create_test_db() -> (DaemonDatabase, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("daemon.db");
        let db = DaemonDatabase::open(&db_path).unwrap();
        (db, tmp)
    }

    // -------------------------------------------------------------------------
    // A1: Schema creation
    // -------------------------------------------------------------------------

    #[test]
    fn test_daemon_db_create_and_migrate() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("daemon.db");
        let db = DaemonDatabase::open(&db_path).unwrap();

        assert!(db.table_exists("workspaces"));
        assert!(db.table_exists("workspace_references"));
        assert!(db.table_exists("codehealth_snapshots"));
        assert!(db.table_exists("tool_calls"));
    }

    #[test]
    fn test_daemon_db_idempotent_open() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("daemon.db");

        let _ = DaemonDatabase::open(&db_path).unwrap();
        let db2 = DaemonDatabase::open(&db_path).unwrap();

        assert!(db2.table_exists("workspaces"));
    }

    // -------------------------------------------------------------------------
    // A2: Workspace CRUD
    // -------------------------------------------------------------------------

    #[test]
    fn test_upsert_and_get_workspace() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("julie_a1b2c3d4", "/Users/test/julie", "ready").unwrap();

        let ws = db.get_workspace("julie_a1b2c3d4").unwrap().unwrap();
        assert_eq!(ws.path, "/Users/test/julie");
        assert_eq!(ws.status, "ready");
        assert_eq!(ws.session_count, 0);
    }

    #[test]
    fn test_increment_decrement_session_count() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/path", "ready").unwrap();

        db.increment_session_count("ws1").unwrap();
        db.increment_session_count("ws1").unwrap();
        assert_eq!(db.get_workspace("ws1").unwrap().unwrap().session_count, 2);

        db.decrement_session_count("ws1").unwrap();
        assert_eq!(db.get_workspace("ws1").unwrap().unwrap().session_count, 1);
    }

    #[test]
    fn test_reset_all_session_counts() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/a", "ready").unwrap();
        db.upsert_workspace("ws2", "/b", "ready").unwrap();
        db.increment_session_count("ws1").unwrap();
        db.increment_session_count("ws2").unwrap();

        db.reset_all_session_counts().unwrap();
        assert_eq!(db.get_workspace("ws1").unwrap().unwrap().session_count, 0);
        assert_eq!(db.get_workspace("ws2").unwrap().unwrap().session_count, 0);
    }

    #[test]
    fn test_update_workspace_stats() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/path", "ready").unwrap();
        db.update_workspace_stats("ws1", 100, 50, Some("jina-code-v2"), Some(80)).unwrap();

        let ws = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.symbol_count, Some(100));
        assert_eq!(ws.file_count, Some(50));
        assert_eq!(ws.embedding_model, Some("jina-code-v2".to_string()));
        assert_eq!(ws.vector_count, Some(80));
    }

    // -------------------------------------------------------------------------
    // A3: Workspace References CRUD
    // -------------------------------------------------------------------------

    #[test]
    fn test_add_and_list_references() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("primary1", "/proj", "ready").unwrap();
        db.upsert_workspace("ref1", "/lib1", "ready").unwrap();
        db.upsert_workspace("ref2", "/lib2", "ready").unwrap();

        db.add_reference("primary1", "ref1").unwrap();
        db.add_reference("primary1", "ref2").unwrap();

        let refs = db.list_references("primary1").unwrap();
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn test_remove_reference() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("p1", "/proj", "ready").unwrap();
        db.upsert_workspace("r1", "/lib", "ready").unwrap();
        db.add_reference("p1", "r1").unwrap();

        db.remove_reference("p1", "r1").unwrap();
        assert_eq!(db.list_references("p1").unwrap().len(), 0);
    }

    #[test]
    fn test_cascade_delete_removes_references() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("p1", "/proj", "ready").unwrap();
        db.upsert_workspace("r1", "/lib", "ready").unwrap();
        db.add_reference("p1", "r1").unwrap();

        db.delete_workspace("r1").unwrap();
        assert_eq!(db.list_references("p1").unwrap().len(), 0);
    }

    #[test]
    fn test_add_reference_duplicate_is_ignored() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("p1", "/proj", "ready").unwrap();
        db.upsert_workspace("r1", "/lib", "ready").unwrap();

        db.add_reference("p1", "r1").unwrap();
        db.add_reference("p1", "r1").unwrap(); // duplicate -- should not error

        assert_eq!(db.list_references("p1").unwrap().len(), 1);
    }

    #[test]
    fn test_list_references_returns_workspace_row_data() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("p1", "/proj", "ready").unwrap();
        db.upsert_workspace("r1", "/lib", "ready").unwrap();
        db.add_reference("p1", "r1").unwrap();

        let refs = db.list_references("p1").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].workspace_id, "r1");
        assert_eq!(refs[0].path, "/lib");
    }

    // -------------------------------------------------------------------------
    // A4: Tool Calls and Retention
    // -------------------------------------------------------------------------

    #[test]
    fn test_insert_and_query_tool_calls() {
        let (db, _tmp) = create_test_db();
        db.insert_tool_call("ws1", "sess1", "fast_search", 12.5, Some(10), None, Some(500), true, None).unwrap();
        db.insert_tool_call("ws1", "sess1", "deep_dive", 45.0, Some(1), None, Some(1200), true, None).unwrap();

        let history = db.query_tool_call_history("ws1", 7).unwrap();
        assert_eq!(history.total_calls, 2);
        assert_eq!(history.per_tool.len(), 2);
    }

    #[test]
    fn test_prune_old_tool_calls() {
        let (db, _tmp) = create_test_db();
        // Insert a call with a very old timestamp (year 2001)
        {
            let conn = db.conn_for_test();
            conn.execute(
                "INSERT INTO tool_calls (workspace_id, session_id, timestamp, tool_name, duration_ms, success)
                 VALUES ('ws1', 's1', 1000000, 'old_call', 1.0, 1)",
                [],
            ).unwrap();
        }
        // Insert a recent call
        db.insert_tool_call("ws1", "s1", "new_call", 1.0, None, None, None, true, None).unwrap();

        db.prune_tool_calls(90).unwrap();

        let count: i64 = {
            let conn = db.conn_for_test();
            conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |r| r.get(0)).unwrap()
        };
        assert_eq!(count, 1); // only the recent one survives
    }
}
