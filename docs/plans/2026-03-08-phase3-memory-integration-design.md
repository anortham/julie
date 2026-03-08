# Phase 3: Memory Integration — Design

## Goal

Replace the Goldfish MCP plugin with native Rust memory tools in Julie. Same 3 tools (`checkpoint`, `recall`, `plan`), same `.memories/` file format, Tantivy-powered search instead of fuse.js.

## Architecture

```
src/memory/                           src/tools/memory/
├── mod.rs         (public API)       ├── mod.rs          (re-exports, tool registration)
├── checkpoint.rs  (save + index)     ├── checkpoint.rs   (CheckpointTool struct)
├── recall.rs      (retrieve + search)├── recall.rs       (RecallTool struct)
├── plan.rs        (plan CRUD)        ├── plan.rs         (PlanTool struct)
├── storage.rs     (YAML frontmatter)
├── index.rs       (Memory Tantivy)
└── git.rs         (git context capture)
```

**Core module** (`src/memory/`): Standalone, testable without MCP. Handles:
- Reading/writing markdown + YAML frontmatter files
- Git context capture (shell out to `git rev-parse`, `git diff --name-only`)
- Memory Tantivy index (create, add, search, rebuild from files)
- Plan CRUD with `.active-plan` file for activation

**Tool wrappers** (`src/tools/memory/`): Thin MCP tool structs that delegate to `src/memory/`. Registered in handler's `tool_router` alongside existing 7 tools.

## Storage

**Format** (unchanged from Goldfish — zero migration):
```
<project>/.memories/
├── .active-plan              # contains active plan ID
├── plans/
│   └── my-plan.md            # YAML frontmatter + markdown
├── 2026-03-08/
│   ├── 141523_a1b2.md        # HHMMSS_hash.md
│   └── 153042_c3d4.md
└── 2026-03-07/
    └── ...
```

**Index location**: `.julie/indexes/memories/tantivy/` — derived data, rebuildable from source files.

## Tantivy Schema for Memories

| Field | Stored | Indexed | Purpose |
|-------|--------|---------|---------|
| `id` | yes | yes | checkpoint_xxxx |
| `body` | yes | yes | Full markdown description (primary search target) |
| `tags` | yes | yes | Space-joined tags |
| `symbols` | yes | yes | Space-joined symbol names |
| `decision` | yes | yes | Decision statement |
| `impact` | yes | yes | Impact description |
| `branch` | yes | yes | Git branch |
| `timestamp` | yes | yes | ISO 8601 (range filtering) |
| `file_path` | yes | no | Source .md path for retrieval |

## Recall Modes

1. **No search query**: Filesystem walk over `.memories/` date dirs, sorted by date, limited by count. Same as Goldfish's default behavior.
2. **With search query**: Tantivy query with BM25 ranking. Replaces fuse.js. Faster, better ranking, no need for manual weights.
3. **Date range**: Tantivy timestamp range query when `since`/`days`/`from`/`to` provided with a search query. Filesystem filtering when no search query.
4. **Cross-project** (daemon mode): Iterate registered workspaces' `.memories/` dirs using existing DaemonState infrastructure.

## Tools (10 total after Phase 3)

### checkpoint

Save a milestone to developer memory. Creates a markdown file with YAML frontmatter and indexes it in Tantivy.

**Parameters:**
- `description` (required): Markdown body
- `type`: checkpoint | decision | incident | learning
- `tags`, `symbols`, `evidence`, `alternatives`, `unknowns`: string arrays
- `decision`, `impact`, `context`, `next`: strings
- `confidence`: number 1-5

**Behavior:**
1. Capture git context (branch, commit, changed files)
2. Generate checkpoint ID from timestamp + description hash
3. Write `{project}/.memories/{date}/{HHMMSS}_{hash4}.md`
4. Index in memory Tantivy at `.julie/indexes/memories/tantivy/`
5. Return confirmation with details

### recall

Retrieve prior context from developer memory.

**Parameters:**
- `limit`: max checkpoints (default: 5, 0 = plan only)
- `since`: human-friendly ("2h", "30m", "3d") or ISO timestamp
- `days`: look back N days
- `from`, `to`: explicit date range
- `search`: Tantivy search query (replaces fuse.js fuzzy search)
- `full`: return full descriptions + git metadata (default: false)
- `workspace`: "current" (default), "all" (cross-project in daemon mode)
- `planId`: filter to checkpoints under a specific plan

**Behavior:**
1. Resolve workspace path
2. If `search` provided: query memory Tantivy index (with optional date range)
3. If no `search`: filesystem walk over date dirs, newest first
4. Apply limit, strip metadata unless `full: true`
5. Include active plan in response
6. For `workspace="all"` in daemon mode: iterate all registered workspaces

### plan

Manage persistent plans.

**Parameters:**
- `action` (required): save | get | list | activate | update | complete
- `id`, `title`, `content`, `tags`, `activate`, `status`, `updates`

**Behavior:**
- Plans stored as `{project}/.memories/plans/{plan-id}.md`
- Active plan tracked via `{project}/.memories/.active-plan`
- One active plan per workspace

## Key Implementation Notes

- **Git context**: Shell out to `git` CLI (`git rev-parse --abbrev-ref HEAD`, `git rev-parse --short HEAD`, `git diff --name-only HEAD`). No `git2` dependency.
- **YAML parsing**: `serde_yaml` crate for frontmatter read/write.
- **File locking**: In-memory mutex per workspace. Single-process model.
- **Index rebuild**: On first search, if Tantivy index is empty/missing, rebuild from `.memories/` files (similar to code index backfill pattern).
- **Backward compatibility**: Reads existing Goldfish `.memories/` directories without migration.
- **Checkpoint ID**: `checkpoint_{first 8 hex chars of SHA-256(timestamp:description)}` — same as Goldfish.
- **Summary generation**: Extract first heading or first line as summary for compact recall display.

## What's NOT in Phase 3 (Deferred)

| Feature | Target Phase | Notes |
|---------|-------------|-------|
| Memory embedding (semantic vectors) | Phase 5 | Same model as code symbols, shared embedding space |
| Memory-enriched `get_context` | Phase 5 | Surface relevant memories alongside code results |
| User-level `~/.julie/memories/` | Phase 5 | Personal cross-project memories, separate from project |
| Cross-content search (code + memories) | Phase 5 | Weighted RRF: fast_search weights code, recall weights memories |
| Web UI memory timeline/browser | Phase 4 | Visual timeline, checkpoint browser in dashboard |

## What Doesn't Change

- Existing `.memories/` directories work as-is (Goldfish format compatibility)
- Code intelligence tools (fast_search, fast_refs, deep_dive, get_context) unchanged
- Code indexes unchanged
- Daemon infrastructure unchanged
- Stdio MCP mode still works (memory tools use workspace_root)
