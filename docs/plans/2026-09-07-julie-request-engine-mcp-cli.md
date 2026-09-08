# Julie request engine, full CLI, and MCP 2026-07-28 implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make every Julie tool callable through one application dispatcher from CLI and modern or legacy MCP, with identical workspace, semantic, safety, and error behavior.

**Architecture:** A request engine receives explicit application requests and invokes the existing tool implementations through their application-level gates. CLI and MCP are adapters; neither constructs a fake peer or performs internal MCP initialization. Existing per-workspace ownership remains in place for this independently shippable plan; the subsequent lifecycle plan replaces its runtime factory without changing the request contract.

**Tech Stack:** Rust 2024, Tokio, serde/serde_json, clap, schemars, rmcp exactly 3.0.1, existing SQLite/Tantivy and semantic runtime.

**Architecture Quality:** High risk at the dispatch/ownership boundary. Move policy and telemetry out of MCP wrappers, not around them. Keep one tool catalog, one workspace resolver, one application dispatcher, and one semantic-readiness hook. Do not introduce a global daemon, HTTP service, internal MCP client, or a second search implementation.

## Global Constraints

- Work in `/home/murphy/source/julie`. The prerequisite extractor migration must be accepted before this implementation begins. Coordinate upstream work at `/home/murphy/source/julie-extractors`; do not change it from this plan.
- This plan must ship before `2026-09-07-julie-workspace-lifecycle.md`; do not require its new manager, failover, continuation, or follower-edit behavior to pass this plan's gates.
- Full functionality includes semantic readiness, all 13 catalog tools, and every supported workspace operation. Do not silently turn semantics off for CLI.
- Preserve the Python/current embedding provider through an adapter. `/home/murphy/source/julie-semantic-sidecar` integration belongs to the native semantic plan.
- No continuous testing. No commits, push, releases, publishing, or user configuration changes are authorized by writing this plan.
- Existing files over 500 implementation lines touched here must be split by the responsibilities below. New implementation files stay under 500 lines and test files under 1,000 lines. Tests live under each package's `src/tests/`; fixture data lives under `fixtures/`.
- The tool schema and existing wire input names are authoritative. CLI aliases translate inputs; they do not invent a smaller alternate tool contract.
- No interactive MCP registration or client restart is required for verification. Spawn the built binary with pipes and an isolated `JULIE_HOME`.
- Every request has an absolute deadline and cancellation token. Cancel before a source/index mutation begins; once a commit starts, finish or roll it back and report its actual disposition. Never report cancellation as proof that no mutation happened.

## Source grounding and prerequisites

Inspected Julie `0432158c173c30ae2f76d92773380c744ddc6542`, `main`, on 2026-09-07. Planning/memory files were already dirty and must remain intact. Re-resolve lines after extractor migration.

| Existing code | Verified behavior and planned treatment |
|---|---|
| `src/handler.rs:2675`, `call_tool` | MCP wrapper owns read deadline, primary binding, deferred repair scheduling, and tool-router dispatch. Extract these policies into application dispatch. |
| `src/handler/tools/edit_file.rs:26`, `edit_file` | Follower gate, preparation, error classification, and telemetry are here. Calling only the underlying tool skips this policy. Preserve its current follower refusal until the lifecycle plan. |
| `src/handler/tools/mod.rs:10` | Thirteen dedicated tool modules compose the MCP tool router. Each gets a transport-independent `execute_*` method called by catalog dispatch. |
| `src/cli_tools/generic.rs:35`, `dispatch_generic_tool` | Already exposes thirteen names but calls tool structs directly. Replace its alternate execution match with request-engine invocation. |
| `src/cli_tools/commands.rs:467` | Generic name is leaked to satisfy `&'static str`; use a borrowed `&str` contract. |
| `src/cli_tools/mod.rs:173`, `bootstrap_standalone_handler` | Uses local storage, always indexes, then marks semantic initialization skipped. Replace that policy with explicit storage and semantic modes. |
| `src/server_in_process.rs:168`, `run_in_process_server` | Owns leader lock and shared index-root selection; currently warms embeddings before serve and relies on `on_initialized` for auto-index. Extract request-independent construction and lazy readiness. |
| `src/cli.rs:30`; `src/main.rs:20` | Named CLI subset plus generic command already exist. Extend, do not build another executable. |
| `crates/julie-core/src/mcp_compat.rs:10` | Central rmcp model adapter exists; update content aliases and result builders here during SDK migration. |
| `Cargo.toml:61`; `crates/julie-core/Cargo.toml:67` | rmcp currently resolves from `1.6` in both manifests. Align both to one workspace dependency. |
| `src/tests/cli_execution_tests.rs:500` | Existing CLI index/read proof, plus tests that currently expect unavailable workspace operations. Replace those expectations only when real parity is implemented. |

`impact(JulieServerHandler)` reaches at least 238 symbols; `trace(bootstrap_standalone_handler)` finds CLI execution, signals, and fixture callers. This is a coordinated extraction, not a two-file CLI patch. Preserve `signals`, `extract`, and standalone dashboard commands as application entry points with their existing behavior.

External facts checked 2026-09-07:

- [Official 3.x migration guide](https://github.com/modelcontextprotocol/rust-sdk/discussions/969) and [stable 3.0.1 release](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.0.1).
- [Pinned SDK server trait](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.0.1/crates/rmcp/src/handler/server.rs): `call_tool` returns `Result<CallToolResponse, ErrorData>`; `supported_protocol_versions` returns `Cow<'static, [ProtocolVersion]>`; default discovery uses that list plus `get_info`.
- [Pinned SDK request metadata](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.0.1/crates/rmcp/src/model/meta.rs): inline requests require `io.modelcontextprotocol/protocolVersion` and `io.modelcontextprotocol/clientCapabilities`; clientInfo is optional. Metadata validation belongs to rmcp, not a Julie replacement parser.
- [Pinned SDK server lifecycle](https://github.com/modelcontextprotocol/rust-sdk/blob/rmcp-v3.0.1/crates/rmcp/src/service/server.rs): use ordinary `ServiceExt::serve` on stdio and exercise its modern request opener. Do not use `serve_directly` to bypass wire negotiation in the acceptance test.

Select `ProtocolVersion::V_2026_07_28` explicitly. The migration guide warns against assuming `LATEST` selects the new version. Modern results carry `resultType`; legacy results omit it. SDK minimum Rust is 1.88. Do not claim support for tasks, subscriptions, elicitation, sampling, or HTTP merely because the SDK implements them.

## Fixed application contract

New types below are proposed interfaces, not claims about existing symbols. Place them in the files listed in task 1.

```rust
pub struct WorkspaceBinding {
    pub workspace_id: String,
    pub root: std::path::PathBuf,
    pub index_root: std::path::PathBuf,
}
pub struct RequestContext {
    pub deadline: tokio::time::Instant,
    pub cancellation: tokio_util::sync::CancellationToken,
    pub origin: RequestOrigin,
}
pub enum RequestOrigin { Cli, Mcp }
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    pub workspace: Option<std::path::PathBuf>,
    pub semantics: SemanticMode,
}
#[derive(serde::Serialize)]
pub struct ToolReply {
    pub schema_version: u32,
    pub tool: String,
    pub workspace_id: Option<String>,
    pub result: serde_json::Value,
    pub readiness: RequestReadiness,
}
#[derive(serde::Serialize)]
pub struct RequestFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: serde_json::Value,
}
```

`RequestEngine::execute(&self, ToolRequest, RequestContext) -> Result<ToolReply, RequestFailure>` is the only public tool executor. `ToolReply.result` preserves `content`, `structuredContent`, and `isError` from existing tool results; strip only transport-specific `resultType` before application comparison. Do not flatten structured JSON into text. MCP adds `readiness`/workspace evidence in result metadata; CLI emits the application envelope.

Resolution is deterministic: request workspace wins, then an explicitly supplied process `--workspace`; a missing MCP workspace fails `WORKSPACE_REQUIRED`. CLI may resolve its own cwd at argument-adaptation time and then passes the canonical absolute root explicitly. No MCP Roots round trip or mutable primary workspace. Conflicting argument-level and envelope workspace selectors fail `WORKSPACE_CONFLICT`. Global `manage_workspace` list operations are catalog-marked unbound. Validate canonical paths and sensitive-root rejection before creating storage. Use shared `$JULIE_HOME/indexes/{workspace_id}` by default; `--standalone` selects explicit local `.julie/indexes/{workspace_id}` storage for both lifecycle and lock placement, not a different tool implementation.

The temporary runtime factory caches a handler per `(canonical root, index root)` and never rebinds that handler to another workspace. Reuse existing leader/follower construction and startup repair. The lifecycle plan replaces this factory behind the same binding contract.

The extractor prerequisite supplies `syntax::SyntaxOptions<'a> { deadline: Option<std::time::Instant>, cancelled: Option<&'a std::sync::atomic::AtomicBool>, max_source_bytes: usize }` and `parse_source_with_options(&Path, &str, &SyntaxOptions) -> Result<ParsedSource, SyntaxError>`. Request-owned source-edit preflight must use this bounded API, not the offline `parse_source` convenience wrapper. Convert the absolute Tokio deadline to `std::time::Instant`, mirror request cancellation into an `Arc<AtomicBool>`, and propagate `SyntaxError::Cancelled`/`DeadlineExceeded` to application failures. The parser and diagnostic walk cooperate with cancellation. Cancelling or aborting a `spawn_blocking` handle does not preempt Rust parsing; keep its admission guard alive until the worker is joined, even if the client has already received a deadline result. Never let a late successful preparation commit after its request deadline.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `docs/plans/verification-ledger-template.md`, package manifests.

**Worker red/green scope:** `cargo nextest run -p julie --lib <exact_test_name>`; changed core adapter tests use `-p julie-core`. Run `cargo check -p julie` before the GREEN test. For wire tests build `cargo build -p julie --bin julie-server` and set `JULIE_TEST_BIN` to its absolute debug path.

**Worker ceiling:** One exact test function per RED/GREEN cycle, two runs. No xtask tiers, no parallel test processes. Missing new interfaces may produce a compile failure in RED; replace that with a behavioral failure before changing a preexisting policy.

**Worker gate invariant:** The named assertion proves real dispatch, output, readiness, or wire behavior; helper mocks may measure cancellation but cannot substitute for final spawned-binary evidence.

**Lead affected-change scope:** `cargo xtask test changed`; on OverBudget use `cargo xtask test changed --scale` with recorded rationale or `cargo xtask test fast`.

**Branch gate:** `cargo xtask test dev`, then `cargo xtask test system` because startup changes. Run `cargo xtask test dogfood` only if retrieval/scoring/tokenization changes beyond this adapter refactor.

**Security scope:** none declared. Dependency verification is `cargo tree -i rmcp` and the locked build; this is not a CVE-audit claim.

**Replay/metric evidence:** Hard gates are identical normalized application results, real modern/legacy protocol behavior, current-vector checks for required semantics, clean stdout, exact exit codes, and no unauthorized writes. Timing and RSS are report-only here; performance thresholds belong to the comparison plan.

**Escalation triggers:** Failure of follower safety, no-initialize request handling, or semantic parity blocks handoff. Do not widen accepted behavior to make snapshots pass.

**Assigned verification failure:** Diagnose within owned files; report plan mismatch or dependency failure to the lead. No broad suite retries.

**Verification ledger:** Record exact scope/HEAD/result/time. Reuse only a matching passing scope and exact HEAD, and never after modifying the verified tree. Include the working diff identity for uncommitted evidence.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| 1: Application dispatch | None - serial | `src/request_engine/{mod,types,catalog,dispatch,binding,runtime_factory}.rs`, `src/lib.rs`, `src/handler.rs`, `src/handler/tools/*.rs`, `src/tests/request_engine.rs`, `src/tests/mod.rs` | Yes | Establishes the sole application entry point and moves shared gates. |
| 2: Full CLI | None - serial | `src/cli.rs`, `src/main.rs`, `src/cli_tools/{mod,commands,generic,subcommands,output,input,replay,catalog}.rs`, `src/tests/{cli_execution_tests,cli_input_contract}.rs` | Yes | Uses task 1 and removes the old dispatcher. |
| 3: Semantic parity | None - serial | `src/request_engine/{semantic,readiness,runtime_factory,dispatch}.rs`, `src/server_in_process.rs`, `src/cli_tools/mod.rs`, `src/tests/semantic_request_contract.rs` | Yes | Requires common execution and updates overlapping construction files. |
| 4: rmcp 3.0.1 | None - serial | `Cargo.toml`, `Cargo.lock`, `crates/julie-core/Cargo.toml`, `crates/julie-core/src/mcp_compat.rs`, `src/handler.rs`, `src/handler/mcp_adapter.rs`, `src/server_in_process.rs`, `src/tests/mcp_protocol_contract.rs` | Yes | Keeps the application contract fixed during protocol changes. |
| 5: Real-wire acceptance and docs | None - serial | `src/tests/{request_transport_parity,request_process_helpers}.rs`, `src/tests/mod.rs`, `fixtures/request-engine/`, `docs/DEVELOPMENT.md`, `docs/TESTING_GUIDE.md` | Yes | Exercises the final binary from tasks 1–4; package test wiring is lead-owned. |

Commit mode for every task is `parallel-lead-commit`: workers hand off owned diffs and evidence without committing; the lead reviews before staging. The user requested Codex as final implementation reviewer. Execution continues between accepted slices; no per-slice approval interruptions.

### Task 1: Extract one guarded application dispatcher

**Files:** Create the task-1 files in the table; modify named handler modules and root test registration. Split handler routing/protocol methods into `src/handler/mcp_adapter.rs` in task 4; do not grow the existing large handler further.

**Interfaces:** Consume existing typed tool inputs and application methods. Produce `ToolRequest`, `RequestContext`, `ToolReply`, `RequestFailure`, immutable `WorkspaceBinding`, `ToolCatalog::{list,schema,decode}`, and `RequestEngine::execute`.

**Contract inputs:** The 13 names in `src/cli_tools/generic.rs:14`: blast_radius, call_path, deep_dive, edit_file, fast_refs, fast_search, get_context, get_symbols, manage_workspace, patterns, rename_symbol, rewrite_symbol, spillover_get.

**File ownership:** Task 1 table row. **Serialization required:** Yes. **Dependency reason:** All subsequent adapters consume this dispatch contract.

**Step 1: Write the failing test.** Add a new fixture helper `RequestFixture::indexed()` that creates a marked temp repo containing `pub fn request_probe() {}` and an isolated home, builds the real existing handler, and returns `engine`, `root`, and `database_path`. `request()` creates a `ToolRequest` and bounded context, not an MCP peer. Its `as_follower()` constructor must acquire the leader lock in a separate held fixture before constructing the tested engine.

```rust
#[tokio::test]
async fn request_engine_does_not_bypass_follower_edit_gate() {
    let fixture = RequestFixture::indexed().await;
    let follower = fixture.as_follower().await;
    let before = std::fs::read(fixture.root.join("src/lib.rs")).unwrap();
    let response = follower.execute("edit_file", serde_json::json!({
        "file_path":"src/lib.rs", "old_text":"request_probe",
        "new_text":"renamed_probe", "dry_run":false
    })).await;
    assert_eq!(response.unwrap_err().code, "FOLLOWER_READ_ONLY");
    assert_eq!(std::fs::read(fixture.root.join("src/lib.rs")).unwrap(), before);
}
```

These match fields are verified against `EditFileTool` at `crates/julie-tools/src/editing/edit_file.rs:175`. Schema decoding must succeed before the follower assertion; a parameter error cannot satisfy this test.

**Step 2: Verify RED.** `cargo nextest run -p julie --lib request_engine_does_not_bypass_follower_edit_gate`. First prove the old generic path bypasses the handler gate with valid arguments, then move the policy.

**Step 3: Implement.** Create one catalog macro listing name, input type, application method and access class. Generate schema/decode/match arms from it. Move the current handler wrapper bodies, including telemetry, into `pub(crate) async fn execute_*` methods. MCP wrappers and CLI invoke `RequestEngine::execute`; neither calls typed `.call_tool` directly. Access classification distinguishes reads, previews, source edits, and index/registry mutations. Preserve current follower refusals for task 1; enabling follower edits belongs to lifecycle task 3.

```rust
pub async fn execute(&self, request: ToolRequest, context: RequestContext)
    -> Result<ToolReply, RequestFailure>
{
    context.check_cancelled()?;
    let decoded = self.catalog.decode(&request.name, request.arguments)?;
    let binding = self.bindings.resolve(request.workspace, decoded.workspace())?;
    let runtime = self.runtimes.acquire(binding.as_ref(), &context).await?;
    runtime.check_access(decoded.access())?;
    let result = self.dispatch(decoded, &runtime, &context).await?;
    self.finish_reply(request.name, binding, result, runtime.readiness()).await
}
```

These helper methods are new, explicit interfaces to implement in the listed modules. Runtime acquisition and reads share the caller's deadline; source/index mutation uses a tracked commit boundary. Keep existing telemetry exactly once per invocation. Add catalog-vs-MCP name/schema equality, missing workspace, conflicting workspace, non-object arguments, unknown tool, and two simultaneous root requests to the same exact test module. Assert failed workspace resolution creates no `.julie` directory.

Add exact `source_preflight_deadline_cancels_and_joins_parser`: place a test-only worker-entry barrier around the real bounded parser call, expire the request after the worker has acquired admission, release the barrier, assert `DEADLINE_EXCEEDED`, await worker completion, assert its admission count returns to zero, and verify source bytes did not change. Upstream exact syntax tests separately prove cancellation inside parser/diagnostic traversal; do not invent a public progress-hook API. `src/request_engine/dispatch.rs` passes cancellation/deadline into the source preflight adapter from the extractor plan rather than allocating an uncancellable parser request.

**Step 4: Verify GREEN.** `cargo check -p julie`, then the exact RED command. The lead owns additional catalog/binding tests after the coherent slice.

**Step 5: Handoff.** Give the lead the owned diff, scope/HEAD, gate result, and all renamed handler entry points. No worker commit.

**Acceptance criteria:**
- [ ] Every public tool uses the same application-level gate and telemetry path from both transports.
- [ ] No CLI-created `Peer`, fake `initialize`, internal duplex MCP client, or unguarded typed-tool shortcut remains.
- [ ] Binding is explicit and immutable per handler; malformed and unknown inputs fail before I/O.
- [ ] Existing follower rejection is preserved until its separately tested replacement.

### Task 2: Make the CLI complete, discoverable, and replayable

**Files:** Task 2 row; create `input.rs`, `replay.rs`, `catalog.rs` and `src/tests/cli_input_contract.rs`; register tests through the lead.

**Interfaces:** Consume `RequestEngine`/`ToolCatalog`. Produce `tool <name> --params <json> | --params-file <path> | --params-stdin`, `tools list --json`, `tools schema <name> --json`, `tools replay --input <jsonl> --json`, and all named aliases. Add `deep-dive`, `edit`, `rename`, `rewrite`, and `spillover`; aliases for existing commands remain.

**Contract inputs:** JSON parameter sources are mutually exclusive. When no parameter input option is supplied, default to `{}`. File/stdin size limit is 16 MiB; supplied input must be one object, UTF-8, without trailing JSON documents. Replay input is one request envelope per nonblank line and executes serially in input order. Replay continues after an ordinary request failure, but stops on malformed framing to avoid uncertain mutation intent.

**File ownership:** Task 2 table row. **Serialization required:** Yes. **Dependency reason:** Removes the old generic branch after task 1 is available.

**Step 1: Write the failing test.** `parse_request_input` takes bytes plus a label and uses the same parser for files/stdin/inline input.

```rust
#[test]
fn cli_request_input_rejects_multiple_json_values() {
    let error = parse_request_input(br#"{"query":"one"} {"query":"two"}"#, "stdin")
        .unwrap_err();
    assert_eq!(error.code, "INVALID_ARGUMENTS");
    assert!(parse_request_input(br#"{"query":"one"}"#, "stdin").is_ok());
    assert!(parse_request_input(b"[]", "stdin").is_err());
}
```

**Step 2: Verify RED.** `cargo nextest run -p julie --lib cli_request_input_rejects_multiple_json_values`.

**Step 3: Implement.** Remove `call_standalone` from `CliToolCommand`; it now returns `&str` and the normalized JSON map only. Replace generic dispatch with an engine call. Build list/schema from the application catalog. Use `clap::ArgGroup` to enforce exclusive input flags. Empty input defaults only when no source was supplied; an empty supplied file/stdin fails. Replay assigns or preserves `request_id` and emits exactly one JSON line per attempted request. Disallow `--params-stdin` when stdin already carries replay records.

```rust
pub fn parse_request_input(bytes: &[u8], source: &str)
    -> Result<serde_json::Map<String, serde_json::Value>, RequestFailure>
{
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(RequestFailure::invalid_arguments(format!("{source}: input exceeds 16 MiB")));
    }
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| RequestFailure::invalid_arguments(format!("{source}: {e}")))?;
    value.as_object().cloned()
        .ok_or_else(|| RequestFailure::invalid_arguments(format!("{source}: expected object")))
}
```

JSON stdout is one complete document plus LF for single invocations, and JSONL for replay. Diagnostics/progress/logging use stderr. Success envelope is `{"schema_version":1,"ok":true,"request_id":...,"reply":...}`; failure is `{"schema_version":1,"ok":false,"request_id":...,"error":{"code":...,"message":...,"retryable":...,"details":...}}`. No timing or log text appears outside it. Exit codes: 0 completed/no tool error; 2 invalid arguments/unknown tool/workspace conflict; 3 tool-domain failure or `isError=true`; 4 unavailable/busy/readiness failure; 5 stale edit/continuation conflict; 124 deadline; 130 cancellation; 1 unclassified internal failure. Replay returns the first nonzero exit code in input order after processing valid lines. Broken stdout pipe terminates with exit 1, without attempting another response write.

Replay lines use this exact serialized shape: `{"schema_version":1,"request_id":"case-01","request":{"name":"fast_search","arguments":{"query":"request_probe"},"workspace":"/absolute/project","semantics":"off"}}`. `request_id` is an optional string; missing IDs become the one-based line number as a string. Require `schema_version:1`; reject duplicate IDs within a replay and unknown envelope fields. `ToolRequest` uses snake_case semantic values and serializes workspace as a host-native path string. Reject non-object arguments before runtime acquisition. Request deadlines are supplied by `--timeout-ms`, never accepted as a serialized monotonic clock value.

Catalog capabilities identify the foreground-only dashboard explicitly. Add `--foreground` to named workspace and generic invocation: `julie-server workspace dashboard --foreground` and `julie-server tool manage_workspace --params '{"operation":"dashboard"}' --foreground` run the same foreground dashboard behavior. Without that flag return `FOREGROUND_REQUIRED` and the supported command; never claim the UI launched when the one-shot process exits. The existing `julie-server dashboard` alias remains.

**Step 4: Verify GREEN.** `cargo check -p julie`; exact RED command. Lead runs the additional input-size, malformed UTF-8, file/stdin exclusivity, repeated generic-name allocation, schema equality, and replay ordering tests.

**Step 5: Handoff.** Supply help text, JSON fixture outputs, exit-code table validation, and owned diff to the lead.

**Acceptance criteria:**
- [ ] All 13 tools have named or generic full-schema access, with named aliases for the five missing command families.
- [ ] Catalog and schemas require no workspace indexing or model startup.
- [ ] Inline/file/stdin/replay routes share validation and application execution.
- [ ] JSON stdout and exact exit-code contract hold for failures as well as success.

### Task 3: Remove CLI-only semantic degradation

**Files:** Task 3 row; preserve native sidecar source unchanged.

**Interfaces:** Produce `SemanticRuntime::ensure_ready(binding, requirement, mode, deadline, cancellation)`. `requirement` identifies whether the requested operation needs query inference, symbol vectors, or both. The later native plan implements the same seam with stronger encoder identity and generation metadata.

**Contract inputs:** `Off` performs zero provider/model/vector work; `Auto` may use lexical results with explicit degradation; `Required` succeeds only when provider and operation-required compatible current vectors are ready. A warmed provider with empty/stale vectors is not ready. CLI flag `--semantics off|auto|required` and application request field map identically; MCP uses an explicitly documented request/tool option.

**File ownership:** Task 3 table row. **Serialization required:** Yes. **Dependency reason:** Requires application binding/deadline and changes shared startup construction.

**Step 1: Write the failing test.** Implement an injected readiness adapter whose query provider is ready but whose actual SQLite fixture has zero compatible symbol vectors.

```rust
#[tokio::test]
async fn required_semantics_refuses_ready_provider_with_missing_vectors() {
    let fixture = SemanticFixture::provider_ready_without_vectors().await;
    let error = fixture.ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await.unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["coverage"], "missing");
    let off = fixture.ensure(SemanticMode::Off, SemanticRequirement::QueryAndSymbols).await.unwrap();
    assert!(matches!(off, SemanticReadiness::Disabled));
}
```

**Step 2: Verify RED.** `cargo nextest run -p julie --lib required_semantics_refuses_ready_provider_with_missing_vectors`.

**Step 3: Implement.** Move eager acquisition out of `run_in_process_server`; catalog/schema/readiness status must answer without model warmup. Adapt `acquire_in_process_embedding_provider` behind the new trait. Delete automatic `mark_standalone_embedding_skipped` use from full-mode construction. Preserve explicit Off mode. Share the existing resident embedding host for CLI and MCP. Keep long initialization owned by the runtime; waiting is bounded by the request deadline and cancellation.

```rust
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticMode { Off, Auto, Required }
pub enum SemanticRequirement { None, Query, Symbols, QueryAndSymbols }
pub enum SemanticReadiness {
    Disabled,
    Starting,
    Ready { model_id: String, dimensions: usize, device: String },
    Degraded { code: String, retryable: bool },
}
#[async_trait::async_trait]
pub trait SemanticRuntime: Send + Sync {
    async fn ensure_ready(
        &self, binding: &WorkspaceBinding, requirement: SemanticRequirement,
        mode: SemanticMode, deadline: tokio::time::Instant,
        cancellation: &tokio_util::sync::CancellationToken,
    ) -> Result<SemanticReadiness, RequestFailure>;
}
```

Add `RequestReadiness` fields for canonical revision, lexical projection revision, semantic mode, readiness and coverage. Validate compatible model/dimension metadata in current SQLite before Required dispatch. If present metadata cannot establish compatibility, return explicit unknown compatibility instead of guessing. The native plan extends this to full encoder identity and vector generation. Do not hold a workspace mutation guard, SQLite mutex, or index-job permit while waiting on inference.

**Step 4: Verify GREEN.** `cargo check -p julie`; exact RED command. Lead verifies Off has zero acquisition calls, Auto reports degradation, Required times out rather than silently falling back, and CLI/MCP modes agree.

**Step 5: Handoff.** Document the trait and readiness fields for the native semantic-plan worker; no provider rewrite here.

**Acceptance criteria:**
- [ ] Full CLI and MCP perform the same semantic readiness checks against the same configured storage.
- [ ] Required mode never searches empty, stale, or incompatible required vectors while reporting full functionality.
- [ ] Discovery is independent of model startup, and every readiness wait is bounded.

### Task 4: Upgrade rmcp and support the modern request lifecycle

**Files:** Task 4 row. Read every dependency declaration with `cargo tree -i rmcp` after migration; no second rmcp major may remain in active application types.

**Interfaces:** Consume application replies; produce rmcp `CallToolResponse` at the MCP boundary. `get_info` advertises 2026-07-28 and supported legacy versions explicitly. Keep `tools/list` schemas generated by the shared catalog.

**Contract inputs:** Official tagged source URLs above. Use SDK constructors for `CallToolResult`, `ListToolsResult`, and metadata. Do not copy legacy struct literals without adding required fields. Modern cache hints for dynamic list results are `ttlMs:0`, private scope; do not advertise cacheable code results before revision-aware invalidation exists.

**File ownership:** Task 4 table row. **Serialization required:** Yes. **Dependency reason:** The SDK types change both manifests and MCP adapter methods.

**Step 1: Write the failing test.** Compile this assertion against the planned adapter helper; task 5 proves the actual wire behavior.

```rust
#[test]
fn mcp_adapter_advertises_modern_version_explicitly() {
    let versions = crate::handler::mcp_adapter::julie_protocol_versions();
    assert_eq!(versions[0], rmcp::model::ProtocolVersion::V_2026_07_28);
    assert!(versions.contains(&rmcp::model::ProtocolVersion::V_2025_11_25));
}
```

**Step 2: Verify RED.** `cargo nextest run -p julie --lib mcp_adapter_advertises_modern_version_explicitly`.

**Step 3: Implement.** Put rmcp `=3.0.1` in workspace dependencies, consume it from the root and julie-core, update lockfile, and ensure Rust >=1.88. Keep currently required features while migrating; prune HTTP features only after `cargo tree` and compilation prove no retained CLI/dashboard code requires them. Convert `Content` aliases through `mcp_compat` using tagged SDK types. Handle non-exhaustive protocol enums safely.

```rust
async fn call_tool(
    &self,
    request: rmcp::model::CallToolRequestParams,
    context: rmcp::service::RequestContext<rmcp::RoleServer>,
) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
    let (request, application_context) = self.adapt_request(request, context)?;
    let reply = self.engine.execute(request, application_context).await
        .map_err(to_mcp_error)?;
    Ok(to_mcp_tool_result(reply)?.into())
}
fn supported_protocol_versions(&self)
    -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]>
{
    std::borrow::Cow::Owned(julie_protocol_versions())
}
```

`to_mcp_error` maps invalid requests to -32602, unknown tool to -32602 with code `UNKNOWN_TOOL`, and runtime failures to -32603 with structured `data.code/retryable/details`. Tool-domain failures may remain `isError=true` to match existing behavior. Do not mask SDK unsupported-version and malformed-metadata errors. Remove required work from `on_initialized`; modern direct calls must acquire readiness through the application engine. Legacy `on_initialized` may schedule optional warming but must not be the sole place initialization occurs.

**Step 4: Verify GREEN.** `cargo check -p julie`; exact RED command; lead runs `cargo check -p julie-core` and `cargo tree -i rmcp`.

**Step 5: Handoff.** Record actual resolved SDK version, enabled features, minimum toolchain evidence, changed protocol imports and legacy compatibility evidence.

**Acceptance criteria:**
- [ ] Application invocation has no `Peer` or `RequestContext<RoleServer>` dependency.
- [ ] Modern request discovery/version metadata are handled by rmcp 3.0.1.
- [ ] Legacy initialization remains supported without modern wire discriminators leaking into its results.

### Task 5: Prove CLI/MCP parity using the real binary

**Files:** Task 5 row. Fixtures contain a small Rust repo with `.git` root marker and `src/lib.rs`, source/control edit pairs, valid/invalid JSONL requests, and no generated benchmark artifacts in indexed source.

**Interfaces:** Create `ProcessFixture::new(binary)` plus `rpc(line)`, `cli(args)`, and `shutdown()` in `src/tests/request_process_helpers.rs`. It owns child processes, temp source/home directories, piped stdin/stdout/stderr, 10-second per-response timeout, and cleanup-on-drop. `rpc` writes one UTF-8 JSON line, reads only the matching response ID while retaining notifications, and fails on non-JSON stdout. Stderr is drained concurrently. Do not build from inside tests.

**Contract inputs:** `JULIE_TEST_BIN=/home/murphy/source/julie/target/debug/julie-server`; use an explicitly supplied absolute binary path in other checkouts. Fixture `JULIE_HOME` is task-owned and isolated. No existing live MCP processes are touched.

**File ownership:** Task 5 table row. **Serialization required:** Yes. **Dependency reason:** Final binary must include all previous slices.

**Step 1: Write the failing test.** Modern tool invocation is the first message, with no initialize and no preceding discovery.

```rust
#[tokio::test]
async fn modern_direct_tools_call_initializes_workspace_without_handshake() {
    let mut fixture = ProcessFixture::from_env().await;
    let result = fixture.rpc(serde_json::json!({
        "jsonrpc":"2.0", "id":1, "method":"tools/call", "params":{
            "name":"fast_search", "arguments":{
                "query":"request_probe", "workspace":fixture.root(), "limit":5
            }, "_meta":{
                "io.modelcontextprotocol/protocolVersion":"2026-07-28",
                "io.modelcontextprotocol/clientCapabilities":{}
            }
        }
    })).await;
    assert_eq!(result["result"]["resultType"], "complete");
    assert_ne!(result["result"]["isError"], true);
    assert!(result["result"]["content"].to_string().contains("request_probe"));
    fixture.shutdown().await;
}
```

**Step 2: Verify RED.** Before lifecycle changes, preserve a baseline failure demonstrating dependence on `on_initialized`; after task 4, adding the test may already pass. In that case record it as a missing-regression-coverage test, do not fabricate a failure. Run `cargo build -p julie --bin julie-server`, then `JULIE_TEST_BIN=/home/murphy/source/julie/target/debug/julie-server cargo nextest run -p julie --lib modern_direct_tools_call_initializes_workspace_without_handshake`.

**Step 3: Implement missing behavior and complete the fixture matrix.** Correct application-readiness or MCP adapter wiring if the real test fails; do not add an initialize message to make it pass. Add exact named tests for modern `server/discover`, modern first-message `tools/list`, unsupported protocol version, missing capabilities, malformed JSON, unknown method, legacy initialize + initialized + tools/call, CLI named/generic/file/stdin parity, structured tool errors, timeout, and shutdown. Compare normalized result payloads excluding only request IDs, timing, protocol metadata and explicitly documented diagnostic timestamps. Preserve content order, source paths, result counts, readiness mode, and error codes.

Run all thirteen tools through both real adapters using valid fixture arguments; use preview mode for edits and paired fresh fixtures for apply. Compare the actual CLI envelope and MCP result through one test normalization helper that removes only the documented transport fields. Assert failure codes, `isError`, readiness, structured content and result ordering separately so an overbroad normalizer cannot hide differences. Include registry stats and health, not only search. Required semantic parity uses a real configured provider and vectors when available; a provider-less environment must pass the equal explicit-unavailable test and record the full-provider gate as unrun, never full-mode success.

Update `docs/DEVELOPMENT.md` with the exact CLI commands, storage selectors, named/generic examples, semantic modes, catalog/schema discovery, exit codes, and no-interactive-registration wire command. Update `docs/TESTING_GUIDE.md` with the isolated process gate. Re-check official SDK documentation before changing protocol instructions.

**Step 4: Verify GREEN.** Rebuild the binary after implementation changes, then run the exact failed test. Lead runs each remaining exact process test sequentially and the declared branch gates once.

**Step 5: Handoff.** Root Codex reviews implementation, source/CLI/MCP coverage table, test ledger, and subprocess stdout/stderr transcripts. Include checkout/branch/HEAD/dirty state, SDK version, binary build command, storage mode, semantic backend/coverage, skipped hardware-specific verification and why. Continue safe verification before asking any final implementation approval.

**Acceptance criteria:**
- [ ] A first-message modern tool request works without initialize; discovery and listing work separately.
- [ ] Legacy initialization produces historical wire shapes; invalid metadata/version receives a structured error.
- [ ] All thirteen tool schemas and normalized outputs agree across application, CLI and MCP routes.
- [ ] Full semantic readiness is proved or explicitly recorded as unverified, never inferred from lexical success.
- [ ] Documentation, final Codex review and required regression gates are complete.

## Verification Ledger

Record one row per verification run. Every column is required. Leave this table
empty until a command has actually run or evidence has actually been reused.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Workspace compilation passes with zero errors | `cargo check --workspace --all-targets` | lead-build-check | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:12:06Z | no |
| Workspace code formatting is clean with zero diffs | `cargo fmt --all -- --check` | lead-fmt-check | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:12:09Z | no |
| Single rmcp v3.0.1 dependency tree across workspace | `cargo tree -i rmcp` | dependency-tree | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:12:03Z | no |
| Follower instance refuses mutating source edits | `cargo nextest run -p julie --lib request_engine_does_not_bypass_follower_edit_gate 2>&1 \| tail -10` | worker-exact | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T03:15:20Z | no |
| RequestEngine core test suite passes (9/9) | `cargo nextest run -p julie --lib tests::request_engine 2>&1 \| tail -10` | contract-test | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T03:18:45Z | no |
| CLI input rejects multiple concatenated JSON values | `cargo nextest run -p julie --lib cli_request_input_rejects_multiple_json_values 2>&1 \| tail -10` | worker-exact | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T03:45:10Z | no |
| CLI subcommands parse global and target workspace flags | `cargo nextest run -p julie --lib cli_subcommands_parse_with_global_and_target_workspace_flags 2>&1 \| tail -10` | worker-exact | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T04:12:30Z | no |
| CLI input contract test suite passes (27/27) | `cargo nextest run -p julie --lib tests::cli_input_contract 2>&1 \| tail -10` | contract-test | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T04:14:15Z | no |
| CLI execution test suite passes (36/36) | `cargo nextest run -p julie --lib tests::cli_execution_tests 2>&1 \| tail -10` | contract-test | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T04:16:00Z | no |
| CLI tools test suite passes (39/39) | `cargo nextest run -p julie --lib tests::cli_tools_tests 2>&1 \| tail -10` | contract-test | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T04:18:20Z | no |
| Required semantics refuses ready provider with missing vectors | `cargo nextest run -p julie --lib required_semantics_refuses_ready_provider_with_missing_vectors 2>&1 \| tail -10` | worker-exact | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T04:35:40Z | no |
| Semantic request contract test suite passes (12/12) | `cargo nextest run -p julie --lib tests::semantic_request_contract 2>&1 \| tail -10` | contract-test | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T04:37:10Z | no |
| McpAdapter advertises modern 2026-07-28 protocol version | `cargo nextest run -p julie --lib mcp_adapter_advertises_modern_version_explicitly 2>&1 \| tail -10` | worker-exact | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:02:15Z | no |
| MCP protocol contract test suite passes (22/22) | `cargo nextest run -p julie --lib tests::mcp_protocol_contract 2>&1 \| tail -10` | contract-test | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:05:00Z | no |
| Modern direct tools/call succeeds on real compiled binary | `cargo test -p julie --lib modern_direct_tools_call_initializes_workspace_without_handshake 2>&1 \| tail -10` | process-acceptance | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:14:00Z | no |
| 13-tool transport parity matrix across CLI and MCP passes | `cargo nextest run -p julie --lib tests::request_transport_parity 2>&1 \| tail -10` | process-acceptance | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:14:30Z | no |
| Extractor integration bucket passes | `cargo xtask test bucket extractor-dep-integration` | lead-extractor-dep | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:14:45Z | no |
| Batch dev regression tier passes | `cargo xtask test dev` | lead-dev | `65e372f65c2331153b49e1b75961a221974488ad` | pass | 2026-09-08T05:15:00Z | no |
