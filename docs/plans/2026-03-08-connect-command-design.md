# Design: `julie-server connect` Command

**Date:** 2026-03-08
**Status:** Approved
**Path:** Lightweight (moderate task, same-session)

## Problem

Starting Julie in daemon mode requires manually running `julie-server daemon start --foreground` before the MCP session. There's no way for an MCP client to auto-start the daemon — the `.mcp.json` `command` type expects a stdio process, not a pre-running HTTP server.

## Solution

A new `julie-server connect` command that:
1. Ensures the daemon is running (starts it if not)
2. Bridges stdio ↔ HTTP, so the MCP client talks stdio while Julie runs as a persistent daemon

This lets `.mcp.json` use `command` type with `julie-server connect`, and the daemon survives session exits.

## Design

### CLI Changes (`src/cli.rs`)

Add `Connect` variant to `Commands` enum:

```rust
pub enum Commands {
    Daemon { action: DaemonAction },
    /// Connect to daemon (auto-starts if needed), bridging stdio ↔ HTTP
    Connect {
        #[arg(long, default_value = "7890", env = "JULIE_PORT")]
        port: u16,
    },
}
```

### Main Dispatch (`src/main.rs`)

Add match arm:
```rust
Some(Commands::Connect { port }) => {
    connect::run_connect(port, workspace_root).await
}
```

### Connect Module (`src/connect.rs`)

Core logic:

```
run_connect(port, workspace_root):
  1. Check if daemon is running (pid_file_path + is_daemon_running)
  2. If not running → spawn daemon as background child process
     - Command: current_exe() with args ["daemon", "start", "--port", port, "--foreground"]
     - Detach from parent (don't inherit stdin/stdout — those are for MCP)
     - Poll /api/health until ready (max ~5s with backoff)
  3. If running → reuse existing daemon
  4. Register this workspace with daemon (POST /api/projects)
  5. Bridge stdio ↔ HTTP:
     - Read JSON-RPC from stdin → POST to http://localhost:{port}/mcp
     - Read responses from HTTP → write to stdout
  6. On bridge error or stdin EOF → exit (daemon keeps running)
```

### Background Daemon Spawn

The daemon is spawned as a detached child process using `std::process::Command`:
- Redirect stdout/stderr to log files (not inherited — stdin/stdout belong to MCP bridge)
- The child runs `daemon start --foreground` (foreground from child's perspective, but detached from parent)
- Parent polls `GET /api/health` with exponential backoff (50ms, 100ms, 200ms, 400ms, 800ms, 1600ms, 2000ms) — ~5s max wait

### Stdio ↔ HTTP Bridge

The MCP protocol uses Streamable HTTP transport at `/mcp`. The bridge:
- Reads newline-delimited JSON-RPC messages from stdin
- POSTs each to `http://localhost:{port}/mcp` with `Content-Type: application/json`
- Writes response body to stdout
- Handles SSE streaming responses (server-sent events from `/mcp`)

### Fallback Behavior

If the daemon can't start (port busy, crash, timeout):
- Log the error
- Fall back to `run_stdio_mode(workspace_root)` — direct stdio MCP, no daemon
- This ensures MCP sessions never fail to start

### MCP Client Configuration

```json
{
  "mcpServers": {
    "julie": {
      "command": "julie-server",
      "args": ["connect"],
      "env": { "JULIE_WORKSPACE": "/path/to/project" }
    }
  }
}
```

## Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `src/cli.rs` | Modify | Add `Connect` variant to `Commands` |
| `src/connect.rs` | Create | Daemon ensure + stdio↔HTTP bridge |
| `src/main.rs` | Modify | Add `Connect` dispatch + import |
| `src/lib.rs` | Modify | Add `pub mod connect;` |

## Acceptance Criteria

- [ ] `julie-server connect` starts daemon if not running
- [ ] `julie-server connect` reuses existing daemon if already running
- [ ] Stdio↔HTTP bridge passes MCP JSON-RPC messages correctly
- [ ] MCP client can connect via `command` type in `.mcp.json`
- [ ] Daemon survives session exit (bridge exits, daemon stays)
- [ ] Falls back to stdio mode if daemon can't start
- [ ] Workspace auto-registered with daemon on connect
- [ ] Tests for daemon-ensure logic and bridge basics

## Key Decisions

1. **Spawn via `std::process::Command`, not `fork()`** — cross-platform, simpler
2. **Poll `/api/health` for readiness** — avoids race conditions vs. fixed sleep
3. **Fallback to stdio** — guarantees MCP sessions always work, even if daemon is broken
4. **Bridge is thin** — just proxies bytes, no MCP protocol awareness needed beyond framing
