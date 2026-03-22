# v6 Phase 1 Plan Review Findings

These findings must be addressed before or during execution of the Phase 1 plan.

## Critical Fixes (Before Starting Any Task)

### CF1: IPC-to-MCP Bridge Design (Task 8)

The `handle_ipc_session` function was left as a TODO. Here is the design:

**Transport:** Use rmcp's `transport-async-rw` feature to serve MCP over the IPC stream. This provides a generic transport that works over any `AsyncRead + AsyncWrite`, which our `IpcStream` already implements.

**Workspace root discovery:** The adapter is *almost* a pure byte proxy, with one exception: before forwarding MCP traffic, it sends a single header line:

```
WORKSPACE:/Users/murphy/source/julie\n
```

The daemon reads this line, creates a handler with that workspace root via `WorkspacePool`, then serves MCP over the remaining stream. This is the same pattern ra-multiplex uses (header line before protocol traffic).

**Implementation sketch:**
```rust
async fn handle_ipc_session(
    mut stream: IpcStream,
    pool: Arc<WorkspacePool>,
) -> Result<()> {
    // 1. Read workspace root header
    let mut header = String::new();
    let mut buf = [0u8; 1];
    loop {
        stream.read_exact(&mut buf).await?;
        if buf[0] == b'\n' { break; }
        header.push(buf[0] as char);
    }
    let workspace_root = header.strip_prefix("WORKSPACE:")
        .ok_or_else(|| anyhow!("Invalid IPC header"))?;
    let workspace_root = PathBuf::from(workspace_root);

    // 2. Get or initialize workspace from pool
    let workspace_id = generate_workspace_id(workspace_root.to_str().unwrap())?;
    let workspace = pool.get_or_init(&workspace_id, &workspace_root).await?;

    // 3. Create handler backed by shared workspace
    let handler = JulieServerHandler::new_with_shared_workspace(workspace, workspace_root).await?;

    // 4. Serve MCP over the IPC stream (rmcp transport-async-rw)
    // The exact API depends on rmcp version; sketch:
    handler.serve(stream).await?;

    Ok(())
}
```

**Adapter side:** After connecting to the daemon socket, the adapter writes the header before forwarding:
```rust
let cwd = std::env::current_dir()?;
stream.write_all(format!("WORKSPACE:{}\n", cwd.display()).as_bytes()).await?;
// Then start byte forwarding
```

**Note:** Verify `rmcp`'s transport-async-rw API before implementing. If `handler.serve()` doesn't accept a raw `AsyncRead + AsyncWrite`, the transport may need wrapping. Check `docs.rs/rmcp` for `transport::async_rw` module.

### CF2: Use tokio::sync::RwLock in WorkspacePool (Task 4)

Replace `std::sync::RwLock` with `tokio::sync::RwLock` in `WorkspacePool` to avoid blocking the async runtime during workspace initialization (which does disk I/O).

```rust
use tokio::sync::RwLock;

pub struct WorkspacePool {
    workspaces: RwLock<HashMap<String, WorkspaceEntry>>,
    indexes_dir: PathBuf,
}
```

The `get_or_init` method becomes properly async with `.await` on lock acquisition.

### CF3: Document Clone Semantics in new_with_shared_workspace (Task 5)

The `Clone` on `JulieWorkspace` shares `Arc<Mutex<SqliteDB>>` and `Arc<Mutex<SearchIndex>>` (the expensive resources), but clones `watcher: None` and creates a new `config`. This is acceptable for Phase 1:
- DB and search index: shared via Arc (correct)
- Watcher: daemon-level, not per-handler (Phase 2 addresses shared watchers)
- Embedding provider: daemon-level (Phase 3 addresses shared embeddings)

Document this in a code comment on `new_with_shared_workspace`.

## Major Fixes

### MF1: Add rmcp Research Spike (Task 0)

Before Task 1, add a Task 0: "Research rmcp 1.x API and verify compatibility."
- Check if rmcp 1.x exists on crates.io (`cargo search rmcp`)
- If 0.x is latest, stay on 0.12 and use existing `transport-io` + add `transport-async-rw`
- Read `docs.rs/rmcp/latest` for transport API, `serve()` signature, and handler trait
- Document actual breaking changes (if any)
- This blocks all other tasks

### MF2: Add Missing Dependencies to Cargo.toml

Task 1 or the setup task must add these to `Cargo.toml`:
- `libc` under `[target.'cfg(unix)'.dependencies]`
- `uuid = { version = "1", features = ["v4"] }` (for session IDs)
- `dirs = "6"` (for home directory detection)
- Verify `windows-sys` vs `windows` crate for Windows PID check

### MF3: Add `#[derive(Clone)]` to DaemonPaths

The `DaemonPaths` struct needs `Clone` since it's passed around by value in several places.

### MF4: Spell Out `set_index_root` Implementation

Add an `index_root_override: Option<PathBuf>` field to `JulieWorkspace`. Modify `indexes_root_path()` to return the override if set:

```rust
pub fn indexes_root_path(&self) -> PathBuf {
    self.index_root_override.clone()
        .unwrap_or_else(|| self.julie_dir.join("indexes"))
}

pub fn set_index_root(&mut self, path: PathBuf) {
    self.index_root_override = Some(path);
}
```

### MF5: Fix Parallelization Claim

Tasks 1-5 CANNOT be parallelized due to file conflicts on `src/lib.rs`, `Cargo.toml`, `src/handler.rs`. Correct dependency order:
- Task 0 (rmcp research) first
- Task 1 (rmcp bump) second (blocks everything)
- Tasks 2 + 3 can be parallel (coordinate on `lib.rs` / `Cargo.toml`)
- Task 4 depends on Task 2
- Task 5 depends on Task 4

### MF6: Remove `workspace_root` from `run_daemon` Signature

The daemon serves multiple workspaces. It should take `DaemonPaths` and `http_port`, not a workspace root. The workspace root is per-session, resolved from the IPC header.

### MF7: Add Missing Spec Coverage

Add three additional tasks or expand existing ones:

**A. Adapter reconnect logic** (expand Task 9):
On socket disconnect, attempt reconnect with bounded retry (3 attempts, exponential backoff, 10s max). If all retries fail, re-run full startup sequence. If that also fails, surface error and exit.

**B. Idle detection and auto-shutdown** (expand Task 8):
The daemon tracks active session count via `SessionTracker`. When count reaches zero, start a 5-minute idle timer. If a new session connects before timeout, cancel timer. If timer expires, graceful shutdown.

**C. Per-project logging** (add to Task 8 or create Task 8b):
The daemon should write tool/indexing logs to `{project}/.julie/logs/julie.log.{date}` (determined from the session's workspace root). On first session for a project, log "Daemon logs at ~/.julie/daemon.log" to the project log for discoverability.

**D. Restart command** (expand Task 6/10):
Add `Restart` CLI subcommand that calls `stop_daemon()` and prints "Daemon will restart on next tool call." This is syntactic sugar over `stop`.

## Minor Fixes

- Add `Default` impl or `#[derive(Default)]` alongside `Clone` for `DaemonPaths`
- Use `Self::tool_router()` not `Self::build_tool_router()` in Task 5
- Ensure RwLock imports are `tokio::sync::RwLock` not `std::sync::RwLock` in handler code
- Add concurrent `get_or_init` test with different workspace IDs in Task 4
- Windows `stop_daemon` should use a graceful mechanism (named event or control message) not `taskkill /F`
