# Phase 3: Memory Integration — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Replace Goldfish MCP plugin with native Rust memory tools (checkpoint, recall, plan) in Julie, using Tantivy for memory search.

**Architecture:** New `src/memory/` module for core storage/indexing logic, thin `src/tools/memory/` wrappers for MCP tool integration. Memories stored as markdown + YAML frontmatter in `<project>/.memories/`, indexed in Tantivy at `.julie/indexes/memories/tantivy/`.

**Tech Stack:** serde_yaml (new dep), sha2 (existing), tantivy (existing), tokio::process for git CLI.

---

### Task 1: YAML Frontmatter Storage Layer

**Files:**
- Create: `src/memory/mod.rs`
- Create: `src/memory/storage.rs`
- Modify: `src/lib.rs:25` (add `pub mod memory`)
- Modify: `Cargo.toml` (add `serde_yaml` dependency)
- Test: `src/tests/memory_storage_tests.rs`

**What to build:** The foundation — types and YAML frontmatter serialization. Define `Checkpoint`, `Plan`, `GitContext`, `RecallOptions`, `RecallResult` structs. Implement reading/writing markdown files with YAML frontmatter (the `---` delimited header).

**Approach:**
- Define types in `src/memory/mod.rs` matching Goldfish's TypeScript interfaces exactly (same field names, same ID format)
- `storage.rs`: `format_checkpoint()` serializes to `---\nyaml\n---\n\nbody`, `parse_checkpoint()` deserializes back
- Checkpoint ID generation: `checkpoint_{SHA256(timestamp:description)[..8]}` using existing `sha2` crate
- Filename: `{HHMMSS}_{hash[..4]}.md`
- Add `serde_yaml = "0.9"` to Cargo.toml

**Acceptance criteria:**
- [ ] `Checkpoint`, `Plan`, `GitContext` structs defined with serde derive
- [ ] `format_checkpoint()` produces valid YAML frontmatter + markdown body
- [ ] `parse_checkpoint()` round-trips correctly
- [ ] `generate_checkpoint_id()` produces deterministic IDs matching Goldfish format
- [ ] `get_checkpoint_filename()` produces `HHMMSS_xxxx.md` format
- [ ] Can parse existing Goldfish `.memories/` checkpoint files
- [ ] Tests pass, committed

---

### Task 2: Git Context Capture

**Files:**
- Create: `src/memory/git.rs`
- Test: `src/tests/memory_git_tests.rs`

**What to build:** Capture current git state (branch, short commit hash, changed files) by shelling out to the `git` CLI. Used by checkpoint to auto-attach git context.

**Approach:**
- `get_git_context(workspace_root: &Path) -> Option<GitContext>` — runs git commands, returns None if not a git repo or git not available
- Commands: `git rev-parse --abbrev-ref HEAD` (branch), `git rev-parse --short HEAD` (commit), `git diff --name-only HEAD` (changed files)
- Use `tokio::process::Command` for async execution
- Graceful failure — git not installed or not a repo returns None, never errors

**Acceptance criteria:**
- [ ] `get_git_context()` returns branch, commit, and changed files when in a git repo
- [ ] Returns `None` gracefully when not in a git repo
- [ ] Handles git not being installed without panicking
- [ ] Tests pass (test in a temp git repo), committed

---

### Task 3: Checkpoint Save (Write to Disk)

**Files:**
- Create: `src/memory/checkpoint.rs`
- Test: `src/tests/memory_checkpoint_tests.rs`

**What to build:** The `save_checkpoint()` function that writes a checkpoint markdown file to `.memories/{date}/{HHMMSS}_{hash}.md`, capturing git context and the active plan ID.

**Approach:**
- `save_checkpoint(workspace_root: &Path, input: CheckpointInput) -> Result<Checkpoint>` — main entry point
- Creates `.memories/{YYYY-MM-DD}/` directory if needed
- Captures git context via `git.rs`
- Reads `.memories/.active-plan` to get current plan ID
- Generates ID, formats with `storage::format_checkpoint()`, writes to disk
- Returns the saved `Checkpoint` for confirmation display

**Acceptance criteria:**
- [ ] `save_checkpoint()` creates the date directory and writes the file
- [ ] File content matches Goldfish format (YAML frontmatter + markdown body)
- [ ] Git context is captured and included in frontmatter
- [ ] Active plan ID is attached if `.active-plan` exists
- [ ] Generated checkpoint ID is deterministic
- [ ] Tests pass, committed

---

### Task 4: Plan CRUD

**Files:**
- Create: `src/memory/plan.rs`
- Test: `src/tests/memory_plan_tests.rs`

**What to build:** Plan management — save, get, list, activate, update, complete. Plans are stored as markdown files with YAML frontmatter at `.memories/plans/{plan-id}.md`.

**Approach:**
- `save_plan(workspace_root, input) -> Result<Plan>` — creates plan file, optionally activates
- `get_plan(workspace_root, id) -> Result<Option<Plan>>` — reads single plan
- `list_plans(workspace_root, status_filter) -> Result<Vec<Plan>>` — lists all plans, optional status filter
- `activate_plan(workspace_root, id) -> Result<()>` — writes plan ID to `.memories/.active-plan`
- `update_plan(workspace_root, id, updates) -> Result<Plan>` — modifies title/content/status/tags
- `complete_plan(workspace_root, id) -> Result<Plan>` — sets status to "completed"
- `get_active_plan(workspace_root) -> Result<Option<Plan>>` — reads `.active-plan`, loads the plan
- Plan ID: slugified from title if not provided (same as Goldfish)

**Acceptance criteria:**
- [ ] All 6 plan operations work (save, get, list, activate, update, complete)
- [ ] Plans stored as markdown with YAML frontmatter at `.memories/plans/`
- [ ] `.active-plan` tracks the current active plan
- [ ] `get_active_plan()` returns None when no plan is active
- [ ] Plan IDs are slugified from titles
- [ ] Tests pass, committed

---

### Task 5: Recall — Filesystem Mode (No Search)

**Files:**
- Create: `src/memory/recall.rs`
- Test: `src/tests/memory_recall_tests.rs`

**What to build:** The recall function for the no-search-query case: walk `.memories/` date directories, load checkpoints, sort by date, apply limit. This is the "last N checkpoints" mode.

**Approach:**
- `recall(workspace_root: &Path, options: RecallOptions) -> Result<RecallResult>` — main entry point
- When no `search` query: scan `.memories/` date dirs in reverse chronological order
- Parse `since`/`days`/`from`/`to` for date filtering (same logic as Goldfish's `parseSince`)
- Apply `limit` (default 5), `full` flag (strip git metadata when false), `planId` filter
- Include active plan in result
- Summary extraction: first `## ` heading or first non-empty line of description

**Acceptance criteria:**
- [ ] `recall()` returns last N checkpoints sorted newest-first
- [ ] Date filtering works: `since` ("2h", "3d"), `days`, `from`/`to`
- [ ] `limit` respected (default 5, 0 = plan only)
- [ ] `full: false` strips git metadata from output
- [ ] `planId` filters to checkpoints under that plan
- [ ] Active plan included in result
- [ ] Tests pass, committed

---

### Task 6: Memory Tantivy Index

**Files:**
- Create: `src/memory/index.rs`
- Test: `src/tests/memory_index_tests.rs`

**What to build:** A Tantivy index for memory search, separate from the code symbol index. Supports adding checkpoints, searching with BM25, and rebuilding from `.memories/` files.

**Approach:**
- `MemoryIndex` struct wrapping a Tantivy index (similar pattern to `SearchIndex` in `src/search/index.rs`)
- Schema: id (STRING stored+indexed), body (TEXT stored+indexed), tags (TEXT stored+indexed), symbols (TEXT stored+indexed), decision (TEXT stored+indexed), impact (TEXT stored+indexed), branch (STRING stored+indexed), timestamp (STRING stored+indexed), file_path (STRING stored)
- Use the default Tantivy tokenizer (not CodeTokenizer — memories are natural language)
- `add_checkpoint(&self, checkpoint: &Checkpoint)` — index a checkpoint
- `search(&self, query: &str, limit: usize) -> Result<Vec<MemorySearchResult>>` — BM25 search
- `rebuild_from_files(workspace_root: &Path)` — scan all `.memories/` files, parse, re-index
- Index path: `.julie/indexes/memories/tantivy/`

**Acceptance criteria:**
- [ ] `MemoryIndex::create()` and `MemoryIndex::open_or_create()` work
- [ ] `add_checkpoint()` indexes a checkpoint in Tantivy
- [ ] `search()` returns ranked results via BM25
- [ ] `rebuild_from_files()` scans `.memories/` and populates the index
- [ ] Search finds checkpoints by description, tags, symbols, decision, impact
- [ ] Tests pass, committed

---

### Task 7: Recall — Search Mode (Tantivy)

**Files:**
- Modify: `src/memory/recall.rs`
- Modify: `src/memory/index.rs` (if needed for date range queries)
- Test: `src/tests/memory_recall_tests.rs` (extend)

**What to build:** Wire Tantivy search into recall. When `search` parameter is provided, query the memory index instead of doing a filesystem walk.

**Approach:**
- In `recall()`, when `options.search` is Some: use `MemoryIndex::search()`
- Lazy index initialization: if Tantivy index doesn't exist or is empty, call `rebuild_from_files()` first
- Apply date range filtering post-search if date params provided (filter results by timestamp)
- Merge search results with active plan (same as filesystem mode)
- Update `save_checkpoint()` to also index in Tantivy when saving (so index stays up to date)

**Acceptance criteria:**
- [ ] `recall({ search: "auth" })` returns relevant checkpoints ranked by BM25
- [ ] Auto-rebuilds index on first search if index is missing
- [ ] Date filtering works with search: `recall({ search: "bug", since: "3d" })`
- [ ] New checkpoints are indexed immediately when saved
- [ ] Tests pass, committed

---

### Task 8: MCP Tool Wrappers

**Files:**
- Create: `src/tools/memory/mod.rs`
- Create: `src/tools/memory/checkpoint.rs`
- Create: `src/tools/memory/recall.rs`
- Create: `src/tools/memory/plan.rs`
- Modify: `src/tools/mod.rs:14` (add `pub mod memory`)
- Modify: `src/handler.rs:388-556` (add 3 new tool methods to `#[tool_router] impl`)
- Modify: `src/handler.rs:20-23` (add imports for new tools)
- Test: `src/tests/memory_tool_tests.rs`

**What to build:** Three MCP tool structs (`CheckpointTool`, `RecallTool`, `PlanTool`) with `call_tool()` methods that delegate to `src/memory/`. Register them in the handler's `#[tool_router]`.

**Approach:**
- Each tool struct has fields matching Goldfish's tool input schema (with serde defaults)
- `call_tool(&self, handler)` extracts `workspace_root` from handler, calls `src/memory/` functions
- Register in handler with `#[tool]` annotations matching Goldfish's descriptions
- CheckpointTool: calls `memory::checkpoint::save_checkpoint()`, formats confirmation
- RecallTool: calls `memory::recall::recall()`, formats output (compact or full)
- PlanTool: dispatches to save/get/list/activate/update/complete based on `action` field

**Acceptance criteria:**
- [ ] `CheckpointTool`, `RecallTool`, `PlanTool` structs defined with JsonSchema
- [ ] All 3 tools registered in handler's tool_router
- [ ] `checkpoint` tool saves and returns confirmation
- [ ] `recall` tool returns checkpoints + active plan
- [ ] `plan` tool handles all 6 actions
- [ ] Tools work in stdio mode (uses handler.workspace_root)
- [ ] Tests pass, committed

---

### Task 9: Cross-Project Recall (Daemon Mode)

**Files:**
- Modify: `src/memory/recall.rs`
- Test: `src/tests/memory_recall_tests.rs` (extend)

**What to build:** When `workspace="all"` is passed to recall in daemon mode, iterate all registered workspaces and aggregate checkpoints.

**Approach:**
- RecallTool passes `handler.daemon_state` to recall when `workspace="all"`
- `recall_cross_project(daemon_state, options)` — iterates Ready workspaces, calls `recall()` per workspace, merges results sorted by timestamp
- Tag each checkpoint with its source project name
- Include workspace summaries in result (project name, checkpoint count, last activity)
- Return error in stdio mode (no daemon_state)

**Acceptance criteria:**
- [ ] `recall(workspace="all")` aggregates checkpoints from all projects in daemon mode
- [ ] Results sorted by timestamp newest-first across projects
- [ ] Each checkpoint tagged with source project
- [ ] Workspace summaries included (name, count, last activity)
- [ ] Clear error in stdio mode
- [ ] Tests pass, committed

---

### Task 10: Integration Tests + Goldfish Compatibility

**Files:**
- Create: `src/tests/memory_integration_tests.rs`
- Modify: `src/tests/mod.rs` (register new test modules)

**What to build:** End-to-end tests verifying the full checkpoint → recall round trip through MCP tools, and backward compatibility with existing Goldfish `.memories/` files.

**Approach:**
- Integration test: save a checkpoint via CheckpointTool, recall it via RecallTool, verify content
- Integration test: save a plan, activate it, checkpoint with plan active, recall with planId filter
- Compatibility test: write a Goldfish-format checkpoint file manually, verify recall can parse it
- Compatibility test: parse a real checkpoint from Julie's own `.memories/` directory
- Register all new test modules in `src/tests/mod.rs`

**Acceptance criteria:**
- [ ] Round-trip test: checkpoint → recall returns saved content
- [ ] Plan workflow test: save → activate → checkpoint → recall with planId
- [ ] Goldfish compatibility: parses existing format files correctly
- [ ] All new test modules registered in `src/tests/mod.rs`
- [ ] All tests pass (fast tier), committed

---

## Task Dependency Graph

```
Task 1 (Storage types + YAML)
  |
  +---> Task 2 (Git context)
  |       |
  |       v
  +---> Task 3 (Checkpoint save)
  |       |
  |       v
  +---> Task 4 (Plan CRUD)
  |       |
  |       v
  +---> Task 5 (Recall filesystem)
  |       |
  |       v
  +---> Task 6 (Memory Tantivy index)
          |
          v
        Task 7 (Recall search mode)
          |
          v
        Task 8 (MCP tool wrappers)
          |
          v
        Task 9 (Cross-project recall)
          |
          v
        Task 10 (Integration tests + compat)
```

Tasks are sequential — each builds on the prior.

## Deferred Features (Track for Future Phases)

- **Phase 4**: Web UI memory timeline/browser
- **Phase 5**: Memory embedding (semantic vectors, same model as code)
- **Phase 5**: Memory-enriched `get_context` (surface memories alongside code)
- **Phase 5**: User-level `~/.julie/memories/` (personal cross-project)
- **Phase 5**: Cross-content search (code + memories weighted RRF)
