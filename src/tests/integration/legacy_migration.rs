//! Tests for the A1.5 legacy migration gate.
//!
//! Unit tests cover the per-file classification logic in `check_or_refuse`
//! and `detect_and_attach`. End-to-end tests spawn a real legacy
//! `julie-server daemon` subprocess in an isolated `HOME` and verify that
//! `julie-daemon start` refuses to coexist (exit code 2) and that
//! `julie-adapter` attaches to the legacy HTTP endpoint instead of
//! spawning a duplicate daemon.

#![cfg(test)]

use std::fs;

use crate::daemon::legacy_migration::{check_or_refuse, detect_and_attach, MigrationDecision};
use crate::daemon::pid::PidFile;
use crate::daemon::singleton::SingletonLock;
use crate::daemon::transport::TransportEndpoint;
use crate::paths::DaemonPaths;

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

/// Empty JULIE_HOME (no daemon ever ran) → `ProceedAndUnlink` with no files
/// to clean. The kernel allows the new daemon to start without any cleanup
/// effort.
#[test]
fn test_check_or_refuse_clean_state_proceeds() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    match check_or_refuse(&paths).expect("check_or_refuse must succeed on clean state") {
        MigrationDecision::ProceedAndUnlink { files_to_clean } => {
            assert!(
                files_to_clean.is_empty(),
                "no legacy files exist, nothing to clean: got {:?}",
                files_to_clean
            );
        }
        other => panic!("expected ProceedAndUnlink, got {:?}", other),
    }
}

/// A `daemon.pid` file whose recorded PID is no longer alive → safe to
/// proceed; the legacy file is listed for unlink.
#[test]
fn test_check_or_refuse_dead_pid_proceeds() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // Write a PID file with a definitely-dead PID. Use a value far above the
    // typical Linux/macOS max so the kill(0) probe returns ESRCH.
    let pid_path = paths.daemon_pid();
    fs::write(&pid_path, "99999999\n").unwrap();

    // Side-effect: PidFile::check_status removes Dead files. So the gate
    // returns no files_to_clean (because the file was already gone by the
    // time the gate inspected it for "still exists on disk"), but the
    // verdict must be ProceedAndUnlink — which is what matters.
    match check_or_refuse(&paths).expect("check_or_refuse on dead PID file must succeed") {
        MigrationDecision::ProceedAndUnlink { files_to_clean: _ } => {
            // File may or may not be in the cleanup list depending on the
            // order of probes (check_status side-effect-deletes it). What
            // matters: the verdict is ProceedAndUnlink, not LegacyDaemonAlive.
            // And after the gate runs, the on-disk file is gone either way.
            assert!(
                !pid_path.exists(),
                "daemon.pid with a dead PID must not survive the gate"
            );
        }
        other => panic!(
            "expected ProceedAndUnlink for a dead-PID daemon.pid, got {:?}",
            other
        ),
    }
}

/// A `daemon.singleton.lock` file with no process holding the fcntl lock →
/// treat as dead, list for unlink.
#[test]
fn test_check_or_refuse_unowned_singleton_lock_proceeds() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // Create an empty singleton lock file — no fcntl holder. The legacy
    // daemon is presumed dead.
    let lock_path = paths.daemon_singleton_lock();
    fs::write(&lock_path, b"").unwrap();

    match check_or_refuse(&paths)
        .expect("check_or_refuse on unowned singleton lock must succeed")
    {
        MigrationDecision::ProceedAndUnlink { files_to_clean } => {
            assert!(
                files_to_clean.iter().any(|p| p == &lock_path),
                "files_to_clean must include the unowned singleton lock: {:?}",
                files_to_clean
            );
        }
        other => panic!(
            "expected ProceedAndUnlink for an unowned singleton lock, got {:?}",
            other
        ),
    }
}

/// A `daemon.pid` whose recorded PID matches the current process (alive!) →
/// gate refuses with `LegacyDaemonAlive`.
#[test]
fn test_check_or_refuse_live_pid_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // PidFile::create writes a PID file for the current process. The current
    // process is, by definition, alive — so the gate must classify this as a
    // live legacy daemon.
    let _pid_handle = PidFile::create(&paths.daemon_pid()).unwrap();

    match check_or_refuse(&paths).expect("check_or_refuse must return a verdict") {
        MigrationDecision::LegacyDaemonAlive { pid, hint } => {
            assert_eq!(
                pid,
                std::process::id(),
                "the gate must report the live PID, not 0 or some other value"
            );
            assert!(
                !hint.is_empty(),
                "the diagnostic hint must be non-empty (operators read this)"
            );
        }
        other => panic!(
            "expected LegacyDaemonAlive for a live-PID daemon.pid, got {:?}",
            other
        ),
    }
}

/// A held `daemon.singleton.lock` (some other process holds the fcntl lock) →
/// gate refuses. We simulate "other process" using a `SingletonLock` guard
/// held in the same test process; POSIX `flock` contends across distinct
/// open-file-descriptions, so a fresh `try_lock_exclusive` from
/// `check_or_refuse` will see `WouldBlock`.
#[test]
fn test_check_or_refuse_held_singleton_lock_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // Hold the singleton lock in this process. The gate's fresh fd will fail
    // to acquire it.
    let _holder = SingletonLock::try_acquire(&paths.daemon_singleton_lock())
        .expect("first acquire must succeed");

    match check_or_refuse(&paths).expect("check_or_refuse must return a verdict") {
        MigrationDecision::LegacyDaemonAlive { pid: _, hint } => {
            assert!(
                hint.to_lowercase().contains("singleton")
                    || hint.to_lowercase().contains("lock")
                    || hint.to_lowercase().contains("legacy"),
                "diagnostic hint must mention the contended file, got: {}",
                hint
            );
        }
        other => panic!(
            "expected LegacyDaemonAlive for a held singleton lock, got {:?}",
            other
        ),
    }
}

/// No legacy files → `detect_and_attach` returns None (no endpoint to attach
/// to, the launcher should fall through to its normal spawn path).
#[test]
fn test_detect_and_attach_no_legacy_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    assert!(
        detect_and_attach(&paths).is_none(),
        "no daemon.port + no daemon.pid → must return None"
    );
}

/// Legacy daemon detected via `daemon.port` + live `daemon.pid` →
/// `detect_and_attach` returns a TransportEndpoint pointing at
/// 127.0.0.1:<port>.
#[test]
fn test_detect_and_attach_legacy_port_returns_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // Simulate a live legacy daemon: PID file for the current process +
    // daemon.port pointing at a (made-up but valid) port.
    let _pid_handle = PidFile::create(&paths.daemon_pid()).unwrap();
    let legacy_port: u16 = 17890; // Arbitrary; we don't connect, just inspect.
    fs::write(&paths.daemon_port(), legacy_port.to_string()).unwrap();

    let endpoint = detect_and_attach(&paths)
        .expect("legacy port + live pid must return Some(endpoint)");

    // Endpoint must be Streamable HTTP on 127.0.0.1:<port>.
    match endpoint {
        TransportEndpoint::StreamableHttp { host, port, .. } => {
            assert_eq!(host, "127.0.0.1", "legacy daemon binds to localhost");
            assert_eq!(port, legacy_port, "endpoint port must match daemon.port");
        }
    }
}

/// Defensive: legacy daemon.port present but `daemon.pid` records a dead PID →
/// `detect_and_attach` returns None (don't attach to a phantom daemon).
#[test]
fn test_detect_and_attach_dead_pid_with_port_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // daemon.port present but daemon.pid records a dead PID.
    fs::write(&paths.daemon_port(), "17890").unwrap();
    fs::write(&paths.daemon_pid(), "99999999\n").unwrap();

    assert!(
        detect_and_attach(&paths).is_none(),
        "dead daemon.pid must veto attaching to a phantom legacy daemon"
    );
}

// ---------------------------------------------------------------------------
// End-to-end tests: real subprocesses
// ---------------------------------------------------------------------------
//
// These tests spawn real `julie-server daemon`, `julie-daemon`, and
// `julie-adapter` processes against an isolated `HOME=<tempdir>`. They
// require the binaries to already be built (see binary_path() below). If
// any test in this module fails because a binary is missing, run:
//
//     cargo build --bin julie-server --bin julie-daemon --bin julie-adapter
//
// before re-running the suite. The binaries are tiny shims around the lib
// crate, so the incremental rebuild cost is small.

#[cfg(unix)]
mod e2e {
    use std::env;
    use std::io::{BufRead, BufReader, Write};
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::paths::DaemonPaths;

    /// Locate a binary in `target/<profile>/`. Tries `debug` first (the usual
    /// `cargo test` profile), then `release` as a fallback for CI matrices
    /// that run tests in release mode.
    fn binary_path(name: &str) -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        for profile in &["debug", "release"] {
            let suffix = if cfg!(windows) { ".exe" } else { "" };
            let candidate = PathBuf::from(manifest_dir)
                .join("target")
                .join(profile)
                .join(format!("{}{}", name, suffix));
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

    /// Spawn the legacy `julie-server daemon` binary with `HOME=<tempdir>`
    /// so all daemon paths route under `<tempdir>/.julie/`. Returns the
    /// child process and the DaemonPaths the subprocess will use.
    ///
    /// IMPORTANT: `DaemonPaths::new()` (called by the subprocess) reads
    /// `$HOME` and joins `.julie`. The test process must mirror that exact
    /// path or it will inspect the wrong directory.
    fn spawn_legacy_daemon(home: &std::path::Path) -> (Child, DaemonPaths) {
        let julie_home = home.join(".julie");
        let paths = DaemonPaths::with_home(julie_home.clone());
        paths.ensure_dirs().expect("ensure_dirs on temp HOME");

        let bin = binary_path("julie-server");
        let mut cmd = Command::new(&bin);
        cmd.arg("daemon")
            .arg("--port")
            .arg("0") // Auto-assign port to avoid clashes in parallel tests.
            .arg("--no-dashboard")
            .env("HOME", home)
            // Some embedded test envs ignore HOME — set XDG_CONFIG_HOME as a
            // belt-and-suspenders backup, even though Julie doesn't read it.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .expect("failed to spawn julie-server daemon subprocess");

        (child, paths)
    }

    /// Poll for the legacy daemon to reach readiness: `daemon.pid` exists +
    /// `daemon.port` exists + the PID inside daemon.pid is alive. Returns
    /// true on success, false on timeout.
    fn wait_for_legacy_ready(paths: &DaemonPaths, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if paths.daemon_pid().exists() && paths.daemon_port().exists() {
                if let Some(pid) = crate::daemon::pid::PidFile::read_pid(&paths.daemon_pid()) {
                    if crate::daemon::pid::PidFile::is_process_alive(pid) {
                        return true;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    /// RAII guard: ensures the legacy daemon child process is terminated
    /// when the guard drops, even on test panic. Uses SIGKILL because tests
    /// don't need graceful shutdown.
    struct DaemonGuard {
        child: Child,
    }

    impl Drop for DaemonGuard {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    /// End-to-end: live legacy daemon + new `julie-daemon start` →
    /// new daemon refuses with exit code 2 and stderr mentions "legacy".
    #[test]
    fn test_e2e_legacy_daemon_refuses_new_daemon() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();

        let (child, paths) = spawn_legacy_daemon(&home);
        let _guard = DaemonGuard { child };

        // Wait for legacy daemon to become ready.
        assert!(
            wait_for_legacy_ready(&paths, Duration::from_secs(45)),
            "legacy daemon must reach readiness within 45s; daemon.pid={}, daemon.port={}",
            paths.daemon_pid().display(),
            paths.daemon_port().display(),
        );

        // Now invoke the new `julie-daemon start` against the SAME HOME.
        // It must refuse and exit with code 2.
        let new_daemon_bin = binary_path("julie-daemon");
        let output = Command::new(&new_daemon_bin)
            .arg("start")
            .env("HOME", &home)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("failed to invoke julie-daemon start");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code();

        assert_eq!(
            exit_code,
            Some(2),
            "julie-daemon start must exit 2 when legacy is alive; got {:?}, stderr={}",
            exit_code,
            stderr
        );
        assert!(
            stderr.to_lowercase().contains("legacy"),
            "stderr must explain the refusal mentions \"legacy\"; got: {}",
            stderr
        );
    }

    /// End-to-end: live legacy daemon + new `julie-adapter` →
    /// adapter attaches to legacy HTTP endpoint, basic MCP request succeeds.
    /// No new daemon process is spawned (legacy daemon.pid is preserved).
    #[test]
    fn test_e2e_legacy_daemon_attached_by_adapter() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();

        let (child, paths) = spawn_legacy_daemon(&home);
        let _guard = DaemonGuard { child };

        assert!(
            wait_for_legacy_ready(&paths, Duration::from_secs(45)),
            "legacy daemon must reach readiness within 45s"
        );

        // Snapshot the legacy PID before starting the adapter.
        let legacy_pid = crate::daemon::pid::PidFile::read_pid(&paths.daemon_pid())
            .expect("legacy daemon.pid must be readable");

        // Spawn julie-adapter against the same HOME. Pipe stdin/stdout so we
        // can send a minimal MCP initialize request.
        let adapter_bin = binary_path("julie-adapter");
        let mut adapter = Command::new(&adapter_bin)
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn julie-adapter");

        // Send a minimal MCP initialize request.
        let initialize = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"legacy-migration-test","version":"0.0.1"}}}"#;
        let mut stdin = adapter.stdin.take().expect("adapter stdin");
        writeln!(stdin, "{}", initialize).expect("write initialize");
        stdin.flush().expect("flush initialize");
        drop(stdin); // EOF signals "no more requests"; adapter exits gracefully.

        // Read a single response line. The adapter should forward the
        // initialize through to the legacy daemon and pipe back the response.
        let stdout = adapter.stdout.take().expect("adapter stdout");
        let mut reader = BufReader::new(stdout);
        let mut response = String::new();

        // Bounded wait: read with a timeout. tokio/async isn't in scope here,
        // so we use a join handle + sleep loop.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = reader.read_line(&mut response);
            let _ = tx.send(response);
        });

        let response = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("adapter must respond to initialize within 30s");

        // Ensure adapter is dead before further checks.
        let _ = adapter.kill();
        let _ = adapter.wait();

        assert!(
            response.contains("\"jsonrpc\"") && response.contains("\"id\":1"),
            "adapter must forward initialize response back from legacy daemon; got: {}",
            response.trim()
        );

        // The legacy daemon must still be alive and the same PID. The adapter
        // must NOT have spawned a replacement.
        let post_pid = crate::daemon::pid::PidFile::read_pid(&paths.daemon_pid())
            .expect("legacy daemon.pid must still be readable after adapter exit");
        assert_eq!(
            post_pid, legacy_pid,
            "adapter must attach to legacy daemon — not spawn a replacement"
        );
        assert!(
            crate::daemon::pid::PidFile::is_process_alive(legacy_pid),
            "legacy daemon must still be alive after adapter session"
        );
    }
}


