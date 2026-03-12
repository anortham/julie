# Julie v5.0.0 — The Great Simplification

## Summary

Strip all v4.x daemon/HTTP/UI/tray/agent/install infrastructure from HEAD, returning Julie to a stdio-only MCP code intelligence server while retaining all core engine improvements made since v3.9.1.

## Problem

The v4.x release cycle (v4.0.0–v4.2.3) added ~12,500 lines of daemon/HTTP/UI/tray/agent production code plus ~10,800 lines of associated test code. This layer:

- **Degraded the primary use case**: The daemon startup/health-check chain introduces failure modes that make agents less likely to reach for Julie tools
- **Weakened agent instructions**: The `manage_workspace` instruction changed from proactive ("first action in new workspace") to reactive ("if you get an error"), making workspace initialization less reliable
- **Consumed disproportionate maintenance effort**: 29+ fix commits since v4.0.0, mostly addressing daemon startup races, connect bridge death spirals, Windows compat, workspace registration bugs, lock ordering deadlocks, and PID file races
- **Added no user value**: The dashboard, tray app, multi-agent dispatch, and system service install are unused. The tray app exists to manage the daemon; the daemon exists to serve the HTTP API; the HTTP API exists to serve the dashboard — a circular dependency of features justifying each other

The core engine (31 tree-sitter extractors, Tantivy search, 7 MCP tools) is solid and has been since v3.9.1. The sidecar singleton concern that motivated the daemon is already handled in stdio mode.

## Approach

Surgical removal from HEAD (not cherry-picking back to v3.9.1). This preserves all legitimate core improvements: Tantivy migration refinements, search quality work, sidecar binary distribution, extractor fixes, and bug fixes to the search/indexing pipeline.

## Scope

### Delete entirely — production code

| Component | Files | ~Lines |
|---|---|---|
| Daemon core | `src/daemon.rs`, `src/daemon_state.rs`, `src/daemon_indexer.rs`, `src/daemon_state_watchers.rs`, `src/daemon_watcher.rs` | 1,976 |
| Connect bridge | `src/connect.rs` | 600 |
| HTTP server | `src/server.rs`, `src/mcp_http.rs` | 386 |
| REST API | `src/api/` (7 files) | 2,337 |
| Agent dispatch | `src/agent/` (5+ files) | 925 |
| System install | `src/install.rs` | 503 |
| Binary monitor | `src/binary_monitor.rs` | 85 |
| UI module | `src/ui.rs` | 78 |
| Registry | `src/registry/` | 227 |
| Federation tools | `src/tools/federation/` (3 files) | 593 |
| Federated get_context | `src/tools/get_context/federated.rs` | 166 |
| Federated fast_refs | `src/tools/navigation/federated_refs.rs` | 332 |
| Web dashboard | `ui/` (entire directory) | ~4,270 |
| Tray app | `tauri-app/` (entire directory) | entire directory |
| Build script | `build.rs` (entire file — only builds Vue UI assets) | ~112 |
| **Subtotal production** | | **~12,500+** |

### Delete entirely — test code

These test files directly import from deleted modules and will cause compilation failures if not removed. Remove the files AND their `pub mod` declarations in `src/tests/mod.rs`.

| Test file | ~Lines | Imports from |
|---|---|---|
| `tests/daemon_tests.rs` | 296 | `daemon::*` |
| `tests/daemon_indexer_tests.rs` | 865 | `daemon_indexer`, `daemon_state`, `server::AppState` |
| `tests/daemon_watcher_tests.rs` | 339 | `daemon_watcher`, `daemon_state` |
| `tests/daemon_integration_tests.rs` | 736 | `api`, `daemon_state`, `mcp_http`, `server`, `ui` |
| `tests/connect_tests.rs` | 419 | `daemon::*`, `axum` |
| `tests/server_tests.rs` | 856 | `api`, `daemon_state`, `mcp_http`, `server::AppState` |
| `tests/api_dashboard_tests.rs` | 258 | `api`, `daemon_state`, `server::AppState` |
| `tests/api_search_tests.rs` | 782 | `api`, `daemon_state`, `server::AppState` |
| `tests/api_agents_tests.rs` | 446 | `agent::*`, `api`, `daemon_state`, `server::AppState` |
| `tests/phase4_integration_tests.rs` | 610 | `agent::*`, `api`, `daemon_state`, `server::AppState` |
| `tests/federation_tests.rs` | 838 | `federation` |
| `tests/federation_integration_tests.rs` | 410 | `daemon_state`, `registry` |
| `tests/ui_tests.rs` | 301 | `ui`, `axum` |
| `tests/registry_tests.rs` | 275 | `registry::GlobalRegistry` |
| `tests/agent_backend_tests.rs` | 416 | `agent::*` |
| `tests/binary_monitor_tests.rs` | 131 | `binary_monitor`, `tokio_util` |
| `tests/tools/daemon_workspace_tests.rs` | 557 | `daemon_state`, `registry` |
| `tests/tools/workspace_target_tests.rs` | 254 | `daemon_state`, `registry` |
| `tests/tools/federated_refs_tests.rs` | 892 | `daemon_state`, `registry` |
| `tests/tools/deep_dive_federation_tests.rs` | 353 | federation code |
| `tests/tools/get_context_federation_tests.rs` | 311 | federation code |
| `tests/tools/search/federated_search_tests.rs` | 383 | federation code |
| `tests/core/handler.rs` | 92 | `DaemonState`, `GlobalRegistry` |
| **Subtotal test** | | **~10,800** |

**Total deletion: ~23,300+ lines** (production + test).

### Modify

#### `src/lib.rs`
Remove `pub mod` declarations for all deleted modules:
- Delete: `agent`, `api`, `binary_monitor`, `connect`, `daemon`, `daemon_indexer`, `daemon_state`, `daemon_state_watchers`, `daemon_watcher`, `install`, `mcp_http`, `registry`, `server`, `ui`
- Rewrite: `cli` (minimal clap struct + `resolve_workspace_root()`)
- Inline into `main.rs` then delete: `stdio` (the stdio transport setup is trivial)

#### `src/main.rs`
Rewrite to v3.9.1 simplicity:
- Parse args via minimal clap (`--workspace`, `--version`, `--help`)
- Set up logging to `{project}/.julie/logs/` (project-local, not `~/.julie/logs/`)
- Initialize `JulieServerHandler`
- Start stdio MCP transport via rmcp (inline from current `stdio.rs`)
- No subcommands, no daemon, no connect

#### `src/cli.rs`
Rewrite to minimal clap struct:
```rust
#[derive(Parser)]
#[command(name = "julie-server", version, about = "Julie - Code Intelligence Server")]
struct Cli {
    /// Workspace root directory
    #[arg(long)]
    workspace: Option<PathBuf>,
}
```
The `resolve_workspace_root()` function stays (it has the tilde expansion + canonicalization logic).

#### `src/handler.rs`
- Remove `daemon_state: Option<Arc<RwLock<DaemonState>>>` field
- Remove `new_with_daemon_state()` constructor
- Remove `new_sync()` (only needed for HTTP service factory)
- Remove `get_database_for_workspace()` and `get_search_index_for_workspace()` daemon-mode branches — keep only the stdio-mode path (open from primary workspace's index directory)
- Remove `get_workspace_root_for_workspace()` daemon-mode branch
- Remove daemon-mode `on_initialized` block (roots/list URI decoding with `percent-encoding`)
- Clean up all `use crate::daemon_state::*` imports

#### `src/health.rs`
- Remove daemon-mode branch that checks `handler.daemon_state` for non-primary workspace health
- Simplify to stdio-only health check

#### `src/tools/search/mod.rs`
- Remove the `workspace="all"` federated search branch (~150 lines)
- Remove `use crate::daemon_state::WorkspaceLoadStatus` import
- Keep primary + reference workspace search

#### `src/tools/deep_dive/mod.rs`
- Remove `workspace="all"` federated deep_dive branch
- Remove `use crate::daemon_state::WorkspaceLoadStatus` import
- Clean up `project_name: Option<&str>` parameter in `deep_dive_impl` (only existed for federation formatting — can be removed or hardcoded to `None`)
- Keep primary + reference workspace support

#### `src/tools/deep_dive/formatting.rs`
- Remove `format_symbol_context_with_project` and any federation-only formatting helpers (dead code after federation removal)

#### `src/tools/get_context/mod.rs`
- Remove `pub mod federated;` declaration
- Remove `workspace="all"` branch that calls `federated::run_federated()`

#### `src/tools/get_context/pipeline.rs`
- Remove `WorkspaceTarget::All` match arm that calls `super::federated::run_federated()`

#### `src/tools/search/line_mode.rs`
- Remove `WorkspaceTarget::All` references (3 occurrences)

#### `src/tools/symbols/mod.rs`
- Remove `WorkspaceTarget::All` match arm

#### `src/tools/navigation/resolution.rs`
- Remove `WorkspaceTarget::All` variant from the enum
- Remove `"all" => Ok(WorkspaceTarget::All)` match arm
- Remove `handler.daemon_state` branch
- All tools matching on `WorkspaceTarget` will need the `All` arm removed too

#### `src/tools/navigation/fast_refs.rs`
- Remove `workspace="all"` branch

#### `src/tools/navigation/formatting.rs`
- Remove `ProjectTaggedResult` struct and `format_federated_refs_results` function (dead code after federation removal)

#### `src/tools/workspace/paths.rs`
- Replace `use crate::daemon::julie_home` with a local implementation or move `julie_home()` to `src/utils/`. The function is just `dirs::home_dir() + ".julie"` — used for the global `~/.julie/` skip logic during workspace root detection.

#### `src/tools/workspace/commands/registry/add_remove.rs`
- Remove `use crate::daemon_state::DaemonState` import
- Remove daemon-mode branches in `handle_add_command` and `handle_remove_command`
- Keep stdio-mode workspace registration logic

#### `src/tools/workspace/commands/registry/list_clean.rs`
- Remove `use crate::daemon_state::{DaemonState, WorkspaceLoadStatus}` imports
- Remove daemon-mode branches in `handle_list_command`
- Keep stdio-mode listing logic

#### `src/tools/refactoring/rename.rs`
- No changes needed. The `scope="all"` option means "all files in this workspace", not federation. It is unrelated to daemon mode.

#### `src/tests/mod.rs`
- Remove all `pub mod` declarations for deleted test files (see test deletion table above)
- This includes top-level declarations (daemon, connect, server, api, federation, ui, registry, agent, binary_monitor, phase4) AND declarations inside the `pub mod tools { }` block (`deep_dive_federation_tests`, `workspace_target_tests`, `daemon_workspace_tests`, `federated_refs_tests`, `get_context_federation_tests`, `search/federated_search_tests`)

#### `src/tests/core/handler.rs`
- Remove daemon-state and registry imports/tests. If the entire file is daemon-only, delete it and remove from `tests/core/mod.rs`.

#### `JULIE_AGENT_INSTRUCTIONS.md`
Revert `manage_workspace` section to stronger v3.9.1 wording:
```
**First action in new workspace:** `manage_workspace(operation="index")`
```
Remove the `path` parameter requirement (stdio mode auto-detects workspace).

#### `Cargo.toml`
- Remove workspace members: `tauri-app/src-tauri`
- Remove `transport-streamable-http-server` from rmcp features
- Remove daemon/HTTP dependencies: `axum`, `tower-http`, `tokio-util`, `tokio-stream`, `reqwest`, `rust-embed`, `mime_guess`, `utoipa`, `utoipa-axum`, `utoipa-scalar`, `percent-encoding`, `fs2`
- Remove `clap` `env` feature (only used by daemon CLI port args)
- Remove `serde_yaml` (unused anywhere in `src/`)
- Remove platform-specific deps: `libc` under `[target.'cfg(unix)'.dependencies]`
- Remove `tower` from `[dev-dependencies]` (only used by deleted test files)
- Keep: `fastembed`, `ort`, `sqlite-vec`, `zerocopy` (embeddings-ort feature, unrelated to daemon)
- Version: `5.0.0`

Note: Expect a large `Cargo.lock` diff from the dependency removals.

### Keep unchanged

- All 31 tree-sitter extractors
- Tantivy search engine + CodeTokenizer + language configs
- SQLite structured storage + all migrations (1-8)
- All 7 MCP tools: `fast_search`, `fast_refs`, `deep_dive`, `get_context`, `get_symbols`, `rename_symbol`, `manage_workspace`
- Reference workspace support (stdio-mode path: open from primary workspace's `.julie/indexes/`)
- Python sidecar + binary distribution (`src/embeddings/`)
- File watcher (`src/watcher/`)
- Startup module (`src/startup/`) — used by handler for auto-indexing
- All search quality improvements, tokenizer refinements, scoring changes
- All non-daemon test infrastructure in `src/tests/`
- Optional `embeddings-ort` feature and related dependencies (`fastembed`, `ort`, `sqlite-vec`, `zerocopy`)

## MCP Client Configuration

After v5.0.0, the MCP config simplifies:

**Before (v4.x with connect bridge):**
```json
{
  "julie": {
    "type": "stdio",
    "command": "/path/to/julie-server",
    "args": ["connect"]
  }
}
```

**After (v5.0.0 stdio-only):**
```json
{
  "julie": {
    "type": "stdio",
    "command": "/path/to/julie-server",
    "args": []
  }
}
```

Note: The goldfish project's config is already in the v5 style (no `connect` arg). Only the julie project config and any others using `connect` need updating.

## Validation

1. **Fast test tier passes**: `cargo test --lib -- --skip search_quality` (~15s)
2. **Full test tier passes**: `cargo test --lib` (~265s)
3. **Dogfood verification**: Build release, restart Claude Code, verify all 7 Julie tools work in a live MCP session
4. **Binary size check**: Confirm binary is smaller without axum/tower/reqwest/utoipa/tauri dependencies
5. **Startup time check**: Confirm stdio startup is near-instant (no health check dance)

## Non-goals

- Changing the sidecar architecture (separate discussion post-v5)
- Adding new features
- Refactoring the core engine
