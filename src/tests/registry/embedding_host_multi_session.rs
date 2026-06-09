//! Acceptance test (Phase 3b, Task 8, HARD GATE): one resident embedding-host
//! serves three concurrent sessions backed by exactly one sidecar.
//!
//! ## Architecture
//!
//! Three `tokio::spawn` tasks each call `run_embedding_host_default` with the
//! **same** `lock_path` and **same** `addr`.  The fs2 singleton lock uses
//! `flock(2)` semantics (per-open-file-description, not per-process), so the
//! three tasks' independent `OpenOptions` file descriptors **genuinely race**
//! even within a single process:
//!
//! - **Winner** (1 task): acquires the lock → calls `make_provider` →
//!   `create_embedding_provider()` runs → sidecar spawns → socket bound.
//! - **Losers** (2 tasks): `try_lock_exclusive` returns `Err` immediately →
//!   `run_embedding_host_default` returns `Err` before ever touching the
//!   provider factory.
//!
//! A build without the lock-first refactor would call `create_embedding_provider`
//! (and spawn the Python sidecar) in all three tasks before any lock is held,
//! making `counter==3` and breaking the HARD GATE.
//!
//! Three `connect_or_spawn_host` clients then take the fast path (socket is live)
//! and perform concurrent `embed_query` + `embed_batch` calls through the single
//! resident host.
//!
//! ## HARD GATE assertions
//!
//! (a) All 3 clients return correctly-dimensioned vectors for `embed_query` and
//!     `embed_batch`.
//! (b) The counter file contains **exactly one** line — one sidecar spawn despite
//!     the three-way host race.
//! (c) `cancel()` → exactly **1 Ok / 2 Err** from the host handles (lock
//!     ordering), socket removed, singleton lock re-acquirable.

#[cfg(all(test, unix))]
#[cfg(feature = "embeddings-sidecar")]
mod unix {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use fs2::FileExt as _;
    use serial_test::serial;
    use tokio_util::sync::CancellationToken;

    use crate::embedding_host_launch::connect_or_spawn_host;
    use crate::paths::RegistryPaths;
    use julie_pipeline::embeddings::EmbeddingProvider;
    use julie_pipeline::embeddings::host_server::run_embedding_host_default;
    use julie_pipeline::embeddings::host_transport::{HostAddress, HostClientConn};

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
            Self {
                key: key.to_owned(),
                previous,
            }
        }

        fn remove(key: &str) -> Self {
            let previous = std::env::var_os(key);
            unsafe { std::env::remove_var(key) };
            Self {
                key: key.to_owned(),
                previous,
            }
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
    /// On startup the script appends `"launch\n"` to `$STUB_COUNTER_FILE`
    /// (cross-process sidecar launch count), then serves the sidecar wire
    /// protocol over stdin/stdout until `shutdown` or EOF.
    /// Vectors: `[len(text) as f32; DIMS]`.
    fn write_stub(dir: &Path) -> PathBuf {
        let script = r#"#!/usr/bin/env python3
import json, sys, os

# Record this sidecar launch.
counter_file = os.environ.get("STUB_COUNTER_FILE", "")
if counter_file:
    with open(counter_file, "a") as f:
        f.write("launch\n")
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
    // Helper: poll until the host's socket accepts connections.
    // -----------------------------------------------------------------------

    fn wait_for_live(addr: &HostAddress, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            if HostClientConn::connect(addr).is_ok() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "embedding-host did not become live within {timeout:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    // -----------------------------------------------------------------------
    // Test
    // -----------------------------------------------------------------------

    /// HARD GATE: three concurrent hosts race for the same singleton lock;
    /// exactly one wins → spawns exactly one sidecar → serves three clients.
    #[tokio::test(flavor = "multi_thread")]
    #[serial]
    async fn one_sidecar_serves_three_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let counter_file = dir.path().join("sidecar_launches");
        let python = test_python_interpreter();
        let stub_path = write_stub(dir.path());

        // Pin JULIE_EMBEDDING_SIDECAR_ROOT so sidecar_root_path() resolves
        // immediately via env-var priority (called even in script-override mode).
        let sidecar_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .map(|a| a.join("python").join("embeddings_sidecar"))
            .find(|p| p.join("pyproject.toml").exists())
            .expect("python/embeddings_sidecar/pyproject.toml not found");

        let _prov_g = EnvGuard::set("JULIE_EMBEDDING_PROVIDER", "sidecar");
        let _prog_g = EnvGuard::set("JULIE_EMBEDDING_SIDECAR_PROGRAM", &python);
        let _scrpt_g = EnvGuard::set(
            "JULIE_EMBEDDING_SIDECAR_SCRIPT",
            stub_path.to_str().expect("stub path utf8"),
        );
        let _raw_g = EnvGuard::remove("JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM");
        let _root_g = EnvGuard::set(
            "JULIE_EMBEDDING_SIDECAR_ROOT",
            sidecar_root.to_str().expect("sidecar root utf8"),
        );
        let _cnt_g = EnvGuard::set(
            "STUB_COUNTER_FILE",
            counter_file.to_str().expect("counter path utf8"),
        );

        let julie_home = dir.path().join("julie_home");
        let paths = RegistryPaths::with_home(julie_home.clone());
        paths.ensure_dirs().expect("ensure_dirs");

        let lock_path = paths.embedding_host_lock();
        let addr = HostAddress::from_paths(&paths);
        let socket_path = addr.socket_path().to_path_buf();

        // -------------------------------------------------------------------
        // 1. Start 3 concurrent run_embedding_host_default tasks, all using
        //    the SAME lock_path and SAME addr.
        //
        // Each task opens its own file descriptor via OpenOptions, so the
        // three try_lock_exclusive() calls genuinely race (flock semantics
        // are per-open-file-description, not per-process):
        //   - Winner  (1): lock held → make_provider → sidecar spawns → socket bound.
        //   - Losers  (2): try_lock_exclusive fails → Err returned immediately,
        //                  before make_provider is ever called.
        // -------------------------------------------------------------------
        let cancel = CancellationToken::new();
        let mut host_handles = Vec::new();
        for _ in 0..3 {
            let c = cancel.clone();
            let a = HostAddress::from_paths(&paths);
            let l = lock_path.clone();
            host_handles.push(tokio::spawn(async move {
                run_embedding_host_default(&a, &l, c).await
            }));
        }

        // 2. Wait until the winner's socket is accepting connections.
        let addr_wait = HostAddress::from_paths(&paths);
        tokio::task::spawn_blocking(move || wait_for_live(&addr_wait, Duration::from_secs(15)))
            .await
            .expect("wait_for_live join");

        // -------------------------------------------------------------------
        // 3. Three concurrent sessions via connect_or_spawn_host.
        //    Socket is already live → all three take the fast path
        //    (is_host_live → true → RpcEmbeddingProvider::new).
        // -------------------------------------------------------------------
        let paths1 = paths.clone();
        let paths2 = paths.clone();
        let paths3 = paths.clone();

        let (r1, r2, r3) = tokio::join!(
            tokio::task::spawn_blocking(move || connect_or_spawn_host(&paths1)),
            tokio::task::spawn_blocking(move || connect_or_spawn_host(&paths2)),
            tokio::task::spawn_blocking(move || connect_or_spawn_host(&paths3)),
        );

        let provider1: Arc<dyn EmbeddingProvider> =
            Arc::new(r1.expect("join 1").expect("connect 1"));
        let provider2: Arc<dyn EmbeddingProvider> =
            Arc::new(r2.expect("join 2").expect("connect 2"));
        let provider3: Arc<dyn EmbeddingProvider> =
            Arc::new(r3.expect("join 3").expect("connect 3"));

        // -------------------------------------------------------------------
        // 4. Concurrent embed_query + embed_batch from all three providers.
        // -------------------------------------------------------------------
        let t0 = Instant::now();

        let (h1, h2, h3) = tokio::join!(
            tokio::task::spawn_blocking({
                let p = Arc::clone(&provider1);
                move || {
                    let q = p.embed_query("hello").expect("session1 embed_query");
                    let b = p
                        .embed_batch(&["a".to_string(), "bb".to_string()])
                        .expect("session1 embed_batch");
                    (q, b)
                }
            }),
            tokio::task::spawn_blocking({
                let p = Arc::clone(&provider2);
                move || {
                    let q = p.embed_query("hello").expect("session2 embed_query");
                    let b = p
                        .embed_batch(&["a".to_string(), "bb".to_string()])
                        .expect("session2 embed_batch");
                    (q, b)
                }
            }),
            tokio::task::spawn_blocking({
                let p = Arc::clone(&provider3);
                move || {
                    let q = p.embed_query("hello").expect("session3 embed_query");
                    let b = p
                        .embed_batch(&["a".to_string(), "bb".to_string()])
                        .expect("session3 embed_batch");
                    (q, b)
                }
            }),
        );

        let embed_elapsed = t0.elapsed();

        let (q1, b1) = h1.expect("join h1");
        let (q2, b2) = h2.expect("join h2");
        let (q3, b3) = h3.expect("join h3");

        // -------------------------------------------------------------------
        // HARD GATE (a): all 3 clients return correctly-dimensioned vectors.
        // -------------------------------------------------------------------
        let sessions: &[(&Vec<f32>, &Vec<Vec<f32>>)] = &[(&q1, &b1), (&q2, &b2), (&q3, &b3)];
        for (i, (qv, bv)) in sessions.iter().enumerate() {
            let session = i + 1;
            assert_eq!(qv.len(), STUB_DIMS, "session {session} embed_query dims");
            assert_eq!(bv.len(), 2, "session {session} embed_batch count");
            for (j, v) in bv.iter().enumerate() {
                assert_eq!(
                    v.len(),
                    STUB_DIMS,
                    "session {session} embed_batch[{j}] dims"
                );
            }
        }

        // Spot-check values: stub returns [len(text) as f32; DIMS].
        // "hello"=5, "a"=1, "bb"=2
        for (i, (qv, bv)) in sessions.iter().enumerate() {
            let s = i + 1;
            assert_eq!(qv[0], 5.0f32, "session {s} query value");
            assert_eq!(bv[0][0], 1.0f32, "session {s} batch[0] value");
            assert_eq!(bv[1][0], 2.0f32, "session {s} batch[1] value");
        }

        // -------------------------------------------------------------------
        // HARD GATE (b): exactly one sidecar launch (lock-first correctness).
        //
        // A build without the lock-first fix would show counter==3, because
        // all three host tasks would call create_embedding_provider() before
        // any of them held the lock.
        // -------------------------------------------------------------------
        let counter_contents = std::fs::read_to_string(&counter_file).unwrap_or_default();
        let launch_count = counter_contents.lines().count();
        assert_eq!(
            launch_count, 1,
            "HARD GATE: expected exactly 1 sidecar launch, got {launch_count}.\n\
             Counter file:\n{counter_contents}"
        );

        // -------------------------------------------------------------------
        // HARD GATE (c): cancel → 1 Ok / 2 Err (lock ordering) → clean shutdown.
        // -------------------------------------------------------------------
        assert!(
            socket_path.exists(),
            "socket must exist while host is running"
        );

        cancel.cancel();

        let mut ok_count = 0usize;
        let mut err_count = 0usize;
        for h in host_handles {
            match h.await.expect("host task join") {
                Ok(()) => ok_count += 1,
                Err(e) => {
                    let msg = e.to_string();
                    assert!(
                        msg.contains("lock") || msg.contains("singleton"),
                        "loser error must mention the lock; got: {msg:?}"
                    );
                    err_count += 1;
                }
            }
        }
        assert_eq!(ok_count, 1, "exactly one host must win the lock");
        assert_eq!(err_count, 2, "exactly two hosts must fail the lock");

        // Winner's shutdown removes the socket.
        assert!(
            !socket_path.exists(),
            "socket file must be removed after shutdown"
        );

        // The singleton lock must be re-acquirable once the winner exits.
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
             embed_elapsed={embed_elapsed:?} | sidecar_launches={launch_count} | \
             host_ok={ok_count} host_err={err_count}"
        );
    }
}
