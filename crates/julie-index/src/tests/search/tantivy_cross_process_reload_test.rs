//! R1 experiment: cross-process Tantivy reload via `OnCommitWithDelay` poll.
//!
//! # Phase 3 load-bearing assumption
//!
//! Follower processes (readers) on the **same on-disk Tantivy index** as the
//! leader (writer) will see new commits within ~500ms via the background
//! `FileWatcher` poll — **WITHOUT** an explicit `reader.reload()` on the read
//! path.  This matters because `search_symbols` / `search_unified` call
//! `self.reader.searcher()` at index.rs:780 and :1145 without a prior reload.
//! Only `num_docs()` (index.rs:601) and `commit()` / `release_writer()` call
//! `reader.reload()` explicitly.
//!
//! # Build items (per Phase-3 design-doc §4)
//!
//! 1. **PRIMARY** (must-have): two independent `SearchIndex` instances in the
//!    same process, same on-disk dir — writer and reader each get their own
//!    `IndexReader` with an independent background file-watcher poll thread.
//!    This exactly mirrors the cross-process path (no shared in-process reload
//!    channel).  Writer commits; reader searches without `reload()`; asserts
//!    docs appear within 2 s.
//!
//! 2. **STRONGER** (OS-process isolation): a genuinely separate OS process acts
//!    as the writer; the reader stays in the test process.  Uses
//!    `std::env::current_exe()` to spawn a subprocess that runs the
//!    `tantivy_cross_process_writer_subprocess` helper (an otherwise-no-op test
//!    that becomes the writer when `_TANTIVY_WRITER_DIR` is set).  Also
//!    validates that mmap'd segments remain readable after the writer process
//!    exits.
//!
//! 3. **Windows caveat** (documented here, not tested):
//!    `managed_directory.rs:171-173` in Tantivy's GC path tries to delete
//!    segment files that are still open/mmap'd.  On Windows, this call fails
//!    with a "file in use" error; Tantivy catches it, logs a warning, and
//!    **skips** the delete.  The result is segment file accumulation (a leak,
//!    not corruption) until all readers release their file handles.  Readers
//!    can search normally — correctness is preserved, only disk is bloated.
//!
//! 4. **Fix** (needed only if tests fail): add `self.reader.reload()` before
//!    `self.reader.searcher()` in both `search_annotation_symbols` (line 780)
//!    and `search_unified_full` (line 1145).  The reload is cheap (~μs when
//!    the index hasn't changed) and makes same-instance visibility deterministic.
//!    This doc is updated inline below if the fix was required.
//!
//! # Staleness contract
//!
//! Cross-process readers are **eventually consistent** (~500 ms lag) — acceptable
//! for code-intelligence queries.  Same-instance read-your-writes is preserved:
//! `commit()` force-reloads the writer's own `IndexReader` immediately.

use std::time::{Duration, Instant};

use tempfile::TempDir;

use crate::search::index::{SearchDocument, SearchFilter, SearchIndex};

/// Tantivy `OnCommitWithDelay` polls `meta.json` every 500 ms.
/// Allow 2 s = 4 poll intervals.
const POLL_TIMEOUT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_symbol(id: u32, prefix: &str) -> SearchDocument {
    SearchDocument::symbol_from_parts(
        &format!("{prefix}_{id}"),
        &format!("{prefix}Symbol{id}"),
        &format!("fn {prefix}_symbol_{id}()"),
        &format!("R1 experiment symbol {id}"),
        "",
        "src/r1_experiment.rs",
        "function",
        "rust",
        id * 10,
    )
}

// ---------------------------------------------------------------------------
// BUILD ITEM 1 — PRIMARY: two independent in-process instances, same on-disk dir
// ---------------------------------------------------------------------------

/// PRIMARY R1 experiment: two independent `SearchIndex` instances on the same
/// on-disk directory.  Each has its own `IndexReader` with its own background
/// poll watcher — exactly the cross-process topology, minus the OS boundary.
///
/// The writer calls `commit()` which calls `self.reader.reload()` on the
/// **writer's** reader only.  The **reader instance** must discover the new
/// meta.json via its background 500 ms poll and surface it through
/// `reader.searcher()` without any explicit `reload()` call.
#[test]
fn test_cross_instance_tantivy_poll_reload() {
    let temp = TempDir::new().unwrap();

    // Create the index (no writer lock held yet — writer is lazy in SearchIndex).
    let writer_idx = SearchIndex::create(temp.path()).unwrap();

    // Open a SEPARATE reader instance on the same directory.
    // This gets its own IndexReader / background poll watcher — no shared channel.
    let reader_idx = SearchIndex::open(temp.path()).unwrap();
    assert_eq!(reader_idx.num_docs(), 0, "reader should start empty");

    // Writer adds 3 symbols and commits.
    // commit() reloads the WRITER's own reader; reader_idx relies on the poll.
    for i in 0..3u32 {
        writer_idx.add_search_doc(&make_symbol(i, "XCrossInst")).unwrap();
    }
    writer_idx.commit().unwrap();

    // Poll reader_idx via search_symbols() — NOT reader.reload() — to exercise
    // the background poll path (index.rs:1145: self.reader.searcher() without reload).
    let start = Instant::now();
    let mut found_count = 0usize;
    while start.elapsed() < POLL_TIMEOUT {
        let results = reader_idx
            .search_symbols("XCrossInst", &SearchFilter::default(), 10)
            .unwrap();
        found_count = results.results.len();
        if found_count >= 3 {
            let elapsed_ms = start.elapsed().as_millis();
            eprintln!(
                "[R1 PRIMARY] Cross-instance poll reload OK: \
                 found {found_count} docs in {elapsed_ms} ms"
            );
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    // VERDICT recorded in assertion message.
    assert!(
        found_count >= 3,
        "VERDICT: cross-instance poll reload FAILED — reader.searcher() did not \
         surface writer commits within {}s (found {} of 3 XCrossInst symbols). \
         Fix: add reader.reload() before reader.searcher() at index.rs:780 and :1145.",
        POLL_TIMEOUT.as_secs(),
        found_count,
    );
}

/// Regression guard: `commit()` on the writer force-reloads the writer's own
/// `IndexReader`.  The writer should see its own commits immediately without
/// waiting for the poll.
#[test]
fn test_writer_sees_own_commits_immediately() {
    let temp = TempDir::new().unwrap();
    let idx = SearchIndex::create(temp.path()).unwrap();

    idx.add_search_doc(&make_symbol(0, "XSelfCommit")).unwrap();
    idx.commit().unwrap();

    // writer's own reader should be synchronously up to date
    let results = idx
        .search_symbols("XSelfCommit", &SearchFilter::default(), 5)
        .unwrap();
    assert!(
        !results.results.is_empty(),
        "writer must see its own commits immediately after commit()"
    );
}

// ---------------------------------------------------------------------------
// BUILD ITEM 2 — STRONGER: genuinely separate OS process
//
// Pattern: `tantivy_cross_process_writer_subprocess` is a regular #[test] that
// acts as a writer subprocess when `_TANTIVY_WRITER_DIR` is set, and is a
// harmless no-op otherwise.  The parent test spawns the already-compiled test
// binary via current_exe() + a substring filter, waits for it, then polls the
// reader for the new docs.
// ---------------------------------------------------------------------------

/// Writer subprocess entry point.
///
/// When `_TANTIVY_WRITER_DIR` is set: opens `SearchIndex` at that path, writes
/// 5 "XOsProc" symbols, commits, releases writer, and returns.
/// When the env var is absent: no-op pass (runs harmlessly in normal test suites).
#[test]
fn tantivy_cross_process_writer_subprocess() {
    let Ok(dir) = std::env::var("_TANTIVY_WRITER_DIR") else {
        // Not acting as subprocess — harmless pass.
        return;
    };

    let path = std::path::Path::new(&dir);
    let index = SearchIndex::open_or_create(path)
        .expect("subprocess: open_or_create failed");

    for i in 0..5u32 {
        index.add_search_doc(&make_symbol(i, "XOsProc")).unwrap();
    }
    // commit() reloads THIS process's reader; the parent test process's reader
    // must discover the change via its own background poll watcher.
    index.commit().unwrap();
    index.release_writer().unwrap();

    // OS releases all file handles (including the writer lock) on return.
}

/// STRONGER R1 experiment: separate OS process as writer; reader lives in this
/// test process.
///
/// Validates that:
/// a) the reader's background `OnCommitWithDelay` poll detects another
///    **process's** meta.json update within ~500 ms, and
/// b) mmap'd segments written by the now-exited writer process remain readable
///    on the current process (Unix: mmap stays valid after unlink; Windows: see
///    the module-level Windows caveat comment).
#[test]
fn test_cross_process_separate_os_process_tantivy_reload() {
    use std::process::Command;

    // ARRANGE: create the index in this process (schema + compat marker).
    // Write a sentinel doc so the reader can confirm the index is open before
    // the subprocess adds the real payload.
    let temp = TempDir::new().unwrap();
    {
        let setup = SearchIndex::create(temp.path()).unwrap();
        setup.add_search_doc(&make_symbol(0, "XSentinel")).unwrap();
        setup.commit().unwrap();
        setup.release_writer().unwrap();
        // `setup` drops here, releasing all handles including the writer lock.
    }

    // Open the reader BEFORE spawning the subprocess — tests the live-update path.
    let reader_idx = SearchIndex::open(temp.path()).unwrap();
    assert_eq!(reader_idx.num_docs(), 1, "reader should see the sentinel doc");

    // SPAWN the writer subprocess: re-invokes this test binary in libtest mode
    // with the `_TANTIVY_WRITER_DIR` env var set so the subprocess helper writes
    // the XOsProc symbols and commits.
    //
    // We strip NEXTEST* env vars so the binary falls back to standard libtest mode
    // (nextest protocol requires specific CLI flags that we don't pass).
    let exe = std::env::current_exe()
        .expect("could not determine test binary path");

    let mut cmd = Command::new(&exe);
    cmd.arg("tantivy_cross_process_writer_subprocess")
        .env("_TANTIVY_WRITER_DIR", temp.path().to_str().unwrap())
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_RUNNER")
        .env_remove("NEXTEST_TEST_BINARY_PROTOCOL_VERSION");

    let output = cmd.output().expect("failed to spawn writer subprocess");

    assert!(
        output.status.success(),
        "Writer subprocess exited with failure ({:?}).\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Writer subprocess has exited — its commit is durable on disk.
    // Poll the reader (NO explicit reload()) for the XOsProc docs.
    //
    // After subprocess exits, reader_idx's background poll will fire within 500 ms
    // and reload from the updated meta.json.  We allow POLL_TIMEOUT for that.
    let start = Instant::now();
    let mut os_proc_count = 0usize;
    while start.elapsed() < POLL_TIMEOUT {
        let results = reader_idx
            .search_symbols("XOsProc", &SearchFilter::default(), 10)
            .unwrap();
        os_proc_count = results.results.len();
        if os_proc_count >= 5 {
            let elapsed_ms = start.elapsed().as_millis();
            eprintln!(
                "[R1 STRONGER] OS-process poll reload OK: \
                 found {os_proc_count} docs in {elapsed_ms} ms. \
                 mmap'd segments readable after writer-process exit: confirmed."
            );
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    // VERDICT recorded in assertion message.
    assert!(
        os_proc_count >= 5,
        "VERDICT: cross-OS-process poll reload FAILED — reader did not see \
         subprocess commits within {}s (found {} of 5 XOsProc symbols). \
         Fix: add reader.reload() before reader.searcher() at index.rs:780 and :1145.",
        POLL_TIMEOUT.as_secs(),
        os_proc_count,
    );
}
