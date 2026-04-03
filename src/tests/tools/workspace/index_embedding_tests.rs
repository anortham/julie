//! Tests for indexing and embedding pipeline fixes.

use crate::database::SymbolDatabase;
use tempfile::TempDir;

/// Helper: create a fresh test DB.
fn create_test_db() -> (SymbolDatabase, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();
    (db, dir)
}

/// Helper: insert a file record and symbol so store_embeddings has a valid FK target.
fn insert_test_symbol(db: &mut SymbolDatabase, id: &str, name: &str, file_path: &str) {
    db.conn
        .execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
             VALUES (?, 'rust', 'deadbeef', 100, 0, 0)",
            rusqlite::params![file_path],
        )
        .expect("Failed to insert test file");
    db.conn
        .execute(
            "INSERT INTO symbols (id, name, kind, file_path, language, start_line, end_line, reference_score)
             VALUES (?, ?, 'function', ?, 'rust', 1, 10, 0.0)",
            rusqlite::params![id, name, file_path],
        )
        .expect("Failed to insert test symbol");
}

/// Verify that `embedding_count()` returns the actual total row count from
/// `symbol_vectors`, not merely the number of vectors stored in a single run.
///
/// This characterizes the correct behavior that `spawn_workspace_embedding`
/// must report to daemon.db: the ground-truth total after the pipeline
/// finishes, regardless of how many vectors were added *this* run.
#[test]
fn test_embedding_count_reflects_total_vectors_not_run_count() {
    let (mut db, _dir) = create_test_db();

    insert_test_symbol(&mut db, "sym_a", "process_data", "src/lib.rs");
    insert_test_symbol(&mut db, "sym_b", "handle_error", "src/lib.rs");

    // Store embeddings for both symbols; count must be 2.
    let stored = db
        .store_embeddings(&[
            ("sym_a".to_string(), vec![0.1_f32; 384]),
            ("sym_b".to_string(), vec![0.2_f32; 384]),
        ])
        .unwrap();
    assert_eq!(stored, 2, "store_embeddings should report 2 stored");
    assert_eq!(
        db.embedding_count().unwrap(),
        2,
        "embedding_count() should be 2 after storing both"
    );

    // Simulate a partial re-embed: delete sym_b's embedding and re-store only sym_a.
    // A pipeline that ran only for sym_a would report stats.symbols_embedded == 1,
    // but the DB ground truth is still 1 total vector (sym_a only).
    db.delete_embeddings_for_file("src/lib.rs").unwrap();
    assert_eq!(
        db.embedding_count().unwrap(),
        0,
        "embedding_count() should be 0 after deleting all embeddings for file"
    );

    // Re-store only sym_a (simulating a partial re-embed run).
    let stored_partial = db
        .store_embeddings(&[("sym_a".to_string(), vec![0.1_f32; 384])])
        .unwrap();
    assert_eq!(stored_partial, 1, "partial re-embed stored 1 vector");

    // The actual DB total is 1, not 2. This is what daemon.db must record.
    assert_eq!(
        db.embedding_count().unwrap(),
        1,
        "embedding_count() must reflect actual DB total (1), not the original run count (2)"
    );
}
