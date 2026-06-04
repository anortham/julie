//! Acceptance test (Phase 3b, Task 8, HARD GATE): one resident embedding-host
//! serves three concurrent sessions backed by exactly one sidecar.
//!
//! ## Architecture
//!
//! The host runs as an **in-process** tokio task via `run_embedding_host_default`,
//! configured with a stub Python sidecar that:
//! - Appends one line to a counter file (`$JULIE_SIDECAR_COUNTER_FILE`) on each
//!   startup — giving a **cross-process** launch count even though the sidecar is a
//!   separate OS process.
//! - Serves `health` / `embed_query` / `embed_batch` / `shutdown` using the
//!   standard sidecar wire protocol (same technique as `sidecar_provider.rs`
//!   circuit-breaker test).
//!
//! Three concurrent sessions are created by calling
//! `connect_or_spawn_host(&paths)` three times.  Because the host is already
//! live, all three take the **fast path** (`is_host_live → true → return client
//! directly`).  The singleton `fs2` lock is held by the in-process host; any
//! stray spawn attempt would fail the lock and produce no extra sidecar.
//!
//! ## HARD GATE assertions
//!
//! (a) All 3 clients return correctly-dimensioned vectors for both `embed_query`
//!     and `embed_batch`.
//! (b) The counter file contains **exactly one** line — one sidecar spawn.
//! (c) Cancelling the host shuts it down cleanly: socket removed, lock
//!     re-acquirable.

#[cfg(all(test, unix))]
#[cfg(feature = "embeddings-sidecar")]
mod unix {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use fs2::FileExt as _;
    use tokio_util::sync::CancellationToken;

    use julie_core::paths::DaemonPaths;
    use julie_pipeline::embeddings::{
        EmbeddingProvider,
        host_server::run_embedding_host_default,
        host_transport::{HostAddress, HostClientConn},
    };

    /// Dimensionality the stub sidecar advertises.
    const STUB_DIMS: usize = 4;

    // -----------------------------------------------------------------------
    // Stub sidecar
    // -----------------------------------------------------------------------

    /// Write a self-contained Python stub sidecar to `dir/stub_sidecar.py`.
    ///
    /// On startup the script appends `"launch\n"` to the path in
    /// `$JULIE_SIDECAR_COUNTER_FILE`, then serves the sidecar wire protocol
    /// over stdin/stdout until the `shutdown` method or EOF.  Vectors are
    /// deterministic: `[len(text) as f32; DIMS]`.
    fn write_stub_sidecar(dir: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script = r#"#!/usr/bin/env python3
import json, sys, os

counter_file = os.environ.get("JULIE_SIDECAR_COUNTER_FILE", "")
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
                           "device": "cpu", "dims": DIMS}}
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
        let mut perms = std::fs::metadata(&path)
            .expect("stub metadata")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod stub");
        path
    }

    // -----------------------------------------------------------------------
    // Helper: poll until host accepts connections
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

    /// HARD GATE: one resident embedding-host serves three concurrent sessions,
    /// and exactly one sidecar is launched despite three concurrent clients.
    #[tokio::test]
    async fn one_sidecar_serves_three_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let counter_file = dir.path().join("sidecar_launches");
        let stub_exe = write_stub_sidecar(dir.path());

        // Set env vars. cargo nextest runs each test in its own OS process, so
        // there are no competing threads reading the environment here.
        unsafe {
            std::env::set_var("JULIE_EMBEDDING_PROVIDER", "sidecar");
            std::env::set_var("JULIE_EMBEDDING_SIDECAR_PROGRAM", &stub_exe);
            std::env::set_var("JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM", "1");
            std::env::set_var("JULIE_SIDECAR_COUNTER_FILE", &counter_file);
        }

        let julie_home = dir.path().join("julie_home");
        let paths = DaemonPaths::with_home(julie_home.clone());
        paths.ensure_dirs().expect("ensure_dirs");

        let lock_path = paths.embedding_host_lock();
        let addr = HostAddress::from_paths(&paths);

        // -------------------------------------------------------------------
        // 1. Start the resident host in-process.
        //
        // `run_embedding_host_default` acquires the singleton lock, spawns
        // the sidecar (which writes to counter_file), runs the health probe,
        // then enters the accept loop.  The blocking Python spawn happens on a
        // tokio worker thread; the multi-thread runtime keeps other tasks alive.
        // -------------------------------------------------------------------
        let cancel = CancellationToken::new();
        let cancel_server = cancel.clone();
        let addr_server = HostAddress::from_paths(&paths);
        let lock_server = lock_path.clone();
        let host_task = tokio::spawn(async move {
            run_embedding_host_default(&addr_server, &lock_server, cancel_server).await
        });

        // Wait until the host accepts connections (sidecar health-checked, socket bound).
        let addr_wait = HostAddress::from_paths(&paths);
        tokio::task::spawn_blocking(move || {
            wait_for_live(&addr_wait, Duration::from_secs(15))
        })
        .await
        .expect("wait_for_live join");

        // -------------------------------------------------------------------
        // 2. Three concurrent sessions via connect_or_spawn_host.
        //
        // The host is already live, so all three calls take the fast path
        // (is_host_live → true → RpcEmbeddingProvider::new).  No additional
        // binary spawn occurs.
        // -------------------------------------------------------------------
        let paths1 = paths.clone();
        let paths2 = paths.clone();
        let paths3 = paths.clone();

        let (r1, r2, r3) = tokio::join!(
            tokio::task::spawn_blocking(move || {
                crate::embedding_host_launch::connect_or_spawn_host(&paths1)
            }),
            tokio::task::spawn_blocking(move || {
                crate::embedding_host_launch::connect_or_spawn_host(&paths2)
            }),
            tokio::task::spawn_blocking(move || {
                crate::embedding_host_launch::connect_or_spawn_host(&paths3)
            }),
        );

        let provider1: Arc<dyn EmbeddingProvider> =
            Arc::new(r1.expect("join 1").expect("connect 1"));
        let provider2: Arc<dyn EmbeddingProvider> =
            Arc::new(r2.expect("join 2").expect("connect 2"));
        let provider3: Arc<dyn EmbeddingProvider> =
            Arc::new(r3.expect("join 3").expect("connect 3"));

        // -------------------------------------------------------------------
        // 3. Concurrent embed_query + embed_batch from all three providers.
        // -------------------------------------------------------------------
        let t0 = Instant::now();

        let (h1, h2, h3) = tokio::join!(
            tokio::task::spawn_blocking({
                let p = Arc::clone(&provider1);
                move || {
                    let q = p.embed_query("hello").expect("session1 embed_query");
                    let b = p
                        .embed_batch(&["hi".to_string(), "there".to_string()])
                        .expect("session1 embed_batch");
                    (q, b)
                }
            }),
            tokio::task::spawn_blocking({
                let p = Arc::clone(&provider2);
                move || {
                    let q = p.embed_query("world!").expect("session2 embed_query");
                    let b = p
                        .embed_batch(&["ab".to_string(), "cde".to_string()])
                        .expect("session2 embed_batch");
                    (q, b)
                }
            }),
            tokio::task::spawn_blocking({
                let p = Arc::clone(&provider3);
                move || {
                    let q = p.embed_query("foo").expect("session3 embed_query");
                    let b = p
                        .embed_batch(&["x".to_string(), "yy".to_string()])
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
            assert_eq!(
                qv.len(),
                STUB_DIMS,
                "session {session} embed_query dims mismatch"
            );
            assert_eq!(bv.len(), 2, "session {session} embed_batch count mismatch");
            for (j, v) in bv.iter().enumerate() {
                assert_eq!(
                    v.len(),
                    STUB_DIMS,
                    "session {session} embed_batch[{j}] dims mismatch"
                );
            }
        }

        // Spot-check actual vector values (stub: vec = [len(text) as f32; DIMS]).
        // Session 1: "hello".len()=5, "hi".len()=2, "there".len()=5
        assert_eq!(q1, vec![5.0f32; STUB_DIMS], "session1 query value");
        assert_eq!(b1[0], vec![2.0f32; STUB_DIMS], "session1 batch[0] value");
        assert_eq!(b1[1], vec![5.0f32; STUB_DIMS], "session1 batch[1] value");
        // Session 2: "world!".len()=6, "ab".len()=2, "cde".len()=3
        assert_eq!(q2, vec![6.0f32; STUB_DIMS], "session2 query value");
        assert_eq!(b2[0], vec![2.0f32; STUB_DIMS], "session2 batch[0] value");
        assert_eq!(b2[1], vec![3.0f32; STUB_DIMS], "session2 batch[1] value");
        // Session 3: "foo".len()=3, "x".len()=1, "yy".len()=2
        assert_eq!(q3, vec![3.0f32; STUB_DIMS], "session3 query value");
        assert_eq!(b3[0], vec![1.0f32; STUB_DIMS], "session3 batch[0] value");
        assert_eq!(b3[1], vec![2.0f32; STUB_DIMS], "session3 batch[1] value");

        // -------------------------------------------------------------------
        // HARD GATE (b): exactly one sidecar launch.
        // -------------------------------------------------------------------
        let counter_contents = std::fs::read_to_string(&counter_file).unwrap_or_default();
        let launch_count = counter_contents.lines().count();
        assert_eq!(
            launch_count,
            1,
            "HARD GATE: expected exactly 1 sidecar launch, got {launch_count}.\n\
             Counter file:\n{counter_contents}"
        );

        // -------------------------------------------------------------------
        // HARD GATE (c): clean shutdown.
        // -------------------------------------------------------------------
        let socket_path = addr.socket_path().to_path_buf();
        assert!(
            socket_path.exists(),
            "socket must exist while host is running"
        );

        cancel.cancel();
        host_task
            .await
            .expect("host task join")
            .expect("host task completed ok");

        assert!(
            !socket_path.exists(),
            "socket file must be removed after shutdown"
        );

        // The singleton lock must be re-acquirable after shutdown.
        let lf = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&lock_path)
            .expect("open lock file after shutdown");
        assert!(
            lf.try_lock_exclusive().is_ok(),
            "singleton lock must be acquirable after shutdown"
        );

        // Report-only: per-call latency.
        println!(
            "T8 PASS — 3 sessions × (embed_query + embed_batch) | \
             embed_elapsed={embed_elapsed:?} | sidecar_launches={launch_count}"
        );

        // Restore env (good hygiene; nextest uses fresh processes anyway).
        unsafe {
            std::env::remove_var("JULIE_EMBEDDING_PROVIDER");
            std::env::remove_var("JULIE_EMBEDDING_SIDECAR_PROGRAM");
            std::env::remove_var("JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM");
            std::env::remove_var("JULIE_SIDECAR_COUNTER_FILE");
        }
    }
}
