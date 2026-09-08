//! crates/julie-core/src/tests/host_slots.rs
//! Verification suite for cross-process host slot admission, barriers, and zero leaks.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::workspace::host_slots::{AdmissionError, IndexJobAdmission, IndexJobPermit};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

// ============================================================================
// Subprocess Entry Point
// ============================================================================

#[test]
fn host_slot_test_subprocess_entry() {
    let Ok(mode) = std::env::var("JULIE_TEST_HOST_SLOT_MODE") else {
        return; // Parent runner: pass immediately!
    };
    let scheduler_dir = PathBuf::from(std::env::var("JULIE_TEST_SCHEDULER_DIR").unwrap());
    let slot_count: usize = std::env::var("JULIE_TEST_SLOT_COUNT")
        .unwrap()
        .parse()
        .unwrap();

    match mode.as_str() {
        "contender" => run_contender(&scheduler_dir, slot_count),
        "conflict_check" => run_conflict_check(&scheduler_dir, slot_count),
        other => panic!("Unknown mode: {other}"),
    }
}

fn run_contender(scheduler_dir: &Path, slot_count: usize) {
    let pool = IndexJobAdmission::open_or_init(scheduler_dir, Some(slot_count))
        .expect("child open_or_init");
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut held_permit: Option<IndexJobPermit> = None;

    while let Some(Ok(line)) = lines.next() {
        let cmd = line.trim();
        match cmd {
            "TRY_ACQUIRE" => match pool.try_acquire() {
                Ok(permit) => {
                    let slot = permit.slot_index();
                    held_permit = Some(permit);
                    println!("ACQUIRED {slot}");
                    std::io::stdout().flush().unwrap();
                }
                Err(AdmissionError::Busy { .. }) => {
                    println!("BUSY");
                    std::io::stdout().flush().unwrap();
                }
                Err(e) => {
                    eprintln!("Child try_acquire failed: {e}");
                    std::process::exit(2);
                }
            },
            "ACQUIRE_BLOCKING" => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let res = rt.block_on(pool.acquire_timeout(Duration::from_secs(10), None));
                match res {
                    Ok(permit) => {
                        let slot = permit.slot_index();
                        held_permit = Some(permit);
                        println!("ACQUIRED {slot}");
                        std::io::stdout().flush().unwrap();
                    }
                    Err(e) => {
                        eprintln!("Child blocking acquire failed: {e}");
                        std::process::exit(3);
                    }
                }
            }
            "RELEASE" => {
                if let Some(permit) = held_permit.take() {
                    let slot = permit.slot_index();
                    drop(permit);
                    println!("RELEASED {slot}");
                    std::io::stdout().flush().unwrap();
                } else {
                    println!("NOT_HELD");
                    std::io::stdout().flush().unwrap();
                }
            }
            "EXIT" => std::process::exit(0),
            _ => panic!("Unknown command: {cmd}"),
        }
    }
}

fn run_conflict_check(scheduler_dir: &Path, requested_slots: usize) {
    match IndexJobAdmission::open_or_init(scheduler_dir, Some(requested_slots)) {
        Ok(_) => {
            println!("UNEXPECTED_OK");
            std::io::stdout().flush().unwrap();
            std::process::exit(0);
        }
        Err(AdmissionError::ResourceConfigConflict { active, requested }) => {
            println!("RESOURCE_CONFIG_CONFLICT active={active} requested={requested}");
            std::io::stdout().flush().unwrap();
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Other conflict error: {e}");
            std::process::exit(4);
        }
    }
}

// ============================================================================
// Process Contender Test Helper
// ============================================================================

struct ChildContender {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout_reader: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

impl ChildContender {
    async fn spawn(scheduler_dir: &Path, slot_count: usize) -> Self {
        let exe = std::env::current_exe().expect("current_exe");
        let mut cmd = Command::new(exe);
        cmd.arg("host_slot_test_subprocess_entry")
            .arg("--nocapture")
            .env("JULIE_TEST_HOST_SLOT_MODE", "contender")
            .env("JULIE_TEST_SCHEDULER_DIR", scheduler_dir)
            .env("JULIE_TEST_SLOT_COUNT", slot_count.to_string())
            .env_remove("NEXTEST")
            .env_remove("NEXTEST_RUNNER")
            .env_remove("NEXTEST_TEST_BINARY_PROTOCOL_VERSION")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child = cmd.spawn().expect("spawn child contender");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let stdout_reader = tokio::io::BufReader::new(stdout).lines();

        Self {
            child,
            stdin,
            stdout_reader,
        }
    }

    async fn send(&mut self, cmd: &str) {
        self.stdin
            .write_all(format!("{cmd}\n").as_bytes())
            .await
            .expect("write to child stdin");
        self.stdin.flush().await.expect("flush child stdin");
    }

    async fn read_response(&mut self) -> String {
        while let Ok(Some(line)) = self.stdout_reader.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("running ")
                || trimmed.starts_with("test ")
                || trimmed.ends_with("... ok")
                || trimmed.starts_with("test result:")
            {
                continue;
            }
            return trimmed.to_string();
        }
        panic!("child closed stdout without sending response");
    }

    async fn wait_success(mut self) {
        self.send("EXIT").await;
        let status = self.child.wait().await.expect("wait for child");
        assert!(status.success(), "Child process failed with {status:?}");
    }
}

// ============================================================================
// Concurrency Tests
// ============================================================================

#[tokio::test]
async fn host_slots_limit_two_processes() {
    let temp_dir = tempfile::tempdir().unwrap();
    let scheduler_dir = temp_dir.path().join("scheduler");

    // Initialize 2-slot pool
    let slot_count = 2;
    let _parent_pool = IndexJobAdmission::open_or_init(&scheduler_dir, Some(slot_count)).unwrap();

    let mut c1 = ChildContender::spawn(&scheduler_dir, slot_count).await;
    let mut c2 = ChildContender::spawn(&scheduler_dir, slot_count).await;
    let mut c3 = ChildContender::spawn(&scheduler_dir, slot_count).await;

    // 1. Contenders 1 and 2 acquire slots
    c1.send("TRY_ACQUIRE").await;
    let out1 = c1.read_response().await;
    assert!(out1.starts_with("ACQUIRED"), "C1 must acquire: {out1}");

    c2.send("TRY_ACQUIRE").await;
    let out2 = c2.read_response().await;
    assert!(out2.starts_with("ACQUIRED"), "C2 must acquire: {out2}");

    // 2. Contender 3 tries non-blocking acquire -> receives BUSY
    c3.send("TRY_ACQUIRE").await;
    let out3 = c3.read_response().await;
    assert_eq!(out3.trim(), "BUSY", "C3 must be BUSY");

    // 3. Conflicting configuration check: attempting 4 slots fails
    let exe = std::env::current_exe().expect("current_exe");
    let conflict_output = Command::new(exe)
        .arg("host_slot_test_subprocess_entry")
        .arg("--nocapture")
        .env("JULIE_TEST_HOST_SLOT_MODE", "conflict_check")
        .env("JULIE_TEST_SCHEDULER_DIR", &scheduler_dir)
        .env("JULIE_TEST_SLOT_COUNT", "4")
        .env_remove("NEXTEST")
        .env_remove("NEXTEST_RUNNER")
        .env_remove("NEXTEST_TEST_BINARY_PROTOCOL_VERSION")
        .output()
        .await
        .expect("run conflict check");

    let stdout_str = String::from_utf8_lossy(&conflict_output.stdout);
    assert!(
        stdout_str.contains("RESOURCE_CONFIG_CONFLICT"),
        "Expected conflict, got: {stdout_str}"
    );
    assert!(
        !scheduler_dir.join("index-2.lock").exists(),
        "index-2.lock must not appear under conflicting configuration"
    );
    assert!(
        !scheduler_dir.join("index-3.lock").exists(),
        "index-3.lock must not appear under conflicting configuration"
    );

    // 4. Contender 3 initiates blocking acquire
    c3.send("ACQUIRE_BLOCKING").await;

    // Verify Contender 3 is blocked (times out on immediate read)
    let blocked = tokio::time::timeout(Duration::from_millis(300), c3.read_response()).await;
    assert!(
        blocked.is_err(),
        "C3 must remain blocked while 2 slots held"
    );

    // 5. Release Contender 1
    c1.send("RELEASE").await;
    let rel1 = c1.read_response().await;
    assert!(rel1.starts_with("RELEASED"), "C1 must release: {rel1}");

    // 6. Contender 3 unblocks and acquires!
    let out3_unblocked = tokio::time::timeout(Duration::from_secs(5), c3.read_response())
        .await
        .expect("C3 timed out unblocking");
    assert!(
        out3_unblocked.starts_with("ACQUIRED"),
        "C3 must acquire after C1 release: {out3_unblocked}"
    );

    // 7. Cleanup
    c2.send("RELEASE").await;
    let rel2 = c2.read_response().await;
    assert!(rel2.starts_with("RELEASED"));

    c3.send("RELEASE").await;
    let rel3 = c3.read_response().await;
    assert!(rel3.starts_with("RELEASED"));

    c1.wait_success().await;
    c2.wait_success().await;
    c3.wait_success().await;
}

#[tokio::test]
async fn cancelled_slot_waiter_leaves_no_leaked_lock() {
    let temp_dir = tempfile::tempdir().unwrap();
    let scheduler_dir = temp_dir.path().join("scheduler");
    let pool = IndexJobAdmission::open_or_init(&scheduler_dir, Some(1)).unwrap();

    let holder = pool.try_acquire().expect("acquire sole slot");
    assert_eq!(holder.slot_index(), 0);

    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    let pool_arc = std::sync::Arc::new(pool);
    let pool_ref = pool_arc.clone();
    let waiter = tokio::spawn(async move { pool_ref.acquire(None, Some(&token_clone)).await });

    tokio::time::sleep(Duration::from_millis(80)).await;
    cancel_token.cancel();

    let result = waiter.await.unwrap();
    assert!(matches!(result, Err(AdmissionError::Cancelled)));

    // Drop holder -> slot 0 must be immediately acquirable (no leak!)
    drop(holder);

    let reacquired = pool_arc
        .try_acquire()
        .expect("slot 0 must be acquirable after drop");
    assert_eq!(reacquired.slot_index(), 0);
}

#[tokio::test]
async fn timeout_slot_waiter_leaves_no_leaked_lock() {
    let temp_dir = tempfile::tempdir().unwrap();
    let scheduler_dir = temp_dir.path().join("scheduler");
    let pool = IndexJobAdmission::open_or_init(&scheduler_dir, Some(1)).unwrap();

    let holder = pool.try_acquire().expect("acquire sole slot");
    let timeout_res = pool.acquire_timeout(Duration::from_millis(100), None).await;
    assert!(matches!(timeout_res, Err(AdmissionError::Timeout)));

    drop(holder);

    let next = pool
        .try_acquire()
        .expect("must acquire after timeout drops holder");
    assert_eq!(next.slot_index(), 0);
}

#[tokio::test]
async fn conflict_without_active_holders_allows_reconfiguration() {
    let temp_dir = tempfile::tempdir().unwrap();
    let scheduler_dir = temp_dir.path().join("scheduler");

    // Open pool with 2 slots
    let pool1 = IndexJobAdmission::open_or_init(&scheduler_dir, Some(2)).unwrap();
    assert_eq!(pool1.max_slots(), 2);
    drop(pool1);

    // No slots are held: reconfiguration to 4 slots should succeed
    let pool2 = IndexJobAdmission::open_or_init(&scheduler_dir, Some(4)).unwrap();
    assert_eq!(pool2.max_slots(), 4);
    assert!(scheduler_dir.join("index-2.lock").exists());
    assert!(scheduler_dir.join("index-3.lock").exists());
}
