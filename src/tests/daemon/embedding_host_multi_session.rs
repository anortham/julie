//! Acceptance test (Phase 3b, Task 8, HARD GATE): one resident embedding-host
//! serves three concurrent sessions backed by exactly one sidecar.
//!
//! ## Architecture
//!
//! Three `std::thread`s each call `connect_or_spawn_host`.  The first to reach
//! `is_host_live → false` spawns the real `julie-embedding-host` binary; the
//! other two also spawn it but those copies fail the **fs2 singleton lock**
//! (acquired before the factory/sidecar starts) and exit immediately.
//!
//! The stub sidecar (`stub_sidecar.py`) is a self-contained Python script:
//! - Appends one `"launch\n"` line to `$STUB_COUNTER_FILE` on startup — giving
//!   a **cross-process** launch count even though the sidecar is a separate
//!   OS process.
//! - Writes `os.getppid()` (= the host binary PID) to `$STUB_PPID_FILE` so
//!   the test can `SIGTERM` the host for a clean shutdown.
//! - Serves `health` / `embed_query` / `embed_batch` / `shutdown` using the
//!   standard sidecar wire protocol over stdin/stdout.
//!
//! The host binary is located via `locate_embedding_host` (sibling of the
//! current exe, then PATH).  The test prepends `target/debug/` to `PATH` so
//! the binary is found when running with `cargo nextest run -p julie`.
//!
//! ## HARD GATE assertions
//!
//! (a) All 3 clients return correctly-dimensioned vectors for both
//!     `embed_query` and `embed_batch`.
//! (b) The counter file contains **exactly one** line — one sidecar spawn.
//! (c) SIGTERM to the host PID → socket removed → lock re-acquirable.

#[cfg(all(test, unix))]
#[cfg(feature = "embeddings-sidecar")]
mod unix {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use fs2::FileExt as _;
    use serial_test::serial;

    use crate::embedding_host_launch::connect_or_spawn_host;
    use crate::paths::DaemonPaths;
    use julie_pipeline::embeddings::EmbeddingProvider;
    use julie_pipeline::embeddings::host_transport::HostAddress;

    /// Dimensionality the stub sidecar advertises.
    const STUB_DIMS: usize = 4;

    // -----------------------------------------------------------------------
    // RAII env guard: restores the previous value on drop.
    // SAFETY: serialised by #[serial] so no concurrent thread mutates the env.
    // -----------------------------------------------------------------------

    struct EnvGuard {
        key: String,
        previous: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::set_var(key, value.as_ref()) };
            Self { key: key.to_owned(), previous }
        }

        fn remove(key: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::remove_var(key) };
            Self { key: key.to_owned(), previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => unsafe { std::env::set_var(&self.key, v) },
                None => unsafe { std::env::remove_var(&self.key) },
            }
        }
    }

    // -----------------------------------------------------------------------
    // Python interpreter detection
    // -----------------------------------------------------------------------

    fn test_python_interpreter() -> String {
        if let Ok(py) = std::env::var("JULIE_TEST_PYTHON") {
            return py;
        }
        for candidate in &["python3", "python"] {
            if std::process::Command::new(candidate)
                .args(["-c", "import sys; sys.exit(0)"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return candidate.to_string();
            }
        }
        panic!("no python3/python found; set JULIE_TEST_PYTHON to override");
    }

    // -----------------------------------------------------------------------
    // Stub sidecar
    // -----------------------------------------------------------------------

    /// Write a self-contained Python stub sidecar to `dir/stub_sidecar.py`.
    ///
    /// On startup the script:
    /// - Appends `"launch\n"` to `$STUB_COUNTER_FILE` (cross-process count).
    /// - Writes `os.getppid()` (= host PID) to `$STUB_PPID_FILE`.
    /// - Serves the sidecar wire protocol over stdin/stdout.
    ///
    /// Vectors are deterministic: `[len(text) as f32; DIMS]`.
    fn write_stub(dir: &Path) -> PathBuf {
        let script = r#"#!/usr/bin/env python3
import json, sys, os

# Record this sidecar launch.
counter_file = os.environ.get("STUB_COUNTER_FILE", "")
if counter_file:
    with open(counter_file, "a") as f:
        f.write("launch\n")
        f.flush()

# Write host PID (our parent) so the test can SIGTERM it.
ppid_file = os.environ.get("STUB_PPID_FILE", "")
if ppid_file:
    with open(ppid_file, "w") as f:
        f.write(str(os.getppid()))
        f.flush()

DIMS = 4

while True:
    line = sys.stdin.readline()
    if not line:
        break
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    method = req.get("method", "")
    req_id = req.get("request_id", "")

    if method == "health":
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id,
                "result": {"ready": True, "runtime": "stub",
                           "device": "cpu", "dims": DIMS,
                           "model_id": "stub-model"}}
    elif method == "embed_query":
        text = req.get("params", {}).get("text", "")
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id,
                "result": {"dims": DIMS, "vector": [float(len(text))] * DIMS}}
    elif method == "embed_batch":
        texts = req.get("params", {}).get("texts", [])
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id,
                "result": {"dims": DIMS,
                           "vectors": [[float(len(t))] * DIMS for t in texts]}}
    elif method == "shutdown":
        resp = {"schema": "julie.embedding.sidecar", "version": 1,
                "request_id": req_id, "result": {"stopping": True}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
        break
    else:
        continue

    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;
        let path = dir.join("stub_sidecar.py");
        std::fs::write(&path, script).expect("write stub sidecar");
        path
    }

    // -----------------------------------------------------------------------
    // Test
    // -----------------------------------------------------------------------

    /// HARD GATE: one resident embedding-host serves three concurrent sessions,
    /// and exactly one sidecar is launched despite the three concurrent spawns.
    #[test]
    #[serial]
    fn one_sidecar_serves_three_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let counter_file = dir.path().join("sidecar_launches");
        let ppid_file = dir.path().join("host_ppid");
        let python = test_python_interpreter();
        let stub_path = write_stub(dir.path());

        // -------------------------------------------------------------------
        // PATH: prepend target/debug so locate_embedding_host finds the binary.
        //
        // Test exe lives at  target/debug/deps/<name>
        //                                          ^ .parent() = deps/
        //                                   ^ .parent() = target/debug/
        // julie-embedding-host is at target/debug/julie-embedding-host.
        // -------------------------------------------------------------------
        let target_debug = std::env::current_exe()
            .expect("current_exe")
            .parent()
            .expect("exe parent (deps/)")
            .parent()
            .expect("exe grandparent (debug/)")
            .to_path_buf();
        let orig_path = std::env::var_os("PATH").unwrap_or_default();
        let new_path = std::env::join_paths(
            std::iter::once(target_debug.clone())
                .chain(std::env::split_paths(&orig_path)),
        )
        .expect("join_paths");

        // Pin JULIE_EMBEDDING_SIDECAR_ROOT to the source checkout so
        // sidecar_root_path() succeeds (it's called before the script
        // override is honoured; the root itself is unused when a script
        // override is set but the path must resolve).
        let sidecar_root = {
            let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest
                .ancestors()
                .map(|a| a.join("python").join("embeddings_sidecar"))
                .find(|p| p.join("pyproject.toml").exists())
                .expect("python/embeddings_sidecar/pyproject.toml not found from CARGO_MANIFEST_DIR")
        };

        let _path_g = EnvGuard::set("PATH", &new_path);
        let _prov_g = EnvGuard::set("JULIE_EMBEDDING_PROVIDER", "sidecar");
        let _prog_g = EnvGuard::set("JULIE_EMBEDDING_SIDECAR_PROGRAM", &python);
        let _scrpt_g = EnvGuard::set(
            "JULIE_EMBEDDING_SIDECAR_SCRIPT",
            stub_path.to_str().expect("stub path utf8"),
        );
        let _raw_g = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM");
        let _root_g =
            EnvGuard::set("JULIE_EMBEDDING_SIDECAR_ROOT", sidecar_root.to_str().expect("utf8"));
        let _cnt_g = EnvGuard::set(
            "STUB_COUNTER_FILE",
            counter_file.to_str().expect("counter path utf8"),
        );
        let _ppid_g =
            EnvGuard::set("STUB_PPID_FILE", ppid_file.to_str().expect("ppid path utf8"));

        let julie_home = dir.path().join("julie_home");
        let paths = DaemonPaths::with_home(julie_home.clone());
        paths.ensure_dirs().expect("ensure_dirs");

        let addr = HostAddress::from_paths(&paths);
        let lock_path = paths.embedding_host_lock();
        let socket_path = addr.socket_path().to_path_buf();

        // -------------------------------------------------------------------
        // 1. Three concurrent sessions.
        //
        // Thread 1: is_host_live→false → spawns binary → binary wins lock →
        //           factory spawns sidecar → host binds socket → client returns.
        // Thread 2: is_host_live→false → spawns binary → binary fails lock →
        //           binary exits → poll_for_liveness waits for thread-1's socket.
        // Thread 3: same as thread 2 (or is_host_live→true if socket is already
        //           up by then → takes the fast path directly).
        // -------------------------------------------------------------------
        let t0 = Instant::now();

        let handles: Vec<_> = (0..3usize)
            .map(|i| {
                let home = julie_home.clone();
                std::thread::spawn(move || -> anyhow::Result<(Vec<f32>, Vec<Vec<f32>>)> {
                    let p = DaemonPaths::with_home(home);
                    let t1 = Instant::now();
                    let provider = connect_or_spawn_host(&p)?;
                    let t_connect = t1.elapsed();

                    let t2 = Instant::now();
                    let q = provider.embed_query("hello")?;
                    let b = provider
                        .embed_batch(&["a".to_string(), "bb".to_string()])?;
                    let t_embed = t2.elapsed();

                    eprintln!("session {i}: connect={t_connect:?} embed={t_embed:?}");
                    Ok((q, b))
                })
            })
            .collect();

        let results: Vec<(Vec<f32>, Vec<Vec<f32>>)> = handles
            .into_iter()
            .enumerate()
            .map(|(i, h)| {
                h.join()
                    .unwrap_or_else(|_| panic!("thread {i} panicked"))
                    .unwrap_or_else(|e| panic!("session {i} error: {e}"))
            })
            .collect();

        eprintln!("All 3 sessions completed in {:?}", t0.elapsed());

        // -------------------------------------------------------------------
        // HARD GATE (a): all 3 clients return correctly-dimensioned vectors.
        // -------------------------------------------------------------------
        for (i, (qv, bv)) in results.iter().enumerate() {
            assert_eq!(qv.len(), STUB_DIMS, "session {i} embed_query dims");
            assert_eq!(bv.len(), 2, "session {i} embed_batch count");
            for (j, v) in bv.iter().enumerate() {
                assert_eq!(v.len(), STUB_DIMS, "session {i} embed_batch[{j}] dims");
            }
        }

        // Spot-check vector values: stub returns [len(text) as f32; DIMS].
        // "hello".len()=5, "a".len()=1, "bb".len()=2
        for (i, (qv, bv)) in results.iter().enumerate() {
            assert_eq!(qv[0], 5.0f32, "session {i} embed_query value");
            assert_eq!(bv[0][0], 1.0f32, "session {i} embed_batch[0] value");
            assert_eq!(bv[1][0], 2.0f32, "session {i} embed_batch[1] value");
        }

        // -------------------------------------------------------------------
        // HARD GATE (b): exactly one sidecar launch (lock-first correctness).
        // -------------------------------------------------------------------
        // By the time all embed calls have returned, the sidecar is live and
        // has written its ppid.  Poll briefly as a safety net.
        let deadline = Instant::now() + Duration::from_secs(5);
        let host_pid: libc::pid_t = loop {
            if let Ok(contents) = std::fs::read_to_string(&ppid_file) {
                let trimmed = contents.trim();
                if !trimmed.is_empty() {
                    break trimmed
                        .parse::<libc::pid_t>()
                        .expect("parse host pid from ppid file");
                }
            }
            assert!(
                Instant::now() < deadline,
                "STUB_PPID_FILE not written within 5s"
            );
            std::thread::sleep(Duration::from_millis(50));
        };

        let counter_contents = std::fs::read_to_string(&counter_file).unwrap_or_default();
        let launch_count = counter_contents.lines().count();
        assert_eq!(
            launch_count,
            1,
            "HARD GATE: expected exactly 1 sidecar launch, got {launch_count}.\n\
             Counter file contents:\n{counter_contents}"
        );

        // -------------------------------------------------------------------
        // HARD GATE (c): SIGTERM → clean shutdown → socket removed → lock free.
        // -------------------------------------------------------------------
        assert!(
            socket_path.exists(),
            "socket must exist before shutdown"
        );

        // SAFETY: valid pid from ppid file; SIGTERM is safe signal.
        unsafe { libc::kill(host_pid, libc::SIGTERM) };

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if !socket_path.exists() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "embedding-host did not shut down within 10s after SIGTERM"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            !socket_path.exists(),
            "socket file must be removed after shutdown"
        );

        let lf = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lock_path)
            .expect("open lock file after shutdown");
        assert!(
            lf.try_lock_exclusive().is_ok(),
            "singleton lock must be acquirable after shutdown"
        );

        eprintln!(
            "T8 PASS — 3 sessions × (embed_query + embed_batch) | \
             total={:?} | sidecar_launches={launch_count} | host_pid={host_pid}",
            t0.elapsed()
        );
    }
}
