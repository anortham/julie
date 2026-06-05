//! A1.8 end-to-end wiring tests.
//!
//! These tests exercise the `julie-server` binary against a freshly built
//! target tree. They verify the acceptance scenario from Phase 3c.3:
//!
//!   1. `julie-server` (no args) serves the MCP session **IN-PROCESS** and
//!      round-trips an `initialize` request **WITHOUT** forking a daemon
//!      (Phase 3c.3 cutover — the no-args path no longer publishes
//!      `discovery.json`).
//!
//! Tests #2–#5 (adapter spawn scenarios) were deleted in Phase 3d.1 when
//! `julie-adapter` was removed.
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
use std::time::Duration;

use crate::daemon::discovery::{DiscoveryFile, DiscoveryState};
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
         Run `cargo build --bin julie-server` first.",
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

/// Poll until the daemon writes its discovery.json (i.e. it has reached
/// `phase=running`). Returns true on success.
fn live_discovery_pid(paths: &DaemonPaths) -> Option<u32> {
    match DiscoveryFile::read_and_validate(&paths.discovery_file()) {
        DiscoveryState::Live(record) => Some(record.pid),
        DiscoveryState::Missing | DiscoveryState::Stale | DiscoveryState::Corrupt(_) => None,
    }
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
        if let Some(pid) = live_discovery_pid(&self.paths) {
            unsafe {
                // SIGKILL: the test is already over, no graceful shutdown
                // needed. Daemon's own cleanup (file removal) is best-
                // effort either way.
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test #1: julie-server (no args) serves IN-PROCESS — no daemon fork
//          (Phase 3c.3 cutover; was test_e2e_julie_server_no_args_spawns_daemon)
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_julie_server_no_args_serves_in_process() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let julie_home = home.join(".julie");
    let paths = DaemonPaths::with_home(julie_home);
    paths.ensure_dirs().expect("ensure_dirs");

    let mut guard = DaemonGuard::new(paths.clone());

    // Run the no-args server with its CWD pinned to the temp dir so the
    // in-process server resolves an isolated, empty workspace (logs + index
    // land under {home}/.julie/) instead of indexing the real repo root.
    let server_bin = binary_path("julie-server");
    let server = Command::new(&server_bin)
        .env("HOME", &home)
        .current_dir(&home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn julie-server (no args)");

    // Keep the child for guard cleanup, but pull stdio first.
    let mut server = server;
    let response = round_trip_initialize(&mut server, Duration::from_secs(60));
    let _ = server.kill();
    let _ = server.wait();
    guard.push(server);

    // The cutover serves MCP directly over stdio, so the round-trip still works.
    assert!(
        response.contains("\"jsonrpc\"") && response.contains("\"id\":1"),
        "julie-server (no args) must round-trip MCP initialize IN-PROCESS; got: {}",
        response.trim()
    );

    // The defining cutover invariant: the no-args path does NOT fork a daemon,
    // so NO discovery.json is ever published.
    assert!(
        live_discovery_pid(&paths).is_none(),
        "julie-server (no args) must serve IN-PROCESS and NOT spawn a daemon — \
         a live discovery.json appeared at {}, meaning the cutover regressed to \
         the fork-daemon path",
        paths.discovery_file().display()
    );
}

