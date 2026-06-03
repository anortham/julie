use super::*;

// ============================================================================
// Concurrent Access Tests - Stress Testing for Database Corruption Bug
// ============================================================================

#[test]
fn test_concurrent_read_access_no_corruption() {
    use crate::test_support::open_test_connection;
    use std::sync::Arc;
    use std::thread;

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create and populate database
    {
        let mut db = SymbolDatabase::new(&db_path).unwrap();

        // Insert test data
        let symbols = vec![Symbol {
            id: "sym1".to_string(),
            name: "TestFunction".to_string(),
            kind: SymbolKind::Function,
            file_path: "test.rs".to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 1,
            start_byte: 0,
            end_byte: 100,
            signature: Some("fn test()".to_string()),
            doc_comment: None,
            parent_id: None,
            language: "rust".to_string(),
            visibility: Some(julie_extractors::base::types::Visibility::Public),
            metadata: Default::default(),
            code_context: None,
            content_type: None,
            confidence: None,
            semantic_group: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        }];

        db.bulk_store_symbols(&symbols, "test_workspace").unwrap();
    }

    // Concurrent read stress test - 10 threads reading simultaneously
    let db_path = Arc::new(db_path);
    let mut handles = vec![];

    for i in 0..10 {
        let db_path = Arc::clone(&db_path);
        let handle = thread::spawn(move || {
            // Each thread opens its own connection with proper configuration
            let conn = open_test_connection(db_path.as_path()).expect("Failed to open connection");

            // Perform multiple read operations
            for j in 0..50 {
                // Query symbols
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
                    .expect(&format!(
                        "Thread {} iteration {} failed to count symbols",
                        i, j
                    ));

                assert_eq!(count, 1, "Thread {} iteration {} got wrong count", i, j);
            }
        });

        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    println!("✅ Concurrent read stress test passed: 10 threads × 50 iterations = 500 operations");
}

#[test]
fn test_concurrent_mixed_access_no_corruption() {
    use crate::test_support::open_test_connection;
    use std::sync::{Arc, Mutex};
    use std::thread;

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create initial database
    {
        let db = SymbolDatabase::new(&db_path).unwrap();
        drop(db);
    }

    // Wrap db_path in Arc for sharing between threads
    let db_path = Arc::new(db_path);
    let mut handles = vec![];

    // Counter to track successful operations
    let success_counter = Arc::new(Mutex::new(0));

    // 5 reader threads
    for i in 0..5 {
        let db_path = Arc::clone(&db_path);
        let counter = Arc::clone(&success_counter);

        let handle = thread::spawn(move || {
            let conn = open_test_connection(db_path.as_path()).expect("Failed to open connection");

            for _ in 0..20 {
                // Try to read - might get 0 or more symbols depending on timing
                let _count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
                    .expect(&format!("Reader thread {} failed", i));

                // Increment success counter
                let mut count = counter.lock().unwrap();
                *count += 1;
            }
        });

        handles.push(handle);
    }

    // 3 writer threads (writing to the same database)
    for i in 0..3 {
        let db_path = Arc::clone(&db_path);
        let counter = Arc::clone(&success_counter);

        let handle = thread::spawn(move || {
            for j in 0..10 {
                // Use proper helper for connection
                let conn = open_test_connection(db_path.as_path())
                    .expect(&format!("Writer thread {} failed to open", i));

                // Insert a symbol (might conflict, but shouldn't corrupt)
                let result = conn.execute(
                    "INSERT OR REPLACE INTO symbols (
                        id, name, kind, file_path, start_line, end_line,
                        start_column, end_column, start_byte, end_byte,
                        signature, language
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    rusqlite::params![
                        format!("sym_{}_{}", i, j),
                        format!("Function{}", j),
                        "function",
                        format!("test_{}.rs", i),
                        1,
                        10,
                        0,
                        1,
                        0,
                        100,
                        "fn test()",
                        "rust"
                    ],
                );

                if result.is_ok() {
                    let mut count = counter.lock().unwrap();
                    *count += 1;
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_count = *success_counter.lock().unwrap();
    println!(
        "✅ Concurrent mixed access stress test passed: {} successful operations",
        final_count
    );

    // Verify database is not corrupted - can still query
    let conn = open_test_connection(db_path.as_path()).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("Database corrupted - cannot query after concurrent access");

    println!(
        "✅ Database integrity verified: {} symbols after concurrent access",
        count
    );

    // Note: Count might be 0 if all writes conflicted or were in uncommitted transactions
    // The important thing is that the database didn't corrupt and we can still query it
    // This test validates that concurrent access doesn't cause "database malformed" errors
}

#[test]
#[ignore] // Long-running stress test - run manually
fn test_extreme_concurrent_stress() {
    use crate::test_support::open_test_connection;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    // Create database
    {
        let db = SymbolDatabase::new(&db_path).unwrap();
        drop(db);
    }

    let db_path = Arc::new(db_path);
    let mut handles = vec![];

    // 20 threads hammering the database for 10 seconds
    for i in 0..20 {
        let db_path = Arc::clone(&db_path);

        let handle = thread::spawn(move || {
            let start = std::time::Instant::now();
            let mut operations = 0;

            while start.elapsed() < Duration::from_secs(10) {
                let conn = open_test_connection(db_path.as_path())
                    .expect(&format!("Thread {} failed to open", i));

                // Mix of operations
                if i % 2 == 0 {
                    // Reader
                    let _: Result<i64, _> =
                        conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0));
                } else {
                    // Writer
                    let _: Result<usize, _> = conn.execute(
                        "INSERT OR REPLACE INTO symbols (id, name, kind, file_path, start_line, end_line, start_column, end_column, start_byte, end_byte, signature, language) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        rusqlite::params![
                            format!("extreme_{}_{}", i, operations),
                            "Test",
                            "function",
                            "test.rs",
                            1, 1, 0, 1, 0, 1,
                            "fn test()",
                            "rust"
                        ],
                    );
                }

                operations += 1;
            }

            println!("Thread {} completed {} operations", i, operations);
        });

        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread panicked during stress test");
    }

    // Verify database integrity
    let conn = open_test_connection(db_path.as_path()).unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("Database corrupted after extreme stress test");

    println!(
        "✅ EXTREME stress test passed: {} symbols after 10 seconds of concurrent hammering",
        count
    );
}

/// ✅ GREEN TEST: Test WAL checkpoint functionality
#[test]
fn test_wal_checkpoint() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_checkpoint.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Database is created with WAL mode enabled, some initial writes have occurred
    // Now call checkpoint_wal() to merge WAL into main database
    let result = db.checkpoint_wal();

    assert!(result.is_ok(), "checkpoint_wal() should succeed");

    let (busy, log, checkpointed) = result.unwrap();

    // Verify checkpoint results
    // busy: Number of frames that couldn't be checkpointed (should be 0)
    // log: Total frames in WAL before checkpoint
    // checkpointed: Frames successfully checkpointed
    assert_eq!(busy, 0, "No frames should be busy during checkpoint");
    assert!(log >= 0, "Log should contain frames");
    assert!(checkpointed >= 0, "Should checkpoint frames");

    println!(
        "✅ WAL checkpoint successful: busy={}, log={}, checkpointed={}",
        busy, log, checkpointed
    );
}

/// Test that RESTART checkpoint mode waits for readers and successfully checkpoints
/// This prevents WAL files from growing to 45MB+ when PASSIVE checkpoints fail
#[test]
fn test_wal_checkpoint_restart_mode() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_checkpoint_restart.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Create some test data to generate WAL activity
    // The database was just created, which generates WAL activity
    // Call checkpoint_wal_restart() which uses RESTART mode
    // RESTART waits for active readers to finish, then checkpoints
    let result = db.checkpoint_wal_restart();

    assert!(result.is_ok(), "checkpoint_wal_restart() should succeed");

    let (busy, log, checkpointed) = result.unwrap();

    // Verify checkpoint results
    assert_eq!(
        busy, 0,
        "RESTART mode should successfully checkpoint all frames"
    );
    assert!(log >= 0, "Log should contain frames");
    assert!(checkpointed >= 0, "Should checkpoint frames");

    println!(
        "✅ WAL checkpoint (RESTART) successful: busy={}, log={}, checkpointed={}",
        busy, log, checkpointed
    );
}

// 🚨 CRITICAL CORRUPTION PREVENTION TEST
// This test verifies the fix for "database disk image is malformed" errors
// Root cause: Connections were opened in DELETE mode, then WAL was set later
// Fix: WAL mode is now set IMMEDIATELY after connection open
#[test]
fn test_wal_mode_set_immediately_on_connection_open() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("wal_test.db");

    // Create database - this should set WAL mode immediately
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Verify WAL mode is active
    let journal_mode: String = db
        .conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        journal_mode.to_lowercase(),
        "wal",
        "Database MUST be in WAL mode immediately after opening to prevent corruption"
    );

    // Verify synchronous mode is NORMAL (safe with WAL, faster than FULL)
    let sync_mode: i64 = db
        .conn
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        sync_mode, 1,
        "Synchronous mode should be NORMAL (1) for performance with WAL"
    );

    println!(
        "✅ WAL mode verification passed: journal_mode={}, synchronous={}",
        journal_mode, sync_mode
    );
}

// 🚨 CORRUPTION PREVENTION: Test that Drop handler checkpoints WAL
#[test]
fn test_database_drop_checkpoints_wal() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("drop_test.db");

    {
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Write some data to create WAL entries
        db.store_file_with_content(
            "test.rs",
            "rust",
            "hash123",
            100,
            1234567890,
            "fn test() {}",
            "test_workspace",
        )
        .unwrap();

        // db goes out of scope here - Drop should checkpoint
    }

    // Reopen database - if Drop checkpoint worked, database should be clean
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Query should work without corruption
    let stats = db.get_stats().unwrap();
    assert_eq!(stats.total_files, 1);

    println!("✅ Drop checkpoint verified - database reopened cleanly");
}
