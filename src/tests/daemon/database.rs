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

    #[test]
    fn test_daemon_db_create_and_migrate() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("daemon.db");
        let db = DaemonDatabase::open(&db_path).unwrap();

        // Verify all four tables exist after migration
        assert!(db.table_exists("workspaces"));
        assert!(db.table_exists("workspace_references"));
        assert!(db.table_exists("codehealth_snapshots"));
        assert!(db.table_exists("tool_calls"));
    }

    #[test]
    fn test_daemon_db_idempotent_open() {
        // Opening the same db twice should not fail (migrations are idempotent)
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("daemon.db");

        let _ = DaemonDatabase::open(&db_path).unwrap();
        let db2 = DaemonDatabase::open(&db_path).unwrap();

        assert!(db2.table_exists("workspaces"));
    }
}
