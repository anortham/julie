# Phase 2: Cross-Project Intelligence — Design

## Problem

Each MCP session is isolated to one project. The daemon holds all loaded workspaces in `DaemonState`, but MCP tools (`fast_search`, `fast_refs`, `deep_dive`, `get_context`) only operate on a single `JulieServerHandler`'s workspace. The `workspace="all"` parameter currently returns an error.

Phase 2 enables querying across all registered projects from a single MCP session.

## Architecture

### Federated Search Layer

A new `src/tools/federation/` module that:
1. Takes a search query + filter parameters
2. Fans out to multiple `SearchIndex` + `SymbolDatabase` pairs in parallel (`tokio::spawn`)
3. Merges results using Reciprocal Rank Fusion (RRF)
4. Tags each result with its source `workspace_id` and project name

Each project's indexes are queried independently — no shared index. This preserves per-project isolation and matches the existing architecture.

### DaemonState Access via Handler (Option A)

Add `Option<Arc<RwLock<DaemonState>>>` to `JulieServerHandler`. In daemon mode it's `Some`, in stdio mode it's `None`.

```rust
pub struct JulieServerHandler {
    // ... existing fields ...
    /// Access to all loaded workspaces (daemon mode only).
    /// None in stdio mode — federated queries return an error.
    pub(crate) daemon_state: Option<Arc<RwLock<DaemonState>>>,
}
```

Tools check this field when `workspace="all"`:
- `Some(state)` -> fan out across `state.workspaces`
- `None` -> return error: "Cross-project search requires daemon mode"

### Workspace Resolution Changes

Two `resolve_workspace_filter` functions need updating:

**`src/tools/navigation/resolution.rs`** (used by `fast_refs`, `deep_dive`, `get_symbols`):
- Currently: `"primary"` -> `None`, `<id>` -> `Some(id)`
- New: `"all"` -> new variant indicating federated mode
- Return type changes from `Option<String>` to an enum:

```rust
pub enum WorkspaceTarget {
    Primary,
    Reference(String),
    All,  // new — federated across daemon workspaces
}
```

**`src/tools/search/mod.rs`** (used by `fast_search`, `get_context`):
- Currently: `"all"` -> error, `"primary"` -> workspace IDs
- New: `"all"` -> federated search path

### Reciprocal Rank Fusion (RRF)

RRF merges ranked lists from different sources without requiring score normalization:

```
RRF_score(symbol) = SUM over all lists: 1 / (k + rank_in_list)
```

Where `k = 60` (standard constant). Each project's search produces a ranked list. RRF combines them into a single ranking.

Implemented in `src/tools/federation/rrf.rs`.

### Result Tagging

Federated results include the source project in formatted output. Not added to the `Symbol` struct (extractor concern), but to the tool output formatting:

```
[project: julie] src/search/index.rs:130  SearchIndex (struct, public)
[project: coa-mcp] src/server.rs:42  McpServer (struct, public)
```

Each tool's output formatter adds the `[project: name]` prefix when results come from federated search.

## Tool-by-Tool Changes

| Tool | `workspace="all"` | Implementation |
|------|-------------------|----------------|
| `fast_search` | Federated Tantivy search, RRF merge, tagged results | Fan out `search_symbols` / `search_content` per workspace |
| `fast_refs` | Cross-project references | Fan out `get_symbols_by_name` + relationship queries per workspace |
| `deep_dive` | Cross-project symbol investigation | Find symbol across projects, aggregate callers/callees |
| `get_context` | Federated pivots + neighbors | Fan out pivot search, cross-project neighbor expansion |
| `get_symbols` | **Not supported** — file-specific, always single workspace | Return error for `"all"` |
| `rename_symbol` | **Not supported** — too dangerous cross-project | Return error for `"all"` |

## What Doesn't Change

- Per-project indexes stay isolated (separate `.julie/indexes/` per project)
- No shared Tantivy index — federation queries each index independently
- Stdio MCP mode stays single-project (no `DaemonState`)
- `SearchIndex`, `SymbolDatabase`, `JulieWorkspace` internals untouched
- Index format, schema, migrations — all unchanged
- File watchers, background indexing — unchanged

## Auto-Reference Discovery (Stretch Goal)

Scan dependency manifests to find relationships between registered projects:
- `Cargo.toml`: `path = "../other-project"` dependencies
- `package.json`: `file:../other-project` dependencies
- `go.mod`: local `replace` directives

When a relationship is found, suggest it via the API / Web UI. This is a nice-to-have and can be deferred to a later phase if scope gets large.

## Key Decisions

1. **Federation at daemon level, not index level** — each project's Tantivy is queried independently. No shared index.
2. **RRF for merging** — score-agnostic ranking fusion, proven technique, simple to implement.
3. **Handler gets DaemonState reference** — simplest integration path, one optional field.
4. **WorkspaceTarget enum** — replaces `Option<String>` for cleaner workspace routing.
5. **Output tagging, not Symbol mutation** — project name goes in formatted output, not the Symbol struct.
6. **Parallel fan-out with tokio::spawn** — each project searched concurrently, results collected.
