# Remove Legacy IPC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Remove the legacy daemon IPC transport now that the rebuilt daemon has proven live MCP traffic over the canonical HTTP transport.

**Architecture:** Keep stdio as the external MCP-client compatibility layer, but make every daemon client path discover and use the Streamable HTTP endpoint. Delete the IPC listener, IPC header protocol, IPC transport variant, IPC tests, and fallback probes after the adapter and CLI daemon clients both use the same HTTP discovery contract.

**Tech Stack:** Rust, rmcp Streamable HTTP client/server, Tokio, Axum, Julie daemon discovery files, cargo nextest, xtask tiers.

---

## File Structure

**Modify**
- `src/adapter/launcher.rs`
  - Remove fallback probing through `daemon_ipc_addr`.
  - Treat `daemon_mcp_transport` as the only daemon transport discovery file.
- `src/adapter/mod.rs`
  - Remove IPC connector, ready-line, and byte-forwarding helpers.
  - Keep `run_adapter_with` delegating to `run_http_adapter`.
- `src/adapter/http_stdio.rs`
  - Keep this as the canonical stdio-to-HTTP bridge.
- `src/cli_tools/daemon.rs`
  - Replace the IPC stream client with an HTTP Streamable client using the daemon discovery file and CLI workspace headers.
- `src/daemon/mod.rs`
  - Remove the IPC listener bind, accept loop, and IPC cleanup.
  - Keep HTTP transport shutdown, lifecycle, session drain, stale-binary admission, dashboard, watcher, and embedding behavior.
- `src/daemon/transport.rs`
  - Make `TransportEndpoint` HTTP-only.
  - Remove `TransportMode::Ipc`, `TransportEndpoint::new`, IPC bind/connect APIs, and IPC readiness probing.
- `src/daemon/mcp_session.rs`
  - Move `workspace_ids_to_disconnect` here or into a small daemon session cleanup helper before deleting `ipc_session.rs`.
- `src/daemon/mod.rs` module exports
  - Stop exporting `ipc`, `ipc_session`, `parse_ipc_headers_block`, `PrefixedIpcStream`, `handle_ipc_session`, and `read_ipc_headers`.
- `src/paths.rs`
  - Remove or deprecate `daemon_ipc_addr` and `daemon_ipc_addr_string` if no product or test code uses them after the transport deletion.
- `xtask/test_tiers.toml`
- `xtask/src/changed.rs`
- `xtask/tests/manifest_contract_tests.rs`
- `docs/plans/2026-05-03-http-daemon-transport.md`
- `docs/plans/2026-05-03-dead-code-audit-cleanup.md`
- `docs/plans/2026-05-03-dead-code-tool-readiness.md`

**Delete When Callers Are Gone**
- `src/daemon/ipc.rs`
- `src/daemon/ipc_session.rs`
- `src/tests/daemon/ipc.rs`
- `src/tests/daemon/ipc_headers.rs`
- `src/tests/daemon/ipc_session.rs`
- `src/tests/adapter/forwarding.rs`

**Retire Or Rewrite Tests**
- `src/tests/daemon/transport.rs`
  - Keep HTTP discovery/readiness tests.
  - Delete IPC bind/connect/discovery tests.
- `src/tests/adapter/launcher.rs`
  - Replace IPC fallback readiness tests with HTTP-only discovery/readiness behavior.
- `src/tests/adapter/ready.rs`
  - Delete ready-line tests if no non-test caller remains.
- `src/tests/integration/daemon_lifecycle.rs`
  - Replace raw IPC lifecycle probes with HTTP discovery readiness probes.

## Implementation Tasks

### Task 1: Move CLI Daemon Client To HTTP

**Files:**
- Modify: `src/cli_tools/daemon.rs`
- Test: existing CLI daemon tests, or add focused tests in the nearest CLI test module.

**What to build:** Make CLI tool execution use the same `daemon_mcp_transport` discovery and Streamable HTTP client configuration as the adapter. CLI requests must still send workspace path, workspace source `cli`, and Julie version headers before calling tools.

**Approach:** Reuse or extract the header/config construction from `src/adapter/http_stdio.rs::http_client_config_for_endpoint` so adapter and CLI do not drift. `DaemonClient::call_tool` can use the rmcp client transport directly instead of hand-writing JSON-RPC lines. Preserve the existing fallback semantics: transport failures may fall back to standalone mode, but daemon-returned tool errors must not.

**Acceptance criteria:**
- No CLI daemon client code imports `crate::daemon::ipc` or `build_ipc_header`.
- CLI daemon calls include workspace, workspace source, Julie version, and bearer token headers.
- Daemon tool errors remain distinct from transport failures.
- Focused CLI tests pass.

### Task 2: Make Discovery And Launcher HTTP-Only

**Files:**
- Modify: `src/adapter/launcher.rs`
- Modify: `src/daemon/transport.rs`
- Modify: `src/tests/adapter/launcher.rs`
- Modify: `src/tests/daemon/transport.rs`

**What to build:** Remove the IPC fallback path from daemon readiness. A running daemon is ready only when state says ready/draining or the HTTP discovery endpoint probes ready.

**Approach:** Collapse `TransportEndpoint` to the `StreamableHttp` shape and keep `read_discovery`, `publish_discovery`, `mcp_url`, `token_path`, `probe_readiness`, and `wait_for_readiness`. Unknown, missing, stale, or invalid discovery should make readiness `Starting` while PID is alive, not silently fall back to IPC.

**Acceptance criteria:**
- No `TransportMode::Ipc` or `TransportEndpoint::new` remains.
- Launcher readiness no longer references `daemon_ipc_addr`.
- HTTP readiness and stale discovery tests pass.

### Task 3: Remove The Daemon IPC Server Path

**Files:**
- Modify: `src/daemon/mod.rs`
- Modify: `src/daemon/mcp_session.rs`
- Delete: `src/daemon/ipc.rs`
- Delete: `src/daemon/ipc_session.rs`
- Modify/delete matching daemon tests.

**What to build:** Delete the IPC listener bind, accept loop, header parsing, ready-line handshake, and IPC stream serving. HTTP remains the only daemon MCP server transport.

**Approach:** Keep lifecycle ownership in `DaemonLifecycleController` and HTTP admission in `HttpSessionAdmission`. Any cleanup helper still needed from `ipc_session.rs`, especially workspace disconnect ordering, must move into `mcp_session.rs` with tests before deleting the IPC module.

**Acceptance criteria:**
- `run_daemon` publishes HTTP discovery, marks lifecycle ready, runs embedding init, and waits on shutdown/restart/stop signals without any IPC listener.
- No product code imports `crate::daemon::ipc` or `crate::daemon::ipc_session`.
- Session cleanup invariants remain covered through HTTP or session-level tests.

### Task 4: Remove Adapter IPC Forwarding Fossils

**Files:**
- Modify: `src/adapter/mod.rs`
- Delete or rewrite: `src/tests/adapter/forwarding.rs`
- Modify: `src/tests/adapter/mod.rs`

**What to build:** Delete old stdio-to-IPC helpers and readiness-line parsing once no non-test caller remains. Keep only the HTTP stdio shim and retry harness.

**Approach:** `ForwardOutcome` can stay if the HTTP shim still uses it. `ReadyOutcome`, `build_ipc_header`, `read_daemon_ready`, `connect_and_handshake`, `forward_bytes`, and `forward_streams` should disappear unless a real HTTP caller still needs the abstraction.

**Acceptance criteria:**
- Adapter default invocation still uses `run_http_adapter`.
- No adapter code imports IPC stream types.
- HTTP stdio adapter tests still pass.

### Task 5: Update Test Buckets, Docs, And Dead-Code Evidence

**Files:**
- Modify: `xtask/test_tiers.toml`
- Modify: `xtask/src/changed.rs`
- Modify: `xtask/tests/manifest_contract_tests.rs`
- Modify: transport and dead-code plan docs listed above.

**What to build:** Remove IPC test buckets and changed-file routes. Update docs so IPC is no longer described as a compatibility transport.

**Approach:** Delete bucket commands for removed test modules. Keep `transport` or `daemon` bucket coverage for HTTP transport, adapter HTTP stdio, daemon lifecycle, and dashboard/session cleanup.

**Acceptance criteria:**
- `cargo xtask test list` does not list deleted IPC test modules.
- Changed routing does not mention deleted IPC files.
- Docs record that HTTP is the only daemon MCP transport.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, and `xtask/test_tiers.toml`.

**Worker red/green scope:** Workers run exact tests only, such as `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10` or `cargo nextest run -p xtask <exact_test_name> 2>&1 | tail -10`.

**Worker ceiling:** Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test reliability`, `cargo xtask test system`, or broad `cargo nextest run --lib`.

**Worker gate invariant:** Every worker report must state the transport invariant it proved, for example "CLI daemon calls use HTTP discovery and preserve daemon tool-error semantics" or "launcher readiness no longer falls back to IPC."

**Lead affected-change scope:** After a coherent batch, the lead runs `cargo xtask test changed`.

**Branch gate:** Before merge, the lead runs `cargo xtask test dev` once.

**Expensive specialist gates:** Run `cargo xtask test reliability` because this deletes daemon session transport code. Run `cargo xtask test system` if startup, workspace init, or lifecycle readiness tests are touched beyond transport deletion.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless this plan is explicitly updated to change that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse same-HEAD scoped evidence instead of rerunning expensive gates.

## Execution Notes

- CLI daemon execution now uses the same Streamable HTTP client configuration helper as the adapter, including workspace, workspace source, Julie version, and bearer-token headers.
- `TransportEndpoint` is HTTP-only. Legacy discovery documents with `"mode": "ipc"` are rejected instead of silently falling back.
- The daemon IPC listener, accept loop, header protocol, ready-line handshake, and stream-serving modules were deleted. Stale-binary disconnect behavior moved to HTTP session cleanup.
- `workspace_ids_to_disconnect` moved to `src/daemon/mcp_session.rs` with session-level tests before `src/daemon/ipc_session.rs` was deleted.
- The adapter module now keeps only the HTTP stdio bridge. `ForwardOutcome` remains because the HTTP bridge uses it to distinguish a normal session end from an immediate daemon disconnect.
- `cargo xtask test changed` selected the full `dev` bucket set for this diff and passed in 367.2s. I did not rerun `cargo xtask test dev` separately because that would repeat the same 22 bucket commands without adding coverage.

## Verification Ledger

| Scope | Invariant | Command | Revision | Result | Timestamp |
| --- | --- | --- | --- | --- | --- |
| worker-red-green | Launcher stays `Starting` when HTTP discovery is stale even if a legacy Unix socket exists. | `cargo nextest run --lib test_readiness_starting_when_http_discovery_is_stale_even_if_legacy_socket_exists 2>&1 \| tail -30` | `04c93a17+dirty` | PASS, 1 test in 0.023s | 2026-05-04T01:54:58Z |
| worker-red-green | Launcher stays `Starting` when only a live PID and legacy socket exist. | `cargo nextest run --lib test_readiness_starting_when_no_state_file_even_if_legacy_socket_exists 2>&1 \| tail -30` | `04c93a17+dirty` | PASS, 1 test in 0.013s | 2026-05-04T01:54:58Z |
| worker-red-green | Daemon MCP discovery paths remain distinct from the dashboard port file. | `cargo nextest run --lib test_daemon_mcp_transport_paths_are_distinct_from_dashboard_port 2>&1 \| tail -30` | `04c93a17+dirty` | PASS, 1 test in 0.012s | 2026-05-04T01:54:58Z |
| worker-red-green | Legacy IPC discovery documents are rejected after the HTTP-only transport collapse. | `cargo nextest run --lib test_transport_discovery_rejects_legacy_ipc_mode 2>&1 \| tail -30` | `04c93a17+dirty` | PASS, 1 test in 0.014s | 2026-05-04T01:54:58Z |
| worker-red-green | Session cleanup still disconnects startup and rebound workspace resources in deterministic order. | `cargo nextest run --lib workspace_ids_to_disconnect 2>&1 \| tail -30` | `04c93a17+dirty` | PASS, 3 tests in 0.014s | 2026-05-04T01:54:58Z |
| worker-red-green | HTTP session DELETE triggers stale-binary restart when the last session disconnects. | `cargo nextest run --lib test_http_julie_session_delete_triggers_restart_when_binary_became_stale 2>&1 \| tail -30` | `04c93a17+dirty` | PASS, 1 test in 0.106s | 2026-05-04T01:54:58Z |
| worker-red-green | Daemon lifecycle still starts, publishes PID, and stops without IPC cleanup. | `cargo nextest run --lib test_daemon_starts_creates_pid_then_stops 2>&1 \| tail -40` | `04c93a17+dirty` | PASS, 1 test in 8.709s | 2026-05-04T01:54:58Z |
| worker-red-green | Daemon startup publishes HTTP discovery with private token material. | `cargo nextest run --lib test_daemon_publishes_http_transport_discovery_with_private_token 2>&1 \| tail -40` | `04c93a17+dirty` | PASS, 1 test in 8.635s | 2026-05-04T01:54:58Z |
| worker-red-green | xtask manifest contract accepts the HTTP-only transport bucket notes and command set. | `cargo nextest run -p xtask manifest_contract_tests_checked_in_manifest_uses_exact_bucket_specs 2>&1 \| tail -40` | `04c93a17+dirty` | PASS, 1 test in 0.010s | 2026-05-04T01:54:58Z |
| affected-change | Diff-selected calibrated gate passes. The selected bucket set matched the checked-in `dev` tier. | `cargo xtask test changed 2>&1 \| tail -120` | `04c93a17+dirty` | PASS, 22 buckets in 367.2s | 2026-05-04T01:54:58Z |
| specialist-gate | Reliability gate passes after deleting daemon transport/session code. | `cargo xtask test reliability 2>&1 \| tail -120` | `04c93a17+dirty` | PASS, 3 buckets in 54.2s | 2026-05-04T01:54:58Z |
| specialist-gate | Transport bucket covers adapter, daemon discovery, HTTP transport, and MCP session cleanup without deleted IPC modules. | `cargo xtask test bucket transport 2>&1 \| tail -120` | `04c93a17+dirty` | PASS, 1 bucket in 2.4s | 2026-05-04T01:54:58Z |

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Plan-specific overrides:** This is transport and lifecycle cleanup, so treat it as shared-invariant work. Use Codex `gpt-5.3-codex high` for bounded worker implementation, `gpt-5.3-codex xhigh` for restart/shutdown or terminal-heavy failures, and keep final review lead-owned.

**Worker eligibility:** Implementation-tier workers are eligible only for narrow write scopes with focused tests: CLI client migration, launcher/transport tests, adapter fossil deletion, or xtask/doc cleanup. Keep `run_daemon` lifecycle deletion and integration review lead-owned unless the task is narrowed further.

**Escalation triggers:** Escalate on any failure involving daemon startup, stale-binary restart, HTTP admission, session cleanup, CLI fallback semantics, or deleted-file watcher behavior.

**Mechanical exclusion:** Mechanical workers may update docs or bucket manifests only after the lead decides which IPC tests/files are deleted. They cannot decide transport contract behavior.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Worker A: CLI daemon HTTP client migration, owned files `src/cli_tools/daemon.rs` and focused CLI tests.
- Worker B: HTTP-only launcher and `TransportEndpoint`, owned files `src/adapter/launcher.rs`, `src/daemon/transport.rs`, and focused tests.
- Lead: daemon `run_daemon` IPC listener removal and session cleanup helper migration.
- Worker C: adapter IPC forwarding fossil deletion after Workers A/B and lead daemon removal settle the new API.
- Worker D: xtask and docs cleanup after deleted files are final.
- Lead: affected-change, reliability, branch-gate, final merge.

## Risks

- CLI daemon commands still used IPC, so deleting IPC before migrating them would silently force standalone fallback or break CLI tool execution.
- Stale-binary restart behavior moved from the IPC accept loop to HTTP admission. Do not delete restart tests unless the HTTP tests prove the same user-visible behavior.
- Some tests encode useful workspace cleanup invariants through IPC session helpers. Move those invariants to session or HTTP tests before deleting the IPC harness.
- A missing or stale discovery file with a live PID should be reported as `Starting`, not `Ready`. Silent fallback would hide broken HTTP transport publication.
