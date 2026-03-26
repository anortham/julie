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
        db.upsert_workspace("julie_a1b2c3d4", "/Users/test/julie", "ready")
            .unwrap();

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
        db.update_workspace_stats("ws1", 100, 50, Some("jina-code-v2"), Some(80), Some(1500))
            .unwrap();

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
    fn test_session_inherits_references() {
        // Verifies the lookup the daemon uses when auto-attaching references on connect.
        let (db, _tmp) = create_test_db();

        db.upsert_workspace("primary_abc", "/proj", "ready")
            .unwrap();
        db.upsert_workspace("ref_xyz", "/lib", "ready").unwrap();
        db.add_reference("primary_abc", "ref_xyz").unwrap();

        let refs = db.list_references("primary_abc").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].workspace_id, "ref_xyz");
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
    // A5: Codehealth Snapshot Storage
    // -------------------------------------------------------------------------

    #[test]
    fn test_snapshot_and_retrieve_codehealth() {
        use crate::daemon::database::CodehealthSnapshot;

        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/path", "ready").unwrap();

        let snapshot = CodehealthSnapshot {
            total_symbols: 7306,
            total_files: 434,
            security_high: 14,
            security_medium: 25,
            security_low: 100,
            change_high: 8,
            change_medium: 30,
            change_low: 200,
            symbols_tested: 180,
            symbols_untested: 47,
            avg_centrality: Some(0.42),
            max_centrality: Some(0.95),
        };

        db.insert_codehealth_snapshot("ws1", &snapshot).unwrap();

        let latest = db.get_latest_snapshot("ws1").unwrap().unwrap();
        assert_eq!(latest.total_symbols, 7306);
        assert_eq!(latest.security_high, 14);
    }

    #[test]
    fn test_snapshot_history() {
        use crate::daemon::database::CodehealthSnapshot;

        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/path", "ready").unwrap();

        for i in 0..3_i64 {
            let snapshot = CodehealthSnapshot {
                total_symbols: 7000 + i * 100,
                total_files: 400,
                security_high: (14 - i) as i32,
                ..Default::default()
            };
            db.insert_codehealth_snapshot("ws1", &snapshot).unwrap();
        }

        let history = db.get_snapshot_history("ws1", 10).unwrap();
        assert_eq!(history.len(), 3);
        // Most recent first: last inserted has total_symbols = 7200
        assert_eq!(history[0].total_symbols, 7200);
    }

    #[test]
    fn test_get_latest_snapshot_returns_none_when_empty() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/path", "ready").unwrap();

        assert!(db.get_latest_snapshot("ws1").unwrap().is_none());
    }

    // -------------------------------------------------------------------------
    // C3: Codehealth Snapshot from SymbolDatabase
    // -------------------------------------------------------------------------

    #[test]
    fn test_snapshot_codehealth_from_symbols_db() {
        use crate::database::SymbolDatabase;

        let (daemon_db, _tmp) = create_test_db();
        daemon_db.upsert_workspace("ws1", "/path", "ready").unwrap();

        // Create a symbols.db with minimal test data
        let sym_tmp = TempDir::new().unwrap();
        let symbols_db = SymbolDatabase::new(sym_tmp.path().join("symbols.db")).unwrap();

        // Insert a file
        symbols_db
            .conn
            .execute(
                "INSERT INTO files (path, language, hash, size, last_modified) \
                 VALUES ('foo.rs', 'rust', 'abc', 100, 0)",
                [],
            )
            .unwrap();

        // Insert a symbol with HIGH security risk
        symbols_db
            .conn
            .execute(
                "INSERT INTO symbols \
                 (id, name, kind, file_path, start_line, end_line, start_col, end_col, language, metadata) \
                 VALUES ('s1', 'foo', 'Function', 'foo.rs', 1, 10, 0, 0, 'rust', \
                 '{\"security_risk\":{\"label\":\"HIGH\",\"score\":0.9}}')",
                [],
            )
            .unwrap();

        daemon_db
            .snapshot_codehealth_from_db("ws1", &symbols_db)
            .unwrap();

        let snapshot = daemon_db.get_latest_snapshot("ws1").unwrap().unwrap();
        assert_eq!(snapshot.total_symbols, 1);
        assert_eq!(snapshot.security_high, 1);
        assert_eq!(snapshot.security_medium, 0);
    }

    // -------------------------------------------------------------------------
    // A4: Tool Calls and Retention
    // -------------------------------------------------------------------------

    #[test]
    fn test_insert_and_query_tool_calls() {
        let (db, _tmp) = create_test_db();
        db.insert_tool_call(
            "ws1",
            "sess1",
            "fast_search",
            12.5,
            Some(10),
            None,
            Some(500),
            true,
            None,
        )
        .unwrap();
        db.insert_tool_call(
            "ws1",
            "sess1",
            "deep_dive",
            45.0,
            Some(1),
            None,
            Some(1200),
            true,
            None,
        )
        .unwrap();

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
        db.insert_tool_call("ws1", "s1", "new_call", 1.0, None, None, None, true, None)
            .unwrap();

        db.prune_tool_calls(90).unwrap();

        let count: i64 = {
            let conn = db.conn_for_test();
            conn.query_row("SELECT COUNT(*) FROM tool_calls", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(count, 1); // only the recent one survives
    }

    // -------------------------------------------------------------------------
    // A6: Workspace ID Migration
    // -------------------------------------------------------------------------

    #[test]
    fn test_migrate_workspace_ids_updates_all_tables() {
        let (db, _tmp) = create_test_db();

        // Insert workspace with old ID
        db.upsert_workspace("julie_316c0b08", "/Users/murphy/source/julie", "ready").unwrap();
        db.update_workspace_stats("julie_316c0b08", 100, 50, None, None, None).unwrap();

        // Insert a reference relationship
        db.upsert_workspace("goldfish_5ed767a5", "/Users/murphy/source/goldfish", "ready").unwrap();
        db.add_reference("julie_316c0b08", "goldfish_5ed767a5").unwrap();

        // Insert codehealth snapshot
        use crate::daemon::database::CodehealthSnapshot;
        db.insert_codehealth_snapshot("julie_316c0b08", &CodehealthSnapshot::default()).unwrap();

        // Insert tool call
        db.insert_tool_call("julie_316c0b08", "sess1", "fast_search", 50.0, Some(5), None, None, true, None).unwrap();

        // Migrate both workspace IDs
        let mut migrations = std::collections::HashMap::new();
        migrations.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());
        migrations.insert("goldfish_5ed767a5".to_string(), "goldfish_aa67f476".to_string());
        db.migrate_workspace_ids(&migrations).unwrap();

        // Verify workspaces table updated
        assert!(db.get_workspace("julie_528d4264").unwrap().is_some());
        assert!(db.get_workspace("julie_316c0b08").unwrap().is_none());
        assert!(db.get_workspace("goldfish_aa67f476").unwrap().is_some());

        // Verify stats preserved
        let ws = db.get_workspace("julie_528d4264").unwrap().unwrap();
        assert_eq!(ws.symbol_count, Some(100));
        assert_eq!(ws.file_count, Some(50));

        // Verify workspace_references updated
        let refs = db.list_references("julie_528d4264").unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].workspace_id, "goldfish_aa67f476");

        // Verify codehealth_snapshots updated
        let snapshot = db.get_latest_snapshot("julie_528d4264").unwrap();
        assert!(snapshot.is_some());
        assert!(db.get_latest_snapshot("julie_316c0b08").unwrap().is_none());

        // Verify tool_calls updated
        let history = db.query_tool_call_history("julie_528d4264", 30).unwrap();
        assert_eq!(history.total_calls, 1);
    }

    #[test]
    fn test_migrate_workspace_ids_idempotent() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "ready").unwrap();

        // Migrate with same old->new (no-op case: old doesn't exist)
        let mut migrations = std::collections::HashMap::new();
        migrations.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());

        // Should not crash even though old ID doesn't exist
        db.migrate_workspace_ids(&migrations).unwrap();

        // Original entry untouched
        let ws = db.get_workspace("julie_528d4264").unwrap();
        assert!(ws.is_some());
    }

    #[test]
    fn test_delete_workspace_with_root_path() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("workspace_e3b0c442", "/", "pending").unwrap();
        assert!(db.get_workspace("workspace_e3b0c442").unwrap().is_some());
        db.delete_workspace("workspace_e3b0c442").unwrap();
        assert!(db.get_workspace("workspace_e3b0c442").unwrap().is_none());
    }

    #[test]
    fn test_migrate_stale_ids_skips_on_disk_failure() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("julie_316c0b08", "/test/julie", "ready").unwrap();
        db.upsert_workspace("sealab_72d18461", "/test/sealab", "ready").unwrap();

        // Simulate: julie rename succeeded, sealab rename failed
        let mut id_map = std::collections::HashMap::new();
        id_map.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());
        // sealab NOT in id_map (simulates being removed after disk failure)

        db.migrate_workspace_ids(&id_map).unwrap();

        // julie was migrated
        assert!(db.get_workspace("julie_528d4264").unwrap().is_some());
        assert!(db.get_workspace("julie_316c0b08").unwrap().is_none());

        // sealab was NOT migrated (disk failure excluded it)
        assert!(db.get_workspace("sealab_72d18461").unwrap().is_some());
    }

    #[test]
    fn test_migrate_workspace_ids_empty_map() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "ready").unwrap();

        let migrations = std::collections::HashMap::new();
        db.migrate_workspace_ids(&migrations).unwrap();

        let ws = db.get_workspace("julie_528d4264").unwrap();
        assert!(ws.is_some());
    }

    #[test]
    fn test_orphan_directory_cleanup() {
        let (db, _tmp) = create_test_db();

        // Register two workspaces in DB
        db.upsert_workspace("julie_528d4264", "/Users/test/julie", "ready").unwrap();
        db.upsert_workspace("goldfish_aa67f476", "/Users/test/goldfish", "ready").unwrap();

        // Create a temp indexes directory with registered + orphan dirs
        let indexes_dir = _tmp.path().join("indexes");
        std::fs::create_dir_all(indexes_dir.join("julie_528d4264")).unwrap();
        std::fs::create_dir_all(indexes_dir.join("goldfish_aa67f476")).unwrap();
        std::fs::create_dir_all(indexes_dir.join("julie_316c0b08")).unwrap();  // orphan
        std::fs::create_dir_all(indexes_dir.join("sealab_72d18461")).unwrap(); // orphan

        // Build registered ID set
        let registered: std::collections::HashSet<String> = db
            .list_workspaces()
            .unwrap()
            .into_iter()
            .map(|ws| ws.workspace_id)
            .collect();

        // Scan and delete orphans (same logic as clean command)
        let mut cleaned_orphans = Vec::new();
        for entry in std::fs::read_dir(&indexes_dir).unwrap().flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                if !registered.contains(&dir_name) {
                    std::fs::remove_dir_all(entry.path()).unwrap();
                    cleaned_orphans.push(dir_name);
                }
            }
        }

        assert_eq!(cleaned_orphans.len(), 2);
        assert!(cleaned_orphans.contains(&"julie_316c0b08".to_string()));
        assert!(cleaned_orphans.contains(&"sealab_72d18461".to_string()));

        // Verify registered dirs still exist
        assert!(indexes_dir.join("julie_528d4264").exists());
        assert!(indexes_dir.join("goldfish_aa67f476").exists());
        // Verify orphans are gone
        assert!(!indexes_dir.join("julie_316c0b08").exists());
        assert!(!indexes_dir.join("sealab_72d18461").exists());
    }

    #[test]
    fn test_upsert_workspace_path_conflict_updates_status() {
        let (db, _tmp) = create_test_db();

        // Insert with old workspace ID
        db.upsert_workspace("julie_316c0b08", "/Users/murphy/source/julie", "ready").unwrap();

        // Upsert same path with different workspace ID -- should not crash
        db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "pending").unwrap();

        // The row should still exist (status updated, workspace_id NOT changed
        // because only the startup migration handles ID changes with FK safety)
        let ws = db.get_workspace("julie_316c0b08").unwrap().unwrap();
        assert_eq!(ws.status, "pending");
        assert_eq!(ws.path, "/Users/murphy/source/julie");
    }
}
