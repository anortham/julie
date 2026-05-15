# Julie Daemon Split + Search Reranker — Design

## Purpose

Three related changes to make julie more reliable and less complex by backporting structural lessons from eros, without abandoning julie's Rust-native stack or its current feature surface:

1. **Daemon split**: replace the single `julie-server` binary (which is simultaneously the adapter spawned by MCP clients and the daemon those adapters bridge to) with two binaries — `julie-adapter` (thin stdio→HTTP forwarder) and `julie-daemon` (HTTP server). Collapse the four-file lifecycle handshake (`daemon.pid` + `daemon.state` + `daemon.lock` + `daemon.singleton`) into a single atomic `discovery.json` + advisory `discovery.lock`. Adopt route-level SQLite locking at the MCP tool boundary to eliminate the parallel-instance blocking-call class of deadlocks.

2. **In-process test harness**: make the daemon embeddable as a library so tests bind a `tokio::TcpListener` on `127.0.0.1:0` and run `DaemonApp::serve(listener)` in-process — eliminating the test-suite resource explosion from dozens of spawned daemon subprocesses.

3. **Search reranker (revised C)**: add a re-ranking stage on top of julie's existing Tantivy results that applies an eros-style scorer — query intent detection (symbol intent, test intent), kind-aware boosts, role-aware default filtering, phrase boost for long queries — backed by enriched Tantivy fields (`role`, `test_role`, `capability_flags`). Tantivy continues to retrieve candidates; the reranker reweighs them. No substring scans, no projection table. This is the source of the search-quality gap measured against eros's bakeoff (julie 70% top5 → eros 100% top5).

These three are sequenced: A enables B (without the split, tests can't run multiple daemons in parallel); C is independent of A/B but shares the same release cadence and benefits from the same lifecycle simplification.

## Architecture Quality

**Affected modules:**
- `src/main.rs`, `src/adapter/*`, `src/daemon/*` — binary split, lifecycle simplification
- `src/paths.rs` — discovery file path consolidation
- `src/handler.rs`, `src/handler/*` — route-level locking boundaries at MCP tool handlers
- `src/tools/*` — handlers wrap mutating DB work in a single locked region
- `src/search/*` (or new `src/search/reranker.rs`) — new reranking stage and enriched Tantivy schema
- `src/tests/*` — test harness refactor
- New: `crates/julie-adapter/` (or a `[[bin]]` target if staying in workspace) — thin adapter binary
- New: `xtask` updates to build both binaries and run the in-process harness as the default test path

**Caller-facing interfaces:**
- MCP tool surface: unchanged. Existing tools (`fast_search`, `deep_dive`, `get_symbols`, etc.) keep their signatures and observed behavior, except search results from `fast_search` improve in quality and ordering.
- CLI: `julie-server` is replaced by `julie-adapter` (default for MCP clients) and `julie-daemon` (explicit `julie-daemon start|stop|status`). The plugin manifest is updated to spawn `julie-adapter` instead of `julie-server`. Existing `julie-server` invocations get a thin compatibility shim that delegates to the right binary based on argv (no behavioral break for any user/plugin update path).
- Discovery on disk: `~/.julie/daemon.pid` + `daemon.state` + `daemon.lock` + `daemon.singleton` → `~/.julie/discovery.json` + `discovery.lock`. Old files are removed during the daemon's first startup if present (one-shot migration cleanup).

**Depth/locality:**
- Adapter binary is small and has no SQLite, no Tantivy, no embeddings, no extractors. Just stdio JSON-RPC parsing, HTTP forwarding, retry on transport error, child-process spawn.
- Daemon binary owns all the heavy state. `run_daemon` becomes `DaemonApp::serve(listener)` — components instantiated via a builder that captures dependency ordering in types rather than implicit comments.
- Reranker is a pure function `rerank(query, candidates) -> ranked` with no coupling to Tantivy internals beyond the result struct shape. Enriched fields live in Tantivy schema; the reranker reads them off the returned doc.

**Test surface:**
- Adapter: integration test that spawns a real `julie-adapter` subprocess against an in-process `DaemonApp`, exercises stdio↔HTTP forwarding, kill/respawn behavior.
- Daemon: every existing daemon test moves to the in-process harness. The `daemon_lifecycle::test_*` suite verifies discovery file write/cleanup, O_EXCL contention behavior, atomic rename invariants.
- Route locking: regression test that two concurrent MCP requests on the same workspace both complete (does not wedge) — this is the eros parallel-route regression test pattern.
- Reranker: pure-function tests for intent detection, kind boost matrix, role filtering. Quality regression test against a small fixed corpus checking top5 stays at or above current numbers.

**Seams / adapters:**
- `DaemonApp` — the embeddable entrypoint. `pub fn new(config: DaemonConfig) -> Self`, `pub async fn serve(self, listener: TcpListener) -> Result<DaemonHandle>`. `DaemonHandle::shutdown()` for graceful teardown in tests.
- `DiscoveryFile` — the read/write API for discovery state, with `acquire_lock_and_write(record) -> Result<DiscoveryGuard>` returning a guard whose Drop cleans up.
- `StoreLockBoundary` — a thin macro or helper for MCP tool handlers that wraps DB writes in a single locked region (`tool_handler!(workspace_id, |store| { ... })`).
- `Reranker` — `pub fn rerank(query: &ParsedQuery, candidates: &[Candidate]) -> Vec<Ranked>`. Lives behind a config flag during rollout; default-on once quality regression test confirms no regressions.

**Rejected shortcuts:**
- Keeping `julie-server` as one binary and only adding internal modules. Doesn't address the core "adapter and daemon share code paths" cause of stale-binary detection + restart-pending complexity.
- Putting vectors in Tantivy (verified: not natively supported through 0.26).
- Swapping the storage stack to LanceDB (eros's own bakeoff data shows LanceDB candidates underperform their non-LanceDB baseline; storage isn't the lever).
- Porting eros's `instr()`-based substring scan retrieval. It's transitional eros scaffolding for candidate comparison, not a viable production design — full-table-scan O(rows × terms × columns) per query.
- Daemon-as-library with no HTTP (running embedded in the adapter process). Would re-create the test-suite resource problem we're trying to fix and would not address the parallel-instances pain.

**Architecture risk:** medium. The binary split is mechanically large but conceptually simple. The route-level locking refactor has the highest risk — it can deadlock or regress concurrent-throughput if locks span too much work. Mitigation: a dedicated parallel-route regression test (the eros pattern) and per-tool locking scope review.

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

`~/.julie/discovery.json` (minimal, eros-shape):

```json
{
  "pid": 12345,
  "host": "127.0.0.1",
  "port": 49213,
  "bearer_token": "<random base64url, 32 bytes>",
  "log_path": "/Users/.../.julie/daemon.log.2026-05-15",
  "daemon_version": "7.7.4",
  "protocol_version": 1,
  "started_at": "2026-05-15T17:42:00Z"
}
```

Atomic write: write to `discovery.json.tmp` → `fsync` → `rename` over `discovery.json`. Adapter reads with single `read_to_string`. Stale-detection by `kill(pid, 0)` (POSIX) or `OpenProcess` (Windows). All other state (workspace counts, embedding state, dashboard URL) lives behind HTTP `/status`; the adapter does not need it for handshake.

#### A.3 Lock invariants

- `discovery.lock` acquired with `O_CREAT | O_EXCL | O_WRONLY` at daemon start. **Only** primitive for "is a daemon already running here." Held for the lifetime of the daemon process. Released on process exit (file unlinked in the Drop guard for clean exits; left behind for crash recovery, but the next daemon detects this by re-attempting and finding the pid in `discovery.json` dead, then unlinking and proceeding).
- The crash-recovery story: on `O_EXCL` failure, daemon reads `discovery.json`. If the recorded pid is dead → unlink both files and retry once. If alive → exit "another daemon is running." This is one decision point, not three.
- No `SingletonLock` separate from PidFile separate from `daemon.state`. The combinations the existing code defends against ("577-daemon cascade") collapse: if `discovery.json` is unreadable or the pid is dead, the lock is the source of truth; if the lock is held, the running daemon's pid is in `discovery.json`. Atomic rename makes "discovery file half-written" impossible.

#### A.4 Route-level SQLite locking

Each MCP tool handler that mutates state acquires a single named lock region at the handler entry point. The pattern (Rust analogue of eros's `with store.locked():`):

```rust
async fn handle_index_command(ctx: ToolContext, args: IndexArgs) -> Result<IndexResponse> {
    let store = ctx.store_for(&args.workspace_id).await?;
    let _guard = store.acquire_write_lock(LockReason::Index).await;
    // ... all SQL writes for this tool call happen under this guard.
}
```

`store.acquire_write_lock(reason)` returns a `StoreWriteGuard<'a>`. Reads happen on a separate connection (eros's pattern: catch-up uses a different connection than request handlers). The lock is per-workspace, keyed by `workspace_id` (matching the existing `mutation_gate` keying convention — these two lock surfaces interoperate without nesting).

The route-level lock differs from the mutation gate in scope: the mutation gate protects the canonical writer pipeline against concurrent writers (filewatcher, catch-up, force-reindex). The route lock protects an entire MCP tool response from interleaved partial reads/writes by a parallel call on the same workspace. They share the same key but acquire in a documented order (route lock → mutation gate) to prevent nested-acquisition deadlock, enforced via a `RouteLockGuard` proof-token pattern mirroring the existing `MutationGuard<'_>`.

Tools that are read-only (the search/inspect family) take a shared/read lock; tools that mutate (`manage_workspace` index/refresh, editing tools, register) take an exclusive/write lock.

### B. In-Process Test Harness

#### B.1 The embeddable surface

```rust
pub struct DaemonConfig {
    pub paths: DaemonPaths,
    pub bearer_token: String,
    pub embedding_provider: EmbeddingProviderConfig,
    pub no_dashboard: bool,
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
};
let listener = TcpListener::bind("127.0.0.1:0").await?;
let handle = DaemonApp::new(config)?.serve(listener).await?;

// Use handle.local_addr to make HTTP calls with reqwest, or use the
// MCP client crate against http://{addr}/.
```

Each test gets a fresh temp `.julie` directory and its own bound listener. Multiple tests run in parallel because nothing shared on disk — `discovery.lock` and `discovery.json` are scoped to the test's temp dir. No subprocess spawning.

#### B.2 Migration of existing tests

- `src/tests/daemon/server.rs` and `src/tests/integration/daemon_lifecycle.rs` tests that currently call `run_daemon` directly inherit the new shape with minor changes (they're already on a similar pattern).
- Tests that spawn `julie-server` via `tokio::process::Command` move to the in-process harness. A small number of explicit adapter-integration tests (verifying real stdio↔HTTP forwarding with a real subprocess) stay — but there should be only a handful, not dozens.
- Embedding provider gets a `Disabled` config variant so tests don't pay the 36s sidecar warmup. Tests that specifically need embeddings opt in with `EmbeddingProviderConfig::Mock` (deterministic vector returns) or `Real` (rare; only for end-to-end coverage).

#### B.3 What this enables

- Parallel tests run cheaply (each takes a port and a tmpdir, neither contended).
- Test of "two MCP sessions on the same daemon don't deadlock" becomes trivial: bind one daemon, fire two concurrent `reqwest` clients at it.
- Test of "adapter respawns daemon if it dies" becomes possible because the adapter is a separate binary with a clean spawn path.

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

#### C.4 Role-aware filtering

Default behavior: `exclude_tests = true` for queries that don't show test intent. When all surviving candidates are tests and `exclude_tests` was implicit, return the test results with a notice (eros pattern). Explicit `exclude_tests = false` skips the filter entirely.

#### C.5 Rollout

- Feature-flagged behind a config option in the search subsystem (`reranker.enabled`, default false during development).
- Existing search quality fixture (`fixtures/search-quality/`) becomes the regression test corpus. `cargo xtask test dogfood` includes both reranker-off and reranker-on runs; reranker-on must not regress on any query category.
- Once green on the fixture, default to true and remove the flag.

## Acceptance Criteria

### A. Daemon Split
- [ ] `julie-adapter` exists as a separate binary and is what the plugin manifest spawns.
- [ ] `julie-daemon` exists as a separate binary with `start`/`stop`/`status` subcommands.
- [ ] `~/.julie/discovery.json` and `~/.julie/discovery.lock` are the only daemon lifecycle files. The four legacy files (`daemon.pid`, `daemon.state`, `daemon.lock`, `daemon.singleton`) are removed from `paths.rs` and cleaned up on daemon startup if encountered.
- [ ] `restart_pending`, stale-binary mtime detection, and the `DaemonLifecycleController` state machine are removed.
- [ ] `run_daemon` is replaced by `DaemonApp::serve(listener)` and is under 200 lines.
- [ ] MCP-tool handlers acquire a route-level write lock for mutating tools and a read lock for read-only tools. The route lock and mutation gate compose without nested-acquisition deadlock (proof-token enforced).
- [ ] Parallel-route regression test passes: two concurrent MCP tool calls on the same workspace complete without wedging.
- [ ] Plugin still works against the new binaries — manual end-to-end test in Claude Code.

### B. In-Process Test Harness
- [ ] `DaemonApp::new(config) -> Result<Self>` and `.serve(listener) -> Result<DaemonHandle>` exist and are documented.
- [ ] `DaemonHandle::shutdown()` cleanly stops the server and releases all resources.
- [ ] All daemon lifecycle tests run in-process. The number of subprocess-spawning tests in the suite drops to a handful explicitly testing adapter↔daemon stdio↔HTTP forwarding.
- [ ] `cargo xtask test dev` runtime drops measurably (track baseline before/after).
- [ ] Tests pass at `--test-threads=8` without machine saturation.

### C. Search Reranker
- [ ] `src/search/reranker.rs` exists, returning ranked candidates by the algorithm above.
- [ ] Tantivy symbol schema includes `role`, `test_role`, `capability_flags` fields.
- [ ] Query parsing detects symbol intent and test intent per the rules above.
- [ ] Role-aware filtering hides tests by default, with the "all results are tests → show anyway with notice" fallback.
- [ ] `cargo xtask test dogfood` shows no regression on the search-quality fixture with reranker enabled.
- [ ] When measured against the same query set eros used in their bakeoff (or julie's own equivalent), top5 hit-rate improves materially over baseline.

## Migration Plan

The three pieces ship in order — A, then B, then C — but they can land as separate PRs. C does not block on A/B.

1. **A.1**: introduce `DaemonApp` and refactor `run_daemon` to call it. Discovery file changes go in this PR. Lock surface refactor in this PR. Both binaries exist; old `julie-server` becomes a thin shim that dispatches to whichever binary the argv asks for.
2. **A.2**: route-level locking refactor. Wraps mutating tool handlers in `acquire_write_lock`. Adds the parallel-route regression test.
3. **B**: tests migrate to in-process harness. Number of subprocess-spawning tests trimmed to a small set.
4. **C**: enriched Tantivy schema + reranker (feature-flagged). Quality fixture validates. Flag flips on.

Plugin distribution updates (`julie-plugin` manifest pointing at `julie-adapter`) ride with A.1.

Old paths (`daemon.pid`, `daemon.state`, `daemon.lock`, `daemon.singleton`) get a one-shot cleanup on first daemon startup — `julie-daemon start` checks for them and unlinks. After one release cycle this cleanup code is removed.
