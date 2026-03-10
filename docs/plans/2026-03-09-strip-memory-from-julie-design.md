# Strip Memory Tools from Julie

## Problem

Julie v4.0 absorbed Goldfish's memory system (checkpoint, recall, plan), adding ~2,800
lines of code, 10+ test files, REST endpoints, dashboard views, and a plugin system.
This created:

- Distribution complexity: plugin clones 150MB repo for ~750 lines of hooks/skills
- Feature overlap: Claude Code and other AI tools are adding built-in memory
- Forced coupling: users who don't want memory still get 3 extra MCP tools
- Maintenance burden: memory code has no relation to code intelligence

Meanwhile, Goldfish already works and has its own distribution story.

## Decision

Strip all memory functionality from Julie. Maintain Goldfish separately for users
who want memory tooling.

## What Julie Keeps (7 tools)

1. `fast_search` — text search with code-aware tokenization
2. `get_symbols` — file structure without reading full content
3. `deep_dive` — understand a symbol before modifying it
4. `fast_refs` — find all references
5. `get_context` — token-budgeted codebase orientation
6. `rename_symbol` — safe workspace-wide renames
7. `manage_workspace` — workspace indexing and management

Dashboard keeps: workspaces, search, stats. No memory/plan/standup views.

## What Gets Removed

### Rust source
- `src/memory/` (9 files, ~2,828 lines) — checkpoint, recall, plan, storage, git, index, embedding, date_filter, mod
- `src/tools/memory/` (4 files) — MCP tool wrappers for checkpoint, recall, plan
- `src/search/unified.rs` + `src/search/content_type.rs` — unified code+memory search
- `src/api/memories.rs` — REST endpoints for dashboard memory/plan views

### Tests
- `src/tests/memory_checkpoint_tests.rs`
- `src/tests/memory_git_tests.rs`
- `src/tests/memory_index_tests.rs`
- `src/tests/memory_plan_tests.rs`
- `src/tests/memory_cross_project_tests.rs`
- `src/tests/memory_embedding_tests.rs`
- `src/tests/memory_recall_tests.rs`
- `src/tests/memory_storage_tests.rs`
- `src/tests/memory_tool_tests.rs`
- `src/tests/memory_integration_tests.rs`
- `src/tests/unified_search_tests.rs`
- `src/tests/api_search_unified_tests.rs`

### Plugin & distribution
- `julie-plugin/` directory (hooks, skills, plugin config)
- `.claude-plugin/` directory (marketplace config)

### Dashboard UI
- Memory/plan/standup Vue components and routes

### Wiring to update
- `src/tools/mod.rs` — remove memory exports
- `src/handler.rs` — remove memory tool registration
- `src/api/` router — remove memory routes
- `src/search/mod.rs` — remove unified search exports
- `src/tests/mod.rs` — remove memory test module declarations
- `src/install.rs` — remove plugin marketplace instructions from install output
- `Cargo.toml` — remove any deps that become unused

## Install Story (after)

1. Download binary or `cargo install julie`
2. `julie-server install`
3. Add MCP config: `{"type": "http", "url": "http://localhost:7890/mcp"}`
4. Done. No hooks, no skills, no plugins.

## Acceptance Criteria

- [ ] Julie exposes exactly 7 MCP tools (no checkpoint/recall/plan)
- [ ] `src/memory/` directory does not exist
- [ ] `julie-plugin/` directory does not exist
- [ ] `.claude-plugin/` directory does not exist
- [ ] Dashboard loads without memory/plan/standup views
- [ ] All remaining tests pass (`cargo test --lib -- --skip search_quality`)
- [ ] `install` command output has no plugin/marketplace references
- [ ] No compile warnings from dead code
