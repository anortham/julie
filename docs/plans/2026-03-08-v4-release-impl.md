# Julie v4.0 Release Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Polish and harden Julie's platform features into a shippable v4.0 release — plugin adoption layer, OpenAPI docs, worktree validation, and filewatcher documentation.

**Architecture:** Four independent workstreams. Workstream 1 (plugin) is the largest — it creates a new `julie-plugin/` directory at repo root with skills, hooks, and plugin manifest, plus enhances MCP tool descriptions in the Rust source. Workstream 2 (OpenAPI) adds `utoipa` annotations to all API handlers and serves a spec endpoint. Workstream 3 (validation) is a manual+automated test effort. Workstream 4 (filewatcher docs) is small documentation work.

**Tech Stack:** Rust (utoipa for OpenAPI), Markdown (skills, hooks), JSON (plugin manifest, hooks config)

---

## Workstream 1: Julie Plugin

### Task 1: Plugin scaffold and manifest

**Files:**
- Create: `julie-plugin/.claude-plugin/plugin.json`
- Create: `julie-plugin/README.md`

**What to build:** The plugin directory structure with manifest. The manifest includes `mcpServers` inline (pointing to `http://localhost:3141/mcp` for daemon mode), metadata, and keywords. README covers installation (binary + plugin), daemon startup, and verification.

**Approach:** Follow goldfish's pattern — `mcpServers` inline in `plugin.json` rather than a separate `.mcp.json`. Use `"type": "http"` for the MCP server connection since we're connecting to the daemon. Plugin name is `"julie"`.

**Acceptance criteria:**
- [ ] `julie-plugin/.claude-plugin/plugin.json` exists with name, version, description, author, repository, keywords, and mcpServers
- [ ] `julie-plugin/README.md` covers binary installation, daemon start, plugin install, and verification
- [ ] Plugin loads via `claude --plugin-dir ./julie-plugin` without errors
- [ ] Committed

### Task 2: Hooks — SessionStart, PreCompact, ExitPlanMode

**Files:**
- Create: `julie-plugin/hooks/hooks.json`

**What to build:** Three prompt hooks that drive automatic memory behavior:

1. **SessionStart** (matcher: `startup|clear`) — Call `recall()` with defaults. If there's an active plan or recent checkpoints, briefly summarize them. If nothing found, continue without comment.
2. **PreCompact** — Call `checkpoint()` NOW to save current progress. Include what you were working on, current state, decisions made, and planned next steps. Do NOT ask permission.
3. **PostToolUse** (matcher: `ExitPlanMode`) — Save the approved plan to Julie using `plan({ action: "save", ... })`. Extract title and content from the plan file. Activate it.

**Approach:** All hooks use `"type": "prompt"`. Follow goldfish's exact hook structure — it's proven to drive agent adoption effectively. The prompts should be assertive ("do this NOW", "do NOT ask permission") because agents tend to second-guess themselves otherwise.

**Acceptance criteria:**
- [ ] `hooks/hooks.json` has all three hooks with correct event names and matchers
- [ ] Hook prompts are clear, assertive, and reference Julie tool names (`mcp__julie__checkpoint`, `mcp__julie__recall`, `mcp__julie__plan`)
- [ ] Committed

### Task 3: Skill — /checkpoint

**Files:**
- Create: `julie-plugin/skills/checkpoint/SKILL.md`

**What to build:** Skill that guides checkpoint usage. Port from goldfish's checkpoint skill but reference Julie's `mcp__julie__checkpoint` tool. Cover when to checkpoint (meaningful milestones, key decisions, non-obvious discoveries), when NOT to (every small edit, routine passes, rapid-fire), and how to write good descriptions (markdown with WHAT/WHY/HOW/IMPACT structure).

**Approach:** Use goldfish's skill at `~/Source/goldfish/skills/checkpoint/` as the template. Update tool references from `mcp__goldfish__checkpoint` to `mcp__julie__checkpoint`. Add `allowed-tools: mcp__julie__checkpoint` in frontmatter.

**Acceptance criteria:**
- [ ] `skills/checkpoint/SKILL.md` exists with frontmatter (name, description, allowed-tools)
- [ ] Covers when to / when not to checkpoint
- [ ] Covers structured description format (WHAT/WHY/HOW/IMPACT)
- [ ] References `mcp__julie__checkpoint` tool
- [ ] Committed

### Task 4: Skill — /recall

**Files:**
- Create: `julie-plugin/skills/recall/SKILL.md`

**What to build:** Skill for context restoration. Port from goldfish's recall skill. Cover default recall, targeted search queries, date filtering, cross-project recall (workspace: "all" in daemon mode), and plan-only recall (limit: 0).

**Approach:** Use goldfish's skill at `~/Source/goldfish/skills/recall/` as template. Update tool references. Note that Julie uses BM25 full-text search (not fuse.js fuzzy search) — the `search` parameter is more powerful.

**Acceptance criteria:**
- [ ] `skills/recall/SKILL.md` exists with frontmatter (name, description, allowed-tools)
- [ ] Covers common scenarios (new session, after compaction, targeted search, cross-project, plan-only)
- [ ] Parameter examples for all key patterns
- [ ] References `mcp__julie__recall` and `mcp__julie__plan` tools
- [ ] Committed

### Task 5: Skill — /plan

**Files:**
- Create: `julie-plugin/skills/plan/SKILL.md`

**What to build:** Skill for persistent plan management. Port from goldfish's plan skill. Cover when to create plans, how to save/activate/update/complete, and the critical ExitPlanMode pattern.

**Approach:** Use goldfish's skill at `~/Source/goldfish/skills/plan/` as template. The core message is: plans represent hours of work, losing them is unacceptable, save immediately after ExitPlanMode without asking permission.

**Acceptance criteria:**
- [ ] `skills/plan/SKILL.md` exists with frontmatter (name, description, allowed-tools)
- [ ] Covers when to create plans and plan lifecycle
- [ ] ExitPlanMode → save pattern prominently documented
- [ ] References `mcp__julie__plan` tool
- [ ] Committed

### Task 6: Skill — /plan-status

**Files:**
- Create: `julie-plugin/skills/plan-status/SKILL.md`

**What to build:** Skill for assessing progress against the active plan. Port from goldfish's plan-status skill. Cross-reference plan tasks against recent checkpoints to report what's done, what's next, and whether work is drifting from the plan.

**Approach:** Use goldfish's skill at `~/Source/goldfish/skills/plan-status/` as template. The skill calls `recall()` to get active plan + recent checkpoints, then analyzes alignment.

**Acceptance criteria:**
- [ ] `skills/plan-status/SKILL.md` exists with frontmatter (name, description, allowed-tools)
- [ ] Covers multi-source plan gathering (Julie plans + docs/plans/ files)
- [ ] Defines assessment methodology (done, in-progress, blocked, not-started)
- [ ] References `mcp__julie__recall` and `mcp__julie__plan` tools
- [ ] Committed

### Task 7: Skill — /standup

**Files:**
- Create: `julie-plugin/skills/standup/SKILL.md`

**What to build:** Skill for cross-project standup reports. Port from goldfish's standup skill. Recall across all workspaces, review active plans, produce concise standup format.

**Approach:** Use goldfish's skill at `~/Source/goldfish/skills/standup/` as template. The skill uses `recall({ workspace: "all", days: 1 })` for cross-project activity, then formats as standup (yesterday, today, blockers).

**Acceptance criteria:**
- [ ] `skills/standup/SKILL.md` exists with frontmatter (name, description, allowed-tools)
- [ ] Covers cross-project recall patterns (1 day, weekend, custom range)
- [ ] Defines standup output format
- [ ] References `mcp__julie__recall` and `mcp__julie__plan` tools
- [ ] Committed

### Task 8: Enhance MCP tool descriptions for memory tools

**Files:**
- Modify: `src/handler.rs:560-624` (tool attribute descriptions for checkpoint, recall, plan)

**What to build:** Replace the minimal one-line tool descriptions with detailed guidance matching goldfish's tool description quality. The `description` field in the `#[tool(...)]` attribute is what agents see when they discover the tool — it's the primary driver of correct usage.

**Approach:** Current descriptions:
- checkpoint: `"Save a development milestone to memory. Creates a searchable checkpoint with git context, tags, and optional structured fields (decision, impact, etc.)."`
- recall: `"Retrieve prior context from developer memory. Returns recent checkpoints and the active plan."`
- plan: `"Manage persistent development plans. Plans track multi-session work and associate checkpoints with goals."`

These need to be expanded with when-to-use guidance, when-NOT-to-use guidance, parameter tips, and examples — similar to goldfish's `tools.ts` descriptions. Keep them concise but actionable.

Also update the `JsonSchema` doc comments on the struct fields in `src/tools/memory/checkpoint.rs`, `recall.rs`, and `plan.rs` — these become the parameter descriptions agents see.

**Acceptance criteria:**
- [ ] checkpoint tool description includes when to/when NOT to checkpoint, markdown format guidance
- [ ] recall tool description includes parameter examples, trust-recalled-context guidance, cross-project tips
- [ ] plan tool description includes ExitPlanMode urgency, plan-vs-checkpoint distinction, activation reminder
- [ ] Field-level doc comments are descriptive (not just "Tags for categorization" but actionable guidance)
- [ ] `cargo test --lib -- --skip search_quality` passes (no compile errors)
- [ ] Committed

### Task 9: Update JULIE_AGENT_INSTRUCTIONS.md with memory workflow

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`

**What to build:** Add a memory tools section to the agent instructions. Currently the file only covers code intelligence tools (fast_search, get_symbols, deep_dive, etc.). Add a section covering memory workflow patterns — when to checkpoint, how recall works, plan lifecycle, and integration with code intelligence tools.

**Approach:** Add a new section after the existing "Workflow Patterns" section. Keep it concise — the detailed guidance lives in the tool descriptions and skills. The instructions should establish the behavioral expectations (checkpoint at milestones, trust recalled context, save plans immediately).

**Acceptance criteria:**
- [ ] New "Memory Tools" section added after "Workflow Patterns"
- [ ] Covers checkpoint, recall, plan at a behavioral level
- [ ] Does NOT duplicate the detailed skill content (references tools, doesn't repeat full guidance)
- [ ] Consistent tone with existing instructions
- [ ] Committed

---

## Workstream 2: OpenAPI Documentation

### Task 10: Add utoipa dependency and OpenAPI scaffold

**Files:**
- Modify: `Cargo.toml` (add utoipa, utoipa-axum dependencies)
- Modify: `src/server.rs:64-207` (add OpenAPI spec endpoint and Swagger UI route)

**What to build:** Add `utoipa` and `utoipa-axum` crates. Create the OpenAPI doc struct with metadata (title: "Julie API", version from Cargo.toml, description). Mount `/api/openapi.json` endpoint that serves the generated spec. Optionally mount `/api/docs` with Swagger UI.

**Approach:** Use `utoipa::OpenApi` derive macro on a top-level struct that aggregates all API paths. The spec endpoint is a simple handler returning the JSON. Check if `utoipa-swagger-ui` is worth including for the `/api/docs` route.

**Acceptance criteria:**
- [ ] `utoipa` and `utoipa-axum` in Cargo.toml
- [ ] OpenAPI doc struct created with correct metadata
- [ ] `/api/openapi.json` returns valid OpenAPI 3.0 JSON
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

### Task 11: Annotate API modules — health, projects, dashboard

**Files:**
- Modify: `src/api/health.rs` (add utoipa path/schema derives)
- Modify: `src/api/projects.rs:23-315` (annotate all handlers and response structs)
- Modify: `src/api/dashboard.rs:24-298` (annotate all handlers and response structs)

**What to build:** Add `#[utoipa::path(...)]` attributes to each handler function and `#[derive(utoipa::ToSchema)]` to all request/response structs in these three modules. These are the simpler modules — good starting point.

**Approach:** For each handler, specify method, path, request body (if any), and responses. For each struct, derive `ToSchema`. The existing `Serialize`/`Deserialize` derives make this straightforward.

**Acceptance criteria:**
- [ ] All handlers in health, projects, dashboard annotated with `#[utoipa::path]`
- [ ] All request/response structs derive `ToSchema`
- [ ] Paths appear in `/api/openapi.json` output
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

### Task 12: Annotate API modules — memories, search, search_unified, agents

**Files:**
- Modify: `src/api/memories.rs:27-337` (annotate handlers + structs)
- Modify: `src/api/search.rs:28-450` (annotate handlers + structs)
- Modify: `src/api/search_unified.rs` (annotate handlers + structs)
- Modify: `src/api/agents.rs:33-385` (annotate handlers + structs)

**What to build:** Same as Task 11 but for the more complex API modules. Search has multiple response types (symbols, content, memories). Agents has SSE streaming. These need careful annotation.

**Approach:** SSE endpoints may need manual OpenAPI annotation since utoipa doesn't auto-detect streaming. Document them as `text/event-stream` responses. For search, document the polymorphic response (symbols vs content vs memories based on search_target).

**Acceptance criteria:**
- [ ] All handlers in memories, search, search_unified, agents annotated
- [ ] All request/response structs derive `ToSchema`
- [ ] SSE endpoints documented with correct content type
- [ ] Complete spec at `/api/openapi.json` covers all routes
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Workstream 3: Worktree & Parallel Validation

### Task 13: Write and execute validation test plan

**Files:**
- Create: `docs/WORKTREE_VALIDATION.md` (test plan + results)

**What to build:** A structured test plan covering four scenarios, executed against a running daemon. Document results, file bugs for any failures, and fix them.

**Scenarios to test:**

**A. Multiple git worktrees:**
1. Create a worktree of the julie repo: `git worktree add ../julie-worktree feat-test`
2. Register both in the daemon: `POST /api/projects` for each
3. Verify separate workspace IDs in `GET /api/projects`
4. Verify each has its own index (separate symbol counts possible)
5. Trigger search in each — results scoped correctly
6. Clean up worktree, verify no orphaned state

**B. Concurrent MCP sessions:**
1. Start daemon, connect two MCP clients simultaneously (can use `curl` against HTTP MCP endpoint)
2. Run concurrent search requests from both
3. Run concurrent indexing (trigger re-index on same project from both)
4. Verify no crashes, deadlocks, or corrupted results

**C. Agent worktrees (razorback):**
1. Use `EnterWorktree` to spawn a worktree
2. Check if Julie indexes it (expected: no, unless explicitly registered)
3. Verify cleanup leaves no orphaned watchers/indexes
4. Document expected behavior

**D. Daemon lifecycle:**
1. Kill daemon while MCP session active — verify client gets clean error
2. Restart daemon — verify projects reload correctly
3. Call `julie daemon start` twice — verify idempotent or clear error
4. Register project while daemon running — verify watcher starts

**Acceptance criteria:**
- [ ] Test plan documented in `docs/WORKTREE_VALIDATION.md`
- [ ] All four scenario groups executed
- [ ] Results documented (pass/fail per test)
- [ ] Any bugs found are fixed with tests
- [ ] Committed

---

## Workstream 4: Filewatcher Documentation

### Task 14: Document filewatcher behavior and add watcher stats

**Files:**
- Modify: `src/api/dashboard.rs:24-29` (add watcher count to DashboardStats)
- Modify: `src/api/dashboard.rs:82-171` (gather watcher count in stats handler)

**What to build:** Add `active_watchers: usize` to the `DashboardStats` response so the dashboard shows how many file watchers are running. Document the filewatcher behavior (watches all ready projects, OS-native watchers, idle cost characteristics per platform) in the project README or docs.

**Approach:** The `DaemonWatcherManager` has `active_watchers()` method that returns the count. Thread it through `AppState` to the dashboard handler. For documentation, add a section to an existing doc file (docs/ARCHITECTURE.md or the main README).

**Acceptance criteria:**
- [ ] `DashboardStats` includes `active_watchers` field
- [ ] Dashboard stats endpoint returns correct watcher count
- [ ] Filewatcher behavior documented (which platforms, idle cost, when watchers start/stop)
- [ ] `cargo test --lib -- --skip search_quality` passes
- [ ] Committed

---

## Execution Order

Tasks are grouped by workstream. Within each workstream, tasks are sequential. Across workstreams, they're independent and can be parallelized.

**Workstream 1 (Plugin):** Tasks 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9
**Workstream 2 (OpenAPI):** Tasks 10 → 11 → 12
**Workstream 3 (Validation):** Task 13 (after workstreams 1-2 complete, so we validate the final state)
**Workstream 4 (Filewatcher):** Task 14 (independent)

**Recommended parallelism:** Run Workstream 1, 2, and 4 in parallel. Run Workstream 3 after the others complete.
