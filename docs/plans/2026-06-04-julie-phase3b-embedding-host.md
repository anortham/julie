# Julie Rescue Phase 3b — Resident Embedding-Host Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `@razorback:subagent-driven-development` (subagent delegation is available — visible team, inline lead review). Fall back to `@razorback:executing-plans` only if delegation becomes unavailable. Follow `@razorback:test-driven-development` for every task.

**Goal:** Stand up a resident "embedding-host" process that owns the single PyTorch sidecar and serves `embed_query`/`embed_batch`/`health` to N independent Julie processes over a UDS (unix) / named-pipe (windows) front door, plus a thin RPC-client `EmbeddingProvider` — proving "one sidecar serves N processes" *additively*, before any daemon teardown.

**Architecture:** Reuse the existing `SidecarEmbeddingProvider` + `RequestEnvelope`/`ResponseEnvelope` newline-delimited-JSON protocol **as-is**. The host process holds one `Arc<dyn EmbeddingProvider>` (the real sidecar) behind an async `tokio` accept loop; each accepted connection dispatches requests to the provider via `spawn_blocking` (the provider's `Mutex<SidecarProcess>` serializes naturally — one model, one GPU). Session/tool processes get a **blocking** `RpcEmbeddingProvider` that implements the same `julie_core::EmbeddingProvider` trait and forwards over the socket. Because every consumer already depends only on `Arc<dyn EmbeddingProvider>` (via `ToolContext::embedding_provider`/`ensure_embedding_provider`), the RPC client drops in at one seam with zero call-site changes. 3b is opt-in (env-gated) and does **not** remove the daemon's existing shared sidecar — the cutover is 3c.

**Tech Stack:** Rust, tokio 1.47.1 (`full`, already a dep — provides `tokio::net::UnixListener` + `tokio::net::windows::named_pipe` with no new crates), `serde_json` (existing envelope types), `fs2` (existing, for the host singleton lock), `tokio_util::sync::CancellationToken` (graceful shutdown, mirrors `HttpTransportServer`). Python sidecar provisioning (`uv`) and the `embeddings-sidecar` cargo feature are unchanged.

**Architecture Quality:** Approved shape — (1) the **wire contract** and the **client struct shape** are *lead-fixed* in §Fixed Contract below; workers fill implementations inside it and may not redesign the protocol or trait surface. (2) New code placement obeys the crate DAG: `RpcEmbeddingProvider` + transport + host-server live in **julie-pipeline** (which already owns the sidecar + protocol and is depended on by tools/runtime/julie); the host **binary** + process-launch glue live in the top **julie** crate; only **two** path-helper methods are added to **julie-core**. No code in `julie-context` (core+index only). **Main architecture risk:** cross-platform IPC — Unix UDS is the must-prove path (fully implemented + tested); Windows named-pipe is compile-guarded and documented-untested (matches the design doc's Unix-proof-is-the-must-have stance). Secondary risk: the sync `EmbeddingProvider` trait vs an async host — resolved by making the **client blocking** (no tokio) so it is safe inside the existing `spawn_blocking` embed call sites, while the **host is async**.

---

## Fixed Contract (lead-owned — do NOT redesign; implement against these exact shapes)

### The substitution seam (verified at `crates/julie-core/src/embeddings_contract.rs:75`)
```rust
pub trait EmbeddingProvider: Send + Sync {
    fn embed_query(&self, text: &str) -> anyhow::Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn device_info(&self) -> DeviceInfo;            // { runtime, device, model_name, dimensions }
    fn accelerated(&self) -> Option<bool> { None }
    fn degraded_reason(&self) -> Option<String> { None }
    fn shutdown(&self) {}
    fn wait_for_exit(&self, _timeout: Duration) -> bool { true }
}
```
The `RpcEmbeddingProvider` implements this trait. `shutdown()` closes its socket; `wait_for_exit()` returns `true` immediately (it does **not** own the host). `dimensions()`/`device_info()`/`accelerated()`/`degraded_reason()` are served from a `health` response cached at first connect.

### The wire protocol (REUSED AS-IS — verified at `crates/julie-pipeline/src/embeddings/sidecar_protocol.rs:10`, all types already `pub`)
- Framing: **newline-delimited JSON**, one object per line, **no length prefix** — identical to `SidecarProcess::send_request_with_timeout` (`sidecar_provider.rs:365`). Write `serde_json::to_writer(stream, &envelope)`, then `b"\n"`, then flush; read one line, `trim()`, `serde_json::from_str`.
- `RequestEnvelope<T> { schema: String, version: u32, request_id: String, method: String, params: T }`
- `ResponseEnvelope<T> { schema, version, request_id, result: Option<T>, error: Option<ProtocolError> }` (exactly one of result/error set)
- Constants: `SIDECAR_PROTOCOL_SCHEMA = "julie.embedding.sidecar"`, `SIDECAR_PROTOCOL_VERSION = 1`
- Methods: `health` (params `{}` → `HealthResult`), `embed_query` (`EmbedQueryRequest{text}` → `EmbedQueryResult{dims,vector}`), `embed_batch` (`EmbedBatchRequest{texts}` → `EmbedBatchResult{dims,vectors}`), `shutdown` (params `{}`).
- Reuse the existing validators: `validate_response_envelope`, `validate_query_response`, `validate_batch_response`, `validate_health_response` (all `pub` from `julie-pipeline/src/embeddings/mod.rs`).

### Concurrency / connection model (lead-decided)
- **One persistent connection per client; one in-flight request per connection** (matches the sync trait). The client holds connection state behind a `Mutex` for interior mutability.
- **Host serializes all embeds** through the single `Arc<dyn EmbeddingProvider>` (its inner `Mutex<SidecarProcess>` already enforces one-request-at-a-time against the Python child — correct, since there is one model on one device). Multiple connections are accepted concurrently; their `embed_*` dispatches queue on that mutex inside `spawn_blocking`.
- `request_id`: per-connection sequential (`req-1`, `req-2`, …) — sufficient because each connection is strictly sequential. No correlation-ID multiplexing.

### Socket / lifecycle conventions (lead-decided)
- **Global per `$JULIE_HOME`** (not per-workspace): one host serves all workspaces, mirroring today's process-global `EmbeddingService`. Unix socket `$JULIE_HOME/embedding-host.sock`; Windows pipe name derived from the `$JULIE_HOME` hash (mirror `DaemonPaths::daemon_shutdown_event` naming); singleton lock `$JULIE_HOME/embedding-host.lock` (`fs2` exclusive).
- **Client = blocking std I/O** (`std::os::unix::net::UnixStream` / Windows named-pipe file handle). NOT tokio. This keeps the sync trait methods safe inside the existing `spawn_blocking` embed sites.
- **Host = async tokio** (`UnixListener` accept loop), graceful shutdown via `CancellationToken` (model on `src/daemon/http_transport.rs:125-335`).
- **Opt-in for 3b:** env var `JULIE_EMBEDDING_USE_HOST` (truthy → use the host path). Absent → existing behavior, fully unchanged. 3b does not flip the daemon's default wiring; that is 3c.
- Client connection failure: reconnect-once-per-call on broken pipe/connection-reset; if the host is unreachable after the retry, return an `anyhow::Err` (the host's own `FATAL_THRESHOLD=3` circuit breaker already governs the Python child — the client adds connection resilience, not a second model-level breaker).

---

## File Structure

| Action | Path | Responsibility |
|---|---|---|
| Modify | `crates/julie-core/src/paths.rs` (DaemonPaths, ~`:243`→end) | Add `embedding_host_socket()`, `embedding_host_pipe_name()`, `embedding_host_lock()` |
| Create | `crates/julie-pipeline/src/embeddings/host_transport.rs` | Cross-platform IPC seam: async server listener + blocking client connection; cfg(unix)/cfg(windows) |
| Create | `crates/julie-pipeline/src/embeddings/rpc_client.rs` | `RpcEmbeddingProvider` — blocking `EmbeddingProvider` over `host_transport` client |
| Create | `crates/julie-pipeline/src/embeddings/host_server.rs` | `run_embedding_host(...)` — owns one provider, accept loop, dispatch, graceful shutdown, singleton lock |
| Modify | `crates/julie-pipeline/src/embeddings/mod.rs` (~`:45`) | Export the three new modules' public items |
| Create | `src/bin/julie-embedding-host.rs` | Thin binary: env/args → tracing → DaemonPaths → `run_embedding_host` → SIGTERM handling |
| Modify | `Cargo.toml` (root, `[[bin]]` block near julie-adapter/julie-daemon) | Register `julie-embedding-host` bin (gate features to include `embeddings-sidecar`) |
| Create | `src/embedding_host_launch.rs` (top crate) + wire into `src/lib.rs`/`mod` | `connect_or_spawn_host(paths) -> Result<RpcEmbeddingProvider>` — launch glue (knows the bin) |
| Modify | `src/daemon/app/helpers.rs` (`spawn_embedding_init`, `:403`) | Opt-in branch: when `JULIE_EMBEDDING_USE_HOST`, build provider via `connect_or_spawn_host` instead of `create_embedding_provider`, then `publish_ready` |
| Create | `crates/julie-pipeline/src/tests/embeddings/host_roundtrip.rs` (+ register) | Unit/integration: transport round-trip, client↔fake-host, protocol mapping |
| Create | `src/tests/daemon/embedding_host_multi_session.rs` (+ register in tests mod) | **Acceptance:** one host + 3 concurrent clients → one sidecar, all embed OK |

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` / `AGENTS.md` (xtask tiers + TDD), `docs/TESTING_GUIDE.md`, `RAZORBACK.md` (routing + gate ownership). Crate-split rules from the rescue: dep-direction tripwires + a `cargo nextest --no-run` build gate.

**Worker red/green scope:** the narrowest behavior proof — `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. Sidecar/host tests that touch the real provider require `--features embeddings-sidecar` (e.g. `cargo nextest run -p julie-pipeline --features embeddings-sidecar <name>`). Workers run **at most twice** per change (RED then GREEN).

**Worker ceiling:** the single named test (or the one new test file) for their task. Workers do **not** run `cargo xtask test changed/dev` or any tier — the lead owns those.

**Worker gate invariant (per task):** stated in each task's acceptance criteria — e.g. Task 8's gate proves "exactly one sidecar child is spawned while 3 concurrent clients all embed successfully."

**Lead affected-change scope:** `cargo xtask test changed` after each coherent batch; if it falls back to `dev`, accept it (shared infra moved).

**Branch gate (lead, before PR):** `cargo xtask test dev` GREEN + the dep-direction tripwire scan + `cargo nextest run --no-run` (cfg(test) build) GREEN + the acceptance test (Task 8) GREEN under `--features embeddings-sidecar`. Per the rescue rule, `--no-run` does NOT execute integration tripwires — so the lead also runs the full relevant `-p` dev tier, not just `--no-run`.

**Replay/metric evidence:** Task 8 is the acceptance gate. **Hard gate:** (a) all 3 concurrent clients return vectors of the correct `dims`; (b) exactly **one** sidecar child spawned (single "Embedding provider initialized" log / single spawn-counter increment). **Report-only:** per-call latency.

**Escalation triggers (→ lead/Opus):** any change to the wire protocol or trait surface; cross-process lifecycle/shutdown ordering; a worker's transport test hangs or flakes; two worker attempts fail review; Windows-path uncertainty.

**Assigned verification failure:** workers stop and report; they do not "fix" the gate or present a failing run as evidence.

**Verification ledger:** use `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, timestamp. Include a **relink-evidence** row proving an edit to a `julie-pipeline` test does not relink the top `julie` crate's test monolith (the rescue's whole point).

---

## Model Routing

**Project source of truth:** `RAZORBACK.md`. This is **shared-invariant work** (transport, process lifecycle, concurrency) per RAZORBACK.md §"shared-invariant areas", so the bar is raised below.

**Strategy / lead (Opus):** the Fixed Contract, IPC abstraction signatures, all review, Task 8 acceptance interpretation, branch + specialist gates. Harness mapping: `opus`.

**Coupled-implementation tier (workers, within the lead-fixed contract):** Tasks 3–8 bounded edits. Harness mapping: `sonnet`. (RAZORBACK: "Coupled implementation → Sonnet high or Opus.")

**Mechanical tier:** none — every task owns executable behavior or a gate. Do not assign a mechanical worker any task here.

**Gate-interpretation reviewer:** the lead (Opus) reads the plan + failing/passing test + diff for Task 8 and the protocol tasks.

**Escalation tier (Opus):** concurrency/shutdown correctness, protocol changes, repeated worker failure, Windows path.

**Worker eligibility:** Tasks 3–8 are worker-eligible **only because** Tasks 1–2 fix the contract (paths + transport seam) first. Workers get a frozen wire format, frozen struct shapes, and disjoint files.

**Lead-owned lanes (not delegated):** Task 1 (julie-core path contract) and Task 2 (the `host_transport` cross-platform seam) — these define the invariant everything else builds on, and Task 2 carries the Windows uncertainty. The lead implements/finalizes these and assigns 3–8 against them.

**Unsupported harness behavior:** Claude Code selects per-agent model via the Agent `model` param (`opus`/`sonnet`/`haiku`) — fully supported; no `inherit` needed.

---

## Tasks

> TDD applies to every task (RED → GREEN → refactor). Commit after each green task. Use Julie's code-intelligence tools (`get_symbols`, `deep_dive`, `fast_refs`) — not blind Read/Grep — to read the real code before editing.

### Task 1 — DaemonPaths socket/lock helpers (LEAD-OWNED)

**Files:**
- Modify: `crates/julie-core/src/paths.rs` (DaemonPaths impl, after the existing daemon_* helpers ~`:243`–end)
- Test: inline `#[cfg(test)]` in `paths.rs` (matches existing path-test convention there) **or** the existing paths test module

**What to build:** Three `DaemonPaths` methods: `embedding_host_socket() -> PathBuf` (unix → `julie_home.join("embedding-host.sock")`), `embedding_host_lock() -> PathBuf` (`julie_home.join("embedding-host.lock")`), and `embedding_host_pipe_name() -> String` (windows-oriented: `format!(r"\\.\pipe\julie-embedding-host-{}", <julie_home hash>)`, reusing the exact hashing already used by `daemon_shutdown_event()` so the name is stable per `$JULIE_HOME`).

**Approach:** Mirror the existing `daemon_*` helpers verbatim for style. For the pipe-name hash, `deep_dive` `daemon_shutdown_event` first and reuse its hash helper — do not invent a new hash.

**Acceptance criteria:**
- [ ] `embedding_host_socket`/`_lock` compose under `julie_home`; test asserts exact suffixes.
- [ ] `embedding_host_pipe_name` is deterministic for a fixed `$JULIE_HOME` and uses the same hash as `daemon_shutdown_event` (test asserts stability + `\\.\pipe\` prefix).
- [ ] `cargo nextest run -p julie-core <test_name>` green.

### Task 2 — Cross-platform IPC transport seam (LEAD-OWNED)

**Files:**
- Create: `crates/julie-pipeline/src/embeddings/host_transport.rs`
- Modify: `crates/julie-pipeline/src/embeddings/mod.rs` (add `mod host_transport;` + `pub use`)
- Test: `crates/julie-pipeline/src/tests/embeddings/host_roundtrip.rs` (transport-only round trip)

**What to build:** The minimal seam both client and server build on, isolating all OS-specific code here.
- **Blocking client:** `pub struct HostClientConn` with `pub fn connect(paths-derived address) -> io::Result<Self>` and `pub fn round_trip(&mut self, request_line: &str) -> io::Result<String>` (write line + `\n` + flush, read one line). `#[cfg(unix)]` wraps `std::os::unix::net::UnixStream`; `#[cfg(windows)]` opens the named pipe as a blocking file handle.
- **Async server:** `pub async fn bind_host_listener(address) -> io::Result<HostListener>` and `HostListener::accept() -> io::Result<HostServerConn>` where `HostServerConn` exposes async `read_line`/`write_line`. `#[cfg(unix)]` → `tokio::net::{UnixListener, UnixStream}`; `#[cfg(windows)]` → `tokio::net::windows::named_pipe`.

**Approach:** Unix is the must-prove path — implement and test it fully. Windows is `cfg`-gated, written to compile, and documented as **untested on this platform** (cannot be exercised on the macOS dev box; matches the design doc's Unix-proof stance). Keep the address type a thin enum/struct carrying either a socket `PathBuf` (unix) or a pipe name `String` (windows). No protocol knowledge here — this module moves bytes/lines only.

**Acceptance criteria:**
- [ ] Unix round-trip test: a throwaway tokio server echoes a line; `HostClientConn::connect` + `round_trip` gets it back. `cargo nextest run -p julie-pipeline <test_name>` green.
- [ ] Windows arms compile under `cfg(windows)` (reviewed by lead; documented untested).
- [ ] No new crate dependencies (tokio `full` + std only).

### Task 3 — `RpcEmbeddingProvider` (thin client)

**Files:**
- Create: `crates/julie-pipeline/src/embeddings/rpc_client.rs`
- Modify: `crates/julie-pipeline/src/embeddings/mod.rs` (export `RpcEmbeddingProvider`)
- Test: extend `crates/julie-pipeline/src/tests/embeddings/host_roundtrip.rs`

**What to build:** A struct implementing the Fixed-Contract `EmbeddingProvider` trait by forwarding over `host_transport::HostClientConn`. Holds: the address, `Mutex<Option<HostClientConn>>` (lazy connect + reconnect), a per-connection `request_id` counter, and cached `dimensions`/`DeviceInfo` populated from a `health` round-trip on first connect.

**Approach:**
- `embed_query`/`embed_batch`: build `RequestEnvelope` (schema/version constants), serialize, `round_trip`, deserialize `ResponseEnvelope`, run the matching existing validator, return `result.vector`/`result.vectors` or map `ProtocolError`→`anyhow::Err`. On `io::Error` (broken pipe), drop the connection and retry **once** (reconnect), else error.
- `dimensions()`/`device_info()`/`accelerated()`/`degraded_reason()`: serve from cached `HealthResult` (connect lazily if not yet cached). `shutdown()` drops the connection; `wait_for_exit()` returns `true`.
- Mirror `SidecarProcess::send_request_with_timeout` (`sidecar_provider.rs:365`) for the exact serialize/flush/read sequence — `deep_dive` it first.

**Acceptance criteria:**
- [ ] Against a fake host (a small in-test tokio server speaking the envelope protocol), `embed_query`/`embed_batch` return correct vectors; `dimensions()`/`device_info()` reflect the health response.
- [ ] A simulated broken pipe triggers exactly one reconnect, then succeeds.
- [ ] `RpcEmbeddingProvider` satisfies `Arc<dyn EmbeddingProvider>` (compile assertion).
- [ ] `cargo nextest run -p julie-pipeline <test_name>` green.

### Task 4 — Embedding-host server

**Files:**
- Create: `crates/julie-pipeline/src/embeddings/host_server.rs`
- Modify: `crates/julie-pipeline/src/embeddings/mod.rs` (export `run_embedding_host` + a config struct)
- Test: extend `host_roundtrip.rs` (host with an injected fake provider)

**What to build:** `pub async fn run_embedding_host(address, lock_path, cancellation: CancellationToken, provider: Arc<dyn EmbeddingProvider>)` (a sibling `run_embedding_host_default` resolves the provider via `create_embedding_provider` for the bin path). It: acquires the `fs2` singleton lock (yield/exit if held), binds via `host_transport::bind_host_listener`, runs an accept loop spawning a task per connection. Each connection task loops: read a line → parse `RequestEnvelope<serde_json::Value>` → match `method` → call the provider's matching method inside `tokio::task::spawn_blocking` (the provider is sync + `Mutex`-guarded) → write `ResponseEnvelope`. `shutdown` method drains + closes. Graceful shutdown on `cancellation.cancelled()` (mirror `HttpTransportServer`), then `provider.shutdown()` + `spawn_blocking(provider.wait_for_exit(3s))`.

**Approach:** `deep_dive` `HttpTransportServer::bind_with_listener` + the accept/`CancellationToken`/`with_graceful_shutdown` structure and copy the shape. Inject the provider so tests use a fake (no Python). Errors → `ResponseEnvelope.error = Some(ProtocolError{code,message})`, never a panic that kills the accept loop.

**Acceptance criteria:**
- [ ] With an injected fake provider, a `HostClientConn` round-trips `health`/`embed_query`/`embed_batch` correctly.
- [ ] Two concurrent client connections both succeed (serialized through the provider mutex).
- [ ] `cancellation.cancel()` stops the accept loop and the function returns; the lock is released.
- [ ] `cargo nextest run -p julie-pipeline <test_name>` green.

### Task 5 — `julie-embedding-host` binary

**Files:**
- Create: `src/bin/julie-embedding-host.rs`
- Modify: root `Cargo.toml` (`[[bin]]` entry near `julie-adapter`/`julie-daemon`; ensure it builds with `embeddings-sidecar`)

**What to build:** A thin entrypoint: init tracing (reuse the daemon's tracing/log-file setup helper — `deep_dive` how `julie-daemon` bin or `run_daemon` initializes tracing and reuse it), resolve `DaemonPaths` from `$JULIE_HOME`, compute the address (socket on unix / pipe on windows), create a `CancellationToken`, spawn a SIGTERM/Ctrl-C handler that cancels it (reuse `shutdown_signal()` from the daemon if reachable), then `run_embedding_host_default(...)`.

**Approach:** Keep this < 80 lines — all logic is in `host_server`. Mirror `src/bin/julie-daemon.rs` for the tokio `#[tokio::main]` + tracing + signal scaffolding.

**Acceptance criteria:**
- [ ] `cargo build --bin julie-embedding-host --features embeddings-sidecar` succeeds.
- [ ] Manual/CLI smoke (lead, recorded in ledger): launching the bin creates the socket and answers a `health` round-trip. (Automated coverage comes via Task 8 using `current_exe`.)

### Task 6 — `connect_or_spawn_host` launch glue

**Files:**
- Create: `src/embedding_host_launch.rs` (top crate) + declare the module
- Test: `src/tests/daemon/embedding_host_multi_session.rs` exercises it (see Task 8) — no separate test file

**What to build:** `pub fn connect_or_spawn_host(paths: &DaemonPaths) -> anyhow::Result<RpcEmbeddingProvider>`: if the socket/pipe is live → construct + return the client. Else spawn `julie-embedding-host` (resolve the sibling binary via `std::env::current_exe()` parent — mirror how the adapter auto-starts the daemon; `deep_dive` `src/adapter/launcher.rs`), pass through the relevant `JULIE_EMBEDDING_*` env vars, poll for the socket up to a bounded timeout, then connect.

**Approach:** This lives in the **top crate** because only it knows the host bin and can launch sibling processes (julie-pipeline must not depend on the bin). Reuse the adapter's current_exe + readiness-poll pattern rather than inventing one.

**Acceptance criteria:**
- [ ] When no host is running, the function spawns one, waits for readiness, and returns a working client (covered by Task 8).
- [ ] When a host is already running, it connects without spawning a second (the host's singleton lock also guarantees this).

### Task 7 — Opt-in coexistence wiring (additive)

**Files:**
- Modify: `src/daemon/app/helpers.rs` — `spawn_embedding_init` (`:403`)
- Test: a daemon-mode test (in `embedding_host_multi_session.rs` or `src/tests/daemon/embedding_service.rs`)

**What to build:** Inside `spawn_embedding_init`'s `spawn_blocking`, branch on `std::env::var("JULIE_EMBEDDING_USE_HOST")`: when truthy, build the provider via `connect_or_spawn_host(&paths)` (wrap as `Arc<dyn EmbeddingProvider>`) and a synthesized `EmbeddingRuntimeStatus`, then `publish_ready` exactly as today; on error `publish_unavailable`. When unset/false, the **existing `create_embedding_provider()` path is unchanged**. `watcher_pool.update_all_provider(...)` continues to receive whatever provider was published — no change.

**Approach:** Purely additive — one `if` at the provider-construction point. Do not touch the session-side guard in `embedding_init.rs:66` (it already prevents stdio double-spawn and works identically for an RPC-client provider). Leave the stdio-mode lazy-init path for a follow-up note (3c) unless trivially gated the same way — do **not** expand scope here.

**Acceptance criteria:**
- [ ] With `JULIE_EMBEDDING_USE_HOST=1`, a daemon-mode test shows the published provider is an `RpcEmbeddingProvider` and N `JulieServerHandler`s share the one `Arc<EmbeddingService>`.
- [ ] With the env unset, behavior is byte-for-byte the existing path (a test asserts `create_embedding_provider` is still the source — or that no host socket is created).
- [ ] `cargo nextest run --lib <test_name>` green.

### Task 8 — Acceptance: one sidecar serves 3 concurrent sessions (HARD GATE)

**Files:**
- Create: `src/tests/daemon/embedding_host_multi_session.rs` + register in the daemon tests mod
- Test seam (if needed): add a `#[cfg(any(test, feature="test-support"))]` spawn-counter (e.g. `AtomicU32`) incremented in `SidecarEmbeddingProvider::spawn_process` (`sidecar_provider.rs:62`) so the test can assert "exactly one sidecar"

**What to build:** An integration test that launches **one** real `julie-embedding-host` (via `current_exe()` sibling, `--features embeddings-sidecar`) configured with a **fake CPU sidecar** through the existing `JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM` hook (an inline echo/stub script that speaks the envelope protocol — copy the technique from the circuit-breaker test in `sidecar_provider.rs` tests), then opens **3 concurrent** `RpcEmbeddingProvider` clients and fires concurrent `embed_query` + `embed_batch` from all three.

**Approach:** `deep_dive` the existing `JULIE_EMBEDDING_SIDECAR_RAW_PROGRAM` test usage first and reuse its stub-sidecar pattern so this needs **no GPU and no real model**. Prove the single-sidecar invariant either via the spawn-counter seam (one increment) or by asserting the host emitted exactly one "Embedding provider initialized" log. Keep total wall-clock low (stub sidecar, short timeouts).

**Acceptance criteria (HARD GATE):**
- [ ] All 3 concurrent clients return vectors of the correct `dims` for both `embed_query` and `embed_batch`. **(hard)**
- [ ] Exactly **one** sidecar child is spawned across all 3 clients. **(hard)**
- [ ] Host shuts down cleanly (cancellation → socket + lock released). **(hard)**
- [ ] Per-call latency recorded. **(report-only)**
- [ ] `cargo nextest run -p julie-pipeline --features embeddings-sidecar <test_name> 2>&1 | tail -20` green (lead runs; this is the 3b proof).

---

## Out of scope for 3b (explicit — do NOT build here)

- Removing the daemon's existing shared sidecar / flipping the default onto the host (that is the 3c cutover).
- The per-workspace **leader-election lock** and WAL/mmap reader model (3c).
- Conditioning `watcher_pool.update_all_provider` on write-leadership (3c — sessions aren't leaders yet).
- Deleting `adapter/**`, HTTP transport, or any daemon subsystem (3d).
- Moving `daemon.db` → `registry.db` or the dashboard re-home (3c/3d).
- Stdio-mode default rewiring onto the host beyond the env-gated opt-in (3c).

## Notes carried to 3c (record, don't act)

- `EmbeddingService` itself need not move out of `src/daemon/` for 3b to work — its only non-std deps are `tokio::sync::watch` + the core trait. Physically relocating it (design §2 "MOVE") can ride 3c's in-process server work; 3b proves the host mechanism without the move. (If the team prefers, relocating the file is a clean mechanical add — but it is not required by any 3b acceptance criterion and must not block the gate.)
- The sync-vs-async sidecar I/O (`std::sync::mpsc` stdout reader) stays as-is; the host wraps it via `spawn_blocking`.
