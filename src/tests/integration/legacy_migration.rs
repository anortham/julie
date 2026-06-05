//! Tests for the A1.5 legacy migration gate.
//!
//! Unit tests cover the per-file classification logic in `check_or_refuse`
//! and `detect_and_attach`. End-to-end tests spawn a real `julie-daemon`
//! subprocess in an isolated `HOME` and verify that `julie-daemon start`
//! refuses to coexist with a legacy daemon (exit code 2).
//!
//! The adapter E2E test (`test_e2e_legacy_daemon_attached_by_adapter`) was
//! deleted in Phase 3d.1 when `julie-adapter` was removed.

#![cfg(test)]

use std::fs;

use crate::daemon::legacy_migration::{MigrationDecision, check_or_refuse, detect_and_attach};
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

    match check_or_refuse(&paths).expect("check_or_refuse on unowned singleton lock must succeed") {
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
fn test_detect_and_attach_reads_legacy_mcp_transport_not_dashboard_port() {
    let dir = tempfile::tempdir().unwrap();
    let paths = DaemonPaths::with_home(dir.path().to_path_buf());
    paths.ensure_dirs().unwrap();

    // Simulate a live legacy daemon. The legacy MCP endpoint is published in
    // daemon-mcp-transport.json; daemon.port is the dashboard port and must
    // not be used for adapter MCP attachment.
    let _pid_handle = PidFile::create(&paths.daemon_pid()).unwrap();
    let dashboard_port: u16 = 17890;
    let mcp_port: u16 = 17891;
    fs::write(&paths.daemon_port(), dashboard_port.to_string()).unwrap();
    TransportEndpoint::streamable_http("127.0.0.1", mcp_port, "/mcp", "/mcp/ready", None)
        .unwrap()
        .publish_discovery(&paths.daemon_mcp_transport())
        .unwrap();

    let endpoint =
        detect_and_attach(&paths).expect("legacy MCP discovery + live pid must return endpoint");

    match endpoint {
        TransportEndpoint::StreamableHttp { host, port, .. } => {
            assert_eq!(host, "127.0.0.1", "legacy daemon binds to localhost");
            assert_eq!(
                port, mcp_port,
                "endpoint port must match MCP transport discovery"
            );
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
// These tests spawn real `julie-daemon` processes against an isolated
// `HOME=<tempdir>`. They require the binary to already be built (see
// binary_path() below). If any test in this module fails because a binary
// is missing, run:
//
//     cargo build --bin julie-daemon
//
// before re-running the suite. The binary is a tiny shim around the lib
// crate, so the incremental rebuild cost is small.

#[cfg(unix)]
mod e2e {
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    use crate::daemon::http_transport::{MCP_PATH, READINESS_PATH};
    use crate::daemon::pid::PidFile;
    use crate::daemon::singleton::SingletonLock;
    use crate::daemon::transport::TransportEndpoint;
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
             Run `cargo build --bin julie-daemon` first.",
            name
        );
    }

    fn legacy_paths_for_home(home: &std::path::Path) -> DaemonPaths {
        let julie_home = home.join(".julie");
        let paths = DaemonPaths::with_home(julie_home.clone());
        paths.ensure_dirs().expect("ensure_dirs on temp HOME");
        paths
    }

    struct LegacyDaemonFixture {
        paths: DaemonPaths,
        port: u16,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
        _pid_file: PidFile,
        _singleton_lock: SingletonLock,
    }

    impl LegacyDaemonFixture {
        fn spawn(home: &std::path::Path) -> Self {
            let paths = legacy_paths_for_home(home);
            let listener =
                TcpListener::bind("127.0.0.1:0").expect("bind fake legacy HTTP MCP listener");
            listener
                .set_nonblocking(true)
                .expect("set fake legacy listener nonblocking");
            let port = listener
                .local_addr()
                .expect("fake legacy listener local_addr")
                .port();

            let singleton_lock = SingletonLock::try_acquire(&paths.daemon_singleton_lock())
                .expect("hold legacy singleton lock");
            let pid_file = PidFile::create(&paths.daemon_pid()).expect("write legacy pid file");
            fs::write(paths.daemon_port(), format!("{port}\n"))
                .expect("write legacy dashboard port");
            fs::write(paths.daemon_state(), "ready\n").expect("write legacy ready state");
            TransportEndpoint::streamable_http("127.0.0.1", port, MCP_PATH, READINESS_PATH, None)
                .expect("build fake legacy transport endpoint")
                .publish_discovery(&paths.daemon_mcp_transport())
                .expect("publish fake legacy MCP transport discovery");

            let stop = Arc::new(AtomicBool::new(false));
            let stop_for_thread = Arc::clone(&stop);
            let thread = thread::spawn(move || {
                while !stop_for_thread.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => handle_legacy_http_connection(stream),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            });

            Self {
                paths,
                port,
                stop,
                thread: Some(thread),
                _pid_file: pid_file,
                _singleton_lock: singleton_lock,
            }
        }

        fn wait_ready(&self, timeout: Duration) -> bool {
            let deadline = Instant::now() + timeout;
            while Instant::now() < deadline {
                let pid_alive = PidFile::read_pid(&self.paths.daemon_pid())
                    .map(PidFile::is_process_alive)
                    .unwrap_or(false);
                let endpoint_ready =
                    TransportEndpoint::read_discovery(&self.paths.daemon_mcp_transport())
                        .map(|endpoint| endpoint.probe_readiness().is_ready())
                        .unwrap_or(false);
                if pid_alive && endpoint_ready {
                    return true;
                }
                thread::sleep(Duration::from_millis(25));
            }
            false
        }
    }

    impl Drop for LegacyDaemonFixture {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = TcpStream::connect(("127.0.0.1", self.port));
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn handle_legacy_http_connection(mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let request = match read_http_request(&mut stream) {
            Some(request) => request,
            None => return,
        };

        let header_end = match find_header_end(&request) {
            Some(end) => end,
            None => return,
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let request_line = headers.lines().next().unwrap_or_default();
        let body = String::from_utf8_lossy(&request[header_end..]).to_string();

        if request_line.starts_with("GET /mcp/ready ") {
            write_http_response(&mut stream, "200 OK", "ok", "text/plain");
            return;
        }

        if request_line.starts_with("POST /mcp ") {
            if body.contains("\"method\":\"initialize\"")
                || body.contains("\"method\": \"initialize\"")
            {
                let id = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|value| value.get("id").cloned())
                    .unwrap_or_else(|| serde_json::json!(1));
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "serverInfo": {
                            "name": "legacy-migration-fixture",
                            "version": "0.0.0"
                        }
                    }
                });
                write_http_response(
                    &mut stream,
                    "200 OK",
                    &response.to_string(),
                    "application/json",
                );
            } else {
                write_http_empty_response(&mut stream, "202 Accepted");
            }
            return;
        }

        write_http_empty_response(&mut stream, "404 Not Found");
    }

    fn read_http_request(stream: &mut TcpStream) -> Option<Vec<u8>> {
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => return None,
                Ok(n) => {
                    request.extend_from_slice(&buf[..n]);
                    if http_request_complete(&request) {
                        return Some(request);
                    }
                }
                Err(_) => return None,
            }
        }
    }

    fn http_request_complete(request: &[u8]) -> bool {
        let Some(header_end) = find_header_end(request) else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        request.len() >= header_end + content_length
    }

    fn find_header_end(request: &[u8]) -> Option<usize> {
        request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|pos| pos + 4)
    }

    fn write_http_response(stream: &mut TcpStream, status: &str, body: &str, content_type: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    }

    fn write_http_empty_response(stream: &mut TcpStream, status: &str) {
        let response =
            format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(response.as_bytes());
    }

    /// End-to-end: live legacy daemon + new `julie-daemon start` →
    /// new daemon refuses with exit code 2 and stderr mentions "legacy".
    #[test]
    fn test_e2e_legacy_daemon_refuses_new_daemon() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = tmp.path().to_path_buf();

        let fixture = LegacyDaemonFixture::spawn(&home);

        // Wait for legacy daemon to become ready.
        assert!(
            fixture.wait_ready(Duration::from_secs(5)),
            "legacy fixture must reach readiness within 5s; daemon.pid={}, transport={}",
            fixture.paths.daemon_pid().display(),
            fixture.paths.daemon_mcp_transport().display(),
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

}
