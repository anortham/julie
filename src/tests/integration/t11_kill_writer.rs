//! T11 — kill-the-writer HARD GATE (Phase 3c.3).
//!
//! This is THE gate that justifies the whole leader-election design: if the
//! single writer (leader) is killed *uncleanly* — mid-flight, holding the lock,
//! after committing to canonical SQLite but BEFORE the Tantivy projection caught
//! up — the system must recover with zero operator intervention. Concretely we
//! prove, across a real OS process boundary:
//!
//!   (a) **Kernel lock-release on death.** The leader is `SIGKILL`ed (no `Drop`,
//!       no clean `unlock`). The OS releases the `flock` when the dead process's
//!       descriptors close. A surviving process can then win the lock.
//!   (b) **A second process wins the lock.** `DaemonLockGuard::try_acquire` on
//!       the same path fails with `AlreadyHeld` WHILE the leader is alive, and
//!       succeeds AFTER it dies — the election is sound across processes.
//!   (c) **The crash-gap write is recovered.** A symbol the dead leader committed
//!       to canonical SQLite but never projected into Tantivy is rebuilt by the
//!       new leader's reconcile path (`ensure_current_from_database`, the same
//!       call `reconcile_projection_lag_if_needed` → `run_primary_workspace_repair`
//!       make on promotion). After recovery the symbol is searchable.
//!
//! Part (d) of the plan's T11 acceptance — *surviving readers degrade to
//! freshness-only (~500 ms) but never error* — is already proven, end to end,
//! by the R1 cross-process reload experiment
//! (`crates/julie-index/.../tantivy_cross_process_reload_test.rs`:
//! `test_cross_process_separate_os_process_tantivy_reload`). That test stands up
//! a separate OS-process writer and shows an in-process reader picks up commits
//! within the poll window without `reload()` and without erroring. We do not
//! re-litigate it here; this file is scoped to the kill→release→recover invariant
//! that nothing else covers.
//!
//! ## Two-process mechanics
//!
//! Reuses the `current_exe()` subprocess pattern from the R1 test. The subprocess
//! entry point `t11_writer_subprocess` is a normal `#[test]` that is a harmless
//! no-op unless `_JULIE_T11_WORKSPACE_DIR` is set; when set it BECOMES the leader:
//! acquires the lock, writes one symbol to canonical SQLite, leaves Tantivy empty
//! (the crash gap), touches a `READY` sentinel, and blocks forever. The parent
//! spawns it, waits for `READY`, asserts cross-process contention, `SIGKILL`s it,
//! asserts the lock frees, then drives recovery and asserts the symbol returns.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::database::types::FileInfo;
use crate::database::SymbolDatabase;
use crate::daemon::discovery::{AcquireError, DaemonLockGuard};
use crate::extractors::{Symbol, SymbolKind};
use crate::search::{SearchIndex, SearchProjection};

/// Env var that flips `t11_writer_subprocess` from no-op into the leader role.
const WS_ENV: &str = "_JULIE_T11_WORKSPACE_DIR";
/// Workspace id shared by the writer (bulk_store) and the recovering reader
/// (projection) — must match for reconcile to find the canonical revision.
const WS_ID: &str = "t11_ws";
/// Name of the symbol the dead leader commits to SQLite but never projects.
const ORPHAN_SYMBOL: &str = "killed_writer_orphan_fn";

// ---------------------------------------------------------------------------
// Helpers (self-contained — mirrors projection_repair.rs / t9_handoff_recovery.rs)
// ---------------------------------------------------------------------------

fn writer_file(path: &str, content: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{path}"),
        size: content.len() as i64,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn writer_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 3,
        end_column: 0,
        start_byte: 0,
        end_byte: 32,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {}() {{}}", name)),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

/// `{ws}/leader.lock` — the per-workspace election lock the leader holds.
fn lock_path(ws: &Path) -> std::path::PathBuf {
    ws.join("leader.lock")
}

/// `{ws}/db/symbols.db` — canonical SQLite store.
fn db_path(ws: &Path) -> std::path::PathBuf {
    ws.join("db").join("symbols.db")
}

/// `{ws}/tantivy` — the projection the dead writer never built.
fn tantivy_path(ws: &Path) -> std::path::PathBuf {
    ws.join("tantivy")
}

/// `{ws}/READY` — sentinel the subprocess touches once it holds the lock and has
/// committed the orphan symbol, so the parent knows the crash-gap state is set up.
fn ready_path(ws: &Path) -> std::path::PathBuf {
    ws.join("READY")
}

// ---------------------------------------------------------------------------
// Subprocess entry point — the doomed leader
// ---------------------------------------------------------------------------

/// Writer subprocess entry point. Harmless no-op pass in normal test runs
/// (when `_JULIE_T11_WORKSPACE_DIR` is unset). When set, this process plays the
/// leader that is about to be killed:
///
/// 1. Acquire `{ws}/leader.lock` (must succeed — fresh dir).
/// 2. Commit one symbol to canonical SQLite via `bulk_store_fresh_atomic`.
///    Tantivy is intentionally NOT built — this is the crash gap.
/// 3. Touch `{ws}/READY`.
/// 4. Block forever, holding both the lock guard and the live DB connection.
///    The parent `SIGKILL`s us here; the lock is released only by the kernel,
///    never by `DaemonLockGuard::drop`.
#[test]
fn t11_writer_subprocess() {
    let Ok(ws) = std::env::var(WS_ENV) else {
        // Not acting as the writer subprocess — harmless pass in the normal suite.
        return;
    };
    let ws = std::path::PathBuf::from(ws);

    // 1. Win the leader lock. Keep the guard alive for the whole process life;
    //    `_guard`-prefixed (not bare `_`) so it is NOT dropped early.
    let _guard = DaemonLockGuard::try_acquire(&lock_path(&ws))
        .expect("subprocess: fresh leader lock must be acquirable");

    // 2. Commit the orphan symbol to canonical SQLite. Tantivy stays empty.
    std::fs::create_dir_all(db_path(&ws).parent().unwrap())
        .expect("subprocess: create db dir");
    let mut db = SymbolDatabase::new(&db_path(&ws)).expect("subprocess: open SQLite");
    db.bulk_store_fresh_atomic(
        &[writer_file("src/killed.rs", "fn killed_writer_orphan_fn() {}\n")],
        &[writer_symbol("sym_orphan", ORPHAN_SYMBOL, "src/killed.rs")],
        &[],
        &[],
        &[],
        WS_ID,
    )
    .expect("subprocess: bulk_store_fresh_atomic must commit");

    // 3. Signal the parent that the crash-gap state is fully set up.
    std::fs::write(ready_path(&ws), b"1").expect("subprocess: touch READY");

    // 4. Block forever holding the lock + the open DB connection. The parent
    //    SIGKILLs us; recovery must work from whatever is durable on disk.
    //    `db` stays in scope (never dropped → no clean WAL checkpoint), modeling
    //    a true mid-flight crash.
    let _db_keepalive = db;
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

// ---------------------------------------------------------------------------
// Parent test — the HARD GATE
// ---------------------------------------------------------------------------

/// T11 HARD GATE: kill the leader uncleanly, prove lock kernel-release + a second
/// process wins it + the crash-gap SQLite write is recovered into Tantivy.
#[test]
fn test_t11_kill_writer_hard_gate() -> Result<()> {
    use std::process::{Command, Stdio};

    // Defense-in-depth against a libtest filter surprise: if THIS test were ever
    // matched by the subprocess invocation, the env var would be set — bail out
    // immediately rather than recursively spawning (fork-bomb guard). The filter
    // string `t11_writer_subprocess` does not substring-match this test's name,
    // so this should never fire; it is belt-and-suspenders.
    if std::env::var(WS_ENV).is_ok() {
        return Ok(());
    }

    let temp = tempfile::tempdir()?;
    let ws = temp.path().to_path_buf();

    // Spawn the writer subprocess: re-invoke this test binary in plain libtest
    // mode (NEXTEST* stripped) filtered to the writer entry point, with the
    // workspace dir wired in. stdio is nulled — the subprocess's only contract is
    // its on-disk side effects (lock held, SQLite committed, READY touched).
    let exe = std::env::current_exe().expect("could not determine test binary path");
    let mut child = Command::new(&exe)
        .arg("t11_writer_subprocess")
        .env(WS_ENV, &ws)
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_RUNNER")
        .env_remove("NEXTEST_TEST_BINARY_PROTOCOL_VERSION")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn writer subprocess");

    // Wait for the subprocess to set up the crash-gap state (lock + SQLite + READY).
    // Generous timeout: cold process start + SQLite init under heavy nextest load.
    let ready = ready_path(&ws);
    let start = Instant::now();
    while !ready.exists() {
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "writer subprocess never signaled READY within 60s — it likely \
                 failed to acquire the lock or commit to SQLite (check that the \
                 `t11_writer_subprocess` filter matched exactly one test)"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // (b) WHILE the leader is alive, the lock is held cross-process: a second
    //     acquirer must fail with AlreadyHeld. Our HELD_DAEMON_LOCKS in-process
    //     set is empty here (separate process), so this exercises the real kernel
    //     flock contention, not the in-process dedup shortcut.
    match DaemonLockGuard::try_acquire(&lock_path(&ws)) {
        Err(AcquireError::AlreadyHeld(_)) => { /* expected — leader holds it */ }
        Ok(_) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "lock was acquirable while the leader subprocess is alive — \
                 cross-process election is BROKEN (kernel flock not contended)"
            );
        }
        Err(other) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("unexpected lock error while leader alive: {other}");
        }
    }

    // (a) Kill the leader UNCLEANLY (SIGKILL on Unix). No Drop runs, so the lock
    //     is released only by the kernel closing the dead process's descriptors.
    child.kill().expect("failed to SIGKILL writer subprocess");
    child.wait().expect("failed to reap writer subprocess");

    // (a)+(b) After death the lock must become acquirable. Poll briefly to absorb
    //         any scheduler delay between SIGKILL delivery and fd teardown.
    let reacquire_start = Instant::now();
    let new_leader_guard = loop {
        match DaemonLockGuard::try_acquire(&lock_path(&ws)) {
            Ok(guard) => break guard,
            Err(AcquireError::AlreadyHeld(_)) => {
                if reacquire_start.elapsed() > Duration::from_secs(10) {
                    panic!(
                        "lock still AlreadyHeld 10s after the leader was SIGKILLed \
                         — kernel did not release the flock on process death"
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(other) => panic!("unexpected lock error after leader death: {other}"),
        }
    };
    // We are now the sole leader; hold the guard through recovery so no other
    // process can race us.
    let _new_leader_guard = new_leader_guard;

    // (c) Recover the crash-gap write. The dead leader committed ORPHAN_SYMBOL to
    //     canonical SQLite but never projected it; Tantivy was never built. The
    //     reconcile path rebuilds Tantivy from canonical SQLite — exactly what
    //     run_primary_workspace_repair → reconcile_projection_lag_if_needed do on
    //     leader promotion.
    let mut db = SymbolDatabase::new(&db_path(&ws))
        .expect("new leader: open canonical SQLite written by the dead leader");
    std::fs::create_dir_all(tantivy_path(&ws)).expect("new leader: create tantivy dir");
    let index =
        SearchIndex::open_or_create(&tantivy_path(&ws)).expect("new leader: open empty Tantivy");
    assert_eq!(
        index.num_docs(),
        0,
        "Tantivy must start empty — the dead leader never projected (crash gap)"
    );

    let projection = SearchProjection::tantivy(WS_ID);
    let state = projection
        .ensure_current_from_database(&mut db, &index)
        .expect("new leader: reconcile Tantivy from canonical SQLite must succeed");

    assert_eq!(
        state.canonical_revision,
        Some(1),
        "canonical_revision must be 1 — the dead leader's commit survived the crash"
    );
    assert_eq!(
        state.status.as_str(),
        "ready",
        "projection must be ready after reconcile"
    );
    assert!(
        index.num_docs() >= 1,
        "reconcile must have rebuilt Tantivy from the canonical symbol (crash-gap recovery)"
    );

    // The orphan symbol the dead leader wrote is now searchable on the new leader.
    let results = index.search_symbols(ORPHAN_SYMBOL, &Default::default(), 10)?;
    assert_eq!(
        results.results.len(),
        1,
        "the killed writer's orphan symbol must be recovered and searchable"
    );
    assert_eq!(results.results[0].name, ORPHAN_SYMBOL);

    Ok(())
}
