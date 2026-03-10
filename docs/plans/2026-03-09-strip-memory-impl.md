# Strip Memory from Julie — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Remove all memory tools (checkpoint, recall, plan) and related code from Julie, leaving a focused 7-tool code intelligence server.

**Architecture:** Delete the `src/memory/` module, `src/tools/memory/`, unified search, memory API endpoints, and dashboard memory views. Update all wiring files that import or register memory components. Clean the UI of memory/standup routes and components.

**Tech Stack:** Rust, Vue 3, axum, rmcp

---

### Task 1: Delete memory core and tool modules

**Files:**
- Delete: `src/memory/` (entire directory — 9 files)
- Delete: `src/tools/memory/` (entire directory — 4 files)
- Modify: `src/tools/mod.rs` — remove `pub mod memory` and `pub use memory::{CheckpointTool, PlanTool, RecallTool}`
- Modify: `src/lib.rs:24` — remove `pub mod memory`

**What to build:** Remove the memory business logic and MCP tool wrappers. Update the two module declaration files that reference them.

**Acceptance criteria:**
- [ ] `src/memory/` directory does not exist
- [ ] `src/tools/memory/` directory does not exist
- [ ] `src/tools/mod.rs` has no memory references
- [ ] `src/lib.rs` has no `pub mod memory` line

---

### Task 2: Remove memory from MCP handler

**Files:**
- Modify: `src/handler.rs:22-25` — remove `CheckpointTool, RecallTool, PlanTool` from imports
- Modify: `src/handler.rs:715-768` — remove the three `#[tool]` methods: `checkpoint()`, `recall()`, `plan()`
- Modify: `src/handler.rs` — remove these tools from `new_router()` if they appear there

**What to build:** Strip the 3 memory MCP tools from the handler so Julie only exposes 7 tools.

**Acceptance criteria:**
- [ ] Handler imports only 7 tool types (no Checkpoint/Recall/Plan)
- [ ] No `checkpoint`, `recall`, or `plan` methods in the `#[tool_router]` impl block
- [ ] `new_router()` doesn't reference memory tools

---

### Task 3: Remove unified search and content_type

**Files:**
- Delete: `src/search/unified.rs`
- Delete: `src/search/content_type.rs`
- Modify: `src/search/mod.rs:6,17` — remove `pub mod content_type` and `pub mod unified`
- Modify: `src/api/search.rs` — remove `content_type` field from `SearchRequest`, remove `MemoryResultResponse` struct, remove `memories` field from `SearchResponse`, remove `parse_content_type()` function, remove unified search dispatch, simplify `search()` to always use code-only path
- Delete: `src/api/search_unified.rs`
- Modify: `src/api/mod.rs:10` — remove `pub mod search_unified`

**What to build:** Remove the unified code+memory search. The API `POST /api/search` always searches code only.

**Approach:** In `src/api/search.rs`, the `search()` function currently branches on `content_type` — if "memory" or "all", it calls `search_unified`. After removal, `search()` simply calls `search_code_only()` directly. Remove the `content_type` field from `SearchRequest` and `MemoryResultResponse` struct + `memories` field from `SearchResponse`.

**Acceptance criteria:**
- [ ] `src/search/unified.rs` and `src/search/content_type.rs` don't exist
- [ ] `src/api/search_unified.rs` doesn't exist
- [ ] `POST /api/search` works for code search (no content_type parameter)
- [ ] No `ContentType` or `MemoryResultResponse` types remain

---

### Task 4: Remove memory API endpoints and dashboard memory stats

**Files:**
- Delete: `src/api/memories.rs`
- Modify: `src/api/mod.rs:7` — remove `pub mod memories`
- Modify: `src/api/mod.rs:44-49` — remove memories paths from `#[openapi]`
- Modify: `src/api/mod.rs:82-87` — remove memory schemas from `#[openapi]`
- Modify: `src/api/mod.rs:114` — remove memories tag
- Modify: `src/api/mod.rs:146-150` — remove memory/plan routes from `routes()` function
- Modify: `src/api/dashboard.rs:17` — remove `use crate::memory`
- Modify: `src/api/dashboard.rs:28,66-70` — remove `MemoryStats` from `DashboardStats`, remove `memories` field
- Modify: `src/api/dashboard.rs:332-449` — remove `gather_memory_stats()`, `is_date_dir()`, `parse_timestamp_from_filename()` functions
- Modify: `src/api/dashboard.rs` stats function — remove memory stats gathering

**What to build:** Remove all REST endpoints for memories/plans and the memory stats from the dashboard API.

**Acceptance criteria:**
- [ ] No `/api/memories`, `/api/plans` routes
- [ ] Dashboard `/api/dashboard/stats` returns without `memories` field
- [ ] No `crate::memory` imports in api module
- [ ] OpenAPI spec has no memory schemas or paths

---

### Task 5: Remove memory from agent dispatch and context assembly

**Files:**
- Modify: `src/agent/dispatch.rs:18` — remove `use crate::memory::{self, CheckpointInput}`
- Modify: `src/agent/dispatch.rs:289-317` — remove `save_result_as_checkpoint()` function entirely
- Modify: `src/agent/context_assembly.rs:17` — remove `use crate::memory::{self, RecallOptions}`
- Modify: `src/agent/context_assembly.rs:160-196` — remove `assemble_memory_context()` function
- Modify: `src/agent/context_assembly.rs:61-112` — remove the call to `assemble_memory_context()` in `assemble_context()`

**What to build:** Agent dispatch no longer saves results as checkpoints. Context assembly no longer includes memory context.

**Acceptance criteria:**
- [ ] No `crate::memory` imports in agent module
- [ ] `assemble_context()` assembles code context + hints only
- [ ] No `save_result_as_checkpoint` function

---

### Task 6: Delete memory test files and update test module

**Files:**
- Delete: `src/tests/memory_checkpoint_tests.rs`
- Delete: `src/tests/memory_git_tests.rs`
- Delete: `src/tests/memory_index_tests.rs`
- Delete: `src/tests/memory_plan_tests.rs`
- Delete: `src/tests/memory_cross_project_tests.rs`
- Delete: `src/tests/memory_embedding_tests.rs`
- Delete: `src/tests/memory_recall_tests.rs`
- Delete: `src/tests/memory_storage_tests.rs`
- Delete: `src/tests/memory_tool_tests.rs`
- Delete: `src/tests/memory_integration_tests.rs`
- Delete: `src/tests/unified_search_tests.rs`
- Delete: `src/tests/api_memories_tests.rs`
- Modify: `src/tests/mod.rs:42,44,49,88-100` — remove all memory and unified search test module declarations
- Check: `src/tests/api_search_unified_tests.rs` — delete if it exists
- Check: `src/tests/phase4_integration_tests.rs` — remove memory-related test cases if any
- Check: `src/tests/phase5_search_enhancement.rs` — remove memory-related test cases if any

**What to build:** Remove all memory-related test files and their module declarations.

**Acceptance criteria:**
- [ ] No `memory_*_tests.rs` files exist in `src/tests/`
- [ ] No `unified_search_tests.rs` or `api_memories_tests.rs`
- [ ] `src/tests/mod.rs` has no memory module declarations
- [ ] No remaining compile errors from missing test modules

---

### Task 7: Update dashboard UI — remove memory/standup views

**Files:**
- Delete: `ui/src/views/Memories.vue`
- Delete: `ui/src/views/Standup.vue`
- Delete: `ui/src/components/MemoryResults.vue`
- Modify: `ui/src/router/index.ts` — remove Memories and Standup imports and routes
- Modify: `ui/src/App.vue:41-51` — remove Memories and Standup nav links
- Modify: `ui/src/views/Dashboard.vue:24-27,53,244-265` — remove MemoryStats interface, `memories` field from stats, and the Memory card from the template
- Modify: `ui/src/views/Search.vue` — remove MemoryResults import, content_type filter UI, memory result display, `MemoryResult` interface
- Modify: `ui/src/components/SearchFilters.vue` — remove memory content type filter if present

**What to build:** Clean the UI so it shows only code intelligence features (projects, search, agents, dashboard without memory card).

**Acceptance criteria:**
- [ ] No Memories or Standup views exist
- [ ] No MemoryResults component
- [ ] Navigation has no Memories/Standup links
- [ ] Dashboard has no memory stats card
- [ ] Search view has no memory/content_type filtering
- [ ] UI builds cleanly (`npm run build` in ui/)

---

### Task 8: Delete plugin infrastructure and update install output

**Files:**
- Delete: `julie-plugin/` (entire directory)
- Delete: `.claude-plugin/` (entire directory)
- Modify: `src/install.rs:70-74` — remove plugin marketplace instructions from install summary
- Modify: `README.md` — remove memory tool references, plugin install instructions, update tool count to 7

**What to build:** Remove all plugin distribution infrastructure. Update install output to show simple MCP config instructions instead of plugin marketplace commands.

**Approach:** The install summary should say something like:
```
Next steps — add Julie to your AI tool's MCP config:
  {"type": "http", "url": "http://localhost:7890/mcp"}
```

**Acceptance criteria:**
- [ ] `julie-plugin/` directory does not exist
- [ ] `.claude-plugin/` directory does not exist
- [ ] `julie-server install` output has no plugin/marketplace references
- [ ] README reflects 7 tools, no memory features

---

### Task 9: Build, test, and clean up

**Files:**
- Check: `Cargo.toml` — remove any dependencies only used by memory (check if `chrono` is still needed elsewhere)
- Check: any remaining compile warnings about dead code
- Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -20`
- Run: `cd ui && npm run build`

**What to build:** Verify everything compiles cleanly and tests pass.

**Acceptance criteria:**
- [ ] `cargo build` succeeds with no warnings
- [ ] Fast test tier passes
- [ ] UI builds cleanly
- [ ] Julie exposes exactly 7 MCP tools
