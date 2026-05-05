use crate::database::SymbolDatabase;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;

#[test]
fn symbol_database_new_serializes_concurrent_initialization() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = Arc::new(temp_dir.path().join("symbols.db"));
    let thread_count = 12;
    let barrier = Arc::new(Barrier::new(thread_count));
    let failures = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();
    for worker_id in 0..thread_count {
        let db_path = Arc::clone(&db_path);
        let barrier = Arc::clone(&barrier);
        let failures = Arc::clone(&failures);

        handles.push(thread::spawn(move || {
            barrier.wait();
            match SymbolDatabase::new(db_path.as_path()) {
                Ok(db) => drop(db),
                Err(err) => failures
                    .lock()
                    .expect("failure list lock")
                    .push(format!("worker {worker_id}: {err:#}")),
            }
        }));
    }

    for handle in handles {
        handle.join().expect("worker thread should not panic");
    }

    let failures = failures.lock().expect("failure list lock");
    assert!(
        failures.is_empty(),
        "concurrent database initialization should not race, failures: {failures:#?}"
    );
}
