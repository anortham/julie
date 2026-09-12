use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> std::path::PathBuf {
    crate::tests::request_process_helpers::resolve_julie_binary()
}

fn home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn service_json(home: &tempfile::TempDir) -> std::path::PathBuf {
    home.path().join("service.json")
}

fn wait_for(path: &std::path::Path, present: bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() == present {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn wait_for_fresh_record(
    path: &std::path::Path,
    stale_port: u16,
    timeout: Duration,
) -> Option<serde_json::Value> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(record) = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .filter(|record| {
                record["port"]
                    .as_u64()
                    .is_some_and(|port| port != u64::from(stale_port))
            })
        {
            return Some(record);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn shim_starts_the_service_answers_and_service_exits_when_idle() {
    let home = home();
    let mut shim = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .env("JULIE_SERVICE_IDLE_SECS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = shim.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{{"_meta":{{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}"#
        )
        .unwrap();
    }
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    drop(shim.stdin.take());
    let out = shim.wait_with_output().unwrap();
    assert!(out.status.success(), "shim exit {:?}", out.status);
    let line = String::from_utf8(out.stdout).unwrap();
    assert!(line.contains("2026-07-28"), "got {line}");
    assert!(
        wait_for(&service_json(&home), false, Duration::from_secs(10)),
        "service did not exit when idle"
    );
}

#[test]
fn stale_service_json_is_replaced_by_a_fresh_service() {
    let home = home();
    let dead_port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    std::fs::write(
        service_json(&home),
        format!(
            r#"{{"port":{dead_port},"token":"{}","pid":0,"version":"{}","started_at":"0"}}"#,
            "0".repeat(64),
            env!("CARGO_PKG_VERSION")
        ),
    )
    .unwrap();
    let child = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .env("JULIE_SERVICE_IDLE_SECS", "1")
        .args(["service", "status"])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let record = wait_for_fresh_record(&service_json(&home), dead_port, Duration::from_secs(5))
        .expect("fresh service record");
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_ne!(record["port"], dead_port);
    assert!(wait_for(
        &service_json(&home),
        false,
        Duration::from_secs(10)
    ));
}

#[test]
fn version_mismatch_exits_3_with_the_exact_message() {
    let home = home();
    let mut svc = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .env("JULIE_SERVICE_IDLE_SECS", "0")
        .arg("service")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(service_json(&home)).unwrap()).unwrap();
    record["version"] = "0.0.0-other".into();
    std::fs::write(service_json(&home), record.to_string()).unwrap();
    let out = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .args(["service", "status"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(
        String::from_utf8_lossy(&out.stderr).trim(),
        format!(
            "julie: service version 0.0.0-other does not match client version {}; run: \"{}\" service restart; then restart old harness clients",
            env!("CARGO_PKG_VERSION"),
            bin().display()
        )
    );
    let _ = svc.kill();
}

#[test]
fn restart_stops_mismatched_service_and_starts_current_binary() {
    let home = home();
    let mut service = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .env("JULIE_SERVICE_IDLE_SECS", "0")
        .arg("service")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(service_json(&home)).unwrap()).unwrap();
    let old_pid = record["pid"].as_u64().unwrap();
    record["version"] = "0.0.0-other".into();
    std::fs::write(service_json(&home), record.to_string()).unwrap();

    let restarted = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .args(["service", "restart"])
        .output()
        .unwrap();
    assert!(
        restarted.status.success(),
        "{}",
        String::from_utf8_lossy(&restarted.stderr)
    );
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(service_json(&home)).unwrap()).unwrap();
    assert_eq!(record["version"], env!("CARGO_PKG_VERSION"));
    assert_ne!(record["pid"].as_u64(), Some(old_pid));
    assert!(service.try_wait().unwrap().is_some());

    let stopped = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .args(["service", "stop"])
        .output()
        .unwrap();
    assert!(stopped.status.success());
    let _ = service.wait();
}

#[test]
fn service_stop_exits_the_service() {
    let home = home();
    let mut svc = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .env("JULIE_SERVICE_IDLE_SECS", "0")
        .arg("service")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    let out = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .args(["service", "stop"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(wait_for(
        &service_json(&home),
        false,
        Duration::from_secs(5)
    ));
    let status = svc.wait().unwrap();
    assert!(status.success());
}
