# Julie v5.0.0 — The Great Simplification Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Strip all v4.x daemon/HTTP/UI/tray/agent/install infrastructure from HEAD, returning Julie to a stdio-only MCP server.

**Architecture:** Surgical deletion from current HEAD. Delete ~23,300 lines (production + test) across daemon, HTTP, API, agent, UI, tray, federation, and install modules. Modify ~15 files to remove daemon branches and `WorkspaceTarget::All` arms. Rewrite `main.rs` and `cli.rs` to the v3.9.1-style stdio-only entry point.

**Tech Stack:** Rust, rmcp (stdio transport only), clap (minimal), Tantivy, SQLite, tree-sitter

**Spec:** `docs/superpowers/specs/2026-03-11-v5-simplification-design.md`

**Note:** Intermediate commits within each chunk are NOT expected to compile. The first clean `cargo check` happens at Task 20. This is intentional — the work should be done on a branch and squash-merged (or the intermediate commits are acceptable as non-compiling history during a cleanup).

---

## Chunk 1: Bulk Deletion and Dependency Cleanup

This chunk gets us from "broken build with v4 code" to "broken build without v4 code" by removing all files that are going away entirely and stripping the dependency list. After this chunk, `cargo check` will fail only on the files that need surgical modification (handler.rs, tools, etc.).

### Task 1: Delete v4.x production source files

**Files to delete:**
- `src/daemon.rs`
- `src/daemon_state.rs`
- `src/daemon_indexer.rs`
- `src/daemon_state_watchers.rs`
- `src/daemon_watcher.rs`
- `src/connect.rs`
- `src/server.rs`
- `src/mcp_http.rs`
- `src/install.rs`
- `src/binary_monitor.rs`
- `src/ui.rs`
- `src/stdio.rs`
- `src/api/` (entire directory)
- `src/agent/` (entire directory)
- `src/registry/` (entire directory)
- `src/tools/federation/` (entire directory)
- `src/tools/get_context/federated.rs`
- `src/tools/navigation/federated_refs.rs`

- [ ] **Step 1: Delete all files**

```bash
# Daemon/connect/server/install infrastructure
rm src/daemon.rs src/daemon_state.rs src/daemon_indexer.rs \
   src/daemon_state_watchers.rs src/daemon_watcher.rs \
   src/connect.rs src/server.rs src/mcp_http.rs \
   src/install.rs src/binary_monitor.rs src/ui.rs src/stdio.rs

# Directory-level deletions
rm -rf src/api/ src/agent/ src/registry/
rm -rf src/tools/federation/

# Individual federation tool files
rm src/tools/get_context/federated.rs
rm src/tools/navigation/federated_refs.rs
```

- [ ] **Step 2: Delete frontend and tray directories**

```bash
rm -rf ui/
rm -rf tauri-app/
```

- [ ] **Step 3: Delete build.rs**

The entire `build.rs` (111 lines) exists only to build Vue UI assets.

```bash
rm build.rs
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore(v5): delete v4.x daemon/HTTP/UI/tray/agent/install source files

Remove ~12,500 lines of production code:
- Daemon core (5 files), connect bridge, HTTP server, REST API (7 files)
- Agent dispatch (5+ files), system install, binary monitor, UI module
- Registry, federation tools, federated get_context/fast_refs
- Web dashboard (ui/), tray app (tauri-app/), build.rs"
```

### Task 2: Delete v4.x test files

**Files to delete** — all test files that import from deleted modules:

- [ ] **Step 1: Delete top-level daemon/server/api/agent test files**

```bash
rm src/tests/daemon_tests.rs src/tests/daemon_indexer_tests.rs \
   src/tests/daemon_watcher_tests.rs src/tests/daemon_integration_tests.rs \
   src/tests/connect_tests.rs src/tests/server_tests.rs \
   src/tests/api_dashboard_tests.rs src/tests/api_search_tests.rs \
   src/tests/api_agents_tests.rs src/tests/phase4_integration_tests.rs \
   src/tests/federation_tests.rs src/tests/federation_integration_tests.rs \
   src/tests/ui_tests.rs src/tests/registry_tests.rs \
   src/tests/agent_backend_tests.rs src/tests/binary_monitor_tests.rs
```

- [ ] **Step 2: Delete tool-level federation/daemon test files**

```bash
rm src/tests/tools/daemon_workspace_tests.rs \
   src/tests/tools/workspace_target_tests.rs \
   src/tests/tools/federated_refs_tests.rs \
   src/tests/tools/deep_dive_federation_tests.rs \
   src/tests/tools/get_context_federation_tests.rs
```

Check if federated_search_tests is in a `search/` subdirectory:
```bash
find src/tests/tools/search/ -name '*federat*' -exec rm {} \;
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore(v5): delete v4.x test files (~10,800 lines)

Remove 23 test files that import from deleted daemon/API/federation modules."
```

### Task 3: Update `src/lib.rs` — remove deleted module declarations

**File:** `src/lib.rs`

- [ ] **Step 1: Rewrite lib.rs**

Remove all `pub mod` lines for deleted modules. Keep: `cli`, `database`, `embeddings`, `extractors`, `handler`, `health`, `language`, `mcp_compat`, `search`, `startup`, `tools`, `tracing`, `utils`, `watcher`, `workspace`.

The new `src/lib.rs` should be:

```rust
// Julie - Cross-Platform Code Intelligence Server Library
//!
//! Julie provides code intelligence across 31 programming languages using
//! Tantivy search with code-aware tokenization (CamelCase/snake_case splitting).

pub mod cli;
pub mod database;
pub mod embeddings;
pub mod extractors;
pub mod handler;
pub mod health;
pub mod language;
pub mod mcp_compat;
pub mod search;
pub mod startup;
pub mod tools;
pub mod tracing;
pub mod utils;
pub mod watcher;
pub mod workspace;

#[cfg(test)]
pub mod tests;

// Re-export common types
pub use extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
pub use tracing::{
    ArchitecturalLayer, ConnectionType, CrossLanguageTracer, DataFlowTrace, TraceOptions, TraceStep,
};
pub use workspace::{JulieWorkspace, WorkspaceConfig, WorkspaceHealth};
```

- [ ] **Step 2: Commit**

```bash
git add src/lib.rs
git commit -m "chore(v5): strip deleted module declarations from lib.rs"
```

### Task 4: Update `src/tests/mod.rs` — remove deleted test module declarations

**File:** `src/tests/mod.rs`

- [ ] **Step 1: Remove all deleted test module declarations**

Remove these top-level declarations:
- Line 12: `pub mod agent_backend_tests;`
- Line 13: `pub mod api_agents_tests;`
- Line 28: `pub mod binary_monitor_tests;`
- Lines 33-36: `pub mod connect_tests;`, `daemon_indexer_tests`, `daemon_tests`, `daemon_watcher_tests`
- Line 41: `pub mod federation_tests;`
- Lines 46-51: `api_dashboard_tests`, `api_search_tests`, `server_tests`, `daemon_integration_tests`, `federation_integration_tests`, `phase4_integration_tests`
- Line 56: `pub mod ui_tests;`
- Line 61: `pub mod registry_tests;`

Remove section comments for deleted sections (Agent Tests, Binary Monitor Tests, Daemon Tests, Federation Tests, Server Tests, UI Tests, Registry Tests).

Remove these inside `pub mod tools { }`:
- Line 113: `pub mod deep_dive_federation_tests;`
- Line 150: `pub mod workspace_target_tests;`
- Line 151: `pub mod daemon_workspace_tests;`
- Line 153: `pub mod federated_refs_tests;`
- Line 156: `pub mod get_context_federation_tests;`

Also remove the federated search test inside the search submodule if it has a declaration.

Keep `pub mod cli_tests;` — the CLI tests will need updating later but they exist for `resolve_workspace_root()` which is kept.

- [ ] **Step 2: Commit**

```bash
git add src/tests/mod.rs
git commit -m "chore(v5): remove deleted test module declarations from tests/mod.rs"
```

### Task 5: Update `Cargo.toml` — strip v4.x dependencies

**File:** `Cargo.toml`

- [ ] **Step 1: Update workspace members**

Change line 2 from:
```toml
members = [".", "crates/julie-extractors", "tauri-app/src-tauri"]
```
to:
```toml
members = [".", "crates/julie-extractors"]
```

- [ ] **Step 2: Update version**

Change line 12 from `version = "4.2.3"` to `version = "5.0.0"`.

- [ ] **Step 3: Strip rmcp feature**

Change line 37 from:
```toml
rmcp = { version = "0.12", features = ["server", "transport-io", "transport-streamable-http-server", "macros"] }
```
to:
```toml
rmcp = { version = "0.12", features = ["server", "transport-io", "macros"] }
```

- [ ] **Step 4: Remove serde_yaml**

Delete line 56: `serde_yaml = "0.9"`

- [ ] **Step 5: Strip clap `env` feature**

Change line 70 from:
```toml
clap = { version = "4.5", features = ["derive", "env"] }
```
to:
```toml
clap = { version = "4.5", features = ["derive"] }
```

- [ ] **Step 6: Remove HTTP server dependencies**

Delete lines 72-76 (the section with comment and deps):
```toml
# HTTP server (daemon mode)
axum = "0.8"
tower-http = { version = "0.6", features = ["cors"] }
tokio-util = "0.7"
tokio-stream = { version = "0.1", features = ["sync"] }
```

- [ ] **Step 7: Remove HTTP client dependency**

Delete lines 78-79:
```toml
# HTTP client (connect command: stdio↔HTTP bridge)
reqwest = { version = "0.12", features = ["json", "stream"] }
```

- [ ] **Step 8: Remove embedded UI dependencies**

Delete lines 81-83:
```toml
# Embedded web UI assets
rust-embed = { version = "8", features = ["debug-embed"] }
mime_guess = "2"
```

- [ ] **Step 9: Remove OpenAPI dependencies**

Delete lines 85-88:
```toml
# OpenAPI documentation
utoipa = { version = "5.4", features = ["axum_extras"] }
utoipa-axum = "0.2"
utoipa-scalar = { version = "0.3", features = ["axum"] }
```

- [ ] **Step 10: Remove percent-encoding**

Delete line 122: `percent-encoding = "2.3.2"`

- [ ] **Step 11: Remove fs2**

Delete lines 124-125:
```toml
# Cross-platform file locking (flock on Unix, LockFileEx on Windows)
fs2 = "0.4"
```

- [ ] **Step 12: Remove platform-specific daemon deps**

Delete lines 127-129:
```toml
# Platform-specific process management (daemon lifecycle)
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

- [ ] **Step 13: Remove tower from dev-dependencies**

Delete line 136: `tower = { version = "0.5", features = ["util"] }`

- [ ] **Step 14: Commit**

```bash
git add Cargo.toml
git commit -m "chore(v5): strip v4.x dependencies from Cargo.toml

Remove axum, tower-http, tokio-util, tokio-stream, reqwest,
rust-embed, mime_guess, utoipa, percent-encoding, fs2, libc,
serde_yaml, tower. Strip rmcp streamable-http-server feature.
Version bump to 5.0.0."
```

---

## Chunk 2: Rewrite Entry Points

This chunk rewrites `main.rs` and `cli.rs` to the simple stdio-only entry point. After this chunk, the binary entry point is correct but the library still won't compile due to daemon references in handler.rs and tools.

### Task 6: Rewrite `src/cli.rs` to minimal clap

**File:** `src/cli.rs`

- [ ] **Step 1: Rewrite cli.rs**

Replace the entire file. Keep `resolve_workspace_root()` (lines 70-120) but remove the `Commands`, `DaemonAction` enums and the `Subcommand` import. New file:

```rust
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "julie-server", version, about = "Julie - Code Intelligence Server")]
pub struct Cli {
    /// Workspace root path (overrides JULIE_WORKSPACE env var)
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

/// Resolve the workspace root path from CLI arg, env var, or current directory.
///
/// Priority order:
/// 1. `--workspace <path>` CLI argument (already parsed by clap)
/// 2. `JULIE_WORKSPACE` environment variable
/// 3. Current working directory (fallback)
///
/// Paths are canonicalized to prevent duplicate workspace IDs for the same logical directory.
/// Tilde expansion is performed for paths like "~/projects/foo".
pub fn resolve_workspace_root(cli_workspace: Option<PathBuf>) -> PathBuf {
    // 1. CLI argument (clap already parsed it, but we still need tilde expansion + canonicalization)
    if let Some(raw_path) = cli_workspace {
        let path_str = raw_path.to_string_lossy();
        let expanded = shellexpand::tilde(&path_str).to_string();
        let path = PathBuf::from(expanded);

        if path.exists() {
            let canonical = path.canonicalize().unwrap_or_else(|e| {
                eprintln!("Warning: Could not canonicalize path {:?}: {}", path, e);
                path.clone()
            });
            eprintln!("Using workspace from CLI argument: {:?}", canonical);
            return canonical;
        } else {
            eprintln!("Warning: --workspace path does not exist: {:?}", path);
        }
    }

    // 2. JULIE_WORKSPACE environment variable
    if let Ok(path_str) = std::env::var("JULIE_WORKSPACE") {
        let expanded = shellexpand::tilde(&path_str).to_string();
        let path = PathBuf::from(expanded);

        if path.exists() {
            let canonical = path.canonicalize().unwrap_or_else(|e| {
                eprintln!("Warning: Could not canonicalize path {:?}: {}", path, e);
                path.clone()
            });
            eprintln!(
                "Using workspace from JULIE_WORKSPACE env var: {:?}",
                canonical
            );
            return canonical;
        } else {
            eprintln!(
                "Warning: JULIE_WORKSPACE path does not exist: {:?}",
                path
            );
        }
    }

    // 3. Fallback to current directory
    let current = std::env::current_dir().unwrap_or_else(|e| {
        eprintln!("Warning: Could not determine current directory: {}", e);
        eprintln!("Using fallback path '.'");
        PathBuf::from(".")
    });

    current.canonicalize().unwrap_or(current)
}
```

- [ ] **Step 2: Commit**

```bash
git add src/cli.rs
git commit -m "refactor(v5): rewrite cli.rs to minimal clap (workspace + version only)"
```

### Task 7: Rewrite `src/main.rs` to stdio-only

**File:** `src/main.rs`

- [ ] **Step 1: Rewrite main.rs**

Combine the v3.9.1-style simplicity with the current `stdio.rs` logic. The new main.rs should:
- Use clap for `--workspace` only
- Log to `{workspace}/.julie/logs/` (project-local)
- Run stdio MCP directly (inlined from `stdio.rs`)

```rust
use std::fs;

use tracing::{debug, error, info, warn};
use tracing_appender::{non_blocking, rolling};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;
use julie::cli::{Cli, resolve_workspace_root};
use julie::handler::JulieServerHandler;
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let workspace_root = resolve_workspace_root(cli.workspace);

    // Initialize logging — file only, stdout reserved for MCP JSON-RPC
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("julie=info"))
        .map_err(|e| anyhow::anyhow!("Failed to initialize logging filter: {}", e))?;

    let logs_dir = workspace_root.join(".julie").join("logs");
    fs::create_dir_all(&logs_dir).unwrap_or_else(|e| {
        eprintln!("Failed to create logs directory at {:?}: {}", logs_dir, e);
    });

    let file_appender = rolling::daily(&logs_dir, "julie.log");
    let (non_blocking_file, _file_guard) = non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking_file)
                .with_target(true)
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    info!("Starting Julie v{} (stdio mode)", env!("CARGO_PKG_VERSION"));
    info!("Workspace root: {:?}", workspace_root);

    // Create handler and start stdio MCP transport
    let handler = JulieServerHandler::new(workspace_root)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create handler: {}", e))?;

    // Capture database reference for shutdown WAL checkpoint
    let db_for_shutdown = if let Ok(Some(workspace)) = handler.get_workspace().await {
        workspace.db.clone()
    } else {
        None
    };

    let service = match handler.serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            error!("Server failed to start: {}", e);
            return Err(anyhow::anyhow!("Server failed to start: {}", e));
        }
    };

    if let Err(e) = service.waiting().await {
        error!("Server error: {}", e);
        return Err(anyhow::anyhow!("Server error: {}", e));
    }

    info!("Julie server stopped");

    // Shutdown cleanup: checkpoint WAL
    if let Some(db_arc) = db_for_shutdown {
        match db_arc.lock() {
            Ok(mut db) => match db.checkpoint_wal() {
                Ok((busy, log, checkpointed)) => {
                    info!(
                        "WAL checkpoint complete: busy={}, log={}, checkpointed={}",
                        busy, log, checkpointed
                    );
                }
                Err(e) => {
                    warn!("WAL checkpoint failed: {}", e);
                }
            },
            Err(e) => {
                warn!("Could not acquire database lock for checkpoint: {}", e);
            }
        }
    } else {
        debug!("No database available for shutdown checkpoint");
    }

    Ok(())
}
```

- [ ] **Step 2: Commit**

```bash
git add src/main.rs
git commit -m "refactor(v5): rewrite main.rs to stdio-only entry point"
```

---

## Chunk 3: Strip Daemon Code from Handler

The handler is the most complex modification — it has daemon branches woven throughout. This chunk surgically removes them.

### Task 8: Strip daemon code from `src/handler.rs`

**File:** `src/handler.rs`

The handler needs these changes:
1. Remove `daemon_state` field from the struct
2. Remove `new_sync()` and `new_with_daemon_state()` constructors
3. Remove daemon branches from workspace accessor helpers
4. Remove daemon branch from `on_initialized`
5. Clean up all daemon imports

- [ ] **Step 1: Remove daemon imports**

Remove these `use` lines:
- `use crate::daemon_state::DaemonState;`
- `use crate::database::SymbolDatabase;`  (only if unused after removing daemon helpers)
- `use crate::search::SearchIndex;` (only if unused after removing daemon helpers)

- [ ] **Step 2: Remove `daemon_state` field from struct**

Remove from the `JulieServerHandler` struct (around line 76):
```rust
pub(crate) daemon_state: Option<Arc<RwLock<DaemonState>>>,
```

- [ ] **Step 3: Remove `new_sync()` constructor (lines ~93-109)**

This was only needed for the HTTP service factory closure. Delete the entire method.

- [ ] **Step 4: Simplify `new()` constructor**

The `new()` method currently delegates to `new_sync()`. Inline the body back:
```rust
pub async fn new(workspace_root: PathBuf) -> Result<Self> {
    info!("Initializing Julie server handler (workspace_root: {:?})", workspace_root);
    Ok(Self {
        workspace_root,
        workspace: Arc::new(RwLock::new(None)),
        is_indexed: Arc::new(RwLock::new(false)),
        indexing_status: Arc::new(IndexingStatus::new()),
        tool_router: Self::tool_router(),
    })
}
```

- [ ] **Step 5: Remove `new_with_daemon_state()` (lines ~111-131)**

Delete the entire method.

- [ ] **Step 6: Remove daemon branches from `get_database_for_workspace()` (lines ~389-440)**

Remove the `if let Some(ref daemon_state) = self.daemon_state { ... }` branch. Keep only the stdio path (opens from primary workspace's index directory).

- [ ] **Step 7: Remove daemon branches from `get_search_index_for_workspace()` (lines ~441-482)**

Same pattern — remove daemon branch, keep stdio path.

- [ ] **Step 8: Remove daemon branches from `get_workspace_root_for_target()` (lines ~483-519)**

Remove daemon branch. Keep the `WorkspaceRegistryService` lookup path.

- [ ] **Step 9: Strip daemon branch from `on_initialized` (lines ~722-803)**

The `on_initialized` method has a large `if self.daemon_state.is_some()` block that does roots/list discovery, percent-encoding URI parsing, and daemon workspace registration. Remove the entire daemon branch. Keep only the stdio branch (line ~806-810) which spawns `run_auto_indexing()`.

Remove the `percent_encoding` import that was used in this block.

- [ ] **Step 10: Run `cargo check` to verify compilation of handler.rs**

```bash
cargo check 2>&1 | head -30
```

This will still fail on tool files, but handler.rs errors should be resolved.

- [ ] **Step 11: Commit**

```bash
git add src/handler.rs
git commit -m "refactor(v5): strip all daemon branches from handler.rs

Remove daemon_state field, new_sync/new_with_daemon_state constructors,
daemon branches from workspace accessor helpers, and daemon on_initialized block."
```

---

## Chunk 4: Strip Federation from Tools

This chunk removes `WorkspaceTarget::All` from the enum and all tool files that match on it, plus federation-related imports and dead code.

### Task 9: Remove `WorkspaceTarget::All` from resolution.rs

**File:** `src/tools/navigation/resolution.rs`

- [ ] **Step 1: Remove `All` variant from `WorkspaceTarget` enum**

Find the enum definition and remove the `All` variant.

- [ ] **Step 2: Remove `"all"` match arm in `resolve_workspace_filter()`**

Remove the `"all" => Ok(WorkspaceTarget::All)` arm (around line 88).

- [ ] **Step 3: Remove `handler.daemon_state` reference**

Around line 90, there's a daemon_state check for workspace ID validation. Remove the daemon branch, keep only the `WorkspaceRegistryService` validation path.

- [ ] **Step 4: Commit**

```bash
git add src/tools/navigation/resolution.rs
git commit -m "refactor(v5): remove WorkspaceTarget::All variant and daemon branch from resolution"
```

### Task 10: Strip federation from search tools

**Files:**
- `src/tools/search/mod.rs`
- `src/tools/search/line_mode.rs`

- [ ] **Step 1: Remove federated search from `src/tools/search/mod.rs`**

- Remove the `workspace="all"` early-return block (around lines 92-97)
- Remove the `WorkspaceTarget::All => unreachable!()` arm (around line 163)
- Remove the entire `federated_search()` method (lines ~230-383)
- Remove `use crate::daemon_state::WorkspaceLoadStatus;` import
- Remove `use crate::tools::federation::search::*` imports

- [ ] **Step 2: Remove federation from `src/tools/search/line_mode.rs`**

- Remove the `WorkspaceTarget::All` check (line 35)
- Remove `WorkspaceTarget::All => unreachable!()` arm (line 233)
- Remove any other `All` references

- [ ] **Step 3: Remove federation dead code from `src/tools/search/formatting.rs`**

After removing `federated_search()`, these functions become dead code:
- `format_federated_lean_results()`
- `format_federated_definition_results()`
- `count_unique()` (if only used by the above)

Remove them.

- [ ] **Step 4: Commit**

```bash
git add src/tools/search/mod.rs src/tools/search/line_mode.rs \
       src/tools/search/formatting.rs
git commit -m "refactor(v5): remove federated search (workspace='all') from search tools"
```

### Task 11: Strip federation from deep_dive

**Files:**
- `src/tools/deep_dive/mod.rs`
- `src/tools/deep_dive/formatting.rs`

- [ ] **Step 1: Remove federated deep_dive from `mod.rs`**

- Remove the `WorkspaceTarget::All` match arm (around line 110) that calls `self.federated_deep_dive()`
- Remove the entire `federated_deep_dive()` method (lines ~156-292)
- Remove federation-only types and functions that are now dead code: `WorkspaceEntry` struct (~line 296), `CrossProjectCaller` struct (~line 303), `format_cross_project_callers()` (~line 311), `find_cross_project_callers()` (~line 334), `deep_dive_query_with_project()` (~line 408)
- Remove `use crate::daemon_state::WorkspaceLoadStatus;` import
- Clean up `project_name` parameter: in `deep_dive_query_impl()`, remove the `project_name: Option<&str>` parameter. Trace all callers and remove the argument. Also remove the code path inside `deep_dive_query_impl` that calls `format_symbol_context_with_project` when `project_name.is_some()`.

- [ ] **Step 2: Remove federation formatting from `formatting.rs`**

- Remove `format_symbol_context_with_project()` function
- If `format_symbol_context_impl()` has a `project_name` parameter only used by the removed function, simplify it too

- [ ] **Step 3: Commit**

```bash
git add src/tools/deep_dive/mod.rs src/tools/deep_dive/formatting.rs
git commit -m "refactor(v5): remove federated deep_dive (workspace='all')"
```

### Task 12: Strip federation from get_context

**Files:**
- `src/tools/get_context/mod.rs`
- `src/tools/get_context/pipeline.rs`

- [ ] **Step 1: Remove federation from `mod.rs`**

- Remove `pub mod federated;` declaration (around line 9)
- Remove any `workspace="all"` branch in the tool call handler

- [ ] **Step 2: Remove federation from `pipeline.rs`**

- Remove the `WorkspaceTarget::All` match arm (around line 515) that calls `super::federated::run_federated()`

- [ ] **Step 3: Commit**

```bash
git add src/tools/get_context/mod.rs src/tools/get_context/pipeline.rs
git commit -m "refactor(v5): remove federated get_context (workspace='all')"
```

### Task 13: Strip federation from navigation and symbols

**Files:**
- `src/tools/navigation/fast_refs.rs`
- `src/tools/navigation/formatting.rs`
- `src/tools/symbols/mod.rs`

- [ ] **Step 1: Remove `federated_refs` module declaration from `navigation/mod.rs`**

In `src/tools/navigation/mod.rs`, remove line 12: `pub(crate) mod federated_refs;` — the file it points to was deleted in Task 1.

- [ ] **Step 2: Remove federation from `fast_refs.rs`**

- Remove `use super::federated_refs;` import (line 20) — the module no longer exists
- Remove the `WorkspaceTarget::All` early check (around line 75) that calls `find_refs_federated()`
- Remove the `WorkspaceTarget::All` match arm (around line 121)

- [ ] **Step 3: Remove federation dead code from `formatting.rs`**

- Remove `ProjectTaggedResult` struct (around line 130)
- Remove `format_federated_refs_results()` function (around line 156, ~100 lines)

- [ ] **Step 4: Remove `WorkspaceTarget::All` from `symbols/mod.rs`**

- Remove the `WorkspaceTarget::All` match arm (around line 90)

- [ ] **Step 5: Commit**

```bash
git add src/tools/navigation/mod.rs src/tools/navigation/fast_refs.rs \
       src/tools/navigation/formatting.rs src/tools/symbols/mod.rs
git commit -m "refactor(v5): remove federation from fast_refs, navigation formatting, and symbols"
```

### Task 14: Remove `pub mod federation;` from tools/mod.rs

**File:** `src/tools/mod.rs`

- [ ] **Step 1: Remove federation module declaration**

Remove lines 16-17:
```rust
// Cross-workspace federation (daemon mode only)
pub mod federation;
```

- [ ] **Step 2: Commit**

```bash
git add src/tools/mod.rs
git commit -m "refactor(v5): remove federation module declaration from tools"
```

---

## Chunk 5: Fix Remaining Compilation Issues

These are smaller surgical fixes in files that import from deleted modules or use removed types.

### Task 15: Fix `src/tools/workspace/commands/index.rs`

**File:** `src/tools/workspace/commands/index.rs`

- [ ] **Step 1: Remove daemon registration block**

Lines 228-237 reference `handler.daemon_state` and `crate::daemon_state::DaemonState::register_project()`. This block registers the project with the daemon after indexing. Remove the entire daemon block. The stdio-mode indexing path does not need daemon registration.

- [ ] **Step 2: Commit**

```bash
git add src/tools/workspace/commands/index.rs
git commit -m "fix(v5): remove daemon registration from workspace index command"
```

### Task 16: Fix `src/tools/workspace/paths.rs`

**File:** `src/tools/workspace/paths.rs`

- [ ] **Step 1: Replace daemon import**

Line 1 imports `use crate::daemon::julie_home;`. Replace with a local helper using env vars (matching the original `daemon::julie_home()` implementation — do NOT use the `dirs` crate since it's only a transitive dependency and may disappear after dep pruning):

```rust
/// Get the global Julie home directory (~/.julie/).
fn julie_home() -> anyhow::Result<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| anyhow::anyhow!("Could not determine home directory"))?;
    Ok(std::path::PathBuf::from(home).join(".julie"))
}
```

- [ ] **Step 2: Commit**

```bash
git add src/tools/workspace/paths.rs
git commit -m "fix(v5): replace daemon::julie_home import with local helper in paths.rs"
```

### Task 16: Fix `src/health.rs`

**File:** `src/health.rs`

- [ ] **Step 1: Remove daemon branch**

Remove the `if let Some(daemon_state) = &handler.daemon_state { ... }` block (lines ~66-91). Keep only the primary workspace and reference workspace health check paths that work in stdio mode.

- [ ] **Step 2: Commit**

```bash
git add src/health.rs
git commit -m "fix(v5): remove daemon branch from health.rs"
```

### Task 17: Fix workspace registry commands

**Files:**
- `src/tools/workspace/commands/registry/add_remove.rs`
- `src/tools/workspace/commands/registry/list_clean.rs`

- [ ] **Step 1: Fix `add_remove.rs`**

- Remove `use crate::daemon_state::DaemonState;` import (line 2)
- Remove the daemon dispatch check: `if let Some(daemon_state) = &handler.daemon_state { ... }` (around line 27-31) and `handle_add_command_daemon()` (lines ~38-82)
- Make `handle_add_command` call the stdio path directly (rename `handle_add_command_stdio` to be the main implementation or inline it)
- Same for remove: remove daemon dispatch (lines ~242-246), delete `handle_remove_command_daemon()` (lines ~252-280), make stdio path the default

- [ ] **Step 2: Fix `list_clean.rs`**

- Remove `use crate::daemon_state::{DaemonState, WorkspaceLoadStatus};` import (line 2)
- Remove the daemon dispatch: `if let Some(daemon_state) = &handler.daemon_state { ... }` (around line 23-27)
- Delete `handle_list_command_daemon()` (lines ~34-152)
- Make `handle_list_command_stdio()` the main implementation

- [ ] **Step 3: Commit**

```bash
git add src/tools/workspace/commands/registry/add_remove.rs \
       src/tools/workspace/commands/registry/list_clean.rs
git commit -m "fix(v5): remove daemon branches from workspace registry commands"
```

### Task 18: Fix `src/tests/core/handler.rs`

**File:** `src/tests/core/handler.rs`

- [ ] **Step 1: Check and fix handler test file**

This file imports `DaemonState` and `GlobalRegistry`. Read the file to determine if ANY tests remain after removing daemon-related tests. If the entire file is daemon-only, delete it and remove the `pub mod handler;` declaration from the `core` module in `src/tests/mod.rs` (line 79). If some tests are non-daemon, keep those and remove only the daemon ones.

- [ ] **Step 2: Commit**

```bash
git add src/tests/core/ src/tests/mod.rs
git commit -m "fix(v5): clean up handler test file (remove daemon imports)"
```

### Task 19: Fix CLI tests

**File:** `src/tests/cli_tests.rs`

- [ ] **Step 1: Check CLI tests for daemon references**

The CLI tests likely reference the `Commands` enum and `DaemonAction` which no longer exist. Read the file and:
- Remove all tests that test daemon/connect/install subcommand parsing
- Update `test_no_args_parses_to_stdio_mode` — it references `cli.command` which no longer exists in the simplified `Cli` struct. Rewrite to just verify the struct parses with no args and `workspace` is `None`.
- Keep tests for `resolve_workspace_root()` and basic `--workspace` parsing

- [ ] **Step 2: Commit**

```bash
git add src/tests/cli_tests.rs
git commit -m "fix(v5): update CLI tests for simplified argument structure"
```

---

## Chunk 6: Compilation Check and Agent Instructions

### Task 20: First compilation check

- [ ] **Step 1: Run `cargo check`**

```bash
cargo check 2>&1 | head -50
```

- [ ] **Step 2: Fix any remaining compilation errors**

Iterate: read the error, find the offending line, fix it. Common issues:
- Stale imports of deleted modules in files not covered above
- `WorkspaceTarget::All` arms in match statements not yet cleaned
- Unused imports after removing daemon code

- [ ] **Step 3: Run `cargo check` again until clean**

```bash
cargo check 2>&1 | head -50
```

- [ ] **Step 4: Commit any additional fixes**

```bash
git add -A
git commit -m "fix(v5): resolve remaining compilation errors"
```

### Task 21: Update `JULIE_AGENT_INSTRUCTIONS.md`

**File:** `JULIE_AGENT_INSTRUCTIONS.md`

- [ ] **Step 1: Revert manage_workspace instruction**

Find the `manage_workspace` section (around line 123-127) and replace:

```markdown
**First action in any session:** If any tool returns a "no database" or "not indexed" error, call `manage_workspace(operation="index")` to register your project. **You MUST include the `path` parameter** set to your project root directory — the server cannot auto-detect it over HTTP.
```javascript
manage_workspace(operation="index", path="/absolute/path/to/your/project")
```
```

With the stronger v3.9.1 wording:

```markdown
**First action in new workspace:** `manage_workspace(operation="index")`
```

Remove the `path` parameter requirement — stdio mode auto-detects the workspace.

- [ ] **Step 2: Remove any `workspace="all"` references from tool documentation**

Search the file for `workspace="all"` or `"all"` federation references. The `workspace` parameter description in tools should mention only `"primary"` (default) and reference workspace IDs.

- [ ] **Step 3: Commit**

```bash
git add JULIE_AGENT_INSTRUCTIONS.md
git commit -m "docs(v5): revert agent instructions to proactive workspace init wording"
```

---

## Chunk 7: Test and Validate

### Task 22: Run fast test tier

- [ ] **Step 1: Run fast tests**

```bash
cargo test --lib -- --skip search_quality 2>&1 | tail -20
```

Expected: All non-dogfood tests pass. If failures, investigate and fix.

- [ ] **Step 2: Fix any test failures**

Read failure output, trace to source, fix. Common issues:
- Tests that reference `daemon_state` on handler struct
- Tests that construct handler with daemon state
- Tests that check for `WorkspaceTarget::All`

- [ ] **Step 3: Commit fixes**

```bash
git add -A
git commit -m "fix(v5): resolve test failures after simplification"
```

### Task 23: Run full test tier

- [ ] **Step 1: Run full test suite**

```bash
cargo test --lib 2>&1 | tail -10
```

Expected: All tests pass including dogfood/search_quality tests.

- [ ] **Step 2: Fix any remaining failures and commit**

### Task 24: Update Cargo.lock

- [ ] **Step 1: Regenerate lock file**

```bash
cargo generate-lockfile
```

Do NOT use `cargo update` — that would also update all transitive dependency versions, which is a separate concern. `cargo generate-lockfile` cleanly regenerates based on the current `Cargo.toml`.

- [ ] **Step 2: Commit**

```bash
git add Cargo.lock
git commit -m "chore(v5): regenerate Cargo.lock after dependency removal"
```

### Task 25: Update CLAUDE.md

**File:** `CLAUDE.md`

- [ ] **Step 1: Update project description**

The CLAUDE.md still references daemon mode, connect command, web dashboard, and OpenAPI docs. Update the "Key Project Facts" section:
- Remove: "Modes: Daemon (persistent HTTP server on port 7890) with `connect` command..."
- Replace with: "Mode: Stdio MCP server (spawned per session by MCP clients)"
- Remove references to web dashboard at `/ui/` and OpenAPI docs at `/api/docs`
- Update version reference to v5.0.0

- [ ] **Step 2: Update log location section**

Remove the daemon log location (`~/.julie/logs/`). All logs are now project-local `.julie/logs/`.

- [ ] **Step 3: Remove "can't build release while running server" note**

The last line of CLAUDE.md says: "You CANNOT build the release build while we're running the server in session!" — This no longer applies since there's no persistent daemon. Each session spawns its own process.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(v5): update CLAUDE.md for stdio-only architecture"
```

### Task 26: Final validation — build and verify binary

- [ ] **Step 1: Build release binary**

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 2: Check binary size reduction**

```bash
ls -lh target/release/julie-server
```

Expected: Noticeably smaller without axum/tower/reqwest/utoipa.

- [ ] **Step 3: Verify `--help` output**

```bash
./target/release/julie-server --help
```

Expected: Simple help with just `--workspace` and `--version`. No daemon/connect/install subcommands.

- [ ] **Step 4: Verify `--version` output**

```bash
./target/release/julie-server --version
```

Expected: `julie-server 5.0.0`
