# HTTP Daemon Transport Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Add localhost Streamable HTTP MCP as the canonical daemon transport while keeping stdio mode as a thin compatibility shim that proxies MCP bytes to the daemon.

**Architecture:** Add an HTTP transport module that serves MCP over localhost using the existing `rmcp` Streamable HTTP server support and the current Tokio/Axum HTTP stack. Keep `src/daemon/ipc.rs` and `src/daemon/ipc_session.rs` working during migration, but make `src/adapter/mod.rs` target the canonical daemon endpoint and keep stdio as the MCP-client compatibility layer. The `rmcp` 1.6.0 release added transport features that overlap with this plan, so Julie should delegate Host and Origin validation, streamable HTTP session timeout behavior, and resumability support to the SDK where the APIs fit. Julie still owns daemon discovery, local token policy, lifecycle, and adapter compatibility.

**Tech Stack:** Rust, Tokio, Axum 0.8, `rmcp` with `transport-streamable-http-server` after an intentional SDK audit. Current `Cargo.toml` declares `rmcp = "1.2"` while `Cargo.lock` resolves `rmcp` 1.5.0; this plan should target an explicit 1.6.0 upgrade or record why 1.5.0 remains pinned. Keep the existing daemon `WorkspacePool`, `EmbeddingService`, `WatcherPool`, `DashboardState`, and adapter forwarding code.

---

## File Structure

- Modify `src/main.rs:12-125`: keep adapter mode as the default no-argument path, but route it through the new transport readiness and discovery flow.
- Modify `Cargo.toml:36` and `Cargo.lock`: make the `rmcp` version decision explicit before relying on Streamable HTTP behaviors from recent SDK releases.
- Modify `src/adapter/mod.rs:81-405`: make stdio remain a thin proxy. Replace or abstract `connect_and_handshake` so it can connect to the canonical HTTP transport while preserving current retry and immediate-disconnect behavior.
- Modify `src/adapter/launcher.rs:33-296`: update readiness probing and port discovery for HTTP while preserving daemon launch serialization through `daemon.lock`.
- Modify `src/daemon/mod.rs:276-733`: bind the HTTP MCP transport alongside or instead of the legacy IPC listener during the transition. Keep lifecycle and cleanup ordering intact.
- Modify `src/daemon/ipc.rs:1-251`: keep legacy IPC types until compatibility tests prove the shim no longer needs them. Do not delete platform IPC in the first transport change.
- Modify `src/daemon/ipc_session.rs:245-400`: extract stream-independent session construction so HTTP and IPC share `JulieServerHandler`, `WorkspacePool`, `EmbeddingService`, restart flag, dashboard events, startup hint, and watcher pool wiring.
- Create `src/daemon/http_transport.rs`: own localhost binding, port file publication, Origin validation, token validation, MCP route construction, and server shutdown handle.
- Modify `src/daemon/transport.rs:1-80`: evolve `TransportEndpoint` into a discovery/probe abstraction that can represent HTTP canonical transport and legacy IPC during migration.
- Modify `src/dashboard/mod.rs:123-186` only if the dashboard router and MCP HTTP router need to be mounted on the same Axum server. Prefer separate routers if security policy differs.
- Test `src/tests/daemon/transport.rs`: add HTTP probe and port-discovery tests.
- Test `src/tests/adapter/retry.rs`: preserve retry behavior through the stdio shim.
- Test `src/tests/integration/daemon_lifecycle.rs`: add one daemon-level HTTP readiness test if unit tests cannot prove port publication and shutdown cleanup.
- Create `src/tests/daemon/http_transport.rs`: test localhost bind, Origin rejection, token rejection or acceptance, and workspace startup hint propagation.

## Implementation Tasks

### Task 0: SDK Capability Audit and Version Lock

**Files:**
- Modify: `Cargo.toml:36`
- Modify: `Cargo.lock`
- Test: `src/tests/daemon/http_transport.rs` after Task 2 creates it, or `src/tests/daemon/transport.rs` for discovery-only SDK checks

**What to build:** Make the MCP Rust SDK version an intentional part of the transport work. Audit `rmcp` 1.6.0 APIs for Streamable HTTP server setup, Host validation, Origin validation, rejection logging, optional session store, resumability, `init_timeout`, HTTP/2 authority fallback, and client-side Streamable HTTP support.

**Approach:** Start by running `cargo update -p rmcp -p rmcp-macros` and inspect the resulting API changes before writing Julie transport code. If the upgrade is clean, raise the declared dependency from `1.2` to an explicit compatible target such as `1.6` so future agents stop reading a stale lower-bound as the intended version. If the upgrade exposes API churn, keep the lock at 1.5.0 for this batch and record exactly which plan items remain Julie-owned because the SDK feature is not available in the pinned version.

**Acceptance criteria:**
- [x] `cargo tree -i rmcp -e normal --depth 1` shows the intended SDK version.
- [x] The plan records which HTTP concerns are delegated to `rmcp` and which remain Julie-owned.
- [x] Existing MCP compile surface still builds after the version decision.
- [x] Worker-scope verification passes.

**Task 0 execution notes:**
- `rmcp` and `rmcp-macros` are intentionally locked to 1.6.0. `Cargo.toml` now declares `rmcp = "1.6"` instead of the stale `1.2` lower bound.
- Julie delegates SDK-covered Host validation, Origin validation, rejection logging, Streamable HTTP `init_timeout`, optional `SessionStore`, resumability hooks, and HTTP/2 `:authority` fallback to `rmcp` 1.6.0 where those APIs fit.
- Julie still owns daemon endpoint discovery, localhost-only policy, bearer-token generation/storage, adapter auth header injection, lifecycle cleanup, and compatibility with the stdio MCP process boundary.
- Enabling `transport-streamable-http-client` requires enabling the `client` feature. Without it, `rmcp` 1.6.0 fails to compile because the client Streamable HTTP module imports client-role types.

### Task 1: Transport Contract and Discovery

**Files:**
- Modify: `src/daemon/transport.rs:1-80`
- Modify: `src/paths.rs:80-151` only if a new HTTP MCP port or token path is needed instead of reusing `daemon_port`
- Test: `src/tests/daemon/transport.rs:16-64`

**What to build:** Define a transport endpoint contract that can discover and probe the canonical HTTP daemon endpoint. It should publish enough data for the adapter to connect without guessing: host, port, scheme, and token location or token value strategy.

**Approach:** Keep localhost binding explicit. Do not reuse dashboard port semantics blindly unless the same server owns both dashboard and MCP routes with different security middleware. If `daemon_port` currently means dashboard HTTP port, add a separate MCP transport state file or make the state file structured enough to avoid ambiguity.

**Acceptance criteria:**
- [x] Adapter can discover the HTTP endpoint without scanning ports.
- [x] Stale port files are rejected by an active readiness probe.
- [x] Existing IPC probe tests still pass during the migration.
- [x] Discovery data records the SDK transport mode and the auth material needed by the adapter, without exposing bearer tokens in logs.
- [x] Worker-scope verification passes.

**Task 1 execution notes:**
- `TransportEndpoint` now supports legacy IPC and `streamable_http` discovery documents. The document records scheme, host, port, MCP path, readiness path, and optional token path, not bearer token contents.
- `DaemonPaths` now has dedicated MCP transport state files: `daemon-mcp-transport.json` and `daemon-mcp.token`. These are intentionally separate from the dashboard `daemon.port` file.
- `DaemonLauncher` probes the discovered canonical transport when the PID is alive but `daemon.state` is missing or unreadable. If no structured transport discovery exists, it falls back to the legacy IPC endpoint for migration compatibility.

### Task 2: HTTP MCP Server Module

**Files:**
- Create: `src/daemon/http_transport.rs`
- Modify: `src/daemon/mod.rs:7-19` for module registration
- Modify: `src/daemon/mod.rs:276-733` for binding and cleanup
- Test: `src/tests/daemon/http_transport.rs`

**What to build:** Add a daemon-owned HTTP MCP server bound to `127.0.0.1` by default, with optional `::1` support only if tests prove equivalent local-only behavior on supported platforms. It should expose Streamable HTTP MCP using the `rmcp` transport enabled in `Cargo.toml`.

**Approach:** Use the SDK Streamable HTTP server primitives instead of hand-rolling MCP framing. Wire SDK-supported session `init_timeout`, keep-alive, and resumability controls explicitly so daemon shutdown and stale-session behavior is testable. Use the existing Axum/Tokio shape from `src/dashboard/mod.rs::create_router` where useful, but do not mix dashboard routes and MCP routes if that muddies security. Construct `JulieServerHandler` per MCP session with the same dependency set currently passed through `handle_ipc_session`.

**Acceptance criteria:**
- [x] Server binds only to localhost addresses, never `0.0.0.0` or external interfaces.
- [x] Port publication happens only after the listener is bound and ready.
- [x] SDK session timeout and resumability behavior is configured intentionally, not left as an accidental default.
- [x] Shutdown cleanup removes the HTTP transport discovery file and stops accepting new HTTP sessions.
- [x] Worker-scope verification passes.

**Task 2 execution notes:**
- `src/daemon/http_transport.rs` now owns the localhost Streamable HTTP server module. It mounts the SDK `StreamableHttpService` at `/mcp` and a Julie readiness route at `/mcp/ready`.
- The module waits for the readiness route before publishing `daemon-mcp-transport.json`, so adapters do not discover a port before the server can answer.
- The first integration point is a generic `Service<RoleServer>` factory. A real `HttpJulieService` wrapper now exists in `src/daemon/mcp_session.rs`; it lazily builds `JulieServerHandler` on `initialize` after reading HTTP request headers from `RequestContext.extensions`.
- The real wrapper is intentionally not exposed from daemon startup yet. HTTP sessions still need the same stale-binary and version gates that IPC currently applies before accepting a production session.
- The `transport` xtask bucket now includes `tests::daemon::http_transport`, so future diff-scoped transport gates actually run the HTTP transport tests.

### Task 3: HTTP Security Middleware

**Files:**
- Modify: `src/daemon/http_transport.rs`
- Test: `src/tests/daemon/http_transport.rs`

**What to build:** Enforce localhost HTTP security requirements: reject non-local binds, validate `Host` and `Origin` for browser-originated requests, and require an auth token if Streamable HTTP state-changing routes can be reached by generic local web content.

**Approach:** Prefer `rmcp` 1.6.0 Host and Origin validation over custom middleware, then add Julie middleware only for policy the SDK does not cover. Accept missing `Origin` for non-browser clients if token validation is present. Accept only Julie-owned origins such as the dashboard localhost origin when dashboard integration needs browser access. Generate a per-daemon token at startup, store it in a user-private state file if the adapter must read it, and require `Authorization: Bearer <token>` or an equivalent header for MCP requests.

**Acceptance criteria:**
- [x] Requests with invalid `Host` or HTTP/2 `:authority` values are rejected before MCP handling.
- [x] Requests with foreign `Origin` are rejected before MCP handling.
- [x] Rejection logs include enough detail to debug local configuration problems without logging bearer tokens.
- [x] Requests without a valid token are rejected when token mode is enabled.
- [x] Adapter requests include the token and pass.
- [x] Tests assert concrete status codes and do not merely check that requests fail.
- [x] Worker-scope verification passes.

**Task 3 execution notes:**
- The HTTP transport now configures SDK Host and Origin validation with loopback hosts and loopback origins for the bound port.
- Julie-owned bearer-token policy is implemented as Axum middleware. Token mode writes `daemon-mcp.token` with `0600` permissions on Unix and publishes only the token path in transport discovery.
- Token middleware currently covers `/mcp` and `/mcp/ready`, which keeps readiness probes honest because `TransportEndpoint::probe_readiness` already reads the token path and sends `Authorization: Bearer ...`.
- The stdio HTTP shim is not wired yet, so the "adapter requests include the token and pass" acceptance is represented by the valid-token initialize test at the HTTP module boundary. Task 4 still owns actual adapter header injection.

### Task 4: Stdio Shim Over HTTP

**Files:**
- Modify: `src/adapter/mod.rs:81-405`
- Modify: `src/adapter/launcher.rs:33-296`
- Modify: `src/main.rs:12-125`
- Test: `src/tests/adapter/retry.rs:12-159`
- Test: add adapter HTTP shim tests under `src/tests/adapter/`

**What to build:** Keep no-argument `julie-server` behavior as stdio MCP from the client perspective, but make it proxy to the daemon HTTP endpoint. The adapter should still ensure the daemon is ready, connect, complete any required handshake, and forward stdin/stdout bytes without changing the MCP client contract.

**Approach:** Preserve `run_adapter_with` as the retry harness if possible. Swap the concrete connector from `IpcConnector` to an HTTP stream client abstraction. Use the SDK client-side Streamable HTTP transport when it fits the stdio shim. Evaluate the SDK's Unix domain socket Streamable HTTP client for macOS and Linux as a safer local transport option, but keep TCP localhost as the cross-platform baseline until Windows has an equivalent tested path. If byte-for-byte stdio forwarding cannot map cleanly to the rmcp HTTP client transport, add a small adapter-side MCP service bridge and keep the stdio contract at the process boundary.

**Acceptance criteria:**
- [ ] `main` still calls adapter mode for default invocation.
- [ ] Existing retry tests remain meaningful and pass with the HTTP connector abstraction.
- [ ] Stdio clients still see JSON-RPC over stdin/stdout, not HTTP details.
- [ ] Worker-scope verification passes.

### Task 5: Legacy IPC Compatibility Window

**Files:**
- Modify: `src/daemon/ipc.rs:1-251`
- Modify: `src/daemon/ipc_session.rs:245-400`
- Modify: `src/daemon/mod.rs:740-1034`
- Test: `src/tests/integration/daemon_lifecycle.rs:406-454`
- Test: `src/tests/daemon/ipc_session.rs:104-406`

**What to build:** Keep legacy IPC working until the HTTP daemon transport and stdio shim are proven. Mark IPC as compatibility transport in code comments and tests, not as the preferred path.

**Approach:** Extract shared MCP session setup before deleting or bypassing IPC. A bad first step would be duplicating handler construction in HTTP and IPC, because session cleanup and workspace binding bugs already have substantial tests in `src/tests/daemon/ipc_session.rs`.

**Acceptance criteria:**
- [x] Existing IPC workspace header protocol still passes during migration.
- [x] Shared session setup has one path for workspace startup hint, session lifecycle cleanup, dashboard events, watcher pool, and restart flag.
- [ ] Compatibility path can be removed in a later plan without affecting HTTP module boundaries.
- [x] Worker-scope verification passes.

**Task 5 execution notes:**
- `src/daemon/mcp_session.rs` now owns shared daemon MCP session construction and cleanup. IPC and HTTP share `DaemonMcpSession::start` for workspace startup hints, `WorkspacePool`, `EmbeddingService`, restart flag, dashboard events, watcher pool, project log session lifecycle, and workspace resource detach.
- `src/daemon/ipc_session.rs` now keeps IPC-only header parsing, ready-line handshake, and stream serving. Handler construction and cleanup moved to the shared module.
- `HttpJulieService` implements `rmcp::Service<RoleServer>` directly. It creates a Julie daemon session tracker entry synchronously, reads `http::request::Parts` from `RequestContext.extensions` on `initialize`, rejects missing or invalid Julie workspace headers with JSON-RPC `-32602`, and delegates later requests/notifications to the cached `JulieServerHandler`.
- Cleanup for HTTP currently runs from `Drop` because `rmcp::Service<RoleServer>` has no explicit close hook. The SDK does call `SessionManager::close_session`, so the next daemon-wiring slice should either keep this tested Drop behavior or wrap `LocalSessionManager` if we need deterministic async cleanup tied to the SDK close call.
- `HttpJulieService` is tested but not yet constructed by production daemon startup. The dead-code annotations on the HTTP wrapper path are intentional until Task 4 wires the stdio shim and the daemon exposes canonical HTTP sessions under the same restart/version gates as IPC.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, current dependencies in `Cargo.toml`, and the MCP Rust SDK release notes for the selected `rmcp` version.

**Worker red/green scope:** Workers run only the exact test they add or modify, for example `cargo nextest run --lib test_http_transport_rejects_foreign_origin 2>&1 | tail -10`, `cargo nextest run --lib test_http_transport_requires_bearer_token 2>&1 | tail -10`, `cargo nextest run --lib test_transport_probe_reports_ready_for_live_http_endpoint 2>&1 | tail -10`, or `cargo nextest run --lib test_run_adapter_with_retries_connect_failure_without_fixed_sleep 2>&1 | tail -10`.

**Worker ceiling:** `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. Workers may run at most one RED and one GREEN command per fix cycle. Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test reliability`, or broad `cargo nextest run --lib`.

**Worker gate invariant:** Each worker report must state the concrete invariant proven, such as "foreign browser Origin is rejected with 403 before MCP session construction" or "adapter discovers a live HTTP endpoint and retries failed initial connection."

**Lead affected-change scope:** After a coherent batch, the lead runs `cargo xtask test changed`.

**Branch gate:** The lead runs `cargo xtask test dev` once before handoff.

**Replay/metric evidence:** No replay or metric evidence is required. Hard gates are security behavior tests, adapter compatibility tests, `changed`, and `dev`.

**Escalation triggers:** Run `cargo xtask test system` because daemon startup and adapter transport are changed. Run `cargo xtask test reliability` when shutdown, restart handoff, session drain, watcher lifecycle, daemon restart behavior, SDK session store behavior, or Streamable HTTP timeout behavior changes. If dashboard and MCP share one Axum server, add dashboard router tests and consider `system` mandatory even for small edits.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. For security tests, record the rejected request shape and expected status code. If the same HEAD already has a passing ledger entry for the required scope, reuse that evidence instead of rerunning the same expensive gate.

| Scope | Invariant | Command | Commit | Result | Time |
|-------|-----------|---------|--------|--------|------|
| worker-red-green | Streamable HTTP discovery records mode/path auth material without copying bearer token values. | `cargo nextest run --lib test_transport_discovery_round_trips_streamable_http_without_token_value 2>&1 \| tail -40` | `40932f23+dirty` | PASS, 1 test in 0.022s | 2026-05-03T22:04:26Z |
| worker-red-green | Live localhost HTTP readiness endpoint with token file probes as ready. | `cargo nextest run --lib test_transport_probe_reports_ready_for_live_http_endpoint 2>&1 \| tail -30` | `40932f23+dirty` | PASS, 1 test in 0.015s | 2026-05-03T22:04:26Z |
| worker-red-green | Stale HTTP discovery port is rejected by active readiness probe. | `cargo nextest run --lib test_transport_probe_rejects_stale_http_endpoint_state 2>&1 \| tail -30` | `40932f23+dirty` | PASS, 1 test in 0.013s | 2026-05-03T22:04:26Z |
| worker-red-green | MCP transport discovery and token files are separate from the dashboard port file. | `cargo nextest run --lib test_daemon_mcp_transport_paths_are_distinct_from_dashboard_port 2>&1 \| tail -30` | `40932f23+dirty` | PASS, 1 test in 0.019s | 2026-05-03T22:04:26Z |
| worker-red-green | Adapter treats live HTTP discovery as daemon-ready when state file is absent. | `cargo nextest run --lib test_readiness_ready_via_http_discovery_when_no_state_file 2>&1 \| tail -35` | `40932f23+dirty` | PASS, 1 test in 0.023s | 2026-05-03T22:04:26Z |
| worker-red-green | Adapter treats stale HTTP discovery as starting, not ready. | `cargo nextest run --lib test_readiness_starting_when_http_discovery_is_stale 2>&1 \| tail -35` | `40932f23+dirty` | PASS, 1 test in 0.014s | 2026-05-03T22:04:26Z |
| worker-red-green | Legacy IPC endpoint round trip still works after transport enum migration. | `cargo nextest run --lib test_transport_endpoint_round_trip 2>&1 \| tail -30` | `40932f23+dirty` | PASS, 1 test in 0.015s | 2026-05-03T22:04:26Z |
| worker-red-green | Legacy stale Unix socket probe still rejects stale socket files. | `cargo nextest run --lib test_transport_probe_rejects_stale_socket_file 2>&1 \| tail -30` | `40932f23+dirty` | PASS, 1 test in 0.011s | 2026-05-03T22:04:26Z |
| affected-change | Diff-scoped batch gate for rmcp version lock plus transport discovery/probe changes. | `cargo xtask test changed 2>&1 \| tail -80` | `40932f23+dirty` | PASS, 22 buckets in 371.9s | 2026-05-03T22:04:26Z |
| expensive-specialist | System gate for adapter transport readiness and daemon lifecycle touch points. | `cargo xtask test system 2>&1 \| tail -80` | `40932f23+dirty` | PASS, 6 buckets in 80.8s | 2026-05-03T22:04:26Z |
| worker-red-green | Stale HTTP discovery does not block legacy IPC readiness fallback during migration. | `cargo nextest run --lib test_readiness_ready_via_ipc_when_http_discovery_is_stale 2>&1 \| tail -30` | `40932f23+dirty` | PASS, 1 test in 0.022s | 2026-05-03T22:08:55Z |
| affected-change | Focused post-review transport bucket after IPC fallback correction. | `cargo xtask test bucket transport 2>&1 \| tail -60` | `40932f23+dirty` | PASS, 1 bucket in 4.0s | 2026-05-03T22:08:55Z |
| worker-red-green | HTTP transport binds loopback, publishes discovery only after readiness, and cleans up discovery on shutdown. | `cargo nextest run --lib test_http_transport_binds_loopback_publishes_discovery_and_cleans_up 2>&1 \| tail -60` | `26f216da+dirty` | PASS, 1 test in 0.032s | 2026-05-03T22:20:05Z |
| worker-red-green | HTTP transport rejects non-loopback bind hosts and does not publish failed discovery. | `cargo nextest run --lib test_http_transport_rejects_non_loopback_bind_host 2>&1 \| tail -30` | `26f216da+dirty` | PASS, 1 test in 0.014s | 2026-05-03T22:20:05Z |
| worker-red-green | HTTP transport session policy explicitly sets SDK init timeout, keepalive, SSE retry, and route paths. | `cargo nextest run --lib test_http_transport_config_sets_sdk_session_policy_intentionally 2>&1 \| tail -30` | `26f216da+dirty` | PASS, 1 test in 0.012s | 2026-05-03T22:20:05Z |
| worker-red-green | HTTP transport accepts an MCP `initialize` POST through the mounted `rmcp` Streamable HTTP service. | `cargo nextest run --lib test_http_transport_accepts_mcp_initialize_request 2>&1 \| tail -30` | `26f216da+dirty` | PASS, 1 test in 0.032s | 2026-05-03T22:20:05Z |
| worker-red-green | Xtask manifest contract includes HTTP transport tests in the transport bucket. | `cargo nextest run --package xtask manifest_contract_tests_checked_in_manifest_uses_exact_bucket_specs 2>&1 \| tail -40` | `26f216da+dirty` | PASS, 1 test in 0.010s | 2026-05-03T22:20:05Z |
| affected-change | Focused transport bucket includes adapter, IPC, and HTTP transport tests. | `cargo xtask test bucket transport 2>&1 \| tail -70` | `26f216da+dirty` | PASS, 1 bucket in 4.4s | 2026-05-03T22:20:05Z |
| worker-red-green | Token mode rejects missing bearer tokens with 401 before MCP session creation. | `cargo nextest run --lib test_http_transport_requires_bearer_token_for_mcp_requests 2>&1 \| tail -70` | `d4b57a6f+dirty` | PASS, 1 test in 0.029s | 2026-05-03T22:26:17Z |
| worker-red-green | Token mode accepts valid bearer token and discovery does not copy token value. | `cargo nextest run --lib test_http_transport_accepts_valid_bearer_token 2>&1 \| tail -30` | `d4b57a6f+dirty` | PASS, 1 test in 0.034s | 2026-05-03T22:26:17Z |
| worker-red-green | Token mode rejects wrong bearer token with 401. | `cargo nextest run --lib test_http_transport_rejects_invalid_bearer_token 2>&1 \| tail -35` | `d4b57a6f+dirty` | PASS, 1 test in 0.014s | 2026-05-03T22:26:17Z |
| worker-red-green | SDK Host validation rejects invalid Host with 403 before MCP handling. | `cargo nextest run --lib test_http_transport_rejects_invalid_host_header 2>&1 \| tail -45` | `d4b57a6f+dirty` | PASS, 1 test in 0.013s | 2026-05-03T22:26:17Z |
| worker-red-green | SDK Origin validation rejects foreign Origin with 403 before MCP handling. | `cargo nextest run --lib test_http_transport_rejects_foreign_origin 2>&1 \| tail -45` | `d4b57a6f+dirty` | PASS, 1 test in 0.013s | 2026-05-03T22:26:17Z |
| affected-change | Focused transport bucket after HTTP security middleware. | `cargo xtask test bucket transport 2>&1 \| tail -70` | `d4b57a6f+dirty` | PASS, 1 bucket in 4.3s | 2026-05-03T22:26:17Z |
| worker-red-green | Missing HTTP Julie workspace header returns JSON-RPC invalid params and removes the daemon session tracker entry. | `cargo nextest run --lib test_http_julie_session_requires_workspace_header_before_initialize 2>&1 \| tail -50` | `c15f2f69+dirty` | PASS, 1 test in 0.054s | 2026-05-03T22:52:10Z |
| worker-red-green | Valid HTTP Julie workspace headers attach the workspace and DELETE cleans up session count plus daemon tracker state. | `cargo nextest run --lib test_http_julie_session_uses_workspace_headers_and_cleans_up_on_delete 2>&1 \| tail -50` | `c15f2f69+dirty` | PASS, 1 test in 0.103s | 2026-05-03T22:52:10Z |
| worker-red-green | Invalid HTTP Julie workspace source header returns JSON-RPC invalid params and removes the daemon session tracker entry. | `cargo nextest run --lib test_http_julie_session_rejects_invalid_workspace_source_header 2>&1 \| tail -80` | `c15f2f69+dirty` | PASS, 1 test in 0.041s | 2026-05-03T22:53:00Z |
| worker-red-green | Existing IPC malformed-stream cleanup still detaches secondary workspace resources after shared session extraction. | `cargo nextest run --lib test_handle_ipc_session_cleans_up_secondary_workspaces_on_serve_error 2>&1 \| tail -80` | `c15f2f69+dirty` | PASS, 1 test in 0.111s | 2026-05-03T22:52:10Z |
| worker-red-green | Existing IPC weak-CWD startup still avoids pre-binding the startup workspace after shared session extraction. | `cargo nextest run --lib test_handle_ipc_session_weak_cwd_startup_is_not_attached_before_first_bind 2>&1 \| tail -80` | `c15f2f69+dirty` | PASS, 1 test in 0.120s | 2026-05-03T22:52:10Z |
| worker-red-green | HTTP session wrapper and shared IPC session extraction compile without warnings. | `cargo check 2>&1 \| tail -60` | `c15f2f69+dirty` | PASS, no warnings | 2026-05-03T22:54:10Z |
| affected-change | Focused transport bucket after shared daemon MCP session wrapper extraction. | `cargo xtask test bucket transport 2>&1 \| tail -80` | `c15f2f69+dirty` | PASS, 1 bucket in 4.6s | 2026-05-03T22:54:10Z |

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** SDK version selection, Streamable HTTP protocol compatibility, Host or Origin policy, bearer-token policy, session timeout or resumability behavior, stdio shim compatibility, and daemon lifecycle integration are strategy or coupled implementation work. Use Codex `gpt-5.5 high` for lead-owned SDK/security decisions, `gpt-5.3-codex high` for bounded transport implementation, and `gpt-5.3-codex xhigh` for auth, restart, shutdown, or terminal-heavy debugging.

**Worker eligibility:** Use implementation-tier workers for isolated tests, local probe logic, and adapter retry tests. Use coupled implementation or lead-owned work for HTTP session construction, stdio compatibility, daemon lifecycle integration, Origin/token policy, and anything touching restart or shutdown.

**Escalation triggers:** Escalate for security policy ambiguity, failure to map stdio byte forwarding to Streamable HTTP cleanly, repeated worker failure, shared server/dashboard coupling, or any transport behavior not covered by strong tests.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates. Split docs-only edits from evidence interpretation.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Lead-owned lane: decide the canonical HTTP contract, security policy, and whether dashboard and MCP share an Axum server. This is architecture and security work, not a rote edit.
- Lead-owned lane: audit and intentionally select the `rmcp` version before assigning HTTP implementation work.
- Worker lane A: add transport discovery and probe tests with a bounded write scope in `src/daemon/transport.rs` and `src/tests/daemon/transport.rs`.
- Worker lane B: add HTTP security tests first, then implement middleware in `src/daemon/http_transport.rs`.
- Worker lane C: adapt stdio shim retry tests around an HTTP connector abstraction, keeping `run_adapter_with` behavior stable.
- Worker lane D: extract shared session setup from `ipc_session.rs` only after HTTP tests define the expected dependency wiring.
- Lead integration lane: run changed/dev gates, then system and reliability gates when the actual diff touches startup, restart, or shutdown.

## Risks

- Localhost HTTP is not automatically safe. Browser-originated requests can hit localhost, so Origin validation and token checks are required unless the implementation proves no state-changing route is exposed.
- The latest relevant SDK release is `rmcp` 1.6.0, but the current lockfile resolves 1.5.0. Depending on 1.6.0 behavior before the lock is upgraded would be the same old "the model thought it was true" problem wearing a nicer hat.
- `daemon_port` currently appears tied to dashboard HTTP. Reusing it for MCP without disambiguation would make adapter discovery brittle.
- SDK Host and Origin validation reduce custom code, but they do not replace Julie's process-local token policy or daemon discovery contract.
- Streamable HTTP is not byte-for-byte equivalent to the current IPC stream. If the adapter tries to blindly pipe stdio bytes into HTTP without using `rmcp` transport semantics, it will probably be wrong.
- Sharing one Axum server with the dashboard may be convenient, but it couples public-ish dashboard browsing behavior to MCP security. Separate routers or middleware boundaries are safer.
- Deleting IPC in the first pass is a bad idea. The current IPC tests cover workspace header and session cleanup behavior that HTTP should learn from before compatibility is removed.
