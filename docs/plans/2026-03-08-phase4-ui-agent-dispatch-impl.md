# Phase 4: Management UI + Agent Dispatch — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Expand Julie's web UI into a full management dashboard and add agent dispatch via Claude CLI.

**Architecture:** New REST API endpoints (`src/api/`) backed by existing modules (`src/memory/`, `src/search/`) plus a new `src/agent/` module. Vue views consume these endpoints. Agent dispatch uses `tokio::process::Command` to shell out to `claude -p` with SSE streaming.

**Tech Stack:** Rust (axum, tokio, serde), Vue 3 + PrimeVue (Composition API), SSE (Server-Sent Events)

---

## Task 1: Search REST API + Debug Infrastructure

**Files:**
- Create: `src/api/search.rs`
- Create: `src/search/debug.rs`
- Modify: `src/api/mod.rs:13-21` (add search routes)
- Test: `src/tests/api_search_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**What to build:** Two search endpoints — a standard search and a debug search that exposes scoring internals. The debug endpoint is the backend for the search playground's debug toggle.

**Approach:**
- `POST /api/search` — accepts `{ query, language?, file_pattern?, limit?, search_target? }`, delegates to `SearchIndex::search_symbols()` / `search_content()`, returns results as JSON
- `POST /api/search/debug` — same search but returns `SearchDebugResult` with BM25 score, centrality score, field matches, query tokens, boost explanation
- New `src/search/debug.rs` — `search_symbols_debug()` function that wraps the normal search path but captures scoring intermediates. Collects: raw BM25 from Tantivy, centrality score from `apply_centrality_boost()`, tokenized query from `tokenize_query()`, which fields matched (run per-field queries), and a human-readable explanation string
- API handler needs access to workspace — resolve via `DaemonState` using project ID param, or use the first Ready workspace
- Keep the normal `search_symbols()` path unchanged for performance

**Acceptance criteria:**
- [ ] `POST /api/search` returns JSON results matching `SymbolSearchResult` fields
- [ ] `POST /api/search/debug` returns results with bm25_score, centrality_score, field_matches, query_tokens, boost_explanation
- [ ] Query tokenization breakdown shows CamelCase/snake_case splits
- [ ] Works for both definition and content search targets
- [ ] Tests pass, committed

---

## Task 2: Memories REST API

**Files:**
- Create: `src/api/memories.rs`
- Modify: `src/api/mod.rs:13-21` (add memory + plan routes)
- Test: `src/tests/api_memories_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**What to build:** REST endpoints for reading memories and plans. Delegates to existing `src/memory/recall.rs` and `src/memory/plan.rs`.

**Approach:**
- `GET /api/memories` — query params: `limit`, `since`, `search`, `planId`, `type`. Builds `RecallOptions`, calls `recall()`. Needs workspace root from a project param or daemon config
- `GET /api/memories/:id` — read single checkpoint by ID. Walk `.memories/` dirs, find by ID prefix match
- `GET /api/plans` — calls `plan::list_plans()`, optional `?status` filter
- `GET /api/plans/:id` — calls `plan::get_plan()`
- `GET /api/plans/active` — calls `plan::get_active_plan()`, returns 404 if none
- All endpoints need a project context (which `.memories/` dir to read). Accept `?project=workspace_id` query param, default to first Ready workspace

**Acceptance criteria:**
- [ ] All 5 endpoints return correct JSON
- [ ] `GET /api/memories?search=auth` uses Tantivy search mode
- [ ] `GET /api/memories?since=3d` filters by date
- [ ] `GET /api/plans/active` returns 404 when no active plan
- [ ] Tests pass, committed

---

## Task 3: Agent Dispatch Backend

**Files:**
- Create: `src/agent/mod.rs`
- Create: `src/agent/backend.rs`
- Create: `src/agent/claude_backend.rs`
- Create: `src/agent/context_assembly.rs`
- Create: `src/agent/dispatch.rs`
- Modify: `src/lib.rs` (add `pub mod agent;`)
- Test: `src/tests/agent_backend_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**What to build:** The agent dispatch engine — backend trait, Claude CLI implementation, context assembly from indexes + memories, dispatch execution with output capture.

**Approach:**
- `AgentBackend` trait: `name()`, `is_available() -> bool` (checks `which claude`), `dispatch(prompt) -> Result<AgentStream>`
- `ClaudeBackend`: implements trait, spawns `claude -p "prompt"` via `tokio::process::Command`, returns stdout as `AgentStream` (an async stream of String chunks)
- `AgentStream`: wrapper around `tokio::io::Lines<BufReader<ChildStdout>>` that yields line-by-line output
- `context_assembly.rs`: `assemble_context(workspace, task, hints) -> String` — runs `get_context`-style queries against the workspace's SearchIndex + recalls from memory, formats into a prompt with sections (Relevant Code, Recent Memories, Task)
- `dispatch.rs`: `DispatchManager` — stores active/completed dispatches, generates dispatch IDs, stores results as checkpoints on completion. Holds `Vec<AgentDispatch>` in memory (not persisted to DB — dispatches are ephemeral, results are persisted as checkpoints)
- `AgentDispatch` struct: id, task, project, status (running/completed/failed), started_at, completed_at, output (accumulated)
- Auto-detection: `detect_backends()` runs `which claude` on startup, stores results

**Acceptance criteria:**
- [ ] `AgentBackend` trait defined, `ClaudeBackend` implements it
- [ ] `ClaudeBackend::is_available()` checks `which claude`
- [ ] `assemble_context()` pulls code context + memories into a prompt string
- [ ] `DispatchManager` tracks dispatches and stores results as checkpoints
- [ ] Tests pass (mock the CLI subprocess), committed

---

## Task 4: Agent REST API + SSE Streaming

**Files:**
- Create: `src/api/agents.rs`
- Modify: `src/api/mod.rs:13-21` (add agent routes)
- Modify: `src/server.rs:26-46` (add DispatchManager to AppState)
- Test: `src/tests/api_agents_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**What to build:** REST endpoints for agent dispatch with SSE streaming for real-time output.

**Approach:**
- `POST /api/agents/dispatch` — accepts `{ task, project, hints? }`. Spawns dispatch in background (tokio::spawn), returns `{ id, status: "running" }` immediately
- `GET /api/agents/:id/stream` — SSE endpoint using `axum::response::Sse`. Opens EventSource connection, streams stdout lines as SSE `data:` events. Sends `event: done` when complete
- `GET /api/agents/history` — list past dispatches (from DispatchManager), optional `?limit`, `?project` filters
- `GET /api/agents/:id` — single dispatch detail (status, output, timing)
- `GET /api/agents/backends` — list detected backends with availability + version
- Add `DispatchManager` (wrapped in `Arc<RwLock<>>`) to `AppState` struct
- SSE streaming: use `tokio::sync::broadcast` channel — dispatch task writes to channel, SSE handler subscribes and forwards. Multiple clients can watch the same dispatch

**Acceptance criteria:**
- [ ] `POST /api/agents/dispatch` returns dispatch ID and spawns background task
- [ ] `GET /api/agents/:id/stream` returns SSE events with agent output lines
- [ ] `GET /api/agents/history` lists past dispatches
- [ ] `GET /api/agents/backends` shows detected backends
- [ ] Completed dispatches store results as checkpoints
- [ ] Tests pass, committed

---

## Task 5: Dashboard Stats Endpoint

**Files:**
- Create: `src/api/dashboard.rs`
- Modify: `src/api/mod.rs:13-21` (add dashboard route)
- Test: `src/tests/api_dashboard_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**What to build:** Aggregated stats endpoint for the expanded dashboard UI.

**Approach:**
- `GET /api/dashboard/stats` — aggregates from DaemonState, filesystem, DispatchManager
- Response shape:
  ```json
  {
    "projects": { "total": 5, "ready": 3, "indexing": 1, "error": 1 },
    "memories": { "total_checkpoints": 42, "active_plan": "my-plan", "last_checkpoint": "2026-03-08T02:33:01Z" },
    "agents": { "total_dispatches": 7, "last_dispatch": "2026-03-08T01:15:00Z" },
    "backends": [{ "name": "claude", "available": true, "version": "1.2.3" }]
  }
  ```
- Projects: count from DaemonState, group by status
- Memories: walk `.memories/` dirs of first Ready workspace (or all?), count files, read active plan
- Agents: from DispatchManager
- Backends: from backend detection results

**Acceptance criteria:**
- [ ] Endpoint returns all 4 stat sections
- [ ] Project counts match DaemonState
- [ ] Memory stats reflect actual filesystem
- [ ] Tests pass, committed

---

## Task 6: Search Playground UI

**Files:**
- Create: `ui/src/views/Search.vue`
- Modify: `ui/src/router/index.ts` (add /search route)
- Modify: `ui/src/App.vue` (add Search nav link)

**What to build:** Search page with results display and debug toggle.

**Approach:**
- Search box with query input, optional filters (language dropdown, file pattern, search target radio: definitions/content)
- Results list: each result shows symbol name, kind badge, file path, signature, score
- Debug toggle: when on, calls `/api/search/debug` instead of `/api/search`. Each result expands to show BM25 score, centrality score, field matches, tokenized query, boost explanation
- Token breakdown display: show the raw query → tokenized form (e.g. "getUserData" → ["get", "User", "Data", "getUserData"])
- Use PrimeVue components: InputText, Dropdown, RadioButton, DataTable or custom list, Tag for badges, Panel for debug expansion
- Style consistent with existing Dashboard/Projects pages (same CSS variables, card style)

**Acceptance criteria:**
- [ ] Search box submits to `/api/search`, displays results
- [ ] Debug toggle switches to `/api/search/debug` with expanded scoring info
- [ ] Token breakdown shows CamelCase/snake_case splits
- [ ] Language and file pattern filters work
- [ ] Nav link added, route registered
- [ ] `npm run build` succeeds

---

## Task 7: Memories Timeline UI

**Files:**
- Create: `ui/src/views/Memories.vue`
- Modify: `ui/src/router/index.ts` (add /memories route)
- Modify: `ui/src/App.vue` (add Memories nav link)

**What to build:** Memory timeline with checkpoint list, plan viewer, and filters.

**Approach:**
- Timeline: vertical list of checkpoints, newest first. Each entry shows summary, tags (as PrimeVue Tag components), type badge, branch + commit, relative timestamp. Click expands to full description + git context
- Plan sidebar/section: shows all plans from `/api/plans`, active plan highlighted. Click to see content + linked checkpoints (filtered by planId)
- Filters bar: date range (PrimeVue Calendar), tag multi-select, type filter (dropdown), plan filter (dropdown), search box
- All data fetched from `/api/memories` and `/api/plans` endpoints
- Use PrimeVue: Timeline component (or custom), Tag, Calendar, MultiSelect, Dropdown, Panel

**Acceptance criteria:**
- [ ] Checkpoint timeline displays with summary, tags, type, branch, timestamp
- [ ] Click expands to full description
- [ ] Plan viewer shows all plans with status, active plan highlighted
- [ ] Filters work: date range, tags, type, plan, search
- [ ] Nav link added, route registered
- [ ] `npm run build` succeeds

---

## Task 8: Agents UI

**Files:**
- Create: `ui/src/views/Agents.vue`
- Modify: `ui/src/router/index.ts` (add /agents route)
- Modify: `ui/src/App.vue` (add Agents nav link)

**What to build:** Agent dispatch interface with task form, streaming output, and history.

**Approach:**
- Dispatch form: task description (textarea), project selector (dropdown from `/api/projects`), optional hints textarea (symbols/files). Submit button calls `POST /api/agents/dispatch`
- Streaming output: after dispatch, open EventSource to `/api/agents/:id/stream`. Render output in a terminal-style monospace panel, auto-scrolling. Show status badge (running/completed/failed)
- History: below the form, list of past dispatches from `/api/agents/history`. Each shows task summary, project, status, timestamp. Click to view full output
- Available backends: small indicator showing detected backends from `/api/agents/backends`
- Use PrimeVue: Textarea, Dropdown, Button, Tag, ScrollPanel or custom terminal panel

**Acceptance criteria:**
- [ ] Dispatch form submits task and shows streaming output via SSE
- [ ] Output renders in terminal-style panel with auto-scroll
- [ ] History shows past dispatches
- [ ] Backend availability indicator works
- [ ] Nav link added, route registered
- [ ] `npm run build` succeeds

---

## Task 9: Dashboard Expansion UI

**Files:**
- Modify: `ui/src/views/Dashboard.vue` (expand with new stat cards)

**What to build:** Expand the 3-card dashboard to show project health, memory stats, agent stats, and available backends.

**Approach:**
- Fetch from `GET /api/dashboard/stats` instead of (or in addition to) `/api/health`
- Project card: total count with breakdown (ready/indexing/error), small status indicators
- Memory card: total checkpoints, active plan name (linked to Memories page), last checkpoint relative time
- Agent card: total dispatches, last dispatch summary
- Backends card: list detected backends with green/red availability dot
- Keep existing 3 cards (status, version, uptime) at top, add new cards below in a second row
- Style consistent with existing card CSS

**Acceptance criteria:**
- [ ] Dashboard shows all 4 new stat sections
- [ ] Cards link to relevant pages (Projects, Memories, Agents)
- [ ] Graceful handling when daemon has no projects/memories/dispatches
- [ ] `npm run build` succeeds

---

## Task 10: Build + Embed Updated UI

**Files:**
- Modify: `ui/package.json` (verify PrimeVue deps)
- Rebuild: `npm run build` in `ui/`
- Verify: embedded assets serve correctly via axum

**What to build:** Build the Vue app and verify the embedded binary serves all new views correctly.

**Approach:**
- Run `npm run build` in `ui/` — produces `ui/dist/`
- Verify SPA fallback works for new routes (/ui/search, /ui/memories, /ui/agents)
- Test navigation between all 5 views
- Verify PrimeVue components render (may need to register new components in `main.ts`)
- This is a verification/integration task, not a code-heavy task

**Acceptance criteria:**
- [ ] `npm run build` succeeds with no errors
- [ ] All 5 views accessible via embedded UI
- [ ] SPA routing works (direct URL access to /ui/search, etc.)
- [ ] PrimeVue components render correctly
- [ ] No console errors in browser

---

## Task 11: Integration Tests + Final Verification

**Files:**
- Create: `src/tests/phase4_integration_tests.rs`
- Modify: `src/tests/mod.rs` (register test module)

**What to build:** End-to-end tests verifying the REST API layer works correctly with real data.

**Approach:**
- Test search API: create workspace with indexed symbols, POST to /api/search, verify results
- Test search debug API: same search via /api/search/debug, verify scoring fields present
- Test memories API: save checkpoint via memory module, GET /api/memories, verify it appears
- Test plans API: save/activate/list/get plans via REST
- Test dashboard stats: verify aggregated response shape
- Test agent backends: verify backend detection returns expected shape
- Skip agent dispatch E2E (requires actual `claude` binary) — test the DispatchManager + context assembly logic instead

**Acceptance criteria:**
- [ ] Search API tests with real indexed data
- [ ] Search debug returns scoring breakdown
- [ ] Memories and plans API round-trip
- [ ] Dashboard stats endpoint returns valid shape
- [ ] All tests pass (fast tier), committed

---

## Deferred Features (tracked, not forgotten)

- **Project dependency graph visualization** — parse Cargo.toml/package.json/go.mod, build relationship graph, render with D3/vis.js
- **Additional agent backends** — codex CLI, Ollama, raw Anthropic API (AgentBackend trait ready)
- **User-level memories** — `~/.julie/memories/` personal cross-project storage
- **Memory embedding** — embed memories in same vector space as code for semantic search
- **Agent dispatch via MCP tools** — excluded to avoid context bloat
