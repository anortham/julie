#[cfg(test)]
mod tests {
    use crate::migration::{
        MigrationState, migrate_workspace_index, run_migration_for_workspace, scan_project_indexes,
    };
    use crate::paths::DaemonPaths;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: create a fake workspace index directory with db/symbols.db and tantivy/meta.json
    fn create_fake_index(base: &std::path::Path, workspace_id: &str) {
        let index_dir = base.join(workspace_id);
        let db_dir = index_dir.join("db");
        let tantivy_dir = index_dir.join("tantivy");
        fs::create_dir_all(&db_dir).unwrap();
        fs::create_dir_all(&tantivy_dir).unwrap();
        fs::write(db_dir.join("symbols.db"), b"fake-sqlite-data").unwrap();
        fs::write(tantivy_dir.join("meta.json"), b"{}").unwrap();
    }

    #[test]
    fn test_migrate_copies_and_validates() {
        let tmp = TempDir::new().unwrap();
        let source_base = tmp.path().join("project/.julie/indexes");
        let dest_base = tmp.path().join("central/indexes");
        fs::create_dir_all(&dest_base).unwrap();

        let ws_id = "myproject_abcd1234";
        create_fake_index(&source_base, ws_id);

        let source = source_base.join(ws_id);
        let dest = dest_base.join(ws_id);

        migrate_workspace_index(ws_id, &source, &dest).unwrap();

        // Destination should exist with expected files
        assert!(dest.join("db/symbols.db").exists());
        assert!(dest.join("tantivy/meta.json").exists());

        // Source should be deleted after successful migration
        assert!(!source.exists());
    }

    #[test]
    fn test_migration_state_tracks_progress() {
        let tmp = TempDir::new().unwrap();
        let state_path = tmp.path().join("migration.json");

        let mut state = MigrationState::new(&state_path);
        assert!(!state.is_migrated("ws_abc12345"));

        state.mark_migrated("ws_abc12345");
        assert!(state.is_migrated("ws_abc12345"));

        state.save().unwrap();

        // Reload and verify persistence
        let reloaded = MigrationState::load(&state_path).unwrap();
        assert!(reloaded.is_migrated("ws_abc12345"));
    }

    #[test]
    fn test_skip_already_migrated_destination_exists() {
        let tmp = TempDir::new().unwrap();
        let source_base = tmp.path().join("project/.julie/indexes");
        let dest_base = tmp.path().join("central/indexes");

        let ws_id = "myproject_abcd1234";
        create_fake_index(&source_base, ws_id);
        create_fake_index(&dest_base, ws_id);

        let source = source_base.join(ws_id);
        let dest = dest_base.join(ws_id);

        // Should skip without error (destination already exists)
        migrate_workspace_index(ws_id, &source, &dest).unwrap();

        // Source should still exist (we didn't touch it since destination was already there)
        assert!(source.exists());
        // Destination should still exist untouched
        assert!(dest.join("db/symbols.db").exists());
    }

    #[test]
    fn test_scan_finds_project_indexes() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path().join("myproject");
        let indexes_dir = project_root.join(".julie/indexes");

        create_fake_index(&indexes_dir, "myproject_abc12345");
        create_fake_index(&indexes_dir, "reflib_def67890");

        let found = scan_project_indexes(&project_root);
        assert_eq!(found.len(), 2);

        let ids: Vec<&str> = found.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"myproject_abc12345"));
        assert!(ids.contains(&"reflib_def67890"));
    }

    #[test]
    fn test_scan_returns_empty_for_no_indexes() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path().join("empty_project");
        // Don't create .julie/ directory at all

        let found = scan_project_indexes(&project_root);
        assert!(found.is_empty());
    }

    #[test]
    fn test_migration_state_persists_across_loads() {
        let tmp = TempDir::new().unwrap();
        let state_path = tmp.path().join("migration.json");

        // First session: mark some workspaces migrated
        {
            let mut state = MigrationState::new(&state_path);
            state.mark_migrated("ws_aaaaaaaa");
            state.mark_migrated("ws_bbbbbbbb");
            state.save().unwrap();
        }

        // Second session: load from disk, verify, add more
        {
            let mut state = MigrationState::load(&state_path).unwrap();
            assert!(state.is_migrated("ws_aaaaaaaa"));
            assert!(state.is_migrated("ws_bbbbbbbb"));
            assert!(!state.is_migrated("ws_cccccccc"));

            state.mark_migrated("ws_cccccccc");
            state.save().unwrap();
        }

        // Third session: all three should be present
        {
            let state = MigrationState::load(&state_path).unwrap();
            assert!(state.is_migrated("ws_aaaaaaaa"));
            assert!(state.is_migrated("ws_bbbbbbbb"));
            assert!(state.is_migrated("ws_cccccccc"));
        }
    }

    #[test]
    fn test_run_migration_for_workspace_end_to_end() {
        let tmp = TempDir::new().unwrap();
        let julie_home = tmp.path().join("julie_home");
        let project_root = tmp.path().join("myproject");

        // Set up DaemonPaths pointing at our temp julie_home
        let daemon_paths = DaemonPaths::with_home(julie_home.clone());
        daemon_paths.ensure_dirs().unwrap();

        // Create a fake per-project index
        let project_indexes = project_root.join(".julie/indexes");
        create_fake_index(&project_indexes, "myproject_abcd1234");

        // Run migration
        run_migration_for_workspace(&daemon_paths, &project_root).unwrap();

        // Central index should now exist
        let central_index = julie_home.join("indexes/myproject_abcd1234");
        assert!(central_index.join("db/symbols.db").exists());
        assert!(central_index.join("tantivy/meta.json").exists());

        // Per-project index should be deleted
        assert!(!project_indexes.join("myproject_abcd1234").exists());

        // Migration state should be persisted
        let state = MigrationState::load(&daemon_paths.migration_state()).unwrap();
        assert!(state.is_migrated("myproject_abcd1234"));
    }

    #[test]
    fn test_scan_ignores_non_workspace_directories() {
        let tmp = TempDir::new().unwrap();
        let project_root = tmp.path().join("myproject");
        let indexes_dir = project_root.join(".julie/indexes");

        // Create a valid workspace index
        create_fake_index(&indexes_dir, "myproject_abc12345");

        // Create some junk directories that don't match workspace ID format
        fs::create_dir_all(indexes_dir.join("random_dir")).unwrap();
        fs::create_dir_all(indexes_dir.join(".DS_Store")).unwrap();
        fs::create_dir_all(indexes_dir.join("no_underscore")).unwrap();

        let found = scan_project_indexes(&project_root);
        // Should only find the one that matches name_hash8 pattern
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "myproject_abc12345");
    }

    #[test]
    fn test_migrate_validation_failure_cleans_up_destination() {
        let tmp = TempDir::new().unwrap();
        let source_base = tmp.path().join("project/.julie/indexes");
        let dest_base = tmp.path().join("central/indexes");
        fs::create_dir_all(&dest_base).unwrap();

        let ws_id = "broken_abcd1234";
        // Create an incomplete source (missing tantivy/meta.json)
        let source = source_base.join(ws_id);
        let db_dir = source.join("db");
        fs::create_dir_all(&db_dir).unwrap();
        fs::write(db_dir.join("symbols.db"), b"fake-data").unwrap();
        // No tantivy/meta.json!

        let dest = dest_base.join(ws_id);

        let result = migrate_workspace_index(ws_id, &source, &dest);
        assert!(result.is_err());

        // Destination should be cleaned up on validation failure
        assert!(!dest.exists());

        // Source should still exist (we didn't delete it since validation failed)
        assert!(source.exists());
    }
}
