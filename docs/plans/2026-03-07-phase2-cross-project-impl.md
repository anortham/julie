# Phase 2: Cross-Project Intelligence — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Enable `workspace="all"` across fast_search, fast_refs, deep_dive, and get_context so MCP clients can search across all registered projects in daemon mode.

**Architecture:** Add `Option<Arc<RwLock<DaemonState>>>` to `JulieServerHandler`. Introduce a `WorkspaceTarget` enum to replace `Option<String>` in workspace resolution. Create a `src/tools/federation/` module with parallel fan-out and RRF merging. Tag federated results with project name in output formatters.

**Tech Stack:** tokio::spawn for parallel fan-out, existing `rrf_merge` pattern from `src/search/hybrid.rs`, existing `DaemonState` from Phase 1.

---

### Task 1: WorkspaceTarget Enum

**Files:**
- Modify: `src/tools/navigation/resolution.rs:38-97` (replace `resolve_workspace_filter` return type)
- Modify: `src/tools/search/mod.rs:185-271` (replace `resolve_workspace_filter` return type)
- Test: `src/tests/` (new file or extend existing resolution tests)

**What to build:** Replace the current `Option<String>` / `Option<Vec<String>>` return types from the two `resolve_workspace_filter` functions with a `WorkspaceTarget` enum:

```rust
pub enum WorkspaceTarget {
    Primary,
    Reference(String),       // specific workspace ID
    All,                     // federated across all daemon workspaces
}
```

The navigation version (`resolution.rs`) currently returns `Option<String>` (None=primary, Some=reference). The search version (`search/mod.rs`) returns `Option<Vec<String>>`. Both need to return `WorkspaceTarget`. The `"all"` match arm currently returns an error — change it to return `WorkspaceTarget::All`.

**Approach:**
- Define `WorkspaceTarget` in `resolution.rs` (it's already the canonical location for workspace resolution)
- Update `resolution.rs::resolve_workspace_filter` to return `Result<WorkspaceTarget>` — Primary for "primary", Reference(id) for valid ID, All for "all"
- Update `search/mod.rs::resolve_workspace_filter` to also return `Result<WorkspaceTarget>` — same mapping
- Update all 6 callers to match on the new enum. For now, `All` arms return an error ("Cross-project search requires daemon mode — coming soon") — federation is wired in Task 4+
- The key callers are:
  - `src/tools/navigation/fast_refs.rs:97` (`find_references_and_definitions`)
  - `src/tools/deep_dive/mod.rs:74` (`call_tool`)
  - `src/tools/search/mod.rs:86,133` (`call_tool` for fast_search)
  - `src/tools/get_context/pipeline.rs:464` (`run`)
  - `src/tools/symbols/mod.rs:74` (`call_tool`)

**Acceptance criteria:**
- [ ] `WorkspaceTarget` enum defined with `Primary`, `Reference(String)`, `All` variants
- [ ] Both `resolve_workspace_filter` functions return `Result<WorkspaceTarget>`
- [ ] All 6 callers updated to match on enum — `All` temporarily returns a clear error message
- [ ] `"all"` no longer returns the old "not supported" error — returns `WorkspaceTarget::All`
- [ ] Existing tests pass (workspace="primary" and workspace=<id> behavior unchanged)
- [ ] New test: passing `workspace="all"` returns `WorkspaceTarget::All` from resolver
- [ ] Tests pass, committed

---

### Task 2: DaemonState on JulieServerHandler

**Files:**
- Modify: `src/handler.rs:56-95` (add `daemon_state` field, update constructors)
- Modify: `src/mcp_http.rs:39-60` (`create_mcp_service` — no daemon_state for default endpoint)
- Modify: `src/daemon_state.rs:199-217` (`create_workspace_mcp_service` — inject daemon_state)
- Test: `src/tests/` (verify handler carries daemon_state in daemon mode, None in stdio)

**What to build:** Add `pub(crate) daemon_state: Option<Arc<RwLock<DaemonState>>>` to `JulieServerHandler`. In daemon mode, the MCP service factory injects it. In stdio mode, it's None.

**Approach:**
- Add the field to `JulieServerHandler` struct
- `new_sync()` sets it to `None` (backward compatible — stdio mode)
- Add `new_with_daemon_state(workspace_root, daemon_state)` constructor for daemon mode
- Update `create_workspace_mcp_service` in `daemon_state.rs` to accept `Arc<RwLock<DaemonState>>` and pass it to the new constructor
- Update `create_mcp_service` in `mcp_http.rs` — keeps using `new_sync()` (no daemon_state for default /mcp endpoint)
- The handler clone needs to work — `Arc<RwLock<DaemonState>>` is already Clone

**Acceptance criteria:**
- [ ] `JulieServerHandler` has `daemon_state: Option<Arc<RwLock<DaemonState>>>`
- [ ] `new_sync()` sets `daemon_state: None` (stdio mode unchanged)
- [ ] `new_with_daemon_state()` constructor exists and sets the field
- [ ] `create_workspace_mcp_service` uses the new constructor with daemon_state
- [ ] Default `/mcp` endpoint still uses `new_sync()` (no daemon_state)
- [ ] All existing tests pass unchanged
- [ ] Tests pass, committed

---

### Task 3: Federation Module — RRF Merge + Parallel Fan-Out

**Files:**
- Create: `src/tools/federation/mod.rs` (module root, re-exports)
- Create: `src/tools/federation/search.rs` (federated search across workspaces)
- Create: `src/tools/federation/rrf.rs` (multi-list RRF merge)
- Modify: `src/tools/mod.rs` (add `pub mod federation`)
- Test: `src/tests/federation_tests.rs`

**What to build:** The core federation engine. Given a list of `LoadedWorkspace` entries, fan out a search query in parallel and merge results with RRF.

**Approach:**
- `rrf.rs`: A generalized RRF merge that takes N ranked lists (not just 2 like `hybrid.rs`). Each list item needs an ID and a rank. Generic over item type. Returns merged + sorted list.
- `search.rs`: `federated_symbol_search()` and `federated_content_search()` functions that:
  1. Take the query params + a slice of `(workspace_id, &JulieWorkspace)` tuples
  2. `tokio::spawn` a search per workspace (using `SearchIndex::search_symbols` / `search_content`)
  3. Collect results, tag each with `workspace_id`
  4. Apply RRF merge across all per-workspace result lists
  5. Return merged results with workspace attribution
- Need a `FederatedResult<T>` wrapper that pairs a result with its source workspace_id and project name
- The existing `SymbolSearchResult` has an `id` field suitable for RRF dedup. However, IDs are only unique within a workspace, not globally. Prefix with workspace_id for cross-project dedup: `"{workspace_id}:{symbol_id}"`

**Acceptance criteria:**
- [ ] `rrf::multi_rrf_merge()` merges N ranked lists using RRF (k=60), generic over item type
- [ ] Unit test: merge 3 lists with overlapping items, verify ranking
- [ ] `FederatedSearchResult` struct wraps `SymbolSearchResult` + workspace_id + project_name
- [ ] `federated_symbol_search()` fans out across workspaces in parallel
- [ ] `federated_content_search()` fans out across workspaces in parallel
- [ ] Unit test: federation with 2 mock workspaces produces merged results
- [ ] Tests pass, committed

---

### Task 4: Wire Federation into fast_search

**Files:**
- Modify: `src/tools/search/mod.rs:78-182` (`call_tool` — add `All` branch)
- Modify: `src/tools/search/text_search.rs:44-548` (or create new entry point for federated path)
- Modify: `src/tools/search/formatting.rs` (add project tagging to output)
- Test: `src/tests/` (federated fast_search tests)

**What to build:** When `workspace="all"` is passed to `fast_search`, route to the federation layer instead of the single-workspace search path.

**Approach:**
- In `FastSearchTool::call_tool()`, when `resolve_workspace_filter` returns `WorkspaceTarget::All`:
  1. Get `daemon_state` from handler — error if None (stdio mode)
  2. Read lock `DaemonState`, collect all `Ready` workspaces
  3. Call `federated_symbol_search()` or `federated_content_search()` depending on `search_target`
  4. Convert `FederatedSearchResult` items to `Symbol` structs (reuse `tantivy_symbol_to_symbol`)
  5. Format with project tags
- Output formatting: modify `format_lean_search_results` and `format_definition_search_results` to accept an optional `project_name` field. When present, prepend `[project: name]` to each result line.
- The `Symbol` struct doesn't need modification — carry project name separately in a parallel vec or a wrapper during formatting

**Acceptance criteria:**
- [ ] `fast_search(workspace="all")` routes to federation layer in daemon mode
- [ ] `fast_search(workspace="all")` returns clear error in stdio mode (no daemon_state)
- [ ] Federated results are tagged with `[project: name]` in output
- [ ] Results are RRF-merged across projects
- [ ] Centrality boost still applied per-project before merge
- [ ] Language and file_pattern filters applied per-project
- [ ] Tests pass, committed

---

### Task 5: Wire Federation into fast_refs

**Files:**
- Modify: `src/tools/navigation/fast_refs.rs:87-348` (`find_references_and_definitions` — add `All` branch)
- Modify: `src/tools/navigation/formatting.rs` (add project tagging)
- Test: `src/tests/` (federated fast_refs tests)

**What to build:** When `workspace="all"` is passed to `fast_refs`, search for references across all registered projects.

**Approach:**
- In `find_references_and_definitions`, when `resolve_workspace_filter` returns `WorkspaceTarget::All`:
  1. Get `daemon_state` from handler
  2. For each Ready workspace: open its DB (or use the LoadedWorkspace's DB), query `get_symbols_by_name` + relationships
  3. Collect all definitions and references across projects
  4. Tag results with project name
  5. Sort by project, then file, then line number
- This is simpler than search federation — no RRF needed, just union + sort
- Reuse `find_references_in_reference_workspace` pattern for opening per-workspace DBs

**Acceptance criteria:**
- [ ] `fast_refs(workspace="all")` finds references across all projects
- [ ] Results tagged with `[project: name]` prefix
- [ ] Definitions and references from all projects included
- [ ] Clear error in stdio mode
- [ ] Tests pass, committed

---

### Task 6: Wire Federation into deep_dive

**Files:**
- Modify: `src/tools/deep_dive/mod.rs:59-140` (`call_tool` — add `All` branch)
- Modify: `src/tools/deep_dive/formatting.rs` (project tagging in header)
- Test: `src/tests/` (federated deep_dive tests)

**What to build:** When `workspace="all"` is passed to `deep_dive`, find the symbol across all projects and show cross-project callers/callees.

**Approach:**
- In `DeepDiveTool::call_tool()`, when `WorkspaceTarget::All`:
  1. Search for the symbol across all workspace DBs
  2. Use `context_file` for disambiguation as usual
  3. Pick the best match (highest centrality or most specific context_file match)
  4. Show cross-project callers — aggregate callers from all projects
  5. Format with project attribution in the header
- The `deep_dive_query` function takes a `&SymbolDatabase` — for federated mode, we run it against the best-matching workspace's DB, then separately gather cross-project callers
- This is the most complex tool to federate. Keep it focused: find the symbol in the "home" project, add cross-project callers as a separate section

**Acceptance criteria:**
- [ ] `deep_dive(workspace="all")` finds symbol across all projects
- [ ] Shows cross-project callers when symbol is used in other projects
- [ ] Header shows which project the symbol is defined in
- [ ] Clear error in stdio mode
- [ ] Tests pass, committed

---

### Task 7: Wire Federation into get_context

**Files:**
- Modify: `src/tools/get_context/pipeline.rs:463-539` (`run` — add `All` branch)
- Modify: `src/tools/get_context/formatting.rs` (project tagging)
- Test: `src/tests/` (federated get_context tests)

**What to build:** When `workspace="all"` is passed to `get_context`, search for pivots across all projects and expand with cross-project neighbors.

**Approach:**
- In `pipeline::run()`, when `WorkspaceTarget::All`:
  1. Run federated symbol search to find pivots across all projects
  2. For each pivot, expand neighbors within its home project (don't cross project boundaries for neighbors — too noisy)
  3. Format with project tags on pivots
  4. File map shows files grouped by project
- Reuse `run_pipeline` for per-project pivot expansion — call it per workspace for each pivot's home project
- Token budget applies globally across all projects

**Acceptance criteria:**
- [ ] `get_context(workspace="all")` finds pivots across all projects
- [ ] Pivots tagged with project name
- [ ] File map grouped by project
- [ ] Token budget respected across all results
- [ ] Clear error in stdio mode
- [ ] Tests pass, committed

---

### Task 8: Integration Tests + Tool Description Updates

**Files:**
- Create or extend: `src/tests/federation_integration_tests.rs`
- Modify: `src/handler.rs` (update tool descriptions to document `workspace="all"`)
- Modify: `src/tools/search/mod.rs` (FastSearchTool field doc)
- Modify: `src/tools/navigation/fast_refs.rs` (FastRefsTool field doc)
- Modify: `src/tools/deep_dive/mod.rs` (DeepDiveTool field doc)
- Modify: `src/tools/get_context/mod.rs` (GetContextTool field doc)

**What to build:** End-to-end integration tests verifying cross-project search through the full MCP tool stack, plus update all tool parameter descriptions to document the new `"all"` option.

**Approach:**
- Integration tests create 2+ temp projects with known symbols, register them in a test DaemonState, create a handler with daemon_state, and verify:
  - `fast_search(workspace="all")` returns results from both projects
  - `fast_refs(workspace="all")` finds cross-project references
  - `deep_dive(workspace="all")` shows cross-project callers
  - `get_context(workspace="all")` returns pivots from multiple projects
- Update the `workspace` field description on all tool structs from `"primary" (default) or workspace ID` to `"primary" (default), workspace ID, or "all" (daemon mode: search all projects)`
- Update agent instructions (`src/agent_instructions.md`) to document `workspace="all"` capability

**Acceptance criteria:**
- [ ] Integration test: federated fast_search finds symbols from 2 projects
- [ ] Integration test: federated fast_refs finds cross-project references
- [ ] Integration test: federated deep_dive shows cross-project callers
- [ ] Integration test: federated get_context returns multi-project pivots
- [ ] All tool descriptions updated to document `"all"` option
- [ ] Agent instructions updated
- [ ] All tests pass (fast tier), committed

---

## Task Dependency Graph

```
Task 1 (WorkspaceTarget enum)
  |
  v
Task 2 (DaemonState on handler)
  |
  v
Task 3 (Federation module)
  |
  +---> Task 4 (fast_search federation)
  +---> Task 5 (fast_refs federation)
  +---> Task 6 (deep_dive federation)
  +---> Task 7 (get_context federation)
  |
  v
Task 8 (Integration tests + docs)
```

Tasks 1-3 are sequential (each builds on the prior). Tasks 4-7 are independent and could be parallelized. Task 8 depends on all of 4-7.

## Notes

- **Auto-reference discovery** (scanning Cargo.toml/package.json for cross-project deps) is deferred — it's a nice-to-have that doesn't block federated search.
- **Web UI cross-project search** is also deferred — the API works, the UI can be added in Phase 4.
- The existing `rrf_merge` in `hybrid.rs` merges exactly 2 lists. Task 3 creates a generalized N-list version. We keep both — `hybrid.rs` is used for keyword+semantic within a single project.
