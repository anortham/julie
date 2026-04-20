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
        assert!(db.table_exists("workspace_cleanup_events"));
        assert!(!db.table_exists("workspace_references"));
        assert!(db.table_exists("codehealth_snapshots"));
        assert!(db.table_exists("tool_calls"));
    }

    #[test]
    fn test_migration_003_drops_legacy_pairings_and_preserves_workspaces() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("daemon.db");

        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
                 CREATE TABLE schema_version (
                     version    INTEGER PRIMARY KEY,
                     applied_at INTEGER NOT NULL
                 );
                 CREATE TABLE workspaces (
                     workspace_id    TEXT PRIMARY KEY,
                     path            TEXT NOT NULL UNIQUE,
                     status          TEXT NOT NULL DEFAULT 'pending',
                     session_count   INTEGER NOT NULL DEFAULT 0,
                     last_indexed    INTEGER,
                     symbol_count    INTEGER,
                     file_count      INTEGER,
                     embedding_model TEXT,
                     vector_count    INTEGER,
                     created_at      INTEGER NOT NULL,
                     updated_at      INTEGER NOT NULL,
                     last_index_duration_ms INTEGER
                 );
                 CREATE TABLE workspace_references (
                     primary_workspace_id    TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                     reference_workspace_id  TEXT NOT NULL REFERENCES workspaces(workspace_id) ON DELETE CASCADE,
                     added_at                INTEGER NOT NULL,
                     PRIMARY KEY (primary_workspace_id, reference_workspace_id)
                 );
                 INSERT INTO workspaces (workspace_id, path, status, created_at, updated_at)
                 VALUES ('ws_old', '/tmp/ws_old', 'ready', unixepoch(), unixepoch());
                 INSERT INTO schema_version (version, applied_at)
                 VALUES (2, unixepoch());",
            )
            .unwrap();
        }

        let db = DaemonDatabase::open(&db_path).unwrap();

        assert!(db.table_exists("workspace_cleanup_events"));
        assert!(!db.table_exists("workspace_references"));
        let row = db
            .get_workspace("ws_old")
            .unwrap()
            .expect("workspace row should survive migration");
        assert_eq!(row.path, "/tmp/ws_old");
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

        // COALESCE: passing None for embedding_model and vector_count preserves them
        db.update_workspace_stats("ws1", 120, 60, None, None, Some(2000))
            .unwrap();
        let ws = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.symbol_count, Some(120));
        assert_eq!(ws.file_count, Some(60));
        assert_eq!(
            ws.embedding_model,
            Some("jina-code-v2".to_string()),
            "COALESCE should preserve embedding_model"
        );
        assert_eq!(
            ws.vector_count,
            Some(80),
            "COALESCE should preserve vector_count"
        );
    }

    #[test]
    fn test_update_vector_count() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("ws1", "/path", "ready").unwrap();

        // Initially null
        let ws = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.vector_count, None);

        // Set vector count
        db.update_vector_count("ws1", 500).unwrap();
        let ws = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(ws.vector_count, Some(500));

        // Update preserves it (via COALESCE)
        db.update_workspace_stats("ws1", 100, 50, None, None, None)
            .unwrap();
        let ws = db.get_workspace("ws1").unwrap().unwrap();
        assert_eq!(
            ws.vector_count,
            Some(500),
            "indexing run should not clobber vector_count"
        );
    }

    // -------------------------------------------------------------------------
    // A3: Cleanup Event Storage
    // -------------------------------------------------------------------------

    #[test]
    fn test_insert_and_list_cleanup_events() {
        let (db, _tmp) = create_test_db();

        db.insert_cleanup_event("ws1", "/tmp/ws1", "auto_prune", "missing_path")
            .unwrap();
        db.insert_cleanup_event("ws2", "/tmp/ws2", "manual_delete", "user_request")
            .unwrap();

        let events = db.list_cleanup_events(10).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].workspace_id, "ws2");
        assert_eq!(events[0].action, "manual_delete");
        assert_eq!(events[1].workspace_id, "ws1");
        assert_eq!(events[1].reason, "missing_path");
    }

    #[test]
    fn test_cleanup_event_log_is_capped_at_fifty_rows() {
        let (db, _tmp) = create_test_db();

        for i in 0..60 {
            db.insert_cleanup_event(
                &format!("ws{i}"),
                &format!("/tmp/ws{i}"),
                "auto_prune",
                "missing_path",
            )
            .unwrap();
        }

        let events = db.list_cleanup_events(100).unwrap();
        assert_eq!(
            events.len(),
            50,
            "cleanup event log should keep the newest 50 rows"
        );
        assert_eq!(events[0].workspace_id, "ws59");
        assert_eq!(events.last().unwrap().workspace_id, "ws10");
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
        };

        db.insert_codehealth_snapshot("ws1", &snapshot).unwrap();

        let latest = db.get_latest_snapshot("ws1").unwrap().unwrap();
        assert_eq!(latest.total_symbols, 7306);
        assert_eq!(latest.total_files, 434);
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
        assert_eq!(snapshot.total_files, 1);
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
    fn test_insert_tool_call_persists_metadata_json() {
        let (db, _tmp) = create_test_db();
        let metadata = serde_json::json!({
            "intent": "api_tool_lookup",
            "trace": {
                "strategy": "fast_search_definitions",
                "top_hits": [
                    {
                        "name": "search_handler",
                        "file": "src/dashboard/routes/search.rs"
                    }
                ]
            }
        });
        let metadata_str = metadata.to_string();
        db.insert_tool_call(
            "ws1",
            "sess1",
            "fast_search",
            12.5,
            Some(1),
            None,
            Some(500),
            true,
            Some(&metadata_str),
        )
        .unwrap();

        let stored_metadata: String = {
            let conn = db.conn_for_test();
            conn.query_row(
                "SELECT metadata FROM tool_calls ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
        };
        let stored_value: serde_json::Value =
            serde_json::from_str(&stored_metadata).expect("stored metadata json");

        assert_eq!(stored_value["intent"], "api_tool_lookup");
        assert_eq!(
            stored_value["trace"]["top_hits"][0]["name"],
            "search_handler"
        );
    }

    #[test]
    fn test_insert_and_list_search_compare_runs_and_cases() {
        let (db, _tmp) = create_test_db();
        let run_id = db
            .insert_search_compare_run(&crate::daemon::database::SearchCompareRunInput {
                baseline_strategy: "shared_current".to_string(),
                candidate_strategy: "legacy_direct".to_string(),
                case_count: 2,
                baseline_top1_hits: 1,
                candidate_top1_hits: 0,
                baseline_top3_hits: 2,
                candidate_top3_hits: 1,
                baseline_source_wins: 2,
                candidate_source_wins: 1,
                convergence_rate: Some(0.5),
                stall_rate: Some(0.25),
            })
            .unwrap();
        db.replace_search_compare_cases(
            run_id,
            &[crate::daemon::database::SearchCompareCaseInput {
                session_id: "sess1".to_string(),
                workspace_id: "ws1".to_string(),
                query: "search handler".to_string(),
                search_target: "definitions".to_string(),
                expected_symbol_name: Some("search_handler".to_string()),
                expected_file_path: Some("src/dashboard/routes/search.rs".to_string()),
                baseline_rank: Some(1),
                candidate_rank: Some(3),
                baseline_top_hit: Some(
                    "search_handler @ src/dashboard/routes/search.rs".to_string(),
                ),
                candidate_top_hit: Some("run_search @ src/dashboard/routes/search.rs".to_string()),
            }],
        )
        .unwrap();

        let runs = db.list_search_compare_runs(10).unwrap();
        let cases = db.list_search_compare_cases(run_id).unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].baseline_strategy, "shared_current");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].baseline_rank, Some(1));
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
        db.upsert_workspace("julie_316c0b08", "/Users/murphy/source/julie", "ready")
            .unwrap();
        db.update_workspace_stats("julie_316c0b08", 100, 50, None, None, None)
            .unwrap();

        db.insert_cleanup_event(
            "julie_316c0b08",
            "/Users/murphy/source/julie",
            "auto_prune",
            "missing_path",
        )
        .unwrap();
        db.upsert_workspace(
            "goldfish_5ed767a5",
            "/Users/murphy/source/goldfish",
            "ready",
        )
        .unwrap();

        // Insert codehealth snapshot
        use crate::daemon::database::CodehealthSnapshot;
        db.insert_codehealth_snapshot("julie_316c0b08", &CodehealthSnapshot::default())
            .unwrap();

        // Insert tool call
        db.insert_tool_call(
            "julie_316c0b08",
            "sess1",
            "fast_search",
            50.0,
            Some(5),
            None,
            None,
            true,
            None,
        )
        .unwrap();

        // Migrate both workspace IDs
        let mut migrations = std::collections::HashMap::new();
        migrations.insert("julie_316c0b08".to_string(), "julie_528d4264".to_string());
        migrations.insert(
            "goldfish_5ed767a5".to_string(),
            "goldfish_aa67f476".to_string(),
        );
        db.migrate_workspace_ids(&migrations).unwrap();

        // Verify workspaces table updated
        assert!(db.get_workspace("julie_528d4264").unwrap().is_some());
        assert!(db.get_workspace("julie_316c0b08").unwrap().is_none());
        assert!(db.get_workspace("goldfish_aa67f476").unwrap().is_some());

        // Verify stats preserved
        let ws = db.get_workspace("julie_528d4264").unwrap().unwrap();
        assert_eq!(ws.symbol_count, Some(100));
        assert_eq!(ws.file_count, Some(50));

        // Verify cleanup events updated
        let cleanup_events = db.list_cleanup_events(10).unwrap();
        assert_eq!(cleanup_events.len(), 1);
        assert_eq!(cleanup_events[0].workspace_id, "julie_528d4264");

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
        db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "ready")
            .unwrap();

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
        db.upsert_workspace("workspace_e3b0c442", "/", "pending")
            .unwrap();
        assert!(db.get_workspace("workspace_e3b0c442").unwrap().is_some());
        db.delete_workspace("workspace_e3b0c442").unwrap();
        assert!(db.get_workspace("workspace_e3b0c442").unwrap().is_none());
    }

    #[test]
    fn test_migrate_stale_ids_skips_on_disk_failure() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("julie_316c0b08", "/test/julie", "ready")
            .unwrap();
        db.upsert_workspace("sealab_72d18461", "/test/sealab", "ready")
            .unwrap();

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

    // -------------------------------------------------------------------------
    // A7: Tool Call Success Rate
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_tool_success_rate() {
        let (db, _tmp) = create_test_db();

        // Insert 8 successful calls and 2 failed calls (10 total)
        for _ in 0..8 {
            db.insert_tool_call(
                "ws1",
                "sess1",
                "fast_search",
                10.0,
                Some(5),
                None,
                None,
                true,
                None,
            )
            .unwrap();
        }
        for _ in 0..2 {
            db.insert_tool_call(
                "ws1",
                "sess1",
                "fast_search",
                5.0,
                Some(0),
                None,
                None,
                false,
                None,
            )
            .unwrap();
        }

        let (total, succeeded) = db.get_tool_success_rate("ws1", 7).unwrap();
        assert_eq!(total, 10, "should count all 10 tool calls");
        assert_eq!(succeeded, 8, "should count only the 8 successful calls");
    }

    #[test]
    fn test_get_tool_success_rate_empty() {
        let (db, _tmp) = create_test_db();

        // No tool calls at all
        let (total, succeeded) = db.get_tool_success_rate("ws1", 7).unwrap();
        assert_eq!(total, 0, "no calls should return total=0");
        assert_eq!(succeeded, 0, "no calls should return succeeded=0");
    }

    #[test]
    fn test_get_tool_success_rate_workspace_isolation() {
        let (db, _tmp) = create_test_db();

        // Insert calls for ws1
        for _ in 0..5 {
            db.insert_tool_call(
                "ws1",
                "sess1",
                "fast_search",
                10.0,
                None,
                None,
                None,
                true,
                None,
            )
            .unwrap();
        }
        // Insert calls for ws2 (should not affect ws1 query)
        for _ in 0..3 {
            db.insert_tool_call(
                "ws2",
                "sess2",
                "deep_dive",
                20.0,
                None,
                None,
                None,
                false,
                None,
            )
            .unwrap();
        }

        let (total, succeeded) = db.get_tool_success_rate("ws1", 7).unwrap();
        assert_eq!(total, 5, "should only count ws1 calls");
        assert_eq!(succeeded, 5, "all ws1 calls succeeded");

        let (total2, succeeded2) = db.get_tool_success_rate("ws2", 7).unwrap();
        assert_eq!(total2, 3, "should only count ws2 calls");
        assert_eq!(succeeded2, 0, "no ws2 calls succeeded");
    }

    #[test]
    fn test_migrate_workspace_ids_empty_map() {
        let (db, _tmp) = create_test_db();
        db.upsert_workspace("julie_528d4264", "/Users/murphy/source/julie", "ready")
            .unwrap();

        let migrations = std::collections::HashMap::new();
        db.migrate_workspace_ids(&migrations).unwrap();

        let ws = db.get_workspace("julie_528d4264").unwrap();
        assert!(ws.is_some());
    }

    #[test]
    fn test_orphan_directory_cleanup() {
        let (db, _tmp) = create_test_db();

        // Register two workspaces in DB
        db.upsert_workspace("julie_528d4264", "/Users/test/julie", "ready")
            .unwrap();
        db.upsert_workspace("goldfish_aa67f476", "/Users/test/goldfish", "ready")
            .unwrap();

        // Create a temp indexes directory with registered + orphan dirs
        let indexes_dir = _tmp.path().join("indexes");
        std::fs::create_dir_all(indexes_dir.join("julie_528d4264")).unwrap();
        std::fs::create_dir_all(indexes_dir.join("goldfish_aa67f476")).unwrap();
        std::fs::create_dir_all(indexes_dir.join("julie_316c0b08")).unwrap(); // orphan
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
}
