# Phase 1: Daemon + HTTP Foundation — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Transform Julie from a per-session stdio MCP server into a persistent daemon with HTTP API, MCP Streamable HTTP transport, and a basic Vue + PrimeVue web UI.

**Architecture:** Julie gains a `daemon` subcommand that forks a background HTTP server (axum). The server hosts both the MCP Streamable HTTP endpoint (via rmcp's `StreamableHttpService`) and a REST management API. A Vue + PrimeVue web UI is embedded in the binary and served as static assets. Stdio mode is preserved for backward compatibility. A global registry at `~/.julie/registry.toml` tracks all known projects.

**Tech Stack:** axum (HTTP), rmcp `transport-streamable-http-server` (MCP over HTTP), clap (CLI), Vue 3 + PrimeVue + Vite (web UI), include_dir/rust-embed (asset embedding), tokio (async runtime)

**Design doc:** `docs/plans/2026-03-07-julie-platform-design.md`

---

## Task 1: CLI Argument Parsing with clap

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Modify: `Cargo.toml`

**What to build:** Replace the manual `--workspace` argument parsing in `main.rs:25-82` with clap. Add a `daemon` subcommand with `start`, `stop`, and `status` sub-subcommands. Keep `--workspace` as a global option. The default behavior (no subcommand) runs in stdio MCP mode for backward compatibility.

**Approach:**
```rust
// src/cli.rs
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "julie", version, about = "Julie - Cross-Platform Code Intelligence Server")]
pub struct Cli {
    /// Workspace root path (overrides JULIE_WORKSPACE env var)
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run Julie as a persistent daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Start the Julie daemon
    Start {
        /// Port to listen on
        #[arg(long, default_value = "7890", env = "JULIE_PORT")]
        port: u16,
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
}
```

Update `main.rs` to parse with clap, resolve workspace path (reuse existing logic but via clap's `Option<PathBuf>`), and branch on subcommand: `None` → stdio mode (current behavior), `Some(Commands::Daemon { action })` → daemon mode.

**Acceptance criteria:**
- [ ] `julie-server` (no args) runs stdio MCP mode exactly as before
- [ ] `julie-server --workspace ~/Source/foo` works as before
- [ ] `julie-server daemon start --port 8080` is parseable (handler stubbed for now)
- [ ] `julie-server daemon stop` and `status` are parseable
- [ ] `clap` added to Cargo.toml dependencies
- [ ] Tests pass, committed

---

## Task 2: Daemon Lifecycle (PID file, start/stop/status)

**Files:**
- Create: `src/daemon.rs`
- Modify: `src/main.rs`
- Modify: `src/cli.rs` (if needed)

**What to build:** Implement the daemon lifecycle: PID file management at `~/.julie/daemon.pid`, process daemonization (or foreground mode), stop via PID signal, and status checking. Cross-platform: use SIGTERM on Unix, `taskkill` on Windows.

**Approach:**
- `daemon_start()`: Check if already running (PID file exists + process alive). If not, create `~/.julie/` dir, write PID file, start the HTTP server. In foreground mode, run directly. In background mode, fork (Unix) or spawn detached (Windows).
- `daemon_stop()`: Read PID file, send signal to stop the process, remove PID file.
- `daemon_status()`: Read PID file, check if process is alive, report.
- Use `dirs` or `home` crate for cross-platform home directory resolution.
- PID file format: plain text, just the PID number.

**Key code:**
```rust
// src/daemon.rs
use std::path::PathBuf;
use anyhow::Result;

pub fn julie_home() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA")
            .map_err(|_| anyhow::anyhow!("APPDATA not set"))?;
        Ok(PathBuf::from(appdata).join("julie"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("HOME not set"))?;
        Ok(PathBuf::from(home).join(".julie"))
    }
}

pub fn pid_file_path() -> Result<PathBuf> {
    Ok(julie_home()?.join("daemon.pid"))
}

pub fn is_daemon_running() -> Result<Option<u32>> {
    // Read PID file, check if process exists
}

pub fn write_pid_file() -> Result<()> {
    // Write current PID to file
}

pub fn remove_pid_file() -> Result<()> {
    // Remove PID file
}

pub fn stop_daemon() -> Result<()> {
    // Read PID, send signal, wait, remove PID file
}

pub fn daemon_status() -> Result<()> {
    // Report daemon status to stdout
}
```

**Acceptance criteria:**
- [ ] `julie_home()` returns `~/.julie` on Unix, `%APPDATA%\julie` on Windows
- [ ] `daemon start --foreground` writes PID file, runs in foreground, removes PID on exit
- [ ] `daemon stop` sends signal and removes PID file
- [ ] `daemon status` reports running/stopped with PID and port
- [ ] Double-start detection (error if already running)
- [ ] Graceful shutdown on SIGTERM/SIGINT (ctrl-c)
- [ ] Tests for PID file operations, committed

---

## Task 3: HTTP Server Foundation (axum)

**Files:**
- Create: `src/server.rs`
- Create: `src/api/mod.rs`
- Create: `src/api/health.rs`
- Create: `src/api/projects.rs`
- Modify: `src/daemon.rs` (wire up server start)
- Modify: `Cargo.toml`

**What to build:** An axum HTTP server that serves a health endpoint, management API, and eventually the MCP endpoint and web UI. The server binds to the configured port and runs inside the tokio runtime.

**Approach:**
- `src/server.rs`: Creates the axum `Router`, binds to `0.0.0.0:{port}`, runs the server.
- `src/api/health.rs`: `GET /api/health` returns JSON with daemon version, uptime, registered project count.
- `src/api/projects.rs`: Stubbed endpoints — `GET /api/projects` (list), `POST /api/projects` (register), `DELETE /api/projects/:id` (remove). These will be wired to the global registry in Task 5.
- Use `tower-http` for CORS middleware (the web UI needs it during development).

**Key dependencies to add:**
```toml
axum = "0.8"
tower-http = { version = "0.6", features = ["cors", "fs"] }
```

**Key code:**
```rust
// src/server.rs
use axum::{Router, routing::get};
use std::net::SocketAddr;
use tokio::net::TcpListener;

pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let app = Router::new()
        .nest("/api", api_routes())
        // MCP endpoint will be added in Task 4
        // Static UI will be added in Task 7
        ;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Julie daemon listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn api_routes() -> Router {
    Router::new()
        .route("/health", get(crate::api::health::health))
        .route("/projects", get(crate::api::projects::list_projects)
            .post(crate::api::projects::register_project))
        .route("/projects/{id}", axum::routing::delete(crate::api::projects::remove_project))
}
```

**Acceptance criteria:**
- [ ] `julie-server daemon start --foreground --port 7890` starts HTTP server
- [ ] `GET /api/health` returns `{ "status": "ok", "version": "3.9.1", "uptime_seconds": N }`
- [ ] `GET /api/projects` returns `[]` (empty for now)
- [ ] Port conflict produces clear error message
- [ ] CORS enabled for development (localhost origins)
- [ ] axum and tower-http added to Cargo.toml
- [ ] Tests for health endpoint, committed

---

## Task 4: MCP Streamable HTTP Transport

**Files:**
- Modify: `src/server.rs` (add MCP route)
- Create: `src/mcp_http.rs`
- Modify: `Cargo.toml` (enable rmcp feature)
- Modify: `src/main.rs` (wire up handler creation for daemon mode)

**What to build:** Mount rmcp's `StreamableHttpService` as a tower service at the `/mcp` route in axum. Each MCP client connection gets its own session with its own `JulieServerHandler` instance. This is the critical integration point.

**Approach:**
- Enable `transport-streamable-http-server` feature in rmcp dependency.
- Create a `StreamableHttpService` with a factory closure that creates a fresh `JulieServerHandler` for each session.
- Mount it as a fallback/nested service in axum at `/mcp`.
- The handler factory needs access to the global state (registry, shared config) to pass workspace context to each handler.

**Key Cargo.toml change:**
```toml
rmcp = { version = "0.12", features = ["server", "transport-io", "transport-streamable-http-server", "macros"] }
```

**Key code:**
```rust
// src/mcp_http.rs
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, StreamableHttpServerConfig,
    session::local::LocalSessionManager,
};
use std::sync::Arc;
use crate::handler::JulieServerHandler;

pub fn create_mcp_service(
    workspace_root: std::path::PathBuf,
) -> StreamableHttpService<JulieServerHandler> {
    let config = StreamableHttpServerConfig::default();
    let session_manager = Arc::new(LocalSessionManager::default());

    StreamableHttpService::new(
        move || {
            // Each MCP session gets a fresh handler
            // We need to block on the async new() — or restructure to allow sync creation
            let rt = tokio::runtime::Handle::current();
            let root = workspace_root.clone();
            rt.block_on(async move {
                JulieServerHandler::new(root)
                    .await
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            })
        },
        session_manager,
        config,
    )
}
```

**Integration with axum (in server.rs):**
```rust
use tower::ServiceExt; // for converting tower services

let mcp_service = create_mcp_service(workspace_root);
let app = Router::new()
    .nest("/api", api_routes())
    .nest_service("/mcp", mcp_service);
```

**Acceptance criteria:**
- [ ] rmcp `transport-streamable-http-server` feature enabled and compiles
- [ ] MCP client can connect to `http://localhost:7890/mcp` via Streamable HTTP
- [ ] Multiple concurrent MCP sessions work (each gets own handler)
- [ ] Session lifecycle works (create on init, close on disconnect)
- [ ] Test: HTTP POST to `/mcp` with MCP initialize request gets valid response
- [ ] Stdio mode (`julie-server` with no subcommand) still works unchanged
- [ ] Committed

---

## Task 5: Global Project Registry

**Files:**
- Create: `src/registry/mod.rs`
- Create: `src/registry/global_registry.rs`
- Modify: `src/api/projects.rs` (wire up to real registry)
- Modify: `src/daemon.rs` (load registry on startup)

**What to build:** A global project registry stored at `~/.julie/registry.toml` that tracks all known projects. This is separate from the per-project `workspace_registry.json` — it's a lightweight index of which projects exist on the machine.

**Approach:**
- Registry stores: project path, workspace ID, display name, last indexed timestamp, index status, index size.
- TOML format for human-readability.
- Atomic writes (write-to-temp, rename).
- Load on daemon start, updated when projects are registered/indexed.
- Exposed via REST API (`GET /api/projects`, `POST /api/projects`, `DELETE /api/projects/:id`).

**Key data structure:**
```rust
// src/registry/global_registry.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRegistry {
    pub version: String,
    pub projects: HashMap<String, ProjectEntry>, // keyed by workspace_id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub path: PathBuf,
    pub workspace_id: String,
    pub last_indexed: Option<String>, // ISO 8601
    pub symbol_count: Option<u64>,
    pub file_count: Option<u64>,
    pub status: ProjectStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProjectStatus {
    Registered,  // Known but not yet indexed
    Indexing,    // Currently being indexed
    Ready,       // Indexed and searchable
    Stale,       // Index exists but may be outdated
    Error(String),
}
```

**Acceptance criteria:**
- [ ] Registry loads from `~/.julie/registry.toml` on daemon start (creates if missing)
- [ ] `POST /api/projects` with `{ "path": "/Users/murphy/Source/julie" }` registers a project
- [ ] `GET /api/projects` returns list of registered projects with status
- [ ] `DELETE /api/projects/:id` removes a project
- [ ] Registry persisted atomically on changes
- [ ] Reuse `generate_workspace_id()` from `src/workspace/registry.rs:308` for consistent IDs
- [ ] Tests for registry CRUD, committed

---

## Task 6: Multi-Project Workspace Loading

**Files:**
- Create: `src/daemon_state.rs`
- Modify: `src/server.rs` (inject shared state)
- Modify: `src/mcp_http.rs` (use shared state for handler creation)

**What to build:** When the daemon starts, it loads (or creates) workspace indexes for all registered projects. Each project gets its own `JulieWorkspace` (with Tantivy + SQLite). The daemon holds all workspaces in a shared map that MCP handlers and API routes can access.

**Approach:**
- `DaemonState` struct holds: global registry, map of workspace_id → `JulieWorkspace`, config.
- Wrapped in `Arc<RwLock<DaemonState>>` and injected into axum as state.
- On startup: iterate registered projects, call `JulieWorkspace::detect_and_load()` for each.
- Lazy loading: don't block startup on indexing all projects. Load indexes that exist, queue re-indexing for stale ones.
- MCP handler factory receives a reference to the daemon state to know which workspace to operate on.

**Key challenge:** Currently `JulieServerHandler::new()` takes a single workspace root. For daemon mode, we need the handler to work with the daemon's shared workspace pool instead. This may require a new constructor or a daemon-aware handler variant.

**Acceptance criteria:**
- [ ] Daemon loads all registered projects' indexes on startup
- [ ] Projects without existing `.julie/` index get status `Registered` (not auto-indexed yet)
- [ ] `GET /api/projects` shows accurate status (Ready, Stale, Registered, etc.)
- [ ] MCP sessions connect to the correct workspace based on client context
- [ ] Daemon startup doesn't block on slow indexing operations
- [ ] Tests for workspace loading, committed

---

## Task 7: Vue + PrimeVue Web UI Scaffold

**Files:**
- Create: `ui/` directory (Vue + Vite project)
- Create: `ui/package.json`
- Create: `ui/vite.config.ts`
- Create: `ui/src/App.vue`
- Create: `ui/src/views/Dashboard.vue`
- Create: `ui/src/views/Projects.vue`
- Modify: `src/server.rs` (serve embedded static assets)
- Modify: `Cargo.toml` (add rust-embed or include_dir for embedding)

**What to build:** A Vue 3 + PrimeVue + Vite web application embedded in the Julie binary. Phase 1 UI is minimal: project list with status, health dashboard, and links.

**Approach:**
- Vue project at `ui/` with Vite, Vue 3, PrimeVue, Vue Router.
- `npm run build` outputs to `ui/dist/`.
- In Rust: use `rust-embed` or `include_dir` to embed `ui/dist/` at compile time.
- axum serves embedded assets at `/ui/` with fallback to `index.html` for SPA routing.
- Development: Vite dev server proxies API calls to `http://localhost:7890/api/`.

**Dashboard view shows:**
- Julie version + daemon uptime
- List of registered projects (table: name, path, status, symbol count, last indexed)
- Quick actions: register new project

**Build integration:**
```toml
# Cargo.toml
rust-embed = { version = "8", features = ["debug-embed"] }
```

**axum static file serving:**
```rust
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "ui/dist/"]
struct UiAssets;

// Serve embedded files with SPA fallback
async fn ui_handler(uri: axum::http::Uri) -> impl axum::response::IntoResponse {
    let path = uri.path().trim_start_matches("/ui/");
    match UiAssets::get(path) {
        Some(content) => { /* serve with correct content-type */ },
        None => { /* fallback to index.html for SPA routing */ },
    }
}
```

**Acceptance criteria:**
- [ ] `ui/` directory with Vue + PrimeVue project scaffolded
- [ ] `npm run build` produces `ui/dist/` output
- [ ] Julie binary embeds and serves UI at `http://localhost:7890/ui/`
- [ ] Dashboard shows daemon health info from `/api/health`
- [ ] Projects table shows data from `/api/projects`
- [ ] Vite dev server works for UI development (proxied API)
- [ ] Tests pass, committed

---

## Task 8: Cross-Project File Watching

**Files:**
- Create: `src/daemon_watcher.rs`
- Modify: `src/daemon_state.rs` (integrate watcher)
- Modify: `src/daemon.rs` (start/stop watchers)

**What to build:** File watchers for all registered projects so the daemon detects changes and can re-index. Uses the existing `notify` crate (already a dependency). One watcher per project, respecting ignore patterns.

**Approach:**
- Reuse patterns from existing `src/watcher.rs` / `IncrementalIndexer`.
- Each project gets its own `notify::RecommendedWatcher` configured with the project's ignore patterns.
- File change events trigger incremental re-indexing of affected files.
- Watcher lifecycle tied to project registration: start on register, stop on remove.
- Debounce file events (existing pattern in `IncrementalIndexer`).

**Acceptance criteria:**
- [ ] File watchers start for all `Ready` projects on daemon startup
- [ ] File changes in a watched project trigger re-indexing of that file
- [ ] Watcher respects `.gitignore` and configured ignore patterns
- [ ] `POST /api/projects` (register) starts a watcher for the new project
- [ ] `DELETE /api/projects/:id` stops the watcher for that project
- [ ] Watcher errors don't crash the daemon
- [ ] Tests for watcher lifecycle, committed

---

## Task 9: Background Indexing Pipeline

**Files:**
- Create: `src/daemon_indexer.rs`
- Modify: `src/daemon_state.rs` (integrate indexer)
- Modify: `src/api/projects.rs` (trigger indexing via API)

**What to build:** A background indexing pipeline that indexes registered projects without blocking the daemon. When a project is registered (or re-index requested), queue it for background indexing. Update project status in the global registry as indexing progresses.

**Approach:**
- Use a tokio task with an mpsc channel: API/watcher sends indexing requests, background task processes them sequentially (one project at a time to avoid resource contention).
- Reuse existing indexing logic from `src/handler.rs:initialize_workspace_with_force()` and `src/startup.rs`.
- Status updates: `Registered` → `Indexing` → `Ready` (or `Error`).
- Expose indexing status via `GET /api/projects/:id/status`.

**Acceptance criteria:**
- [ ] `POST /api/projects/:id/index` triggers background indexing
- [ ] Indexing runs in a background tokio task
- [ ] Project status updates visible via `GET /api/projects`
- [ ] Indexing errors are captured and stored in project status
- [ ] Multiple index requests are queued (not concurrent)
- [ ] Re-index of an already-indexed project works (force mode)
- [ ] Tests for indexing pipeline, committed

---

## Task 10: Integration Testing & Polish

**Files:**
- Create: `src/tests/daemon/mod.rs`
- Create: `src/tests/daemon/lifecycle_tests.rs`
- Create: `src/tests/daemon/api_tests.rs`
- Modify: `src/lib.rs` (add daemon module exports)
- Modify: `src/main.rs` (final wiring)

**What to build:** End-to-end integration tests for the daemon: start, register a project, verify indexing, query via MCP HTTP, check web UI serves, stop.

**Approach:**
- Use `reqwest` (or axum's test utilities) to make HTTP requests in tests.
- Test the full flow: daemon start → register project → trigger index → MCP search → verify results.
- Test backward compatibility: stdio mode still works.
- Polish: error messages, logging, startup banner.

**Acceptance criteria:**
- [ ] Integration test: start daemon, register project, index, search via API
- [ ] Integration test: MCP Streamable HTTP connection + tool call
- [ ] Integration test: web UI assets served correctly
- [ ] Stdio mode regression test (no subcommand still works)
- [ ] All existing tests still pass (`cargo test --lib -- --skip search_quality`)
- [ ] Clean startup/shutdown logging
- [ ] Committed with clean git history

---

## Verification Plan

### Manual Testing
1. `cargo build` — verify compilation with new deps
2. `./target/debug/julie-server` — verify stdio mode still works
3. `./target/debug/julie-server daemon start --foreground --port 7890` — daemon starts
4. `curl http://localhost:7890/api/health` — health check responds
5. `curl -X POST http://localhost:7890/api/projects -d '{"path":"/Users/murphy/Source/julie"}'` — register project
6. `curl http://localhost:7890/api/projects` — see registered project
7. Open `http://localhost:7890/ui/` — web dashboard loads
8. Configure MCP client to connect to `http://localhost:7890/mcp` — verify tool calls work
9. `./target/debug/julie-server daemon stop` — clean shutdown

### Automated Testing
```bash
# Fast tier (after each task)
cargo test --lib -- --skip search_quality 2>&1 | tail -5

# Full suite (after all tasks complete)
cargo test --lib 2>&1 | tail -5
```

---

## Dependency Summary

**New Cargo.toml dependencies:**
```toml
clap = { version = "4", features = ["derive"] }
axum = "0.8"
tower-http = { version = "0.6", features = ["cors", "fs"] }
rust-embed = { version = "8", features = ["debug-embed"] }
```

**Modified dependency:**
```toml
rmcp = { version = "0.12", features = ["server", "transport-io", "transport-streamable-http-server", "macros"] }
```

**New devDependency (for integration tests):**
```toml
reqwest = { version = "0.12", features = ["json"] }
```

**UI dependencies (package.json):**
```json
{
  "vue": "^3.5",
  "vue-router": "^4",
  "primevue": "^4",
  "primeicons": "^7",
  "@primevue/themes": "^4",
  "vite": "^6",
  "@vitejs/plugin-vue": "^5"
}
```
