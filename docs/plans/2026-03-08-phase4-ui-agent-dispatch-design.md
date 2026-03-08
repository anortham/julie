# Phase 4: Management UI + Agent Dispatch — Design

## Overview

Expand Julie's web UI from a basic dashboard into a full management interface, and add agent dispatch capability via the Claude CLI. The UI becomes the command center for search debugging, memory browsing, and dispatching agents with Julie-assembled context.

---

## Architecture

Three new backend modules, five UI views (2 existing expanded + 3 new), wired through REST API:

```
Backend (Rust)                    REST API                     UI (Vue + PrimeVue)
─────────────────                ──────────                   ──────────────────
src/agent/                       POST /api/agents/dispatch    views/Agents.vue
  mod.rs (types, trait)          GET  /api/agents/:id/stream    - task input form
  claude_backend.rs              GET  /api/agents/history       - SSE streaming output
  context_assembly.rs            GET  /api/agents/:id           - dispatch history
                                 GET  /api/agents/backends

src/api/ (extend existing)       POST /api/search             views/Search.vue
  agents.rs                      POST /api/search/debug         - search box + results
  search.rs                                                     - debug toggle (scores,
  memories.rs                    GET  /api/memories               tokens, field matches)
                                 GET  /api/memories/:id
                                 GET  /api/plans              views/Memories.vue
                                 GET  /api/plans/:id            - checkpoint timeline
                                 GET  /api/plans/active         - plan viewer
                                                                - filters

                                 GET  /api/dashboard/stats    views/Dashboard.vue (expand)
                                                                - project/memory/agent stats

                                                              views/Projects.vue (expand)
                                                                - existing, minor tweaks
```

Navigation: Dashboard, Projects, Search, Memories, Agents.

---

## Agent Dispatch

### Interaction Model

**Single-shot (fire-and-forget):** Assemble context → `claude -p "prompt"` → capture output → store as checkpoint. No interactive back-and-forth — the value is in Julie assembling the right context, not building a chat UI (that's what Claude Code already is).

### Context Assembly

Context assembly is generous — no artificial token budget. The whole point of single-shot dispatch is giving the agent everything it needs upfront. Assembly pulls from:

1. **Code context:** Run `get_context(task_description)` against target project's index. Returns relevant symbols with full code bodies, neighbor signatures, file map.
2. **Memory context:** Run `recall(search: task_description)` for relevant checkpoints from the project's memory.
3. **File contents:** If the user specifies files or symbols as hints, include their full contents.

The assembled prompt format:

```
# Context (assembled by Julie)

## Relevant Code
[get_context results — pivots with code, neighbors with signatures]

## Recent Memories
[recall results — relevant checkpoints]

## Specific Files
[if user provided file/symbol hints]

# Task
[user's task description]
```

### Backend Trait

```rust
trait AgentBackend: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;  // checks `which claude`
    async fn dispatch(&self, prompt: &str) -> Result<AgentStream>;
}
```

Only `ClaudeBackend` implemented for now. The trait exists for future extensibility (codex, Ollama, raw API).

### Dispatch Flow

1. User fills form in UI: task description, target project, optional hints (symbols, files)
2. `POST /api/agents/dispatch` → returns dispatch ID immediately
3. Backend assembles context from target project's index + memories
4. Spawns `claude -p "assembled prompt"` via `tokio::process::Command`
5. Streams stdout via SSE at `GET /api/agents/:id/stream`
6. On completion, stores output as checkpoint with `type: "agent_result"`, tagged with task + backend
7. Dispatch record stored in daemon state for history

### Auto-Detection

On daemon startup (or on-demand via `GET /api/agents/backends`), detect available backends:

```rust
// Check: which claude
// Parse: claude --version (if available)
```

Store detection results in `DaemonState`.

---

## Search Playground

### Two Modes

**Search mode:** Type a query, see results. Syntax-highlighted code snippets, file paths, symbol kind badges, language tags.

**Debug mode** (toggle): Each result expands to show scoring breakdown:

- **BM25 score** — raw Tantivy score
- **Centrality score** — graph-based boost from reference count
- **Final score** — combined score after all boosts
- **Field matches** — which fields matched (name, signature, doc_comment, code_body, file content)
- **Token breakdown** — how `CodeTokenizer` tokenized the query (CamelCase splits, snake_case splits, affixes stripped)
- **Ranking explanation** — why this result ranked where it did (e.g. "exact name match, centrality boost ×1.3")

### Backend Changes

Currently `search_symbols()` returns `SymbolSearchResult` with a flat `score: f32`. For debug mode, need a richer return type:

```rust
struct SearchDebugResult {
    result: SymbolSearchResult,
    bm25_score: f32,
    centrality_score: f32,
    field_matches: Vec<String>,     // which schema fields matched
    query_tokens: Vec<String>,      // how the query was tokenized
    boost_explanation: String,      // human-readable scoring explanation
}
```

This is a new code path (`search_symbols_debug`) — the normal search path stays unchanged for performance.

---

## Memories View

### Timeline

Chronological view of checkpoints, newest first. Each entry shows:
- Summary (first line or heading)
- Tags as badges
- Type badge (checkpoint / decision / incident / learning)
- Branch + commit hash
- Relative timestamp ("2 hours ago")
- Click to expand full description + git context

### Plan Viewer

Sidebar or sub-section showing:
- All plans with status badges (active / completed / archived)
- Active plan highlighted at top
- Click plan to see its content + linked checkpoints

### Filters

- Date range picker
- Tag filter (multi-select from existing tags)
- Plan filter (select a plan to see its checkpoints)
- Type filter (checkpoint / decision / incident / learning)
- Search box (queries Tantivy memory index)

---

## Dashboard Expansion

Current: 3 cards (status, version, uptime).

Expand to include:
- **Projects:** count, index health breakdown (fresh / stale / indexing / error)
- **Memories:** total checkpoints, active plan name, last checkpoint timestamp
- **Agents:** recent dispatch count, last dispatch result summary
- **Backends:** detected agent CLIs with version info

Single endpoint: `GET /api/dashboard/stats` aggregates from daemon state + filesystem.

---

## REST API Surface

### Existing (no changes)
```
GET    /api/health
GET    /api/projects
POST   /api/projects
DELETE /api/projects/:id
```

### New — Search
```
POST   /api/search          — run fast_search, return results
POST   /api/search/debug    — same search with scoring breakdown
```

### New — Memories
```
GET    /api/memories         — list checkpoints (?limit, ?since, ?search, ?planId, ?type)
GET    /api/memories/:id     — single checkpoint detail
GET    /api/plans            — list all plans (?status filter)
GET    /api/plans/:id        — single plan detail
GET    /api/plans/active     — current active plan (or 404)
```

### New — Agents
```
POST   /api/agents/dispatch  — start dispatch (returns { id, status: "running" })
GET    /api/agents/:id/stream — SSE stream for in-progress dispatch
GET    /api/agents/history   — list past dispatches (?limit, ?project)
GET    /api/agents/:id       — single dispatch result
GET    /api/agents/backends  — available backends with versions
```

### New — Dashboard
```
GET    /api/dashboard/stats  — aggregated stats for all dashboard cards
```

---

## Deferred Features

These are explicitly deferred, not forgotten:

- **Project dependency graph visualization** — requires parsing dependency manifests (Cargo.toml, package.json, go.mod) across projects, building a relationship graph, rendering with a JS graph library (e.g. D3, vis.js). Significant scope. Revisit in Phase 5+.
- **Additional agent backends** — codex CLI, Ollama, raw Anthropic API. The `AgentBackend` trait is ready; just needs new implementations when needed.
- **User-level memories** — `~/.julie/memories/` for personal cross-project learnings. Per-project memories are sufficient for now. Revisit when cross-project memory patterns emerge from usage.
- **Memory embedding** — embed memories in the same vector space as code symbols for cross-content semantic search. Tantivy BM25 is sufficient for now.
- **Agent dispatch from MCP tools** — deliberately excluded to avoid context bloat. Agent dispatch is a UI-driven workflow.

---

## Constraints

- No implementation file over 500 lines
- All Rust tests in `src/tests/`
- TDD methodology
- Cross-platform: no platform-specific code in core
- UI built with Vue 3 + PrimeVue, embedded in binary via `rust-embed`
- Agent dispatch: Claude CLI only (`claude -p`), single-shot, no interactive session
