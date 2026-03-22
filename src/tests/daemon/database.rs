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
}
