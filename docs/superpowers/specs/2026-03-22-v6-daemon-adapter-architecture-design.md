# Julie v6: Daemon + Adapter Architecture

## Summary

Evolve Julie from a per-session stdio process to a persistent local daemon with thin stdio adapters. One daemon owns all expensive resources (indexes, VRAM, file watchers); multiple MCP clients connect through adapters or direct HTTP. Centralized index storage eliminates duplication. Phased delivery ensures each step ships independently.

## Problem

The MCP ecosystem has evolved. Agent teams, git worktree agents, and parallel development sessions are now standard. Julie's current per-process architecture breaks under concurrency:

- **Tantivy writer lock**: Only one process can write to an index. Second Julie instance gets `LockBusy` on startup. Agent teams are blocked entirely.
- **VRAM multiplication**: Each process loads its own embedding model (~270MB for Jina-Code-v2). 3 sessions = 810MB+ for the same model.
- **Redundant file watchers**: N sessions on the same project = N watchers on the same directory.
- **Duplicate reference indexes**: Adding projectB as a reference from projectA re-indexes projectB entirely into projectA's `.julie/`. Three projects referencing the same library = 4 copies of its index.

These aren't hypothetical. The user already runs 3+ Claude Code sessions concurrently across projects and wants to adopt agent teams for parallel within-project work.

## Prior Art & Research

### v4.x Retrospective (Julie's own history)

Julie v4.0.0-v4.2.3 attempted to solve this with a daemon + HTTP + dashboard + tray app + federation + memory tools. Added ~49,000 lines across 202 files. Collapsed under its own weight: 29+ fix commits for daemon startup races, connect bridge death spirals, Windows compat, lock ordering deadlocks. Ripped out in v5.0.0 ("The Great Simplification", 23,300 lines deleted).

**Key lesson:** The goal was right; the scope was wrong. A circular dependency of features justifying each other ("tray app manages daemon, daemon serves HTTP, HTTP serves dashboard") consumed all effort on infrastructure and delivered zero user value.

**What we keep from v4:** The `JulieServerHandler` already derives `Clone` with `Arc`-wrapped state. Multi-session ready at the handler level. The embedding pipeline, indexing system, and all tools are session-agnostic.

### LSP Ecosystem Precedent

No LSP server solved multi-client at the protocol level. The converged answer:

- **ra-multiplex (lspmux)**: Persistent daemon on TCP, thin stdio adapters that editors spawn. Daemon manages one rust-analyzer per workspace, reuses across clients. Request IDs rewritten for session routing. Auto-GC on idle.
- **clangd**: Layered index architecture with optional remote index server for large codebases. Each editor still gets its own process, but the index is shared on disk.
- **TypeScript/tsserver**: No multi-client support. Per-editor instance.

### MCP Transport Research

- **Streamable HTTP** is the current MCP spec recommendation (March 2025). SSE deprecated.
- **All 7 major MCP clients support HTTP**: Claude Code, VS Code/Copilot, Cursor, Codex CLI, Windsurf, Gemini CLI, OpenCode.
- **All 7 also support stdio**: Universal baseline, zero-config.
- **rmcp crate** (Julie's MCP library) has `transport-streamable-http-server` feature. Integrates with axum via `StreamableHttpService`. Feature flag + axum dependency = HTTP transport.
- **VS Code ships Unix domain socket support** for MCP servers (local IPC, no port conflicts).

### MCP Multi-Client Patterns

- **mock-mcp**: Daemon per project via IPC (Unix domain sockets / named pipes). Adapters connect to daemon. Claim-based concurrency.
- **MCP Gateway pattern**: Intermediary proxy between clients and server(s). Session affinity, load balancing. Overkill for local use.
- **Supergateway**: Wraps any stdio MCP server in HTTP. Simple bridge, no shared state.

## Architecture

### Single Binary, Multiple Modes

```
julie-server              # adapter mode (default, what MCP clients spawn)
julie-server daemon       # daemon mode (persistent, owns all resources)
julie-server restart      # signal daemon to reload (dev cycle)
julie-server stop         # graceful shutdown
julie-server status       # health check
```

### System Topology

```
                                         +---------------------+
 Claude Code --stdio--> Adapter --IPC--> |                     |
 Claude Code --stdio--> Adapter --IPC--> |   Julie Daemon      |
 VS Code -----stdio--> Adapter --IPC--> |                     |
 Cursor ------http------------------------> |   ~/.julie/         |
 Gemini ------http------------------------> |   +-- indexes/      |
                                         |   +-- daemon.log    |
                                         |   +-- daemon.sock   |
                                         |   +-- registry.json |
                                         +---------------------+
                                                   |
                                          writes project logs to
                                                   |
                                     +-------------+-------------+
                                     v             v             v
                               projectA/     projectB/     projectC/
                               .julie/logs/  .julie/logs/  .julie/logs/
```

### Adapter Mode (Default)

The adapter is what MCP clients spawn. It's a pure byte-forwarding proxy between stdin/stdout and the daemon's IPC socket. It does not parse, rewrite, or inspect JSON-RPC messages. Each adapter gets its own dedicated socket connection; the daemon accepts multiple connections and creates a `JulieServerHandler` per connection. No session multiplexing, no request ID rewriting.

**Startup sequence with concurrent-safe daemon launch:**

1. Acquire advisory file lock on `~/.julie/daemon.lock` (using `fs2::FileLock` / `LockFile` on Windows). This lock is held only for the duration of the check-and-spawn sequence (steps 1-6), not for the adapter's lifetime. It serializes concurrent adapters trying to start the daemon simultaneously.
2. Check `~/.julie/daemon.pid`: validate PID is alive (kill -0 on Unix, OpenProcess on Windows)
3. If alive: release lock, connect to socket. Done.
4. If not alive (stale PID): delete stale PID file and socket.
5. Spawn `{current_exe} daemon` as detached background process.
6. Release lock (daemon will acquire its own lock on PID file during startup).
7. Wait for socket (poll with backoff: 50ms, 100ms, 200ms, max 2s total).
8. Connect to `~/.julie/daemon.sock` (Unix) or `\\.\pipe\julie-daemon` (Windows).
9. Forward bytes bidirectionally between stdin/stdout and the socket.
10. On socket disconnect: attempt reconnect with bounded retry (3 attempts, exponential backoff, 10s max). If all retries fail, re-run the startup sequence from step 1. If that also fails, surface error to MCP client and exit.

The adapter is stateless. All intelligence lives in the daemon.

### Daemon Mode

The daemon is a long-running process that owns:

| Resource | Scope | Sharing |
|----------|-------|---------|
| Tantivy search indexes | Per-workspace in `~/.julie/indexes/{id}/tantivy/` | Shared across all sessions referencing that workspace |
| SQLite databases | Per-workspace in `~/.julie/indexes/{id}/db/symbols.db` | Shared (WAL mode, concurrent reads) |
| File watchers | One per workspace | Shared; reference-counted by active sessions |
| Embedding model | One global (ORT or sidecar) | Shared; single VRAM footprint |
| Workspace registry | Global `~/.julie/registry.json` | Daemon-owned |

The daemon exposes two transports:
- **IPC** (Unix domain socket / named pipe): For adapters. Low latency, no network.
- **Streamable HTTP** (via rmcp + axum): For direct HTTP clients. Session management via `MCP-Session-Id`.

Both transports serve `JulieServerHandler` instances backed by shared workspace resources (see Handler Refactoring below).

### Storage Layout

```
~/.julie/
+-- indexes/
|   +-- julie_a1b2c3d4/
|   |   +-- db/symbols.db
|   |   +-- tantivy/
|   +-- labhandbook_e5f6g7h8/
|   |   +-- db/symbols.db
|   |   +-- tantivy/
|   +-- sharedlib_cafe1234/        # referenced by multiple projects
|       +-- db/symbols.db
|       +-- tantivy/
+-- daemon.sock                     # IPC endpoint (or named pipe on Windows)
+-- daemon.lock                     # advisory file lock for adapter startup serialization
+-- daemon.pid                      # PID file (atomic write)
+-- daemon.log                      # daemon lifecycle logs
+-- registry.json                   # workspace registry

{project}/.julie/
+-- logs/julie.log.{date}          # project-scoped logs (written by daemon)
+-- config.toml                     # optional per-project config overrides

Note: On first connection for a project, the daemon logs "Daemon logs at ~/.julie/daemon.log"
to the project log for discoverability. Project logs contain tool/indexing activity; daemon logs
contain lifecycle events (startup, shutdown, client connect/disconnect).
```

## Handler Refactoring for Shared Workspaces

### The Problem

The current `JulieServerHandler` has per-session state that won't survive daemon multiplexing:

```rust
pub struct JulieServerHandler {
    pub(crate) workspace_root: PathBuf,                     // per-session
    pub workspace: Arc<RwLock<Option<JulieWorkspace>>>,      // per-session (contains db, search_index, watcher)
    pub is_indexed: Arc<RwLock<bool>>,                       // per-session
    pub indexing_status: Arc<IndexingStatus>,                 // per-session
    pub session_metrics: Arc<SessionMetrics>,                 // per-session
    pub(crate) embedding_task: Arc<...>,                     // per-session
    tool_router: ToolRouter<Self>,                           // stateless
}
```

Each `Clone` shares the same `Arc`s (fine within a session), but each *new* session needs its own handler wired to *shared* workspace resources. The current `JulieWorkspace::initialize()` creates fresh db/search_index/watcher every time, which means the second session would create a second Tantivy writer and hit `LockBusy`.

### The Solution: WorkspacePool

Introduce a daemon-level `WorkspacePool` that maps workspace IDs to initialized, shared `JulieWorkspace` instances:

```rust
pub struct WorkspacePool {
    workspaces: RwLock<HashMap<String, Arc<JulieWorkspace>>>,
}

impl WorkspacePool {
    /// Get or initialize a workspace. First caller initializes; subsequent callers
    /// get the existing Arc. Thread-safe via RwLock.
    pub async fn get_or_init(&self, workspace_id: &str, path: &Path) -> Result<Arc<JulieWorkspace>> {
        // Fast path: read lock, check if exists
        // Slow path: write lock, double-check, initialize if still missing
    }
}
```

The daemon creates one `WorkspacePool` at startup. When a new session connects:

1. Resolve workspace path from MCP `initialize` roots or adapter cwd
2. Compute workspace ID via `generate_workspace_id`
3. Call `pool.get_or_init(id, path)` to get a shared `Arc<JulieWorkspace>`
4. Create a new `JulieServerHandler` with its own `session_metrics` and `indexing_status`, but pointing to the shared `JulieWorkspace`

This means `JulieServerHandler::new()` gains a new constructor:

```rust
impl JulieServerHandler {
    /// Create a handler for daemon mode, backed by shared workspace from the pool.
    pub fn new_with_shared_workspace(
        workspace: Arc<JulieWorkspace>,
        workspace_root: PathBuf,
    ) -> Self { ... }
}
```

### Auto-Indexing Guard

The current `on_initialized` handler triggers auto-indexing:

```rust
async fn on_initialized(&self, _context: NotificationContext<RoleServer>) {
    let handler = self.clone();
    tokio::spawn(async move { handler.run_auto_indexing().await; });
}
```

In daemon mode, the second session's `on_initialized` must not re-trigger indexing for an already-indexed workspace. The `WorkspacePool` tracks indexing state per workspace. The `on_initialized` implementation checks: if the workspace is already indexed (`is_indexed` flag on the shared `JulieWorkspace`), skip auto-indexing and just attach. If not yet indexed, only one session triggers indexing (using a `Once`-style guard or the existing `is_indexed` flag).

### Metrics in Daemon Mode

- **Per-session metrics** (`SessionMetrics`): Each handler gets its own. `query_metrics` returns the current session's metrics by default.
- **Daemon-level metrics**: The `WorkspacePool` can aggregate across sessions when `query_metrics` is called with a `scope: "daemon"` parameter (future enhancement, not Phase 1).

## Session Management

### Sessions

Each adapter connection or HTTP client creates a session in the daemon:

- **Session ID**: Generated on connect
- **Primary workspace**: Resolved from MCP `initialize` roots or adapter's cwd
- **Reference workspaces**: Added via `manage_workspace(operation="add")`
- **Active tool calls**: Tracked for graceful shutdown

Tool calls route based on the `workspace` parameter:
- `workspace: "primary"` -> session's primary workspace index
- `workspace: "{ref_id}"` -> that specific workspace's index in `~/.julie/indexes/`

### Workspace Registry

The daemon maintains a central registry of all known workspaces. The registry is held in memory (protected by `RwLock`) and flushed to `~/.julie/registry.json` on writes. All registry mutations are serialized through the `RwLock`; concurrent sessions cannot corrupt the registry.

```json
{
  "workspaces": {
    "julie_a1b2c3d4": {
      "path": "/Users/murphy/source/julie",
      "indexed_at": "2026-03-22T14:30:00Z",
      "status": "ready",
      "watcher_active": true,
      "session_count": 2
    }
  }
}
```

**Note on phasing:** Phase 1 uses the `WorkspacePool` (in-memory only) for workspace tracking. The persistent `registry.json` is introduced in Phase 2 when shared references need to survive daemon restarts. In Phase 1, workspace lookup is purely path-derived: compute workspace ID from the path, check if `~/.julie/indexes/{id}/` exists on disk.

When a session initializes:
1. Compute workspace ID from path (existing `generate_workspace_id`)
2. Check `WorkspacePool` (Phase 1) or registry (Phase 2): already indexed?
3. If yes: attach session, increment `session_count`. Instant.
4. If no: create index, start indexing, attach when ready.

### Reference Workspaces (Shared Indexes)

`manage_workspace(operation="add", path="/path/to/other/project")`:

1. Compute workspace ID for the reference path
2. Check registry: already indexed?
   - **Yes**: Register session as reader. Milliseconds.
   - **No**: Index into `~/.julie/indexes/{id}/`. Register when complete.
3. Session can query via `workspace="{id}"`

No more duplicate indexes. One copy per workspace, shared by all sessions.

### File Watchers

One watcher per workspace, reference-counted by sessions:

- Created when workspace is first indexed
- Stays alive while any session references the workspace
- Shut down after last session disconnects + grace period (5 minutes)
- Re-referenced within grace period: reuses existing watcher
- Watcher triggers incremental re-indexing into the central index

## Embedding Pipeline

### Single Model, Shared Queue

The daemon loads the embedding model once. All workspaces share it:

- **Embedding queue**: Workspace-tagged requests, serialized GPU access
- **Priority levels**: Force re-embed > auto-index
- **Cross-workspace fairness**: Round-robin between concurrent embedding requests
- **Cancellation**: Already supported; wired into the queue for force re-embed scenarios

### Sidecar Management

For Python sidecar (CodeRankEmbed):
- One sidecar process per daemon (not per session)
- Lifecycle tied to daemon
- Existing `uv`-based bootstrap unchanged
- `JULIE_EMBEDDING_SIDECAR_SCRIPT` env var still works for dev

### VRAM Impact

| Scenario | Before (per-process) | After (daemon) |
|----------|---------------------|----------------|
| 1 session | ~270MB | ~270MB |
| 3 sessions, same project | ~810MB | ~270MB |
| 3 sessions, 3 projects | ~810MB | ~270MB |
| 5 sessions + agent team | ~1350MB+ | ~270MB |

## Daemon Lifecycle

### Auto-Start

The daemon is never started manually by users. Adapters auto-start on first connection:

1. Check `~/.julie/daemon.pid` (validate PID is alive)
2. If alive: connect to socket
3. If not: spawn `{current_exe} daemon` detached, wait for socket

### Graceful Shutdown

- **Idle detection**: Daemon tracks active connections (both adapter IPC and HTTP sessions). A connection is "active" if it has an open socket (adapter) or has sent a request within the last 5 minutes (HTTP). When all connections are inactive, the 5-minute idle timer starts.
- Timer expires with no new connections -> commit Tantivy, close SQLite, remove socket + PID
- `julie-server stop` -> immediate graceful shutdown (same sequence)
- SIGTERM/SIGINT -> graceful shutdown
- Stale HTTP sessions (SSE connection open but no requests for >5 minutes) are counted as inactive and do not prevent shutdown.

### Crash Recovery

- Adapter detects dead socket -> clean up stale PID -> auto-restart daemon
- Tantivy indexes survive crashes (on-disk, commit-based)
- SQLite WAL mode survives crashes
- Only loss: uncommitted Tantivy writes (re-synced on next index)

### Dev Cycle

**On macOS/Linux:**
```bash
cargo build --release
julie-server restart      # daemon shuts down, adapters auto-restart new binary
```

The `restart` command sends a shutdown signal to the running daemon, then exits. The daemon shuts down gracefully (commit writers, close connections). The next tool call from any MCP client triggers the adapter's auto-start sequence, which spawns the freshly built binary as the new daemon. No need to exit Claude Code. Brief interruption (~1-2 seconds).

This is a stop/auto-restart cycle, not an in-place exec. Simpler, more reliable, works on all platforms.

**On Windows:**
Windows holds an exclusive file lock on the running `julie-server.exe`. You cannot `cargo build --release` while the daemon is running (linker error `LNK1104`). The workflow is:

```bash
julie-server stop         # shut down daemon, release binary lock
cargo build --release     # now the linker can write the new binary
                          # next tool call auto-starts the new daemon
```

This is the same constraint as v5 stdio mode (documented in CLAUDE.md), but slightly better: you only need to stop the daemon, not exit your entire Claude Code session. Adapters stay alive and reconnect when the new daemon starts.

**Future improvement:** Build to a staging path (`julie-server.new.exe`), then the restart command swaps binaries and restarts. This would eliminate the build-while-running limitation on Windows. Not in scope for Phase 1.

## Migration

### Index Migration (Phase 1)

On first daemon startup, detect existing per-project indexes and migrate safely:

1. Scan known workspace paths for `{project}/.julie/indexes/{id}/`
2. **Copy** (not move) to `~/.julie/indexes/{id}/`
3. Validate the copied index: open SQLite, verify symbol count > 0; open Tantivy, verify `meta.json` exists
4. On successful validation: delete the original `{project}/.julie/indexes/{id}/` directory
5. Write migration state per-workspace to `~/.julie/migration.json` tracking status (pending, copied, validated, cleaned)
6. Log migration to both daemon log and project log

If migration fails partway through (power loss, disk full), the state file allows resumption on next startup. Original indexes are not deleted until the copy is validated, so there is no data loss window.

If a v5 stdio process starts for a workspace that has been migrated, it won't find indexes in the project-local path and will re-create them. This is acceptable: the daemon will detect the duplicate on next startup and skip it (the central index is authoritative). To minimize this, the adapter should always be the entry point; v5 stdio mode becomes a fallback only.

### Configuration Migration

- Existing `.mcp.json` with `type: "stdio"` and `julie-server` command: works unchanged (adapter mode is default)
- No user action required for Phase 1

## Tool API

**Unchanged.** All 8 MCP tools (`fast_search`, `fast_refs`, `deep_dive`, `get_context`, `get_symbols`, `rename_symbol`, `manage_workspace`, `query_metrics`) keep their existing parameters and behavior. The `workspace` parameter still accepts `"primary"` or a workspace ID. Routing happens inside the daemon.

## Plugin Distribution

The `julie-plugin` repo bundles the same single binary:

```
julie-plugin/
+-- plugin.json                          # stdio MCP config (adapter mode)
+-- scripts/launch.sh (+launch.cmd)      # platform detection, exec binary
+-- bin/{platform}/julie-server           # per-platform binary (~25MB each)
+-- sidecar/...                          # Python embedding sidecar
+-- skills/...                           # search-debug, explore-area, etc.
```

`plugin.json` configures Julie as stdio:
```json
{
  "mcpServers": {
    "julie": {
      "type": "stdio",
      "command": "bash",
      "args": ["${CLAUDE_PLUGIN_ROOT}/scripts/launch.sh"]
    }
  }
}
```

Users install plugin, adapter auto-starts daemon. Agent teams, multiple sessions, all just work.

## Phased Delivery

### Phase 1: Daemon Core + Adapter

**Goal:** Multiple MCP clients on the same project without conflicts.

- `julie-server daemon` subcommand with Streamable HTTP + IPC
- `julie-server` adapter mode with concurrent-safe auto-start (advisory file lock)
- `julie-server stop/status` lifecycle commands (`restart` = stop + auto-restart on next call)
- `WorkspacePool` for shared workspace state across sessions
- `JulieServerHandler::new_with_shared_workspace()` constructor
- Auto-indexing guard in `on_initialized` (skip if workspace already indexed)
- Centralized index storage at `~/.julie/indexes/`
- Per-project logs at `{project}/.julie/logs/` (with daemon log cross-reference)
- PID file lifecycle, crash recovery, stale cleanup
- Safe index migration (copy, validate, then delete original)
- rmcp bump to 1.x (verify API compatibility first as a sub-task)

**Dogfood gate:** Rebuild release, use for real dev work. Verify multi-session stability before proceeding.

### Phase 2: Multi-Workspace + Shared References

**Goal:** Reference workspaces share indexes instead of duplicating.

- Persistent workspace registry at `~/.julie/registry.json` (RwLock-protected, flush on write)
- `manage_workspace add` checks registry before indexing (instant attach if already indexed)
- One file watcher per workspace (shared, reference-counted by sessions)
- Watcher lifecycle tied to session count + grace period
- Session-to-workspace routing for reference workspaces

**Dogfood gate:** Add reference workspaces, verify instant attach, verify shared watcher.

### Phase 3: Shared Embedding Pipeline

**Goal:** Single VRAM footprint regardless of workspace count.

- Daemon-level embedding queue
- Single ORT model load / single sidecar process
- Cross-workspace fairness scheduling
- Priority-based cancellation

**Dogfood gate:** Run multiple sessions, verify VRAM usage, verify no queue starvation.

### Phase 4: Plugin Distribution

**Goal:** Zero-config installation for users.

- `julie-plugin` repo with platform binaries + sidecar + skills
- `plugin.json` with stdio adapter config
- CI pipeline for build + package on release
- Documentation for non-Claude-Code clients

**Dogfood gate:** Install plugin fresh, verify zero-config experience.

## Development Methodology

All phases follow the full superpowers discipline:

- **TDD**: Failing test first, minimal implementation, verify green
- **Written implementation plan** (superpowers:writing-plans) before code
- **Subagent-driven execution** with review checkpoints
- **Code review** at each phase completion
- **Verification before completion** on every deliverable
- **Systematic debugging** for any failures
- **Dogfood gate** between phases: rebuild, restart, use for real work

## Non-Goals

- Dashboard / web UI (v4 lesson: no circular feature dependencies)
- System tray app (adapter auto-start eliminates the need)
- System service installation (auto-start eliminates the need)
- Cross-project federated search (agent workflows handle this better)
- Backward compatibility with v4 daemon protocol

## References

- [v5 Simplification Spec](2026-03-11-v5-simplification-design.md)
- [MCP Transports Specification](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports)
- [ra-multiplex / lspmux](https://github.com/pr2502/ra-multiplex)
- [mock-mcp daemon architecture](https://github.com/mcpland/mock-mcp)
- [rmcp Streamable HTTP](https://docs.rs/rmcp/latest/rmcp/transport/index.html)
- [MCP Transport Future (Dec 2025)](https://blog.modelcontextprotocol.io/posts/2025-12-19-mcp-transport-future/)
