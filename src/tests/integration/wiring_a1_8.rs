//! A1.8 end-to-end wiring tests.
//!
//! These tests exercise the integrated `julie-server` compat shim,
//! `julie-adapter`, and `julie-daemon` binaries against a freshly built
//! target tree. They verify the four acceptance scenarios from the plan:
//!
//!   1. `julie-server` (no args) on a clean `JULIE_HOME` spawns the daemon
//!      and round-trips an MCP `initialize` request.
//!   2. `julie-adapter` invoked directly does the same.
//!   3. `julie-adapter` against an already-running daemon attaches without
//!      spawning a duplicate.
//!   4. The daemon survives adapter exit (detached spawn); the adapter
//!      exits cleanly when the daemon dies.
//!
//! All tests run UNIX-only — the cross-binary process orchestration is
//! identical on Windows but the test harness has not been wired for it yet
//! (see plan §A1.8 acceptance).

#![cfg(test)]
#![cfg(unix)]

use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::daemon::pid::PidFile;
use crate::paths::DaemonPaths;

// ---------------------------------------------------------------------------
// Shared helpers (mirror src/tests/integration/legacy_migration.rs::e2e)
// ---------------------------------------------------------------------------

/// Locate a binary in `target/<profile>/`. Tries debug first, then release.
fn binary_path(name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    for profile in &["debug", "release"] {
        let candidate = PathBuf::from(manifest_dir)
            .join("target")
            .join(profile)
            .join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    panic!(
        "binary `{}` not found in target/debug or target/release. \
         Run `cargo build --bin julie-server --bin julie-daemon --bin julie-adapter` first.",
        name
    );
}

/// Send a minimal MCP `initialize` to an adapter child and read one
/// response line. Closes stdin (EOF) so the adapter exits cleanly after
/// responding — caller must not rely on the adapter staying alive.
/// Returns the response or panics on timeout.
fn round_trip_initialize(adapter: &mut Child, timeout: Duration) -> String {
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"wiring-a1_8-test","version":"0.0.1"}}}"#;
    let mut stdin = adapter.stdin.take().expect("adapter stdin");
    writeln!(stdin, "{}", initialize).expect("write initialize");
    stdin.flush().expect("flush initialize");
    drop(stdin); // EOF: adapter exits gracefully after responding.

    let stdout = adapter.stdout.take().expect("adapter stdout");
    let mut reader = BufReader::new(stdout);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut response = String::new();
        let _ = reader.read_line(&mut response);
        let _ = tx.send(response);
    });

    rx.recv_timeout(timeout)
        .expect("adapter must respond to initialize within timeout")
}

/// Send a minimal MCP `initialize` WITHOUT closing stdin. The adapter
/// stays running afterward, so the caller can observe its lifecycle
/// reaction to external events (e.g. the daemon dying).
fn send_initialize_keep_stdin(
    adapter: &mut Child,
    timeout: Duration,
) -> String {
    let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"wiring-a1_8-test","version":"0.0.1"}}}"#;
    // Borrow stdin instead of taking it, so the caller's Child still owns
    // the handle. This keeps the adapter's stdin open after we return.
    {
        let stdin = adapter.stdin.as_mut().expect("adapter stdin");
        writeln!(stdin, "{}", initialize).expect("write initialize");
        stdin.flush().expect("flush initialize");
    }

    let stdout = adapter.stdout.take().expect("adapter stdout");
    let mut reader = BufReader::new(stdout);

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut response = String::new();
        let _ = reader.read_line(&mut response);
        let _ = tx.send(response);
    });

    rx.recv_timeout(timeout)
        .expect("adapter must respond to initialize within timeout")
}

/// Poll until the daemon writes its discovery.json (i.e. it has reached
/// `phase=running`). Returns true on success.
fn wait_for_daemon_ready(paths: &DaemonPaths, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if paths.discovery_file().exists() && paths.daemon_pid().exists() {
            if let Some(pid) = PidFile::read_pid(&paths.daemon_pid()) {
                if PidFile::is_process_alive(pid) {
                    return true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// RAII guard: kills a child process (and the daemon it might have spawned)
/// on drop so panicking tests don't leak processes.
struct DaemonGuard {
    paths: DaemonPaths,
    children: Vec<Child>,
}

impl DaemonGuard {
    fn new(paths: DaemonPaths) -> Self {
        Self {
            paths,
            children: Vec::new(),
        }
    }

    fn push(&mut self, child: Child) {
        self.children.push(child);
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        // Kill any direct children we tracked.
        for mut child in self.children.drain(..) {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Also kill any daemon spawned via the launcher (it's detached, so
        // not a direct child of the test process).
        if let Some(pid) = PidFile::read_pid(&self.paths.daemon_pid()) {
            if PidFile::is_process_alive(pid) {
                unsafe {
                    // SIGKILL: the test is already over, no graceful shutdown
                    // needed. Daemon's own cleanup (file removal) is best-
                    // effort either way.
                    libc::kill(pid as libc::pid_t, libc::SIGKILL);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test #1: julie-server (no args) spawns daemon, round-trips initialize
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_julie_server_no_args_spawns_daemon() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let julie_home = home.join(".julie");
    let paths = DaemonPaths::with_home(julie_home);
    paths.ensure_dirs().expect("ensure_dirs");

    let mut guard = DaemonGuard::new(paths.clone());

    let server_bin = binary_path("julie-server");
    let adapter = Command::new(&server_bin)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn julie-server (no args)");

    // Keep the child for guard cleanup, but pull stdio first.
    let mut adapter = adapter;
    let response = round_trip_initialize(&mut adapter, Duration::from_secs(60));
    let _ = adapter.kill();
    let _ = adapter.wait();
    guard.push(adapter);

    assert!(
        response.contains("\"jsonrpc\"") && response.contains("\"id\":1"),
        "julie-server (no args) must round-trip MCP initialize; got: {}",
        response.trim()
    );

    // Daemon must have been spawned (discovery.json + pid exist).
    assert!(
        paths.discovery_file().exists(),
        "daemon must have published discovery.json at {}",
        paths.discovery_file().display()
    );
    assert!(
        paths.daemon_pid().exists(),
        "daemon must have written daemon.pid at {}",
        paths.daemon_pid().display()
    );
}

// ---------------------------------------------------------------------------
// Test #2: julie-adapter directly spawns daemon, round-trips initialize
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_julie_adapter_direct_spawns_daemon() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let julie_home = home.join(".julie");
    let paths = DaemonPaths::with_home(julie_home);
    paths.ensure_dirs().expect("ensure_dirs");

    let mut guard = DaemonGuard::new(paths.clone());

    let adapter_bin = binary_path("julie-adapter");
    let mut adapter = Command::new(&adapter_bin)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn julie-adapter");

    let response = round_trip_initialize(&mut adapter, Duration::from_secs(60));
    let _ = adapter.kill();
    let _ = adapter.wait();
    guard.push(adapter);

    assert!(
        response.contains("\"jsonrpc\"") && response.contains("\"id\":1"),
        "julie-adapter (direct) must round-trip MCP initialize; got: {}",
        response.trim()
    );
    assert!(
        paths.discovery_file().exists(),
        "daemon must have published discovery.json at {}",
        paths.discovery_file().display()
    );
}

// ---------------------------------------------------------------------------
// Test #3: julie-adapter attaches to an already-running daemon
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_julie_adapter_attaches_to_running_daemon() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let julie_home = home.join(".julie");
    let paths = DaemonPaths::with_home(julie_home);
    paths.ensure_dirs().expect("ensure_dirs");

    let mut guard = DaemonGuard::new(paths.clone());

    // Pre-start julie-daemon so the adapter has something to attach to.
    let daemon_bin = binary_path("julie-daemon");
    let daemon = Command::new(&daemon_bin)
        .arg("start")
        .arg("--port")
        .arg("0")
        .arg("--no-dashboard")
        .env("HOME", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn julie-daemon start");
    guard.push(daemon);

    assert!(
        wait_for_daemon_ready(&paths, Duration::from_secs(45)),
        "pre-started daemon must reach readiness within 45s"
    );

    // Snapshot the PID before starting the adapter.
    let pre_pid =
        PidFile::read_pid(&paths.daemon_pid()).expect("daemon.pid must be readable");

    // Now run the adapter; it must attach, NOT spawn a replacement.
    let adapter_bin = binary_path("julie-adapter");
    let mut adapter = Command::new(&adapter_bin)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn julie-adapter");

    let response = round_trip_initialize(&mut adapter, Duration::from_secs(30));
    let _ = adapter.kill();
    let _ = adapter.wait();
    guard.push(adapter);

    assert!(
        response.contains("\"jsonrpc\"") && response.contains("\"id\":1"),
        "adapter must round-trip initialize through pre-started daemon; got: {}",
        response.trim()
    );

    // Crucial invariant: adapter must NOT have replaced the daemon.
    let post_pid =
        PidFile::read_pid(&paths.daemon_pid()).expect("daemon.pid must be readable post-adapter");
    assert_eq!(
        pre_pid, post_pid,
        "adapter must attach to running daemon — not spawn a replacement (pre_pid={}, post_pid={})",
        pre_pid, post_pid
    );
    assert!(
        PidFile::is_process_alive(post_pid),
        "pre-started daemon must still be alive after adapter exit"
    );
}

// ---------------------------------------------------------------------------
// Test #4: daemon survives adapter exit (detached spawn)
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_daemon_survives_adapter_exit() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let julie_home = home.join(".julie");
    let paths = DaemonPaths::with_home(julie_home);
    paths.ensure_dirs().expect("ensure_dirs");

    let mut guard = DaemonGuard::new(paths.clone());

    let adapter_bin = binary_path("julie-adapter");
    let mut adapter = Command::new(&adapter_bin)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn julie-adapter");

    let response = round_trip_initialize(&mut adapter, Duration::from_secs(60));
    assert!(
        response.contains("\"jsonrpc\""),
        "adapter must succeed on initialize before we test survival"
    );

    // Kill the adapter.
    let _ = adapter.kill();
    let _ = adapter.wait();
    guard.push(adapter);

    // Daemon must still be alive a moment later (detached spawn).
    std::thread::sleep(Duration::from_secs(1));
    let pid =
        PidFile::read_pid(&paths.daemon_pid()).expect("daemon.pid must exist after adapter exit");
    assert!(
        PidFile::is_process_alive(pid),
        "daemon (PID {}) must survive adapter exit — detached spawn is the contract",
        pid
    );
}

// ---------------------------------------------------------------------------
// Test #5: adapter exits cleanly when daemon dies
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_adapter_exits_when_daemon_dies() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let julie_home = home.join(".julie");
    let paths = DaemonPaths::with_home(julie_home);
    paths.ensure_dirs().expect("ensure_dirs");

    let mut guard = DaemonGuard::new(paths.clone());

    // Start the adapter (which spawns a detached daemon).
    let adapter_bin = binary_path("julie-adapter");
    let mut adapter = Command::new(&adapter_bin)
        .env("HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn julie-adapter");

    // Wait until the daemon has published discovery.json so we know it's up.
    assert!(
        wait_for_daemon_ready(&paths, Duration::from_secs(60)),
        "daemon must reach readiness within 60s"
    );

    let daemon_pid =
        PidFile::read_pid(&paths.daemon_pid()).expect("daemon.pid must be readable");

    // Send a minimal MCP initialize WITHOUT closing stdin. The adapter
    // stays running after the response.
    let response = send_initialize_keep_stdin(&mut adapter, Duration::from_secs(30));
    assert!(
        response.contains("\"jsonrpc\""),
        "adapter must complete one MCP round-trip before we kill the daemon"
    );

    // Kill the daemon. The daemon's parent is the adapter (cmd.spawn()
    // returns a child handle even though setsid() detached the session); the
    // zombie won't be reaped until the adapter exits. That's fine for this
    // test — we're asserting on adapter exit, not on the daemon's reaped
    // state.
    unsafe {
        libc::kill(daemon_pid as libc::pid_t, libc::SIGKILL);
    }

    // The adapter has no heartbeat on the HTTP transport — it only notices
    // the daemon is gone when forwarding the next request. Send a second
    // MCP request to force the adapter through its forward loop, where the
    // daemon's disappearance will surface as an HTTP error. The adapter's
    // retry logic (MAX_RETRIES=5 with exponential backoff) will then run
    // and eventually exit.
    {
        let stdin = adapter.stdin.as_mut().expect("adapter stdin");
        let _ = writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{}}}}"#
        );
        let _ = stdin.flush();
    }

    // Close stdin so the adapter doesn't wait for more requests once retries
    // are exhausted.
    drop(adapter.stdin.take());

    // Verify the adapter exits cleanly within a bounded time. The adapter
    // does MAX_RETRIES=5 attempts with exponential backoff (1+2+4+8+16=31s
    // pure sleep), then exits with an error. We allow 90s to absorb the
    // retry budget plus per-attempt HTTP timeouts. The contract under test
    // is "the adapter EVENTUALLY exits on its own" — not the tightness of
    // that bound.
    let exit_deadline = Instant::now() + Duration::from_secs(90);
    let mut exited = false;
    while Instant::now() < exit_deadline {
        match adapter.try_wait() {
            Ok(Some(_status)) => {
                exited = true;
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => break,
        }
    }

    if !exited {
        let _ = adapter.kill();
        let _ = adapter.wait();
        guard.push(adapter);
        panic!("adapter did not exit within 90s of daemon death");
    }
    let _ = adapter.wait();
    guard.push(adapter);
}
