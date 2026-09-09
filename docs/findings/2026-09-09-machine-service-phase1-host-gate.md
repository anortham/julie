# Finding: Phase 1 Three-Host Gate (Claude Code, Codex, Cursor)

**Date**: 2026-09-09  
**Binary**: `target/release/julie-server` (v7.18.1, commit `7922e1b1` + host gate fixes)  
**Host Environment**: Linux x86_64  

---

## Verdict: GATE PASSED

Stateless MCP 2026-07-28 over Streamable HTTP and the stdio byte-forwarding shim have **passed across all three target hosts** (Claude Code, OpenAI Codex, and Cursor Agent).

- **Zero session state required**: No host required persistent `Mcp-Session-Id` tracking or server-held session memory. `NeverSessionManager` handled all requests statelessly.
- **Zero Tasks extension required**: All tool calls completed synchronously within standard client timeouts without needing the MCP Tasks proposal.
- **Stdio shim transparently connects**: Every host successfully connected to the running service via the default no-argument stdio shim, forwarding JSON-RPC requests to the HTTP service over localhost.

---

## Host Gate Matrix

| Host | Transport | Protocol Version Negotiated | `tools/list` Count | `manage_workspace list` | `fast_search` | `get_symbols` | Overall Result |
|---|---|---|---|---|---|---|---|
| **Claude Code** | Streamable HTTP | `2026-07-28` | 13 tools | PASS | PASS | PASS | **PASS** |
| **Claude Code** | Stdio Shim | `2025-11-25` | 13 tools | PASS | PASS | PASS | **PASS** |
| **OpenAI Codex** | Streamable HTTP | `2026-07-28` | 13 tools | PASS | PASS | PASS | **PASS** |
| **OpenAI Codex** | Stdio Shim | `2026-07-28` | 13 tools | PASS | PASS | PASS | **PASS** |
| **Cursor Agent** | Streamable HTTP | `2026-07-28` | 13 tools | PASS | PASS | PASS | **PASS** |
| **Cursor Agent** | Stdio Shim | `2026-07-28` | 13 tools | PASS | PASS | PASS | **PASS** |

---

## Detailed Run Evidence

### 1. Claude Code

#### 1.1 Streamable HTTP (`julie-http-test`)
- **Configuration**:
  ```bash
  claude mcp add --transport http julie-http-test "http://127.0.0.1:37525/mcp?token=<token>"
  ```
- **Handshake & Protocol Version**:
  - Claude Code connected via HTTP POST to `/mcp` with protocol version `2026-07-28`.
  - Initial `tools/list` response required `ttlMs` and `cacheScope` fields per MCP 2026-07-28 spec. `McpAdapter::list_tools` was updated to provide `.with_ttl_ms(0).with_cache_scope(rmcp::model::CacheScope::Private)`.
  - 13 tools successfully discovered.
- **Tool Invocations**:
  1. `manage_workspace(operation="list")`:
     - Returned registered workspaces: `julie` (`julie_5cb3ea69`) with 1368 files, 71599 symbols, status `ready`.
  2. `fast_search(query="RequestEngine", workspace="/home/murphy/source/julie")`:
     - Returned 6 matches including definition at `src/request_engine/dispatch.rs:15`.
  3. `get_symbols(file_path="src/lib.rs", workspace="/home/murphy/source/julie")`:
     - Returned 31 symbols from `src/lib.rs`.

#### 1.2 Stdio Shim (`julie-stdio-test`)
- **Configuration**:
  ```bash
  claude mcp add julie-stdio-test /home/murphy/source/julie/.worktrees/machine-service/target/release/julie-server
  ```
- **Handshake & Protocol Version**:
  - Claude Code's stdio client initiated the handshake with `params.protocolVersion = "2025-11-25"`.
  - The stdio shim read newline-delimited JSON-RPC from stdin and forwarded it to `http://127.0.0.1:37525/mcp`.
  - Service `default_mcp_headers` dynamically inspects the request body to mirror `params.protocolVersion` if provided, allowing smooth backwards-compatible negotiation with `2025-11-25` clients while serving `2026-07-28` by default.
- **Tool Invocations**:
  - `manage_workspace list`: Success.
  - `fast_search`: Success.
  - `get_symbols`: Success.

---

### 2. OpenAI Codex

#### 2.1 Streamable HTTP (`julie-http-test`)
- **Configuration**:
  ```bash
  codex mcp add julie-http-test --url "http://127.0.0.1:37525/mcp?token=<token>"
  ```
- **Handshake & Protocol Version**:
  - Codex negotiated `2026-07-28` Streamable HTTP.
  - Listed all 13 MCP tools: `blast_radius`, `call_path`, `deep_dive`, `edit_file`, `fast_refs`, `fast_search`, `get_context`, `get_symbols`, `manage_workspace`, `patterns`, `rename_symbol`, `rewrite_symbol`, `spillover_get`.
- **Tool Invocations** (run non-interactively via `codex exec`):
  ```text
  - manage_workspace(list) succeeded: julie_5cb3ea69 is ready with 1,368 files and 71,599 symbols; tmp_e9671acd is ready but empty.
  - fast_search("RequestEngine") succeeded: definition at src/request_engine/dispatch.rs:15, plus five other matches.
  - get_symbols("src/lib.rs") succeeded: 31 symbols, including module declarations and public re-exports.
  ```

#### 2.2 Stdio Shim (`julie-stdio-test`)
- **Configuration**:
  ```bash
  codex mcp add julie-stdio-test -- /home/murphy/source/julie/.worktrees/machine-service/target/release/julie-server
  ```
- **Handshake & Protocol Version**:
  - Stdio process spawned `julie-server`, which connected to the running service daemon via discovery file `~/.julie/service.json`.
  - Full handshake completed cleanly over stdin/stdout.
- **Tool Invocations**:
  ```text
  - manage_workspace(list) succeeded: julie_5cb3ea69 has 1,368 files and 71,599 symbols; tmp_e9671acd is empty. Both report ready.
  - fast_search("RequestEngine") succeeded: definition at src/request_engine/dispatch.rs:15, plus five other matches.
  - get_symbols("src/lib.rs") succeeded: 31 symbols, including module declarations and public re-exports.
  ```

---

### 3. Cursor Agent

#### 3.1 Streamable HTTP (`julie-http-test`)
- **Configuration** (`~/.cursor/mcp.json`):
  ```json
  {
    "mcpServers": {
      "julie-http-test": {
        "url": "http://127.0.0.1:37525/mcp?token=<token>"
      }
    }
  }
  ```
- **Discovery**:
  - `agent mcp list` reported `julie-http-test: ready`.
  - `agent mcp list-tools julie-http-test` returned 13 tools with full schemas.
- **Tool Invocations** (run via `agent --print --trust -f --approve-mcps`):
  ```text
  1. manage_workspace(operation='list')
  - julie (julie_5cb3ea69) — /home/murphy/source/julie — ready, 1368 files, 71599 symbols
  - tmp (tmp_e9671acd) — /tmp — ready, 0 files, 0 symbols

  2. fast_search(query='RequestEngine', workspace='/home/murphy/source/julie')
  - Definition: src/request_engine/dispatch.rs:15 — pub struct RequestEngine
  - Other hits: mod.rs re-export, mcp_adapter.rs import, handler.rs:2774 accessor, edit_recovery_contract.rs test field, src/tests/request_engine.rs

  3. get_symbols(file_path='src/lib.rs', workspace='/home/murphy/source/julie')
  - 31 symbols — mostly pub mod crates (cli, handler, request_engine, tools, …) plus re-exports (julie_index, julie_core, julie_runtime, extractors, workspace types)
  ```

#### 3.2 Stdio Shim (`julie-stdio-test`)
- **Configuration** (`~/.cursor/mcp.json`):
  ```json
  {
    "mcpServers": {
      "julie-stdio-test": {
        "type": "stdio",
        "command": "/home/murphy/source/julie/.worktrees/machine-service/target/release/julie-server",
        "args": []
      }
    }
  }
  ```
- **Discovery**:
  - `agent mcp list` reported `julie-stdio-test: ready`.
  - `agent mcp list-tools julie-stdio-test` returned 13 tools.
- **Tool Invocations**:
  - All 3 tool calls succeeded identically to HTTP.

---

## Protocol Learnings & Adjustments Made

1. **MCP 2026-07-28 Cache Metadata**:
   - Claude Code strictly checks `ttlMs: number` on `tools/list` responses when negotiating 2026-07-28. Without it, Claude Code rejects the list with `-32600`.
   - Adding `.with_ttl_ms(0).with_cache_scope(rmcp::model::CacheScope::Private)` in `src/handler/mcp_adapter.rs` satisfies 2026-07-28 compliance while maintaining compatibility with older clients.

2. **Handshake Protocol Negotiation**:
   - The MCP stdio implementation in Claude Code defaults to client version `2025-11-25`.
   - `rmcp` requires that the `mcp-protocol-version` HTTP header match `params.protocolVersion` during the `initialize` handshake.
   - The HTTP router's `default_mcp_headers` middleware extracts `params.protocolVersion` from the body if present on `initialize`, avoiding header mismatch rejections.

3. **Workspace Targeting without Process Context**:
   - In the long-running service, `BindingResolver` is constructed without a fixed `process_workspace`.
   - When tools like `fast_search` or `get_symbols` are invoked, clients should pass `workspace: "<path_or_id>"` unless an active workspace is explicitly bound. If omitted and default is `"primary"`, Julie correctly errors with `WORKSPACE_REQUIRED`.
