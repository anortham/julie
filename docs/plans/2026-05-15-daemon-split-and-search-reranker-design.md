# Julie Daemon Split + Search Reranker — Design

## Purpose

Three related changes to make julie more reliable and less complex by backporting structural lessons from eros, without abandoning julie's Rust-native stack or its current feature surface:

1. **Daemon split**: replace the single `julie-server` binary (which is simultaneously the adapter spawned by MCP clients and the daemon those adapters bridge to) with two binaries — `julie-adapter` (thin stdio→HTTP forwarder) and `julie-daemon` (HTTP server). Replace the four-file lifecycle handshake (`daemon.pid` + `daemon.state` + `daemon.lock` + `daemon.singleton`) with two purpose-clear primitives: a kernel-held advisory `daemon.lock` (the actual singleton, fcntl/LockFileEx on a long-lived fd) and an atomically-written `discovery.json` (endpoint metadata + pid+creation-time identity). Add a hard migration gate so a new daemon cannot start beside a live legacy one. Switch MCP request handling to connection-per-request so parallel calls can't deadlock on a shared SQLite connection, while keeping the existing per-workspace `mutation_gate` as the single serialization point for all writers (watcher, catch-up, MCP mutations).

2. **In-process test harness**: make the daemon embeddable as a library so tests bind a `tokio::TcpListener` on `127.0.0.1:0` and run `DaemonApp::serve(listener)` in-process — eliminating the test-suite resource explosion from dozens of spawned daemon subprocesses.

3. **Search reranker (revised C)**: add a re-ranking stage on top of julie's existing Tantivy results that applies an eros-style scorer — query intent detection (symbol intent, test intent), kind-aware boosts, role-aware default filtering, phrase boost for long queries — backed by enriched Tantivy fields (`role`, `test_role`, `capability_flags`). Tantivy continues to retrieve candidates; the reranker reweighs them. No substring scans, no projection table. This is the source of the search-quality gap measured against eros's bakeoff (julie 70% top5 → eros 100% top5).

These three are sequenced: A enables B (without the split, tests can't run multiple daemons in parallel); C is independent of A/B but shares the same release cadence and benefits from the same lifecycle simplification.

## Architecture Quality

**Affected modules:**
- `src/main.rs`, `src/adapter/*`, `src/daemon/*` — binary split, lifecycle simplification
- `src/paths.rs` — discovery file path consolidation, legacy-file detection
- `src/handler.rs`, `src/handler/*` — connection-per-request acquisition at MCP tool handlers
- `src/tools/*` — handlers receive their own SQLite connection; mutating handlers also acquire the per-workspace `mutation_gate`
- `src/daemon/workspace_pool.rs` / new `connection_pool.rs` — per-workspace SQLite connection pool
- `src/workspace/mutation_gate.rs` — injectable registry (for tests), `MutationGuard` proof-token already in place
- `src/search/*` (or new `src/search/reranker.rs`) — new reranking stage and enriched Tantivy schema
- `src/tests/*` — test harness refactor with `DaemonRuntimeContext` isolation
- New: `crates/julie-adapter/` (or a `[[bin]]` target if staying in workspace) — thin adapter binary
- New: `xtask` updates to build both binaries and run the in-process harness as the default test path

**Caller-facing interfaces:**
- MCP tool surface: unchanged. Existing tools (`fast_search`, `deep_dive`, `get_symbols`, etc.) keep their signatures and observed behavior, except search results from `fast_search` improve in quality and ordering.
- CLI: `julie-server` is replaced by `julie-adapter` (default for MCP clients) and `julie-daemon` (explicit `julie-daemon start|stop|status`). The plugin manifest is updated to spawn `julie-adapter` instead of `julie-server`. Existing `julie-server` invocations get a thin compatibility shim that delegates to the right binary based on argv (no behavioral break for any user/plugin update path).
- Discovery on disk: `~/.julie/daemon.pid` + `daemon.state` + `daemon.lock` + `daemon.singleton` → `~/.julie/daemon.lock` (kernel-held singleton) + `~/.julie/discovery.json` (endpoint metadata) + `~/.julie/daemon.token` (mode 0600 bearer token, referenced by `token_path` in discovery). Legacy files are detected and refused-on-live (hard gate), unlinked when confirmed dead — not unconditionally cleaned up.

**Depth/locality:**
- Adapter binary is small and has no SQLite, no Tantivy, no embeddings, no extractors. Just stdio JSON-RPC parsing, HTTP forwarding, retry on transport error, child-process spawn.
- Daemon binary owns all the heavy state. `run_daemon` becomes `DaemonApp::serve(listener)` — components instantiated via a builder that captures dependency ordering in types rather than implicit comments.
- Reranker is a pure function `rerank(query, candidates) -> ranked` with no coupling to Tantivy internals beyond the result struct shape. Enriched fields live in Tantivy schema; the reranker reads them off the returned doc.

**Test surface:**
- Adapter: integration test that spawns a real `julie-adapter` subprocess against an in-process `DaemonApp`, exercises stdio↔HTTP forwarding, kill/respawn behavior.
- Daemon: every existing daemon test moves to the in-process harness. The `daemon_lifecycle::test_*` suite verifies kernel-held lock contention, discovery file atomic-write durability, pid+creation_time identity validation under simulated PID reuse, and the legacy-daemon migration gate.
- Concurrency regression test: 8 concurrent MCP requests on the same workspace (mix of read and write tools) plus an active filewatcher event stream — all complete within bounded time, no wedging. Replaces the original "two parallel routes" framing with full writer-set coverage (codex review caught the original was too narrow).
- Reranker: pure-function tests for intent detection, kind boost matrix, role-aware reweighting. Quality regression test against a small fixed corpus checking top5 stays at or above current numbers, AND a targeted test that `fast_search("fixture builder", target="definitions")` still surfaces test helpers (codex review caught the original draft had a filter-default regression here).

**Seams / adapters:**
- `DaemonApp` — the embeddable entrypoint. `pub fn new(config: DaemonConfig) -> Self`, `pub async fn serve(self, listener: TcpListener) -> Result<DaemonHandle>`. `DaemonHandle::shutdown()` for graceful teardown in tests.
- `DiscoveryFile` — the read/write API for discovery state, with atomic write (tmp + fsync + rename + parent fsync) and pid+creation_time validation. `acquire_singleton(paths) -> Result<DaemonLockGuard>` holds the kernel advisory lock for the daemon lifetime.
- `WorkspaceConnectionPool` — per-workspace SQLite connection pool. `pool.acquire().await -> PooledConn` for handlers. Connections are short-lived (per-request) and never shared.
- `DaemonRuntimeContext` — test-injection seam for process-global state (mutation gate registry, tracing, env overrides, watcher quota).
- `Reranker` — `pub fn rerank(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked>`. Lives behind a config flag during rollout; default-on once quality regression test confirms no regressions and no filter-default regressions.

**Rejected shortcuts:**
- Keeping `julie-server` as one binary and only adding internal modules. Doesn't address the core "adapter and daemon share code paths" cause of stale-binary detection + restart-pending complexity.
- Putting vectors in Tantivy (verified: not natively supported through 0.26).
- Swapping the storage stack to LanceDB (eros's own bakeoff data shows LanceDB candidates underperform their non-LanceDB baseline; storage isn't the lever).
- Porting eros's `instr()`-based substring scan retrieval. It's transitional eros scaffolding for candidate comparison, not a viable production design — full-table-scan O(rows × terms × columns) per query.
- Daemon-as-library with no HTTP (running embedded in the adapter process). Would re-create the test-suite resource problem we're trying to fix and would not address the parallel-instances pain.
- **Route-level write lock at MCP tool handlers (original draft).** Codex review caught that this doesn't compose with the existing background writers (watcher, catch-up, repair, retry, register) which take the mutation gate directly and never go through MCP handlers. Either they'd skip the route lock (so it protects nothing) or they'd acquire `mutation_gate → route_lock` while handlers acquire `route_lock → mutation_gate` — a textbook deadlock that no two-request test would catch. Replaced with connection-per-request (no shared connection to contend over) + the mutation gate as the single per-workspace writer-serialization point.
- Collapsing singleton enforcement to a single `O_EXCL` file. Codex review caught that `O_EXCL` is creation-time-only — a crashed daemon leaves the file behind, and stale-pid recovery via `kill(pid, 0)` is vulnerable to PID reuse on busy systems. Replaced with a kernel-held advisory lock (releases on process exit, crash or clean) + pid+creation_time identity in discovery for adapter-side validation.
- Embedding the bearer token in `discovery.json`. Codex review caught that `discovery.json` is read by unrelated MCP clients on the local box (that's how they find the daemon) and is created with default umask. Replaced with `token_path` in discovery pointing at a separate `daemon.token` file (mode 0600).

**Architecture risk:** medium. The binary split and connection-per-request reshape are mechanically large but conceptually clean. The largest residual risk is **legacy-daemon coexistence during the upgrade window** — both `julie-adapter` and `julie-daemon` must correctly detect a live legacy `julie-server daemon` and either attach or refuse, not silently spawn beside it. Mitigation: explicit hard-gate logic with end-to-end test of "upgraded plugin against running legacy daemon."

## Background

### Why now

Three symptoms motivated this work:

1. **Daemon crashes and hangs** under sustained use. Reading `run_daemon` (`src/daemon/mod.rs:284-794`) shows the structural cause: a 510-line function hand-wiring ~10 long-lived shared components with implicit ordering, three concurrent locking primitives (`SingletonLock` + `PidFile` + `DaemonLifecycleController.state_path`) that the code comments describe as "defense-in-depth" because the "577-daemon cascade" regression had them fail compoundly. Stale-binary mtime tracking, `restart_pending` state, drain-with-timeout-or-lose-writes, three shutdown paths racing in a `tokio::select!`. Each is reasonable in isolation; together they're a brittle web.

2. **Blocking calls under multiple instances.** Eros independently hit and fixed this same class of bug (`docs/plans/2026-05-15-confidence-artifacts-and-hub-deadlock.md` in the eros tree): parallel MCP routes doing raw SQL on a shared `CanonicalStore` connection wedged for 120s. Their fix was to wrap response-assembly paths in `with store.locked():` at the route boundary. Julie's per-workspace mutation gate (`src/workspace/mutation_gate.rs`) addresses write coordination but not the broader parallel-route SQL contention class.

3. **Test-suite resource explosion.** Daemon tests spawn `julie-server` subprocesses. Multiple parallel tests + per-test daemon startup (sidecar warmup, watcher init, workspace pool init) saturates the developer machine. Eros's tests use FastAPI's `TestClient` in-process and pay none of this cost.

The eros design doc explicitly positions eros as julie's successor for the agent-efficiency core, but julie is julie's open-source product line — it stays. The lessons travel, the codebases don't merge.

### What stays the same

- All 34 tree-sitter extractors and their golden tests.
- Workspace per-`.julie/indexes/{workspace_id}` layout.
- SQLite + sqlite-vec + Tantivy storage stack (verified that Tantivy still has no native vector support; swap is not on the table).
- Embedding sidecar with CUDA/MPS/DirectML detection (user-confirmed: not the source of pain).
- File watcher and mutation gate (user-confirmed: keeping; eros will get one too).
- MCP tool surface — same 12 tools, same parameters.
- Plugin distribution via `julie-plugin` (hooks, skills, agent instructions).

### What changes

Components within `src/daemon/` get decomposed and the binary split moves transport concerns into `julie-adapter`. The eight `src/daemon/*.rs` files become a smaller core: `app.rs` (the embeddable `DaemonApp`), `discovery.rs` (the file + lock), `mcp_session.rs` (kept), `embedding_service.rs` (kept), and the pools (`watcher_pool.rs`, `workspace_pool.rs` kept). `pid.rs`, `singleton.rs`, `lifecycle.rs` (the LifecyclePhase state machine for the state file), `shutdown_event.rs`, `http_client.rs`, and most of `mod.rs` go away or shrink dramatically.

## Design

### A. Daemon Split + Discovery Consolidation + Route-Level Locking

#### A.1 Binary topology

**`julie-adapter`** (new binary, replaces `julie-server` with no args)
- Tiny: stdio JSON-RPC parser, HTTP forwarder, child-process spawn helper.
- On startup:
  1. Read `~/.julie/discovery.json`. If present and the recorded `pid` is alive and the bearer token is readable → connect to `http://host:port` and start forwarding.
  2. If missing, stale (pid dead), or unparseable → spawn `julie-daemon` as a detached child (`std::process::Command::new("julie-daemon").arg("start").stdout(Stdio::null()).stderr(piped()).spawn()`). Wait up to N seconds (configurable, default 60) for `discovery.json` to appear with a live pid. On timeout → emit a structured error on stderr and exit.
  3. Once connected, run a forwarding loop: read JSON-RPC from stdin, POST to daemon's MCP HTTP endpoint with the bearer token, stream response back to stdout. On transport error → reconnect with backoff; if daemon process has died → respawn (single attempt per session).
- Has no SQLite, no Tantivy, no extractors, no embeddings. The binary is small enough that stale-binary concerns disappear — users update via the normal plugin update path and the next `julie-adapter` invocation gets the new code.

**`julie-daemon`** (new binary, replaces `julie-server daemon`)
- Subcommands: `start`, `stop`, `status`. Default = `start` (so plugin auto-spawn invokes the bare binary).
- `start`:
  1. Acquire `~/.julie/discovery.lock` via `O_CREAT | O_EXCL | O_WRONLY`. If held → exit with "another daemon is running" message. If acquired → write pid to the lock file (for diagnostics only; not consulted for lifecycle).
  2. Bind `TcpListener` on requested port (default 0 = auto-assign).
  3. Construct `DaemonApp::new(config)` and call `.serve(listener)`.
  4. After bind, write `discovery.json` atomically (temp + fsync + `rename`).
  5. On shutdown (SIGTERM/SIGINT/`julie-daemon stop`) → graceful HTTP shutdown via `axum::Server::with_graceful_shutdown`, then drop the lock guard (releases `discovery.lock` and clears `discovery.json`).

The adapter never participates in daemon shutdown coordination. The drain-with-timeout-or-lose-writes dance goes away because daemon shutdown is normal HTTP graceful-stop, and the lock guard's `Drop` ordering guarantees discovery cleanup happens after the server is fully stopped.

#### A.2 Discovery file format

`~/.julie/discovery.json` (endpoint metadata only — never credentials):

```json
{
  "pid": 12345,
  "pid_creation_time_ns": 1747349520123456789,
  "host": "127.0.0.1",
  "port": 49213,
  "token_path": "/Users/.../.julie/daemon.token",
  "log_path": "/Users/.../.julie/daemon.log.2026-05-16",
  "daemon_version": "7.7.4",
  "protocol_version": 1,
  "schema_version": 1,
  "started_at": "2026-05-16T17:42:00Z"
}
```

Atomic write: write to `discovery.json.tmp` → `fsync(file)` → `rename` → `fsync(parent_dir)` so the rename itself is durable across crash. Adapter reads with a single `read_to_string` and validates the schema.

**Token handling:** the bearer token is written to `~/.julie/daemon.token` with mode `0600` (POSIX) / restricted ACL (Windows). `discovery.json` only contains the path. Adapter reads the token from the path; daemon enforces the file mode on write. **Never** embed the token in `discovery.json` — that file is created with the default umask (typically `0644`) and is readable by other local users by design (it's how an unrelated MCP client can find the daemon). The token must stay private. Token is regenerated on every daemon start; an adapter holding a stale token gets `401` and re-reads `discovery.json` + `daemon.token`.

**Identity validation:** `pid_creation_time_ns` is the proc-creation timestamp from `/proc/<pid>/stat` (Linux), `proc_pidinfo` (macOS), or `GetProcessTimes` (Windows). Adapter and daemon both consult this to defend against PID reuse — if the recorded pid is alive but its creation_time doesn't match, the discovery file is stale and the named pid is some unrelated process. See `src/daemon/pid.rs:457-467` for the existing implementation; the new code reuses it.

All other state (workspace counts, embedding state, dashboard URL) lives behind HTTP `/status`; the adapter does not need it for handshake.

#### A.3 Lock invariants (two layers, on purpose)

The current code defends against the "577-daemon cascade" regression with three primitives (`SingletonLock` + `PidFile` + `daemon.state`). Codex review correctly flagged that collapsing this to a single `O_EXCL`-created file is not equivalent: `O_EXCL` is creation-time-only and leaves a stale file behind on crash, after which a new daemon either refuses to start (no recovery) or unlinks the file (no actual singleton enforcement). Stale-pid recovery via `kill(pid, 0)` is also vulnerable to PID reuse on systems with low pid space (containers, busy macOS boxes).

The design keeps **two** layers, but cleaner than the legacy four-file dance:

- **`~/.julie/daemon.lock`** — kernel-held advisory lock. On POSIX, acquired via `fcntl(F_SETLK, F_WRLCK)` on an opened file descriptor held for the daemon's lifetime; on Windows, `LockFileEx` with `LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY`. This is the actual singleton — the OS releases the lock when the holder's fd closes (whether on clean exit or crash), so there is no stale-state recovery problem. Lock file persists across daemon lifetimes; only the lock itself is per-process.
- **`~/.julie/discovery.json`** — endpoint metadata (above). Includes `pid` and `pid_creation_time_ns` so any consumer (adapter, dashboard, ad-hoc tooling) can answer "is this discovery record live?" without contending on the lock.

Startup decision tree:

1. Open `~/.julie/daemon.lock` (`O_CREAT | O_RDWR`, mode `0600`).
2. Try to acquire the kernel write lock (`fcntl(F_SETLK)` / `LockFileEx`, non-blocking).
3. **Lock acquired** → we are the singleton. Now check for a stale `discovery.json`:
   - If exists and (`pid` dead OR `pid_creation_time_ns` mismatches the live pid) → unlink, treat as clean start.
   - If exists and pid is alive *and* matches creation_time → impossible (we hold the lock), but treat as adversarial state and refuse with diagnostic.
   - Otherwise → bind listener, write `daemon.token` (mode `0600`) + `discovery.json` atomically.
4. **Lock not acquired** → another daemon is live. Read `discovery.json`, validate pid+creation_time. If valid → exit with "daemon already running at host:port." If invalid → the holder is a zombie that hasn't cleaned up yet; sleep + retry with bounded attempts before failing.

Shutdown reverses this: stop accepting new connections → run bounded drain (see A.5) → unlink `discovery.json` and `daemon.token` → drop the lock fd (kernel releases the lock).

This preserves the two real safety properties of the current four-file design (kernel-enforced singleton, pid-reuse-immune identity) without the four files or the "defense-in-depth" framing.

#### A.4 Legacy daemon migration gate

Codex review caught that an upgrade landing while an old `julie-server daemon` is still running would cause the new `julie-adapter` to look only at the new `discovery.json`, find nothing, and spawn `julie-daemon` — yielding two daemons writing the same workspace indexes. This is a hard correctness issue, not best-effort cleanup.

`julie-daemon start` runs this gate **before** any new-file work:

1. Check for legacy `~/.julie/daemon.singleton.lock`. If present, attempt `fcntl(F_SETLK, F_WRLCK)`. If the lock acquires → no live legacy daemon, unlink legacy file. If it doesn't → a legacy daemon is alive; exit with: `"Legacy julie-server daemon is running (pid {N}). Stop it with 'pkill julie-server' or wait for it to exit, then retry."`
2. Check for legacy `~/.julie/daemon.pid`. If present, read pid + creation_time. If alive and matching → exit (same diagnostic). If dead/mismatched → unlink.
3. Same for `~/.julie/daemon.state`, `~/.julie/daemon.lock`.
4. Only after all four legacy files are confirmed dead (or absent) → proceed to A.3 startup.

`julie-adapter` runs a parallel gate before spawning a daemon:

1. Read both new `~/.julie/discovery.json` AND any legacy `daemon.pid`. If either points to an alive process with matching identity, attach to that endpoint (legacy daemon listens on a port that's discoverable via the existing `daemon.port` file).
2. Only auto-spawn `julie-daemon` if both are absent or dead.

This means an upgraded plugin can safely run against a still-running legacy daemon during the rollout. After one release cycle the legacy detection is removed.

#### A.5 Bounded shutdown drain

Codex review correctly flagged that "graceful HTTP shutdown" without a bounded drain leaves stop semantics undefined when an MCP request is mid-mutation. axum's `with_graceful_shutdown` waits for in-flight requests indefinitely; if the request is stuck (deadlocked, waiting on the sidecar, etc.) the daemon hangs forever. Conversely an abrupt drop mid-mutation can leave SQLite WAL or Tantivy writer state inconsistent.

The new shutdown contract:

1. On shutdown signal (`SIGTERM` / `julie-daemon stop` / Windows shutdown event): publish `phase = "stopping"` in `discovery.json` via atomic rewrite. Adapter and dashboard read this and stop forwarding new requests.
2. Reject new HTTP requests with `503 Service Unavailable` + `Retry-After`.
3. Wait up to `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` (default 60, range 1-120, env-overridable as today) for in-flight requests to complete naturally. Counter is published to logs every second.
4. On timeout: write a recovery marker at `~/.julie/indexes/{workspace_id}/.unclean_shutdown` listing the in-flight mutations that were aborted. Force-abort the remaining requests with `502 Bad Gateway`. Daemon startup checks for this marker and surfaces it via `/status` so the user knows a forced shutdown happened and which workspaces may need a re-index.
5. After drain (clean or forced): run shutdown sequence in reverse dependency order — HTTP transport → embedding service → workspace pool → watcher pool → file unlinking → lock release.

This matches the safety property of today's `drain_sessions` (`src/daemon/mod.rs:731-750`) but with an explicit recovery marker instead of a silent best-effort log line.

#### A.6 Concurrency model: connection-per-request, not route-level lock

The original draft proposed a route-level `acquire_write_lock` at MCP tool entry. Codex review caught that this doesn't compose with julie's existing background writers — watcher event-processor, repair scan, repair-replay, tantivy retry, catch-up, register, refresh-stats — which take the mutation gate directly (`src/watcher/runtime.rs:139-158`, `:372-374`) and don't go through MCP handlers. Either they'd skip the route lock (so the lock protects nothing against watcher mid-write), or they'd take both in the order `mutation_gate → route_lock`, opposite the MCP-handler order, producing a deadlock surface that no two-MCP-request test would catch.

The actual fix for the "blocking calls under multiple julie instances" symptom is **connection-per-request**, not lock-at-handler. Eros's hub-deadlock bug was specifically "raw SQL on a shared `CanonicalStore` connection" wedging a parallel route. Julie's equivalent is the per-workspace connection state inside `WorkspacePool`. The reshape:

- Each MCP request acquires its own `rusqlite::Connection` from a per-workspace pool (size = `min(cpus * 2, 16)`, idle-evicted after 60s). Connections are pooled, cheap to acquire, and never shared across requests.
- Reads use `BEGIN DEFERRED TRANSACTION` for SQLite's MVCC-style read snapshot — concurrent readers don't block each other.
- Writes still go through the existing **`mutation_gate`** as the ONE canonical per-workspace serialization point. All writers — watcher, catch-up, repair scans, register, refresh-stats, MCP mutating tool handlers — acquire `mutation_gate::acquire_gate(workspace_id)` before mutating. The proof-token API (`MutationGuard<'_>`) stays. The route-level lock concept goes away.
- Tantivy: writers serialize through `IndexWriter` (already true); readers go through `IndexReader` with the `reload()` policy julie already uses.
- sqlite-vec: writes serialize via the mutation gate (sqlite-vec is just SQL); reads are connection-local.

The "parallel MCP requests deadlock" pattern eros hit becomes structurally impossible because no two requests share a SQLite connection or a Tantivy writer. Background writers and MCP requests interact only at the mutation gate, which is non-reentrant and proof-token enforced.

What remains to verify is *throughput under contention*, not correctness. The regression test becomes: 8 concurrent MCP requests against the same workspace (mix of read and write tools) plus an active filewatcher event stream — all complete within a bounded time, no wedging. This is broader than the original "two parallel routes" test and covers the full writer set codex called out.

### B. In-Process Test Harness

#### B.1 The embeddable surface

```rust
pub struct DaemonConfig {
    pub paths: DaemonPaths,
    pub bearer_token: String,
    pub embedding_provider: EmbeddingProviderConfig,
    pub no_dashboard: bool,
    pub runtime: DaemonRuntimeContext,  // see B.3
}

pub struct DaemonApp {
    config: DaemonConfig,
    // built-up state: pools, services, db handles
}

impl DaemonApp {
    pub fn new(config: DaemonConfig) -> Result<Self>;
    pub async fn serve(self, listener: TcpListener) -> Result<DaemonHandle>;
}

pub struct DaemonHandle {
    pub local_addr: SocketAddr,
    pub bearer_token: String,
    join_handle: JoinHandle<()>,
    shutdown_signal: oneshot::Sender<()>,
}

impl DaemonHandle {
    pub async fn shutdown(self) -> Result<()>;
}
```

Tests construct one:

```rust
let tmp = tempfile::tempdir()?;
let paths = DaemonPaths::for_julie_home(tmp.path())?;
let config = DaemonConfig {
    paths,
    bearer_token: "test-token".into(),
    embedding_provider: EmbeddingProviderConfig::Disabled,
    no_dashboard: true,
    runtime: DaemonRuntimeContext::for_test(),  // isolates global state
};
let listener = TcpListener::bind("127.0.0.1:0").await?;
let handle = DaemonApp::new(config)?.serve(listener).await?;

// Use handle.local_addr to make HTTP calls with reqwest, or use the
// MCP client crate against http://{addr}/.
```

Each test gets a fresh temp `.julie` directory, its own bound listener, and its own runtime context. Multiple tests run in parallel because nothing on disk is shared. No subprocess spawning.

#### B.2 Migration of existing tests

- `src/tests/daemon/server.rs` and `src/tests/integration/daemon_lifecycle.rs` tests that currently call `run_daemon` directly inherit the new shape with minor changes (they're already on a similar pattern).
- Tests that spawn `julie-server` via `tokio::process::Command` move to the in-process harness. A small number of explicit adapter-integration tests (verifying real stdio↔HTTP forwarding with a real subprocess) stay — a handful at most, not dozens.
- Embedding provider gets a `Disabled` config variant so tests don't pay the 36s sidecar warmup. Tests that specifically need embeddings opt in with `EmbeddingProviderConfig::Mock` (deterministic vector returns) or `Real` (rare; only for end-to-end coverage).

#### B.3 Process-global isolation contract

Codex review correctly flagged that tempdir isolation does not equal process isolation. Several pieces of julie's state are static or live in `lazy_static!`/`OnceCell`/`OnceLock` and survive across tests inside the same test binary:

- **Mutation gate registry** (`src/workspace/mutation_gate.rs:10-33`) — a process-global `DashMap<WorkspaceId, Arc<Mutex<()>>>` with an existing `clear_cache_for_test` hook used today via `serial_test`.
- **`tracing` subscriber** — `tracing_subscriber::registry().init()` can only be called once per process; subsequent inits silently fail.
- **Embedding provider singletons** — the sidecar service today has process-global lifetime semantics (one Python subprocess, one provider Arc).
- **Environment variables** — Rust 2024 marks `std::env::set_var` unsafe because env is process-global. Existing daemon tests serialize env mutations (`src/tests/integration/daemon_lifecycle.rs:202-218`).
- **OS-level filewatcher / inotify limits** — Linux has a system-wide watcher cap; spawning N watcher pools in parallel hits it.

`DaemonRuntimeContext::for_test()` materializes a per-test isolation harness:

```rust
pub struct DaemonRuntimeContext {
    pub mutation_gate_registry: Arc<MutationGateRegistry>,  // injectable, not global
    pub tracing_handle: Option<TracingHandle>,              // None = inherit parent
    pub env_overrides: HashMap<String, String>,             // applied to DaemonConfig, not process env
    pub workspace_id_namespace: Option<String>,             // prefixes generated workspace IDs
    pub watcher_quota: WatcherQuota,                        // bounds inotify/FSEvents use per harness
}
```

Production code path keeps the existing process-global behavior; tests get an injected context that owns its own state. Required mechanical changes:

- `mutation_gate` exposes a `Registry::new()` constructor so tests aren't forced to share the global cache (today only `clear_cache_for_test` is available). Production wires `Registry::global()`.
- All daemon code that reads env vars (drain timeout, embedding provider config, etc.) reads through `DaemonConfig` fields populated from env at startup, not via direct `std::env::var` calls inside long-lived code paths.
- Tracing subscriber init is centralized behind `DaemonRuntimeContext::install_tracing()`, which is a no-op when called repeatedly within the same process (idempotent, not panicky).
- Watcher pool accepts a max-watchers-per-pool config; tests cap this at a small number (default 4) to avoid inotify exhaustion.

Without this contract, `--test-threads=8` flakes via shared mutation_gate state, lost env mutations, or watcher exhaustion — symptoms easy to misdiagnose as test bugs. With it, parallelism is real.

#### B.4 What this enables

- Parallel tests run cheaply (each takes a port and a tmpdir; neither contended).
- Test of "two MCP sessions on the same daemon don't deadlock" becomes trivial: bind one daemon, fire two concurrent `reqwest` clients at it.
- Test of "adapter respawns daemon if it dies" stays possible because the adapter is a separate binary with a clean spawn path — those few tests still pay the subprocess cost, but they're the minority.
- The expanded parallel-route regression test from A.6 (8 concurrent MCP requests + active watcher) becomes a unit-test-cost check rather than a "spin up 9 daemon subprocesses" sleep-and-pray.

### C. Tantivy Reranker

#### C.1 Enriched Tantivy schema

Add three text fields to the symbol index alongside existing ones:
- `role` (e.g., `"source"`, `"test"`, `"docs"`, `"vendor"`, `"generated"`)
- `test_role` (e.g., `"unit"`, `"integration"`, `"smoke"`, or empty)
- `capability_flags` (concatenation of contract-capability flags if any)

These are derived during extraction (or as a one-time index migration) from path heuristics + symbol metadata. Same rule as julie's existing path-heuristic test detection: works for all 8+ project layouts in CLAUDE.md.

#### C.2 Query parsing

`parse_query(raw: &str) -> ParsedQuery`:
- Tokenize on whitespace, lowercase.
- Detect **symbol intent**: if `len >= 3` and `terms[0]` matches a symbol-kind keyword (`function`, `class`, `method`, `struct`, `trait`, `interface`, `type`, `enum`) → set `intent = Symbol(kind)`, `target_terms = terms[1..]`.
- Detect **test intent**: if `len >= 3` and `terms[0] == "test"` → set `intent = Test`, `target_terms = terms[1..]`.
- Otherwise `intent = Free`, `target_terms = terms`.

#### C.3 Reranker scoring

After Tantivy returns top-K candidates (K tuned for recall, default 50), apply a re-rank pass:

```rust
pub fn rerank(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked> {
    candidates.iter().map(|c| {
        let mut score = c.tantivy_score;  // base from BM25 + centrality
        let title_lc = c.title.to_lowercase();
        let title_lc_str = title_lc.as_str();

        for term in &query.target_terms {
            if title_lc_str == term { score += 100.0; }
            else if title_lc_str.contains(term) { score += 50.0; }
            if c.path.contains(term) { score += 40.0; }
            if c.body_contains_term(term) { score += 10.0; }
        }

        // Phrase boost
        if query.target_terms.len() >= 4 {
            let phrase = query.target_terms.join(" ");
            if c.body_contains_phrase(&phrase) {
                score += 260.0;
                if c.is_file_doc() { score += 120.0; }
                if c.is_source_language() { score += 50.0; }
            }
        }

        // Intent boosts
        match &query.intent {
            QueryIntent::Symbol(kind) => {
                if c.kind_matches(*kind) && c.title_matches_terms(&query.target_terms) {
                    score += 180.0;
                    if !c.is_test() { score += 120.0; }
                }
            }
            QueryIntent::Test => {
                if c.title_matches_terms(&query.target_terms) {
                    score += 180.0;
                    if c.is_test() { score += 120.0; }
                }
            }
            QueryIntent::Free => {}
        }

        // Kind boost on exact title match
        if title_lc_str == query.target_terms.first().map(String::as_str).unwrap_or("") {
            score += kind_boost(c.kind);
        }

        Ranked { candidate: c.clone(), final_score: score }
    }).collect::<Vec<_>>().sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap())
}
```

Numbers are starting points lifted from eros's `_score()`. Final weights tuned via the existing julie search-quality bucket (`cargo xtask test dogfood`).

#### C.4 Role-aware reweighting (NOT a new filter default)

Codex review correctly flagged that the original draft proposed `exclude_tests = true` as the new default unless test intent was detected — which would silently break common agent queries like `fast_search("fixture builder", target="definitions")` that expect to find test helpers. Today's behavior (`src/tools/search/text_search.rs:73-77`) only auto-excludes tests for NL-shaped content searches; definition searches include tests by default. **The reranker preserves this target-specific default.**

Revised role handling:

- **`target="definitions"` (symbol lookup):** include tests by default, exactly like today. The reranker re-weights — production code gets a small boost, tests get a small relative de-emphasis — but tests stay in the result set. A user explicitly searching for `MockFooProvider` or `assertion_helper` still finds them on page one.
- **`target="content"` and NL-shaped queries:** continue today's auto-exclude behavior (tests hidden by default unless `exclude_tests=false` or test intent detected).
- **Test intent detected** (query starts with `test ` followed by ≥2 more terms): tests are boosted heavily (+180 + role match), production results de-emphasized correspondingly. This matches the original eros scorer behavior for explicit test queries.
- **"All results are tests" fallback:** if filtering would empty the result set for an NL query, return the test results with a `filtered_tests_notice` so the agent knows what happened. Same as eros.
- **Explicit `exclude_tests` parameter:** caller wins. Always.

Regression tests must cover:
1. `target="definitions"` for a known test helper symbol still returns it on page one.
2. NL query for an implementation concept doesn't show test files first (today's behavior).
3. Explicit `test foo` query surfaces tests at the top.
4. Mixed impl-and-test files (Rust `#[cfg(test)] mod tests` inside a non-test path) classified correctly via symbol metadata, not just path heuristics.
5. Generated-file role (`role="generated"`) and vendored-file role (`role="vendor"`) get distinct de-emphasis weights from `"test"`, since they're different concerns.

The classification rules must work across all 8+ project layouts in `CLAUDE.md` (Rust `src/tests/`, .NET `*.Tests`, Python `tests/` and `test_*.py`, JS `__tests__/` and `*.test.ts`, Java `src/test/java/`, Go `*_test.go`, Ruby `test/` / `spec/`, Swift `Tests/`). Path heuristics + symbol metadata + extractor-emitted `test_role` — same rule julie already uses.

#### C.5 Rollout

- Feature-flagged behind a config option in the search subsystem (`reranker.enabled`, default false during development).
- Existing search quality fixture (`fixtures/search-quality/`) becomes the regression test corpus. `cargo xtask test dogfood` includes both reranker-off and reranker-on runs; reranker-on must not regress on any query category.
- Once green on the fixture, default to true and remove the flag.

## Acceptance Criteria

### A. Daemon Split
- [ ] `julie-adapter` exists as a separate binary and is what the plugin manifest spawns.
- [ ] `julie-daemon` exists as a separate binary with `start`/`stop`/`status` subcommands.
- [ ] On-disk lifecycle state is `~/.julie/daemon.lock` (kernel-held advisory lock) + `~/.julie/discovery.json` (endpoint metadata) + `~/.julie/daemon.token` (mode 0600 bearer token). The four legacy files are detected and refused-on-live (hard gate), unlinked only when confirmed dead via pid+creation_time validation.
- [ ] Bearer token is never embedded in `discovery.json`. Only `token_path` appears there. Daemon enforces 0600 mode on the token file (POSIX) and a restricted ACL (Windows) at write time.
- [ ] `restart_pending`, stale-binary mtime detection, and the `DaemonLifecycleController` state machine are removed.
- [ ] `run_daemon` is replaced by `DaemonApp::serve(listener)` and is under 200 lines.
- [ ] Singleton enforcement uses a kernel-held advisory lock (`fcntl(F_SETLK)` / `LockFileEx`) on a long-lived file descriptor. PID identity is validated by `pid_creation_time_ns` to defend against PID reuse.
- [ ] Legacy-daemon migration gate: starting `julie-daemon` while a live `julie-server daemon` process holds either the legacy singleton lock or a legacy `daemon.pid` with matching creation_time → daemon refuses to start with a clear diagnostic. `julie-adapter` against a live legacy daemon → adapter attaches to the legacy endpoint (via `daemon.port` file), doesn't spawn a new daemon.
- [ ] Bounded shutdown drain: on SIGTERM, daemon publishes `phase=stopping` in `discovery.json`, returns `503 Retry-After` on new requests, waits up to `JULIE_DAEMON_DRAIN_TIMEOUT_SECS` (default 60) for in-flight mutations to complete. On timeout, writes `.unclean_shutdown` recovery markers under affected workspace index dirs and force-aborts; next daemon startup surfaces them via `/status`.
- [ ] Concurrency regression test: 8 concurrent MCP requests (mixed read+write tool calls) on the same workspace, plus an active filewatcher event stream, all complete within a bounded time with no wedging. Replaces the original two-request framing.
- [ ] No MCP-tool handler holds a route-level write lock. Handlers acquire a per-request SQLite connection from `WorkspaceConnectionPool`. Mutating handlers also acquire the per-workspace `mutation_gate` (same primitive used by watcher/catch-up/repair/etc.) — the gate is the single canonical writer-serialization point.
- [ ] Plugin still works against the new binaries — manual end-to-end test in Claude Code.
- [ ] End-to-end test of "upgraded plugin against running legacy daemon" passes: adapter attaches to the legacy daemon, no second daemon spawned, all four legacy files remain untouched until the legacy daemon exits.

### B. In-Process Test Harness
- [ ] `DaemonApp::new(config) -> Result<Self>` and `.serve(listener) -> Result<DaemonHandle>` exist and are documented.
- [ ] `DaemonHandle::shutdown()` cleanly stops the server and releases all resources.
- [ ] All daemon lifecycle tests run in-process. The number of subprocess-spawning tests in the suite drops to a handful explicitly testing adapter↔daemon stdio↔HTTP forwarding.
- [ ] `DaemonRuntimeContext::for_test()` exists and isolates mutation gate registry, tracing init, env overrides, workspace-id namespace, and watcher quota per test. Production paths use `DaemonRuntimeContext::production()` which preserves today's process-global behavior.
- [ ] `mutation_gate` exposes `Registry::new()` for injection; tests do not use the global cache.
- [ ] Tracing init is idempotent (`install_tracing()` can be called many times per process without panicking).
- [ ] All daemon code reads env vars via `DaemonConfig` fields populated at startup, not via direct `std::env::var` in long-lived paths.
- [ ] Watcher pool accepts a `max_watchers` config; tests default to 4 to avoid inotify exhaustion.
- [ ] `cargo xtask test dev` runtime drops measurably (track baseline before/after).
- [ ] Tests pass at `--test-threads=8` without machine saturation AND without `serial_test` annotations on the migrated tests.

### C. Search Reranker
- [ ] `src/search/reranker.rs` exists, returning ranked candidates by the algorithm above.
- [ ] Tantivy symbol schema includes `role`, `test_role`, `capability_flags` fields.
- [ ] Query parsing detects symbol intent and test intent per the rules above.
- [ ] Role-aware reweighting preserves target-specific `exclude_tests` defaults — definitions include tests by default, NL queries auto-exclude, explicit caller param always wins.
- [ ] `fast_search("fixture builder", target="definitions")` regression test passes: known test helper symbols still appear on page one.
- [ ] Explicit `test foo` query regression test passes: test symbols appear at top.
- [ ] `cargo xtask test dogfood` shows no regression on the search-quality fixture with reranker enabled, AND no regression in the test-helper-discoverability fixture.
- [ ] When measured against the same query set eros used in their bakeoff (or julie's own equivalent), top5 hit-rate improves materially over baseline.

## Migration Plan

The three pieces ship in order — A, then B, then C — but they can land as separate PRs. C does not block on A/B.

1. **A.1 (lifecycle + binaries)**: introduce `DaemonApp` and refactor `run_daemon` to call it. Discovery file + token file + kernel advisory lock land here. Legacy-daemon detection gate lands here. Both binaries exist; old `julie-server` becomes a thin shim that dispatches to whichever binary the argv asks for. Bounded shutdown drain (A.5) implemented.
2. **A.2 (concurrency)**: `WorkspaceConnectionPool` introduced. MCP handlers acquire connection-per-request. Mutating handlers acquire `mutation_gate` (already exists, just expanded coverage). 8-way concurrency regression test added. **No route-level lock is introduced.**
3. **B (test harness)**: `DaemonRuntimeContext` introduced. Tests migrate to in-process harness. Subprocess-spawning tests trimmed to the small adapter-integration set.
4. **C (reranker)**: enriched Tantivy schema + reranker (feature-flagged). Quality fixture + filter-default regression fixture validate. Flag flips on.

Plugin distribution updates (`julie-plugin` manifest pointing at `julie-adapter`) ride with A.1, but the legacy `julie-server` shim stays in place for one release cycle so existing installs continue to work even before plugin update.

Legacy lifecycle files (`daemon.pid`, `daemon.state`, `daemon.lock`, `daemon.singleton`) are NOT unconditionally cleaned. `julie-daemon start` only unlinks them after verifying the legacy daemon is dead (per A.4). After two release cycles with no legacy daemon sightings in telemetry, the detection code is removed.

## Revision History

**2026-05-16 (v2)** — incorporated codex adversarial review (`gpt-5.5` high reasoning). Seven findings addressed:

| # | Severity | Finding | Resolution |
|---|----------|---------|------------|
| 1 | critical | `discovery.lock` via `O_EXCL` doesn't actually enforce singleton; PID-reuse vulnerable | A.3 rewritten: kernel-held advisory `daemon.lock` + `pid_creation_time_ns` in discovery |
| 2 | high | New daemon can spawn beside live legacy daemon during upgrade | New A.4 hard migration gate; both adapter and daemon detect live legacy state |
| 3 | high | Proposed route-level write lock doesn't compose with background writers; deadlock surface | Route lock removed entirely; replaced with connection-per-request + existing `mutation_gate` as single per-workspace writer serialization (A.6) |
| 4 | high | Bearer token in `discovery.json` is a security regression vs current `token_path` design | A.2 revised: `token_path` only in discovery; separate `daemon.token` file mode 0600 |
| 5 | high | Removing drain leaves shutdown undefined for in-flight mutations | New A.5 bounded drain spec with recovery markers |
| 6 | medium | In-process harness assumes tempdir isolation = process isolation | New B.3 isolation contract via `DaemonRuntimeContext` |
| 7 | medium | Reranker default `exclude_tests=true` breaks definition queries for test helpers | C.4 rewritten to preserve target-specific defaults; reranker re-weights, doesn't filter |

**2026-05-15 (v1)** — initial design.
