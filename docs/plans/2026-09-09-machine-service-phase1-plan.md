# Machine Service Phase 1: Service Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** One `julie-server service` process per machine serves the existing request engine over stateless MCP HTTP, a stdio shim, and plain JSON, with `service.json` discovery, a bearer token, idle exit, and a `/status` page, and the old in-process stdio serve path is deleted.

**Architecture:** The service is a new `src/service/` module in the `julie` crate. It builds the existing `RequestEngine` once, mounts three bindings on one axum router, and writes `~/.julie/service.json` after the port is bound. Clients (the stdio shim, the CLI, the browser) read that file, connect, and start the service if it is absent. No indexing changes: the runtime under the engine is the one that exists today. Phase 2 replaces it.

**Tech Stack:** Rust, `rmcp` 3.0.1 (`transport-streamable-http-server`, `NeverSessionManager`), `axum` 0.8, `tokio`, `reqwest` (already pulled by `rmcp`'s `transport-streamable-http-client-reqwest` feature), `serde_json`, `uuid`, `clap`, `cargo nextest`, `cargo xtask test`.

**Architecture Quality:** Design `docs/plans/2026-09-09-machine-service-design.md`, sections 5, 12, 16. The service module owns process lifecycle and bindings only. Tools, storage, and workspace runtime stay behind `RequestEngine::execute`. Risk: high for the transport cutover (the stdio entry that every plugin install uses changes shape). Control: the shim is byte-forwarding, the old path is deleted in the same task the shim lands, and the phase gate tests three real hosts before phase 2.

## Global Constraints

- Design section 4 is normative. A worker who needs any of these words in new code stops and reports: lock, lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair, continuation, handoff.
- The process model in `src/service/` (start, `service.json`, token, idle exit, shim spawn and forward) is at most 600 lines excluding tests. Task 5 adds the test that counts them.
- Durable machine files: `~/.julie/registry.sqlite` (exists today as the registry database) and the runtime file `~/.julie/service.json`. No other new file under `~/.julie/`.
- `service.json` is written by write-to-temp then rename, mode 0600 on Unix, and only after `TcpListener::bind` succeeds. Its fields are exactly: `port` (u16), `token` (string, 64 hex chars), `pid` (u32), `version` (string, `env!("CARGO_PKG_VERSION")`), `started_at` (RFC 3339 string).
- The service binds `127.0.0.1:0` only. Never `0.0.0.0`.
- Every request to `/mcp`, `/api/*`, `/status`, `/shutdown`, and `/` requires the token, as `Authorization: Bearer <token>` or `?token=<token>`. A missing or wrong token returns HTTP 401 with body `{"error":"unauthorized"}`.
- Idle exit: the service exits with code 0 after `JULIE_SERVICE_IDLE_SECS` seconds (default 1800) with no request in flight or completed. Zero disables it.
- Client start: a client that cannot connect to the port in `service.json` deletes the file, spawns `current_exe() service` detached, polls for a new file for up to 10 seconds, and retries once. It never spawns twice in one invocation.
- Version mismatch: a client whose `CARGO_PKG_VERSION` differs from the service's `version` field fails with exit code 3 and the exact message `julie: service version <svc> does not match client version <cli>; run: julie-server service restart`.
- The binary stays `julie-server` in this phase. Renaming to `julie` is phase 6.
- No new crates in `Cargo.toml`. `reqwest` is already a transitive dependency through `rmcp`; make it a direct dependency with the same version `rmcp` uses, features `json` only.
- Tests bind ephemeral ports in process and use a temporary `JULIE_HOME`. Only Task 5's bucket spawns subprocesses.
- Every task ends with `cargo build` green and its worker scope green.

## Verification Strategy

**Project source of truth:** `AGENTS.md` sections "Commands" and "Test tiers"; `xtask/test_tiers.toml`.

**Worker red/green scope:** `cargo nextest run --lib tests::service::<test_module>` for the module the task adds. Exact commands are in each task.

**Worker ceiling:** `cargo nextest run --lib tests::service::` plus `cargo build`. Workers do not run `cargo xtask test dev`.

**Worker gate invariant:** Each task's acceptance list states the behavior its tests prove.

**Lead affected-change scope:** `cargo xtask test changed` after each batch. Expect fallback to `dev` for Tasks 3 and 4 because they touch `src/main.rs` and `src/cli.rs`.

**Branch gate:** `cargo xtask test dev` green, then `cargo xtask test bucket service-process` green, then Task 6's three-host gate recorded.

**Security scope:** `cargo deny check` (config `deny.toml` exists in `julie-extractors`; Julie has none declared, so run `cargo audit` if installed, else record `none declared`). Secrets scan: `git diff main...HEAD | grep -iE 'token|secret|password' ` reviewed by the lead; the only expected hit is the `token` field name.

**Replay/metric evidence:** The 600-line budget is a hard gate (Task 5 test). Fast bucket wall time is report-only in this phase; the design's 10 s target is gated from phase 3.

**Escalation triggers:** Any change to `src/handler.rs` beyond deleting `server_in_process` callers escalates to the lead. Any need for a second file under `~/.julie/` stops the task.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-09-09-machine-service-phase1-ledger.md` (create from `docs/plans/verification-ledger-template.md`). If the same HEAD already has a passing entry for the required scope, reuse it.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Service process and JSON API | None - serial | Create `src/service/mod.rs`, `src/service/discovery.rs`, `src/service/http.rs`, `src/service/status.rs`, `src/tests/service/mod.rs`, `src/tests/service/http_api.rs`; modify `src/lib.rs`, `src/cli.rs`, `src/main.rs`, `src/tests/mod.rs`, `Cargo.toml`, `crates/julie-core/src/paths.rs` | Yes | Every later task mounts on this router and reads this discovery file. |
| Task 2: MCP over HTTP | Batch A | Create `src/service/mcp.rs`, `src/tests/service/mcp_http.rs`; modify `src/service/http.rs` (one `nest_service` line) | No | None - safe parallel batch. |
| Task 3: Client connector and stdio shim | Batch A | Create `src/service/client.rs`, `src/service/shim.rs`, `src/tests/service/client.rs`, `src/tests/service/shim.rs`; modify `src/cli.rs`, `src/main.rs`; delete `src/server_in_process.rs` and its `pub mod` line in `src/lib.rs` | No | None - safe parallel batch. Task 3 owns `src/main.rs` and `src/cli.rs` in this batch; Task 2 does not touch them. |
| Task 4: Service control and dashboard mount | None - serial | Create `src/tests/service/control.rs`; modify `src/service/http.rs`, `src/service/client.rs`, `src/cli.rs`, `src/main.rs`, `src/dashboard/standalone.rs`, `src/request_engine/dispatch.rs`, `src/tests/dashboard/` files that reference `spawn_background_server` | Yes | Depends on Task 3's client connector and Task 1's router. |
| Task 5: Multi-process bucket and line budget | None - serial | Create `src/tests/service/process.rs`, `src/tests/service/budget.rs`; modify `xtask/test_tiers.toml`, `src/tests/service/mod.rs` | Yes | Exercises the finished binary from Tasks 1 to 4. |
| Task 6: Three-host gate and docs | None - serial | Create `docs/findings/2026-09-DD-machine-service-phase1-host-gate.md`; modify `docs/WORKSPACE_ARCHITECTURE.md`, `AGENTS.md`, `README.md` | Yes | Lead task; needs the built binary and real clients. |

Commit mode: `serial-worker-commit` for Tasks 1, 4, 5. `parallel-lead-commit` for Batch A (Tasks 2 and 3). Task 6 is lead work.

---

## Task 1: Service process and JSON API

**Files:**
- Create: `src/service/mod.rs`, `src/service/discovery.rs`, `src/service/http.rs`, `src/service/status.rs`
- Create: `src/tests/service/mod.rs`, `src/tests/service/http_api.rs`
- Modify: `src/lib.rs` (add `pub mod service;`), `src/tests/mod.rs` (add `mod service;`), `src/cli.rs` (add `Command::Service`), `src/main.rs` (add match arm), `Cargo.toml` (direct `reqwest` dependency), `crates/julie-core/src/paths.rs` (add `service_json`)

**Interfaces:**
```rust
// src/service/mod.rs
pub struct ServiceConfig { pub idle: Option<std::time::Duration>, pub registry_paths: julie_core::paths::RegistryPaths }
pub struct ServiceApp { /* engine, router, discovery */ }
impl ServiceApp {
    pub fn new(config: ServiceConfig) -> anyhow::Result<Self>;
    /// Binds nothing itself: the caller binds so tests can use 127.0.0.1:0.
    pub async fn serve(self, listener: tokio::net::TcpListener) -> anyhow::Result<()>;
}
pub async fn run_service(config: ServiceConfig) -> anyhow::Result<()>; // binds 127.0.0.1:0, calls serve

// src/service/discovery.rs
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct ServiceRecord { pub port: u16, pub token: String, pub pid: u32, pub version: String, pub started_at: String }
pub fn write_record(paths: &RegistryPaths, record: &ServiceRecord) -> std::io::Result<()>;
pub fn read_record(paths: &RegistryPaths) -> std::io::Result<Option<ServiceRecord>>;
pub fn remove_record(paths: &RegistryPaths) -> std::io::Result<()>;
pub fn new_token() -> String;

// crates/julie-core/src/paths.rs
impl RegistryPaths { pub fn service_json(&self) -> PathBuf { self.julie_home.join("service.json") } }
```

**Contract inputs:** `RequestEngine::new(BindingResolver::new(None, false, registry_paths.clone()), Arc::new(RuntimeFactory::new(registry_paths)))` exactly as `src/cli_tools/mod.rs:157-164` builds it, but with `process_workspace = None` and `standalone = false` because the service has no home workspace. `RequestEngine::execute(ToolRequest, RequestContext) -> Result<ToolReply, RequestFailure>` at `src/request_engine/dispatch.rs:47`. `ToolRequest { name, arguments: Map<String, Value>, workspace: Option<PathBuf>, semantics }` at `src/request_engine/types.rs:143`. `RequestContext::new(RequestOrigin, Option<Duration>, CancellationToken)` at `types.rs:27`. `RequestOrigin` variants at `types.rs:69-72`; use the variant for MCP or add none.

**File ownership:** as listed. **Serialization required:** Yes. **Dependency reason:** every later task mounts on this router.

**Step 1: Write the failing tests.**

```rust
// src/tests/service/mod.rs
mod http_api;

// src/tests/service/http_api.rs
use crate::service::{discovery, ServiceApp, ServiceConfig};
use julie_core::paths::RegistryPaths;
use std::time::Duration;

pub(crate) struct Running {
    pub paths: RegistryPaths,
    pub base: String,
    pub token: String,
    _home: tempfile::TempDir,
    task: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Running {
    pub(crate) async fn start(idle: Option<Duration>) -> Running {
        let home = tempfile::tempdir().unwrap();
        let paths = RegistryPaths::with_home(home.path().to_path_buf());
        let app = ServiceApp::new(ServiceConfig { idle, registry_paths: paths.clone() }).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(app.serve(listener));
        let record = wait_for_record(&paths).await;
        assert_eq!(record.port, port);
        Running { paths, base: format!("http://127.0.0.1:{port}"), token: record.token, _home: home, task }
    }
    pub(crate) fn client(&self) -> reqwest::Client { reqwest::Client::new() }
    pub(crate) async fn finished(self) -> anyhow::Result<()> { self.task.await.unwrap() }
}

pub(crate) async fn wait_for_record(paths: &RegistryPaths) -> discovery::ServiceRecord {
    for _ in 0..100 {
        if let Ok(Some(r)) = discovery::read_record(paths) { return r; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("service.json not written");
}

#[tokio::test]
async fn service_json_names_the_bound_port_and_a_64_hex_token() {
    let running = Running::start(None).await;
    assert_eq!(running.token.len(), 64);
    assert!(running.token.chars().all(|c| c.is_ascii_hexdigit()));
    let record = discovery::read_record(&running.paths).unwrap().unwrap();
    assert_eq!(record.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(record.pid, std::process::id());
}

#[cfg(unix)]
#[tokio::test]
async fn service_json_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let running = Running::start(None).await;
    let mode = std::fs::metadata(running.paths.service_json()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn status_without_token_is_401() {
    let running = Running::start(None).await;
    let res = running.client().get(format!("{}/status", running.base)).send().await.unwrap();
    assert_eq!(res.status(), 401);
    assert_eq!(res.text().await.unwrap(), r#"{"error":"unauthorized"}"#);
}

#[tokio::test]
async fn status_with_header_or_query_token_reports_version_and_uptime() {
    let running = Running::start(None).await;
    let by_header = running.client().get(format!("{}/status", running.base))
        .bearer_auth(&running.token).send().await.unwrap();
    assert_eq!(by_header.status(), 200);
    let body: serde_json::Value = by_header.json().await.unwrap();
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert!(body["uptime_seconds"].is_number());
    assert!(body["requests"].is_array());
    let by_query = running.client()
        .get(format!("{}/status?token={}", running.base, running.token)).send().await.unwrap();
    assert_eq!(by_query.status(), 200);
}

#[tokio::test]
async fn api_workspace_list_returns_a_tool_reply_envelope() {
    let running = Running::start(None).await;
    let res = running.client().post(format!("{}/api/manage_workspace", running.base))
        .bearer_auth(&running.token)
        .json(&serde_json::json!({"operation": "list"}))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["tool"], "manage_workspace");
    assert_eq!(body["schema_version"], 1);
    assert!(body["result"].is_object());
}

#[tokio::test]
async fn api_unknown_tool_returns_request_failure_as_400() {
    let running = Running::start(None).await;
    let res = running.client().post(format!("{}/api/no_such_tool", running.base))
        .bearer_auth(&running.token).json(&serde_json::json!({})).send().await.unwrap();
    assert_eq!(res.status(), 400);
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["code"].is_string());
    assert!(body["message"].is_string());
}

#[tokio::test]
async fn status_lists_the_last_requests_newest_first() {
    let running = Running::start(None).await;
    for _ in 0..3 {
        running.client().post(format!("{}/api/manage_workspace", running.base))
            .bearer_auth(&running.token).json(&serde_json::json!({"operation": "list"}))
            .send().await.unwrap();
    }
    let body: serde_json::Value = running.client().get(format!("{}/status", running.base))
        .bearer_auth(&running.token).send().await.unwrap().json().await.unwrap();
    let requests = body["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0]["tool"], "manage_workspace");
    assert!(requests[0]["latency_ms"].is_number());
    assert_eq!(requests[0]["outcome"], "ok");
}

#[tokio::test]
async fn idle_exit_removes_service_json() {
    let running = Running::start(Some(Duration::from_millis(300))).await;
    let paths = running.paths.clone();
    tokio::time::timeout(Duration::from_secs(5), running.finished()).await
        .expect("service did not exit when idle").unwrap();
    assert!(discovery::read_record(&paths).unwrap().is_none());
}
```

**Step 2: Run the tests to verify they fail.**

```bash
cargo nextest run --lib tests::service::
```
Expected: compile error, `crate::service` does not exist.

**Step 3: Implement.**

`crates/julie-core/src/paths.rs`, inside `impl RegistryPaths`:
```rust
    /// Runtime discovery record for the machine service. Not a database.
    pub fn service_json(&self) -> PathBuf {
        self.julie_home.join("service.json")
    }
```

`Cargo.toml`, `[dependencies]`: add `reqwest = { version = "<same as rmcp's lock entry>", default-features = false, features = ["json", "rustls-tls"] }`. Read the version from `Cargo.lock` (`grep -A1 'name = "reqwest"' Cargo.lock`) and use it verbatim so no second copy is compiled. Verify with `cargo tree -i reqwest` that exactly one version appears.

`src/service/discovery.rs`:
```rust
use julie_core::paths::RegistryPaths;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ServiceRecord {
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub version: String,
    pub started_at: String,
}

pub fn new_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn write_record(paths: &RegistryPaths, record: &ServiceRecord) -> std::io::Result<()> {
    let target = paths.service_json();
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = target.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    file.write_all(serde_json::to_string_pretty(record)?.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&temp, &target)
}

pub fn read_record(paths: &RegistryPaths) -> std::io::Result<Option<ServiceRecord>> {
    read_record_at(&paths.service_json())
}

fn read_record_at(path: &Path) -> std::io::Result<Option<ServiceRecord>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn remove_record(paths: &RegistryPaths) -> std::io::Result<()> {
    match std::fs::remove_file(paths.service_json()) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
```

`src/service/status.rs`:
```rust
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const KEEP_REQUESTS: usize = 50;
const KEEP_ERRORS: usize = 20;

#[derive(Serialize, Clone)]
pub struct RequestRecord {
    pub tool: String,
    pub workspace_id: Option<String>,
    pub latency_ms: u128,
    pub outcome: &'static str,
    pub at: String,
}

#[derive(Serialize, Clone)]
pub struct ErrorRecord {
    pub tool: String,
    pub code: String,
    pub message: String,
    pub at: String,
}

pub struct StatusLog {
    started: Instant,
    inner: Mutex<Inner>,
}

struct Inner {
    requests: VecDeque<RequestRecord>,
    errors: VecDeque<ErrorRecord>,
    last_activity: Instant,
    in_flight: usize,
}

#[derive(Serialize)]
pub struct StatusDocument {
    pub version: &'static str,
    pub pid: u32,
    pub uptime_seconds: u64,
    pub in_flight: usize,
    pub rss_bytes: Option<u64>,
    pub requests: Vec<RequestRecord>,
    pub errors: Vec<ErrorRecord>,
}

impl StatusLog {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            inner: Mutex::new(Inner {
                requests: VecDeque::new(),
                errors: VecDeque::new(),
                last_activity: now,
                in_flight: 0,
            }),
        }
    }

    pub fn begin(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.in_flight += 1;
        inner.last_activity = Instant::now();
    }

    pub fn end(&self, record: RequestRecord, error: Option<ErrorRecord>) {
        let mut inner = self.inner.lock().unwrap();
        inner.in_flight = inner.in_flight.saturating_sub(1);
        inner.last_activity = Instant::now();
        inner.requests.push_front(record);
        inner.requests.truncate(KEEP_REQUESTS);
        if let Some(error) = error {
            inner.errors.push_front(error);
            inner.errors.truncate(KEEP_ERRORS);
        }
    }

    pub fn idle_for(&self) -> Option<Duration> {
        let inner = self.inner.lock().unwrap();
        (inner.in_flight == 0).then(|| inner.last_activity.elapsed())
    }

    pub fn document(&self) -> StatusDocument {
        let inner = self.inner.lock().unwrap();
        StatusDocument {
            version: env!("CARGO_PKG_VERSION"),
            pid: std::process::id(),
            uptime_seconds: self.started.elapsed().as_secs(),
            in_flight: inner.in_flight,
            rss_bytes: rss_bytes(),
            requests: inner.requests.iter().cloned().collect(),
            errors: inner.errors.iter().cloned().collect(),
        }
    }
}

#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096)
}

#[cfg(not(target_os = "linux"))]
fn rss_bytes() -> Option<u64> {
    None
}

pub fn now_rfc3339() -> String {
    // chrono is not a dependency; format seconds since epoch as a plain integer string
    // if the crate has no RFC 3339 helper. Check `Cargo.toml` for `chrono` or `time` first
    // and use it when present.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}
```
If `Cargo.toml` already has `chrono` or `time` (check with `grep -nE '^(chrono|time)' Cargo.toml`), use it for `now_rfc3339` and produce a real RFC 3339 string; the `started_at` field in `service.json` must then be RFC 3339. If neither exists, keep the integer seconds string and change the Global Constraint wording for `started_at` to "seconds since epoch as a string" in this plan and the design.

`src/service/http.rs`:
```rust
use crate::request_engine::types::{RequestContext, RequestFailure, RequestOrigin, ToolRequest};
use crate::request_engine::RequestEngine;
use crate::service::status::{now_rfc3339, ErrorRecord, RequestRecord, StatusLog};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<RequestEngine>,
    pub status: Arc<StatusLog>,
    pub token: Arc<str>,
    pub request_timeout: Duration,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/api/{tool}", post(api_call))
        .layer(axum::middleware::from_fn_with_state(state.clone(), require_token))
        .with_state(state)
}

async fn require_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or_else(|| query.get("token").cloned());
    if presented.as_deref() == Some(&*state.token) {
        next.run(request).await
    } else {
        (StatusCode::UNAUTHORIZED, r#"{"error":"unauthorized"}"#).into_response()
    }
}

async fn status(State(state): State<AppState>) -> Json<crate::service::status::StatusDocument> {
    Json(state.status.document())
}

async fn api_call(
    State(state): State<AppState>,
    Path(tool): Path<String>,
    Json(arguments): Json<Map<String, Value>>,
) -> Response {
    let started = Instant::now();
    state.status.begin();
    let request = ToolRequest::new(tool.clone(), arguments);
    let context = RequestContext::new(
        RequestOrigin::Mcp,
        Some(state.request_timeout),
        tokio_util::sync::CancellationToken::new(),
    );
    let result = state.engine.execute(request, context).await;
    let latency_ms = started.elapsed().as_millis();
    match result {
        Ok(reply) => {
            state.status.end(
                RequestRecord { tool, workspace_id: reply.workspace_id.clone(), latency_ms, outcome: "ok", at: now_rfc3339() },
                None,
            );
            Json(reply).into_response()
        }
        Err(failure) => {
            state.status.end(
                RequestRecord { tool: tool.clone(), workspace_id: None, latency_ms, outcome: "error", at: now_rfc3339() },
                Some(ErrorRecord { tool, code: failure.code.clone(), message: failure.message.clone(), at: now_rfc3339() }),
            );
            failure_response(failure)
        }
    }
}

pub fn failure_response(failure: RequestFailure) -> Response {
    let code = if failure.code == "INTERNAL" { StatusCode::INTERNAL_SERVER_ERROR } else { StatusCode::BAD_REQUEST };
    (code, Json(failure)).into_response()
}
```
Check the real `RequestOrigin` variant names at `src/request_engine/types.rs:69-72` and the real internal failure code constant in `types.rs` before writing `"INTERNAL"`; use the constants the crate exports. If `RequestOrigin` has no MCP-like variant, use the one the in-process server used (find with `grep -n RequestOrigin:: src/handler/mcp_adapter.rs`).

`src/service/mod.rs`:
```rust
pub mod discovery;
pub mod http;
pub mod status;

use crate::request_engine::{BindingResolver, RequestEngine, RuntimeFactory};
use anyhow::Context;
use julie_core::paths::RegistryPaths;
use std::sync::Arc;
use std::time::Duration;

pub struct ServiceConfig {
    pub idle: Option<Duration>,
    pub registry_paths: RegistryPaths,
}

impl ServiceConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let registry_paths = RegistryPaths::try_new().context("resolve Julie home")?;
        let idle = match std::env::var("JULIE_SERVICE_IDLE_SECS").ok().and_then(|v| v.parse::<u64>().ok()) {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => Some(Duration::from_secs(1800)),
        };
        Ok(Self { idle, registry_paths })
    }
}

pub struct ServiceApp {
    config: ServiceConfig,
    state: http::AppState,
}

impl ServiceApp {
    pub fn new(config: ServiceConfig) -> anyhow::Result<Self> {
        let paths = config.registry_paths.clone();
        let resolver = BindingResolver::new(None, false, paths.clone());
        let runtimes = Arc::new(RuntimeFactory::new(paths));
        let engine = Arc::new(RequestEngine::new(resolver, runtimes));
        let state = http::AppState {
            engine,
            status: Arc::new(status::StatusLog::new()),
            token: Arc::from(discovery::new_token()),
            request_timeout: Duration::from_secs(120),
        };
        Ok(Self { config, state })
    }

    pub fn state(&self) -> &http::AppState {
        &self.state
    }

    pub async fn serve(self, listener: tokio::net::TcpListener) -> anyhow::Result<()> {
        let port = listener.local_addr()?.port();
        let record = discovery::ServiceRecord {
            port,
            token: self.state.token.to_string(),
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: status::now_rfc3339(),
        };
        discovery::write_record(&self.config.registry_paths, &record)?;

        let shutdown = tokio_util::sync::CancellationToken::new();
        let idle_watch = {
            let status = Arc::clone(&self.state.status);
            let shutdown = shutdown.clone();
            let idle = self.config.idle;
            async move {
                let Some(idle) = idle else { std::future::pending::<()>().await; return; };
                loop {
                    tokio::time::sleep(idle.min(Duration::from_millis(250))).await;
                    if status.idle_for().is_some_and(|d| d >= idle) {
                        shutdown.cancel();
                        return;
                    }
                }
            }
        };
        let router = http::router(self.state.clone());
        let server = axum::serve(listener, router).with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move { shutdown.cancelled().await }
        });
        let result = tokio::select! {
            r = server => r.map_err(anyhow::Error::from),
            _ = idle_watch => Ok(()),
        };
        discovery::remove_record(&self.config.registry_paths)?;
        result
    }
}

pub async fn run_service(config: ServiceConfig) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.context("bind 127.0.0.1:0")?;
    ServiceApp::new(config)?.serve(listener).await
}
```
Note on the `select!`: when `idle_watch` fires it cancels `shutdown`, which drains the server; the branch order means the idle arm may win first. Either arm ending removes the record. If `axum::serve(...).with_graceful_shutdown` in axum 0.8 has a different name, check `~/.cargo/registry/src/*/axum-0.8*/src/serve/mod.rs` and use what exists.

`src/cli.rs`: add to `Command`:
```rust
    /// Run the machine service in the foreground (started automatically by clients).
    Service(ServiceArgs),
```
and
```rust
#[derive(clap::Args, Debug, Clone)]
pub struct ServiceArgs {
    #[command(subcommand)]
    pub action: Option<ServiceAction>,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum ServiceAction {
    /// Print the running service's status document.
    Status,
    /// Ask the running service to exit.
    Stop,
    /// Stop the running service, then start a new one.
    Restart,
}
```
Also add `Command::Service(_)` to `cli_command_needs_workspace_startup_hint` returning `false` (read the function in `src/cli.rs` and follow its match shape).

`src/main.rs`: add an arm before `None`:
```rust
        Some(Command::Service(args)) => match args.action {
            None => julie::service::run_service(julie::service::ServiceConfig::from_env()?).await?,
            Some(_) => anyhow::bail!("service status/stop/restart arrive in Task 4"),
        },
```
Task 4 replaces the `Some(_)` arm; leaving a bail here keeps the build green without a stub that pretends to work.

`src/lib.rs`: `pub mod service;`. `src/tests/mod.rs`: `mod service;`.

**Step 4: Run the tests to verify they pass.**

```bash
cargo nextest run --lib tests::service::
cargo build
```
Expected: 8 tests pass, build green.

**Step 5: Commit** (`serial-worker-commit`): `feat(service): add machine service skeleton with discovery, token, status, and JSON API`.

**Acceptance criteria:**
- [ ] `service.json` is written only after bind, names the bound port, carries a 64-hex token, pid, version, and start time, and is mode 0600 on Unix.
- [ ] Every route returns 401 with the exact body on a missing or wrong token; header and query token both work.
- [ ] `POST /api/<tool>` runs `RequestEngine::execute` and returns the `ToolReply` envelope on success and the `RequestFailure` as JSON with 400 or 500 on failure.
- [ ] `/status` reports version, pid, uptime, in-flight count, RSS on Linux, the last fifty requests newest first, and the last twenty errors.
- [ ] The service exits and removes `service.json` after the idle period with nothing in flight.
- [ ] No new crate other than a direct `reqwest` at the version already in `Cargo.lock`.

---

## Task 2: MCP over HTTP

**Files:**
- Create: `src/service/mcp.rs`, `src/tests/service/mcp_http.rs`
- Modify: `src/service/http.rs` (mount), `src/tests/service/mod.rs` (add `mod mcp_http;`)

**Interfaces:**
```rust
// src/service/mcp.rs
pub fn mcp_service(engine: Arc<RequestEngine>) -> rmcp::transport::streamable_http_server::tower::StreamableHttpService<McpAdapter, rmcp::transport::streamable_http_server::session::never::NeverSessionManager>;
```

**Contract inputs:** `McpAdapter::new(Arc<RequestEngine>, Option<PathBuf>)` at `src/handler/mcp_adapter.rs:197`; `McpAdapter` implements `rmcp::ServerHandler` at `mcp_adapter.rs:266`. `StreamableHttpService::new(service_factory: impl Fn() -> Result<S, io::Error> + Send + Sync + 'static, session_manager: Arc<M>, config: StreamableHttpServerConfig)` in `rmcp-3.0.1/src/transport/streamable_http_server/tower.rs:963`. `NeverSessionManager` in `tower.rs`'s sibling `session/never.rs:19`. Config builders `with_json_response(bool)`, `with_legacy_session_mode(bool)`, `with_allowed_hosts`, `with_allowed_origins` at `tower.rs:155-205`. `JULIE_PROTOCOL_VERSIONS` at `mcp_adapter.rs:28` lists `V_2026_07_28` then `V_2025_11_25`.

**File ownership:** as listed. **Serialization required:** No. **Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing tests.**

```rust
// src/tests/service/mcp_http.rs
use super::http_api::Running;
use serde_json::{json, Value};

async fn rpc(running: &Running, body: Value) -> (u16, Value) {
    let res = running.client().post(format!("{}/mcp", running.base))
        .bearer_auth(&running.token)
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Method", body["method"].as_str().unwrap_or(""))
        .json(&body).send().await.unwrap();
    let status = res.status().as_u16();
    let text = res.text().await.unwrap();
    let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw": text}));
    (status, value)
}

fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "julie-test", "version": "0"}
    })
}

#[tokio::test]
async fn mcp_without_token_is_401() {
    let running = Running::start(None).await;
    let res = running.client().post(format!("{}/mcp", running.base))
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{}}))
        .send().await.unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn server_discover_lists_2026_07_28_without_a_session_header() {
    let running = Running::start(None).await;
    let res = running.client().post(format!("{}/mcp", running.base))
        .bearer_auth(&running.token)
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Method", "server/discover")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta": meta()}}))
        .send().await.unwrap();
    assert!(res.headers().get("Mcp-Session-Id").is_none());
    let body: Value = res.json().await.unwrap();
    let versions = body["result"]["protocolVersions"].as_array().expect("protocolVersions");
    assert!(versions.iter().any(|v| v == "2026-07-28"));
}

#[tokio::test]
async fn tools_list_over_http_matches_the_adapter_catalog() {
    let running = Running::start(None).await;
    let (_, body) = rpc(&running, json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta": meta()}})).await;
    let names: Vec<&str> = body["result"]["tools"].as_array().unwrap()
        .iter().map(|t| t["name"].as_str().unwrap()).collect();
    let expected: Vec<&str> = crate::request_engine::catalog::ToolCatalog::list()
        .iter().map(|t| t.name).collect();
    assert_eq!(names, expected);
    let (_, again) = rpc(&running, json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{"_meta": meta()}})).await;
    assert_eq!(body["result"]["tools"], again["result"]["tools"], "tool order must be deterministic");
}

#[tokio::test]
async fn tools_call_manage_workspace_list_returns_a_complete_result() {
    let running = Running::start(None).await;
    let (status, body) = rpc(&running, json!({
        "jsonrpc":"2.0","id":4,"method":"tools/call",
        "params":{"name":"manage_workspace","arguments":{"operation":"list"},"_meta": meta()}
    })).await;
    assert_eq!(status, 200);
    assert_eq!(body["result"]["resultType"], "complete");
    assert!(body["result"]["content"].is_array());
}
```
Check the exact field name the `server/discover` result uses for versions in `rmcp-3.0.1/src/model.rs` (search `struct ServerDiscoverResult` or similar) and the exact `ToolInfo` field for the name at `src/request_engine/catalog.rs:9` before running; fix the test to the real names, not the other way around.

**Step 2: Run the tests to verify they fail.**

```bash
cargo nextest run --lib tests::service::mcp_http
```
Expected: 404 on `/mcp` or compile error on `mcp_service`.

**Step 3: Implement.**

`src/service/mcp.rs`:
```rust
use crate::handler::mcp_adapter::McpAdapter;
use crate::request_engine::RequestEngine;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::tower::{StreamableHttpServerConfig, StreamableHttpService};
use std::sync::Arc;

pub fn mcp_service(engine: Arc<RequestEngine>) -> StreamableHttpService<McpAdapter, NeverSessionManager> {
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_allowed_hosts(vec!["127.0.0.1".into(), "localhost".into()])
        .with_allowed_origins(vec![]);
    StreamableHttpService::new(
        move || Ok(McpAdapter::new(Arc::clone(&engine), None)),
        Arc::new(NeverSessionManager::default()),
        config,
    )
}
```
Verify `NeverSessionManager` implements `Default` (see `session/never.rs:19`); if not, use its constructor. Verify the allowed-hosts and allowed-origins builder argument types at `tower.rs:155-178`; if allowed-origins with an empty list rejects browser requests to `/mcp`, that is correct behavior (the browser never calls `/mcp`).

`src/service/http.rs`, in `router`, before `.layer(...)`:
```rust
        .nest_service("/mcp", crate::service::mcp::mcp_service(Arc::clone(&state.engine)))
```
The token middleware layer wraps it because the layer is added after the nest.

**Step 4: Run the tests to verify they pass.**

```bash
cargo nextest run --lib tests::service::
cargo build
```

**Step 5: Hand the diff to the lead** (`parallel-lead-commit`).

**Acceptance criteria:**
- [ ] `/mcp` requires the token like every other route.
- [ ] `server/discover` answers with 2026-07-28 among its versions and no `Mcp-Session-Id` header.
- [ ] `tools/list` over HTTP returns the same names in the same order as the adapter catalog, on repeated calls.
- [ ] `tools/call` for `manage_workspace list` returns a `complete` result.

---

## Task 3: Client connector and stdio shim

**Files:**
- Create: `src/service/client.rs`, `src/service/shim.rs`, `src/tests/service/client.rs`, `src/tests/service/shim.rs`
- Modify: `src/cli.rs` (add `Command::McpStdio`), `src/main.rs` (replace the `None` arm; add `McpStdio` arm), `src/tests/service/mod.rs`
- Delete: `src/server_in_process.rs`; remove `pub mod server_in_process;` from `src/lib.rs`; delete or repoint tests that import it (`grep -rn server_in_process src/tests`).

**Interfaces:**
```rust
// src/service/client.rs
pub struct ServiceClient { pub base: String, pub token: String, pub version: String, http: reqwest::Client }
pub enum ConnectError { VersionMismatch { service: String, client: String }, Unavailable(String) }
/// Read service.json; connect; if that fails, remove the file, spawn `current_exe() service`
/// detached, wait up to 10 s for a fresh record, and try once more.
pub async fn connect_or_start(paths: &RegistryPaths, spawn: impl Fn() -> std::io::Result<()>) -> Result<ServiceClient, ConnectError>;
pub fn spawn_detached_service() -> std::io::Result<()>;
impl ServiceClient {
    pub async fn post_mcp(&self, body: &[u8], method: &str) -> reqwest::Result<reqwest::Response>;
    pub async fn status(&self) -> reqwest::Result<serde_json::Value>;
}

// src/service/shim.rs
/// Read newline-delimited JSON-RPC from `input`, POST each message to /mcp, write each
/// JSON response as one line to `output`. Notifications (no `id`) get no output line.
pub async fn forward<R: tokio::io::AsyncBufRead + Unpin, W: tokio::io::AsyncWrite + Unpin>(client: &ServiceClient, input: R, output: W) -> anyhow::Result<()>;
pub async fn run_stdio_shim() -> anyhow::Result<()>; // connect_or_start + forward(stdin, stdout)
```

**Contract inputs:** `discovery::{read_record, remove_record}` from Task 1. The service's `/mcp` accepts one JSON-RPC message per POST with `Accept: application/json, text/event-stream` and `Mcp-Method: <method>`; with `with_json_response(true)` a request-response tool call returns `application/json`. If the body is `text/event-stream`, the shim reads SSE `data:` lines and writes the last complete JSON object whose `id` matches. `std::process::Command` with `.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())`; on Unix add `.process_group(0)` (stable since Rust 1.64 via `std::os::unix::process::CommandExt`); on Windows add `creation_flags(0x00000008 | 0x00000200)` (`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`) via `std::os::windows::process::CommandExt`.

**File ownership:** as listed. **Serialization required:** No. **Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing tests.**

```rust
// src/tests/service/client.rs
use super::http_api::Running;
use crate::service::client::{connect_or_start, ConnectError};
use crate::service::discovery::{self, ServiceRecord};
use julie_core::paths::RegistryPaths;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn connects_to_a_running_service_without_spawning() {
    let running = Running::start(None).await;
    let spawns = Arc::new(AtomicUsize::new(0));
    let s = Arc::clone(&spawns);
    let client = connect_or_start(&running.paths, move || { s.fetch_add(1, Ordering::SeqCst); Ok(()) }).await.unwrap();
    assert_eq!(client.token, running.token);
    assert_eq!(spawns.load(Ordering::SeqCst), 0);
    assert!(client.status().await.unwrap()["version"].is_string());
}

#[tokio::test]
async fn stale_record_is_removed_and_the_spawn_hook_runs_once() {
    let home = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home.path().to_path_buf());
    let dead_port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    discovery::write_record(&paths, &ServiceRecord {
        port: dead_port, token: "0".repeat(64), pid: 0,
        version: env!("CARGO_PKG_VERSION").into(), started_at: "0".into(),
    }).unwrap();
    let spawns = Arc::new(AtomicUsize::new(0));
    let s = Arc::clone(&spawns);
    let result = connect_or_start(&paths, move || { s.fetch_add(1, Ordering::SeqCst); Ok(()) }).await;
    assert!(matches!(result, Err(ConnectError::Unavailable(_))));
    assert_eq!(spawns.load(Ordering::SeqCst), 1);
    assert!(discovery::read_record(&paths).unwrap().is_none());
}

#[tokio::test]
async fn spawn_hook_that_writes_a_record_lets_the_client_connect() {
    let running = Running::start(None).await;
    let live = discovery::read_record(&running.paths).unwrap().unwrap();
    let home = tempfile::tempdir().unwrap();
    let paths = RegistryPaths::with_home(home.path().to_path_buf());
    let target = paths.clone();
    let client = connect_or_start(&paths, move || discovery::write_record(&target, &live)).await.unwrap();
    assert_eq!(client.token, running.token);
}

#[tokio::test]
async fn version_mismatch_is_reported_with_both_versions() {
    let running = Running::start(None).await;
    let mut record = discovery::read_record(&running.paths).unwrap().unwrap();
    record.version = "0.0.0-other".into();
    discovery::write_record(&running.paths, &record).unwrap();
    let err = connect_or_start(&running.paths, || Ok(())).await.err().unwrap();
    match err {
        ConnectError::VersionMismatch { service, client } => {
            assert_eq!(service, "0.0.0-other");
            assert_eq!(client, env!("CARGO_PKG_VERSION"));
        }
        other => panic!("expected mismatch, got {other:?}"),
    }
}
```

```rust
// src/tests/service/shim.rs
use super::http_api::Running;
use crate::service::client::connect_or_start;
use crate::service::shim::forward;
use serde_json::{json, Value};

fn meta() -> Value {
    json!({"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {}})
}

#[tokio::test]
async fn shim_forwards_requests_and_drops_notifications() {
    let running = Running::start(None).await;
    let client = connect_or_start(&running.paths, || Ok(())).await.unwrap();
    let input = format!(
        "{}\n{}\n{}\n",
        json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta": meta()}}),
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":99,"_meta": meta()}}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"manage_workspace","arguments":{"operation":"list"},"_meta": meta()}}),
    );
    let mut output = Vec::new();
    forward(&client, tokio::io::BufReader::new(input.as_bytes()), &mut output).await.unwrap();
    let lines: Vec<Value> = String::from_utf8(output).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2, "one line per request, none for the notification");
    assert_eq!(lines[0]["id"], 1);
    assert!(lines[0]["result"]["protocolVersions"].is_array());
    assert_eq!(lines[1]["id"], 2);
    assert_eq!(lines[1]["result"]["resultType"], "complete");
}

#[tokio::test]
async fn shim_result_equals_direct_http_result_for_the_same_call() {
    let running = Running::start(None).await;
    let client = connect_or_start(&running.paths, || Ok(())).await.unwrap();
    let call = json!({"jsonrpc":"2.0","id":7,"method":"tools/list","params":{"_meta": meta()}});
    let direct: Value = client.post_mcp(call.to_string().as_bytes(), "tools/list").await.unwrap().json().await.unwrap();
    let mut output = Vec::new();
    forward(&client, tokio::io::BufReader::new(format!("{call}\n").as_bytes()), &mut output).await.unwrap();
    let via_shim: Value = serde_json::from_str(String::from_utf8(output).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(direct["result"]["tools"], via_shim["result"]["tools"]);
}
```

**Step 2: Run the tests to verify they fail.**

```bash
cargo nextest run --lib tests::service::client tests::service::shim
```
Expected: compile errors for the missing modules.

**Step 3: Implement.**

`src/service/client.rs`:
```rust
use crate::service::discovery::{self, ServiceRecord};
use julie_core::paths::RegistryPaths;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ConnectError {
    VersionMismatch { service: String, client: String },
    Unavailable(String),
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::VersionMismatch { service, client } => write!(
                f,
                "julie: service version {service} does not match client version {client}; run: julie-server service restart"
            ),
            ConnectError::Unavailable(why) => write!(f, "julie: service unavailable: {why}"),
        }
    }
}

impl std::error::Error for ConnectError {}

pub struct ServiceClient {
    pub base: String,
    pub token: String,
    pub version: String,
    http: reqwest::Client,
}

impl ServiceClient {
    fn from_record(record: &ServiceRecord) -> Self {
        Self {
            base: format!("http://127.0.0.1:{}", record.port),
            token: record.token.clone(),
            version: record.version.clone(),
            http: reqwest::Client::builder().timeout(Duration::from_secs(130)).build().expect("reqwest client"),
        }
    }

    pub async fn status(&self) -> reqwest::Result<serde_json::Value> {
        self.http.get(format!("{}/status", self.base)).bearer_auth(&self.token).send().await?.error_for_status()?.json().await
    }

    pub async fn post_mcp(&self, body: &[u8], method: &str) -> reqwest::Result<reqwest::Response> {
        self.http.post(format!("{}/mcp", self.base))
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .header("Mcp-Method", method)
            .body(body.to_vec())
            .send().await
    }

    pub async fn post_shutdown(&self) -> reqwest::Result<reqwest::Response> {
        self.http.post(format!("{}/shutdown", self.base)).bearer_auth(&self.token).send().await
    }
}

async fn try_connect(paths: &RegistryPaths) -> Result<Option<ServiceClient>, ConnectError> {
    let Some(record) = discovery::read_record(paths).map_err(|e| ConnectError::Unavailable(e.to_string()))? else {
        return Ok(None);
    };
    let client = ServiceClient::from_record(&record);
    match client.status().await {
        Ok(_) => {
            if record.version != env!("CARGO_PKG_VERSION") {
                return Err(ConnectError::VersionMismatch { service: record.version, client: env!("CARGO_PKG_VERSION").to_string() });
            }
            Ok(Some(client))
        }
        Err(_) => {
            discovery::remove_record(paths).map_err(|e| ConnectError::Unavailable(e.to_string()))?;
            Ok(None)
        }
    }
}

pub async fn connect_or_start(
    paths: &RegistryPaths,
    spawn: impl Fn() -> std::io::Result<()>,
) -> Result<ServiceClient, ConnectError> {
    if let Some(client) = try_connect(paths).await? {
        return Ok(client);
    }
    spawn().map_err(|e| ConnectError::Unavailable(format!("could not start service: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(client) = try_connect(paths).await? {
            return Ok(client);
        }
    }
    Err(ConnectError::Unavailable("service did not start within 10 s".into()))
}

pub fn spawn_detached_service() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new(exe);
    command.arg("service")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    command.spawn().map(|_| ())
}
```
Note: `try_connect` removes a stale record before the spawn hook runs, so the second test's assertion that the record is gone holds even though the hook wrote nothing. In `stale_record_is_removed...` the hook runs exactly once because `connect_or_start` spawns at most once.

`src/service/shim.rs`:
```rust
use crate::service::client::{connect_or_start, spawn_detached_service, ServiceClient};
use anyhow::Context;
use julie_core::paths::RegistryPaths;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub async fn forward<R, W>(client: &ServiceClient, mut input: R, mut output: W) -> anyhow::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let message: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("parse error: {e}")}});
                output.write_all(format!("{err}\n").as_bytes()).await?;
                continue;
            }
        };
        let method = message["method"].as_str().unwrap_or("").to_string();
        let is_request = !message["id"].is_null();
        let response = client.post_mcp(trimmed.as_bytes(), &method).await.context("POST /mcp")?;
        if !is_request {
            continue;
        }
        let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let body = response.text().await?;
        let reply = if content_type.starts_with("text/event-stream") {
            last_json_event(&body).unwrap_or_else(|| body.clone())
        } else {
            body
        };
        output.write_all(reply.trim().as_bytes()).await?;
        output.write_all(b"\n").await?;
        output.flush().await?;
    }
}

fn last_json_event(sse: &str) -> Option<String> {
    sse.lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(str::trim)
        .filter(|d| serde_json::from_str::<serde_json::Value>(d).map(|v| v.get("id").is_some()).unwrap_or(false))
        .last()
        .map(str::to_owned)
}

pub async fn run_stdio_shim() -> anyhow::Result<()> {
    let paths = RegistryPaths::try_new().context("resolve Julie home")?;
    let client = connect_or_start(&paths, spawn_detached_service).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    let stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let stdout = tokio::io::stdout();
    forward(&client, stdin, stdout).await
}
```

`src/cli.rs`: add `/// Forward MCP stdio to the machine service.` `McpStdio,` to `Command`. Return `false` for it in `cli_command_needs_workspace_startup_hint`.

`src/main.rs`: replace the whole `None => { ... run_in_process_server ... }` arm with:
```rust
        Some(Command::McpStdio) | None => {
            julie::service::shim::run_stdio_shim().await?;
        }
```
Remove the now-unused `needs_workspace_startup_hint` computation and `resolve_workspace_startup_hint` call if nothing else uses them (check with `grep -n resolve_workspace_startup_hint src/main.rs`). Delete `src/server_in_process.rs` and its `pub mod` line in `src/lib.rs`. Run `grep -rn "server_in_process" src crates` and remove or repoint every remaining reference; a test that only exercised `run_in_process_server` is deleted, a test that exercised `acquire_in_process_embedding_provider` moves that function to `src/embeddings/` if it is still used by the runtime factory (check `grep -rn acquire_in_process_embedding_provider src`), otherwise it is deleted too.

`src/service/mod.rs`: add `pub mod client; pub mod shim;`.

**Step 4: Run the tests to verify they pass.**

```bash
cargo nextest run --lib tests::service::
cargo build
grep -rn "server_in_process" src crates || echo "clean"
```
Expected: all service tests pass, build green, grep prints `clean`.

**Step 5: Hand the diff to the lead** (`parallel-lead-commit`).

**Acceptance criteria:**
- [ ] A client connects to a live service without spawning.
- [ ] A stale record is removed, the spawn hook runs exactly once, and a service that never appears yields `Unavailable`.
- [ ] A version mismatch produces the exact message from Global Constraints.
- [ ] The shim writes one line per request, nothing for notifications, and its result equals the direct HTTP result.
- [ ] `julie-server` with no arguments runs the shim. `src/server_in_process.rs` is gone.

---

## Task 4: Service control and dashboard mount

**Files:**
- Create: `src/tests/service/control.rs`
- Modify: `src/service/http.rs` (add `/shutdown` and mount the dashboard router at `/`), `src/service/mod.rs` (shutdown token plumbing), `src/cli.rs`, `src/main.rs` (service status/stop/restart, dashboard command), `src/dashboard/standalone.rs` (delete background spawn), `src/request_engine/dispatch.rs:60-66` (dashboard operation returns the service URL instead of `foreground_required`), `src/tests/service/mod.rs`, dashboard tests that reference `spawn_background_server` or `ensure_background_server`

**Interfaces:**
```rust
// http.rs
// POST /shutdown  -> 202 {"stopping":true}; cancels the serve loop after the response is sent.
// GET  /          -> existing dashboard router (crate::dashboard::create_router) with the token middleware in front.
// dispatch.rs: manage_workspace operation=dashboard returns
//   {"url": "http://127.0.0.1:<port>/?token=<token>"} when the caller is the service; the CLI prints and opens it.
```

**Contract inputs:** `crate::dashboard::create_router(dashboard: DashboardState, config: DashboardConfig) -> Result<Router, tera::Error>` at `src/dashboard/mod.rs:123`; `DashboardState::new(...)` at `src/dashboard/state.rs:141` (read its arguments; they are registry paths plus assets). `launch_dashboard`, `ensure_background_server`, `spawn_background_server`, `cached_server` at `src/dashboard/standalone.rs:38-136` are the deletion targets; `serve_dashboard_forever` at `:76` becomes a thin call that connects to the service and prints the URL. The engine's dashboard check at `src/request_engine/dispatch.rs:60-66` returns `foreground_required` today.

**File ownership:** as listed. **Serialization required:** Yes. **Dependency reason:** depends on Tasks 1 and 3.

**Step 1: Write the failing tests.**

```rust
// src/tests/service/control.rs
use super::http_api::{wait_for_record, Running};
use crate::service::client::connect_or_start;
use std::time::Duration;

#[tokio::test]
async fn shutdown_stops_the_service_and_removes_the_record() {
    let running = Running::start(None).await;
    let client = connect_or_start(&running.paths, || Ok(())).await.unwrap();
    let res = client.post_shutdown().await.unwrap();
    assert_eq!(res.status(), 202);
    let paths = running.paths.clone();
    tokio::time::timeout(Duration::from_secs(5), running.finished()).await.unwrap().unwrap();
    assert!(crate::service::discovery::read_record(&paths).unwrap().is_none());
}

#[tokio::test]
async fn dashboard_root_requires_token_and_renders_html() {
    let running = Running::start(None).await;
    let anon = running.client().get(format!("{}/", running.base)).send().await.unwrap();
    assert_eq!(anon.status(), 401);
    let page = running.client().get(format!("{}/?token={}", running.base, running.token)).send().await.unwrap();
    assert_eq!(page.status(), 200);
    let ct = page.headers().get("content-type").unwrap().to_str().unwrap().to_string();
    assert!(ct.starts_with("text/html"), "got {ct}");
}

#[tokio::test]
async fn manage_workspace_dashboard_returns_the_service_url() {
    let running = Running::start(None).await;
    let body: serde_json::Value = running.client().post(format!("{}/api/manage_workspace", running.base))
        .bearer_auth(&running.token).json(&serde_json::json!({"operation": "dashboard"}))
        .send().await.unwrap().json().await.unwrap();
    let url = body["result"]["url"].as_str().expect("url");
    assert!(url.starts_with(&running.base));
    assert!(url.contains(&running.token));
    let _ = wait_for_record(&running.paths).await;
}
```
The third test depends on how `ToolReply.result` is shaped for `manage_workspace`; read one existing `manage_workspace` reply in `src/tests/request_engine.rs` and match its shape. The engine needs to know the service URL: add `pub service_url: Option<String>` to `RequestEngine` set by `ServiceApp::serve` after bind (a plain field set through a `with_service_url` builder), and have the dashboard branch in `dispatch.rs` return `Ok(ToolReply::from_result("manage_workspace", None, json!({"url": url}), readiness))` when it is `Some`, else the existing `foreground_required` failure. Read `RequestReadiness` construction in `dispatch.rs` for the readiness value to use.

**Step 2: Run the tests to verify they fail.**

```bash
cargo nextest run --lib tests::service::control
```

**Step 3: Implement.**

- `http.rs`: add `.route("/shutdown", post(shutdown))` where `shutdown` sets a `CancellationToken` carried in `AppState` (`pub shutdown: tokio_util::sync::CancellationToken`) after returning `(StatusCode::ACCEPTED, r#"{"stopping":true}"#)`; use `tokio::spawn` with a 50 ms delay so the response flushes first. `ServiceApp::serve` uses that same token for graceful shutdown instead of creating its own.
- `http.rs`: build the dashboard router with `crate::dashboard::create_router(DashboardState::new(<args from state.rs:141>), DashboardConfig::default())` and `.merge(...)` it into the service router before the token layer. If `create_router` returns `Err`, `ServiceApp::new` fails; do not fall back.
- `dashboard/standalone.rs`: delete `ensure_background_server`, `spawn_background_server`, `cached_server`, `DashboardServer`, `launch_dashboard_for_paths`, and `build_dashboard_server`. `serve_dashboard_forever` becomes:
```rust
pub async fn serve_dashboard_forever() -> Result<()> {
    let paths = RegistryPaths::try_new()?;
    let client = crate::service::client::connect_or_start(&paths, crate::service::client::spawn_detached_service)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let url = format!("{}/?token={}", client.base, client.token);
    println!("{url}");
    let _ = open::that(&url); // only if `open` is already a dependency; otherwise print only
    Ok(())
}
```
Check `grep -n '^open' Cargo.toml`; if `open` is not a dependency, print the URL only. Rename the function to `open_dashboard` and update its two callers in `src/main.rs`.
- `main.rs`: `Command::Service(args)` arms for `Status` (print `client.status()` pretty JSON), `Stop` (`post_shutdown`, print `stopped`), `Restart` (`Stop` if a record exists, then `spawn_detached_service`, then `connect_or_start` and print the new status). Exit code 3 on `ConnectError::VersionMismatch` for every CLI path: map it in one place, a small `fn exit_code(e: &ConnectError) -> i32` in `client.rs`, and call `std::process::exit` from `main` only.
- Delete tests that tested the removed dashboard spawn path (`grep -rn 'spawn_background_server\|ensure_background_server\|cached_server' src/tests`).

**Step 4: Run the tests to verify they pass.**

```bash
cargo nextest run --lib tests::service:: tests::dashboard::
cargo build
```

**Step 5: Commit** (`serial-worker-commit`): `feat(service): add shutdown, service control verbs, and dashboard mount; delete standalone dashboard spawn`.

**Acceptance criteria:**
- [ ] `POST /shutdown` returns 202 and the service exits and removes its record.
- [ ] `GET /` renders the existing dashboard HTML behind the token.
- [ ] `manage_workspace dashboard` returns the tokenized service URL through the API; `julie-server dashboard` prints it.
- [ ] `julie-server service status|stop|restart` work against a live service; version mismatch exits 3 with the exact message.
- [ ] The background dashboard spawn path is deleted.

---

## Task 5: Multi-process bucket and line budget

**Files:**
- Create: `src/tests/service/process.rs`, `src/tests/service/budget.rs`
- Modify: `xtask/test_tiers.toml` (new buckets `service` and `service-process`; add `service` to `fast`, `smoke`, `dev`, `full`; add `service-process` to `dev` and `full`), `src/tests/service/mod.rs`

**Contract inputs:** bucket format at `xtask/test_tiers.toml:21-35`: `expected_seconds`, `timeout_seconds`, `scope_label`, `notes`, `commands = [...]`. Tiers at `:3-17`. Existing subprocess tests use `std::process::Command::new(env!("CARGO_BIN_EXE_julie-server"))`; confirm the exact env name with `grep -rn CARGO_BIN_EXE src/tests | head -3` and reuse it.

**File ownership:** as listed. **Serialization required:** Yes. **Dependency reason:** exercises the finished binary.

**Step 1: Write the failing tests.**

```rust
// src/tests/service/budget.rs
#[test]
fn service_process_model_stays_under_600_lines() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/service");
    let mut total = 0usize;
    let mut per_file = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }
        let lines = std::fs::read_to_string(&path).unwrap().lines().count();
        per_file.push(format!("{}: {lines}", path.file_name().unwrap().to_string_lossy()));
        total += lines;
    }
    assert!(total <= 600, "src/service is {total} lines; budget is 600. Design section 16 says the design is wrong, not the budget.\n{}", per_file.join("\n"));
}

#[test]
fn service_modules_contain_no_coordination_words() {
    let banned = ["lock", "lease", "fence", "generation", "epoch", "cursor", "claim", "pin", "coordinator", "broker", "journal", "repair", "continuation", "handoff"];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/service");
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") { continue; }
        for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
            let lower = line.to_lowercase();
            for word in banned {
                if lower.split(|c: char| !c.is_alphanumeric()).any(|t| t == word) {
                    hits.push(format!("{}:{}: {word}", path.file_name().unwrap().to_string_lossy(), n + 1));
                }
            }
        }
    }
    assert!(hits.is_empty(), "coordination words in src/service:\n{}", hits.join("\n"));
}
```
The `Mutex` in `status.rs` is a word the test does not ban; `lock()` as a method call is caught by the token split. Rename the `StatusLog` internals to use `parking`-free wording: call the method through a small helper `fn guard(&self) -> MutexGuard<Inner>` so the word `lock` appears once, and add that single line to an allowlist inside the test (`("status.rs", "lock")` with a comment-free tuple). Keep the allowlist to that one entry.

```rust
// src/tests/service/process.rs
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str { env!("CARGO_BIN_EXE_julie-server") }

fn home() -> tempfile::TempDir { tempfile::tempdir().unwrap() }

fn service_json(home: &tempfile::TempDir) -> std::path::PathBuf { home.path().join("service.json") }

fn wait_for(path: &std::path::Path, present: bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() == present { return true; }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn shim_starts_the_service_answers_and_service_exits_when_idle() {
    let home = home();
    let mut shim = Command::new(bin())
        .env("JULIE_HOME", home.path())
        .env("JULIE_SERVICE_IDLE_SECS", "1")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().unwrap();
    {
        use std::io::Write;
        let stdin = shim.stdin.as_mut().unwrap();
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{{"_meta":{{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{{}}}}}}}}"#).unwrap();
    }
    drop(shim.stdin.take());
    let out = shim.wait_with_output().unwrap();
    assert!(out.status.success(), "shim exit {:?}", out.status);
    let line = String::from_utf8(out.stdout).unwrap();
    assert!(line.contains("2026-07-28"), "got {line}");
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    assert!(wait_for(&service_json(&home), false, Duration::from_secs(10)), "service did not exit when idle");
}

#[test]
fn stale_service_json_is_replaced_by_a_fresh_service() {
    let home = home();
    let dead_port = { let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap(); l.local_addr().unwrap().port() };
    std::fs::write(service_json(&home), format!(
        r#"{{"port":{dead_port},"token":"{}","pid":0,"version":"{}","started_at":"0"}}"#,
        "0".repeat(64), env!("CARGO_PKG_VERSION")
    )).unwrap();
    let out = Command::new(bin())
        .env("JULIE_HOME", home.path()).env("JULIE_SERVICE_IDLE_SECS", "1")
        .args(["service", "status"]).stderr(Stdio::piped()).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(service_json(&home)).unwrap()).unwrap();
    assert_ne!(record["port"], dead_port);
    assert!(wait_for(&service_json(&home), false, Duration::from_secs(10)));
}

#[test]
fn version_mismatch_exits_3_with_the_exact_message() {
    let home = home();
    let mut svc = Command::new(bin())
        .env("JULIE_HOME", home.path()).env("JULIE_SERVICE_IDLE_SECS", "0")
        .arg("service").stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    let mut record: serde_json::Value = serde_json::from_slice(&std::fs::read(service_json(&home)).unwrap()).unwrap();
    record["version"] = "0.0.0-other".into();
    std::fs::write(service_json(&home), record.to_string()).unwrap();
    let out = Command::new(bin()).env("JULIE_HOME", home.path()).args(["service", "status"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&out.stderr).trim(),
        format!("julie: service version 0.0.0-other does not match client version {}; run: julie-server service restart", env!("CARGO_PKG_VERSION")));
    let _ = svc.kill();
}

#[test]
fn service_stop_exits_the_service() {
    let home = home();
    let mut svc = Command::new(bin())
        .env("JULIE_HOME", home.path()).env("JULIE_SERVICE_IDLE_SECS", "0")
        .arg("service").stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    assert!(wait_for(&service_json(&home), true, Duration::from_secs(5)));
    let out = Command::new(bin()).env("JULIE_HOME", home.path()).args(["service", "stop"]).output().unwrap();
    assert!(out.status.success());
    assert!(wait_for(&service_json(&home), false, Duration::from_secs(5)));
    let status = svc.wait().unwrap();
    assert!(status.success());
}
```
If `RegistryPaths::try_new` does not honor `JULIE_HOME` for `service_json` (it does, per `paths.rs:250-254`), these tests fail on the first run and that is the bug to fix, not the test. In `version_mismatch...` the shim spawn is not involved; `service status` connects directly.

`xtask/test_tiers.toml`:
```toml
[buckets.service]
# In-process machine service: discovery, token, JSON API, MCP over HTTP, shim forwarding.
expected_seconds = 20
timeout_seconds = 60
scope_label = "service"
notes = "Machine service bindings, in process on ephemeral ports"
commands = [
  "cargo nextest run --lib tests::service::http_api",
  "cargo nextest run --lib tests::service::mcp_http",
  "cargo nextest run --lib tests::service::client",
  "cargo nextest run --lib tests::service::shim",
  "cargo nextest run --lib tests::service::control",
  "cargo nextest run --lib tests::service::budget",
]

[buckets.service-process]
# Real subprocesses: shim starts service, stale record recovery, idle exit, version mismatch, stop.
expected_seconds = 20
timeout_seconds = 90
scope_label = "service-process"
notes = "The one multi-process bucket. Design section 12 caps it at 20 s."
commands = [
  "cargo build",
  "cargo nextest run --lib tests::service::process --test-threads 1",
]
```
Add `"service"` to `fast`, `smoke`, `dev`, and `full`; add `"service-process"` to `dev` and `full`. Keep list order alphabetical where the file already is.

**Step 2: Run the tests to verify they fail.**

```bash
cargo nextest run --lib tests::service::budget
cargo build && cargo nextest run --lib tests::service::process --test-threads 1
```
Expected: budget passes or fails on real counts (fix the code, not the number); process tests fail until the binary paths from Tasks 3 and 4 are wired.

**Step 3: Implement.** Only wiring fixes surfaced by the process tests. Any change that adds coordination words fails the budget test and stops the task.

**Step 4: Run the tests to verify they pass.**

```bash
cargo xtask test bucket service
cargo xtask test bucket service-process
```
Expected: both green, each under its `expected_seconds`.

**Step 5: Commit** (`serial-worker-commit`): `test(service): add service buckets, process tests, and the 600-line budget gate`.

**Acceptance criteria:**
- [ ] `src/service/` is at most 600 lines and contains no banned word beyond the one allowlisted `lock()` call.
- [ ] A real shim subprocess starts a real service, gets a `server/discover` answer, and the service exits when idle.
- [ ] A stale record is replaced by a fresh service in a subprocess run.
- [ ] Version mismatch exits 3 with the exact message; `service stop` exits the service.
- [ ] Both buckets exist, are in their tiers, and finish under budget.

---

## Task 6: Three-host gate and docs (lead)

**Files:**
- Create: `docs/findings/2026-09-DD-machine-service-phase1-host-gate.md` (DD is the run date)
- Modify: `docs/WORKSPACE_ARCHITECTURE.md` (replace the in-process stdio description with the service and shim), `AGENTS.md` (Commands: add `julie-server service status|stop|restart`; note the shim is the default no-arg mode), `README.md` (install snippet: HTTP registration and stdio fallback)

**Contract inputs:** the design's phase 1 gate: "if stateless MCP plus the shim needs session state or the Tasks extension to work with any of the three hosts, stop and reconsider before phase 2."

**File ownership:** as listed. **Serialization required:** Yes. **Dependency reason:** needs the built binary.

**Step 1: Build and start.**

```bash
cargo build --release
./target/release/julie-server service &          # or let the first client start it
./target/release/julie-server service status | head -20
```

**Step 2: Register with each host over HTTP and run the same three calls.** Read the port and token from `~/.julie/service.json`.

- Claude Code: `claude mcp add --transport http julie-svc http://127.0.0.1:<port>/mcp --header "Authorization: Bearer <token>"`, then in a session: `workspace list`, `search` for a known symbol in a registered workspace, `inspect` on that symbol.
- Codex: add an HTTP MCP server entry in `~/.codex/config.toml` per the current Codex docs (`codex mcp add` if the installed version has it), same three calls.
- Cursor: `~/.cursor/mcp.json` entry with `"url": "http://127.0.0.1:<port>/mcp"` and the header, same three calls.
- Each host again with the stdio shim: register `julie-server` with no arguments as a stdio server, same three calls.

**Step 3: Record.** For each host and transport: protocol version negotiated (from `/status` requests or the host's MCP log), whether `tools/list` appeared, whether the three calls returned, and any error text verbatim. If any host needed a session header, an `initialize` handshake the shim did not forward, or the Tasks extension, write **GATE FAILED** at the top with the evidence, and stop. Do not start phase 2.

**Step 4: Update docs** as listed. Keep each edit to the paragraphs that describe the old in-process path.

**Step 5: Ledger and commit.** Run `cargo xtask test dev`, record it in the ledger, commit: `docs(service): record phase 1 three-host gate and update architecture docs`.

**Acceptance criteria:**
- [ ] Finding records six runs (three hosts, two transports) with the same three calls each and verbatim errors.
- [ ] Gate verdict is stated at the top of the finding.
- [ ] `docs/WORKSPACE_ARCHITECTURE.md`, `AGENTS.md`, and `README.md` describe the service and shim and no longer describe the in-process stdio server.
- [ ] `cargo xtask test dev` is green at the final commit and recorded in the ledger.

---

## Execution handoff

- `local_commit_authority: authorized — user approved the design and asked for the plan to be written and prepped for a new session (2026-09-09).`
- `push_authority: missing — not requested.`
- `pr_authority: missing — not requested.`
- `reviewer_choice: codex` — the user chose Codex for the design review; carry it to the pre-merge review of this phase.

The next session opens in `/home/murphy/source/julie/.worktrees/machine-service` on branch `machine-service`, reads this plan and the design, and runs `razorback:subagent-driven-development` with this plan as input. Tasks 2 and 3 dispatch together; everything else serializes.
