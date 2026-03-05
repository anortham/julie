# GPT Review Findings Fix — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Fix all four findings from the GPT code review: workspace path propagation, tool execution mutex, embedding test isolation, and agent instructions path resolution.

**Architecture:** Thread the resolved workspace root from `main.rs` into the handler so all components share a single source of truth. Remove the redundant tool execution mutex (rmcp already serializes stdio writes). Add an embedding skip mechanism for auto-indexing. Resolve agent instructions relative to workspace root.

**Tech Stack:** Rust, rmcp, tokio

---

### Task 1: Thread workspace root from main.rs into JulieServerHandler

**Files:**
- Modify: `src/main.rs:82-85` (pass workspace_root to handler)
- Modify: `src/handler.rs:55-90` (add workspace_root field, accept in new(), remove get_workspace_path())
- Modify: `src/handler.rs:108-114` (use self.workspace_root instead of get_workspace_path())
- Modify: `src/handler.rs:339-344` (load_agent_instructions uses workspace root — fixes P3 too)
- Test: `src/tests/core/workspace_init.rs`

**What to build:** Add a `workspace_root: PathBuf` field to `JulieServerHandler`. Change `new()` to accept it as a parameter. `main.rs` passes the already-resolved `get_workspace_root()` result. Replace `get_workspace_path()` (which only calls `current_dir()`) with `self.workspace_root.clone()`. Update `load_agent_instructions()` to read from `workspace_root.join("JULIE_AGENT_INSTRUCTIONS.md")` instead of cwd-relative, making it a method on `&self` instead of a free function.

**Approach:**
- `JulieServerHandler::new(workspace_root: PathBuf)` — single source of truth
- `get_workspace_path()` method returns `self.workspace_root.clone()` (or just inline it at the one call site)
- `load_agent_instructions(&self)` becomes a method reading from `self.workspace_root`
- `main.rs` changes from `JulieServerHandler::new().await` to `JulieServerHandler::new(workspace_root).await`
- The `ManageWorkspaceTool::resolve_workspace_path` already handles JULIE_WORKSPACE — that's fine, it's the tool-level path for when agents explicitly provide a path. The handler-level workspace root is the *default*.

**Acceptance criteria:**
- [ ] `JulieServerHandler` has a `workspace_root: PathBuf` field
- [ ] `main.rs` passes resolved workspace root to handler
- [ ] `get_workspace_path()` removed or returns `self.workspace_root`
- [ ] `load_agent_instructions` reads from workspace root, not cwd
- [ ] `initialize_workspace_with_force` uses `self.workspace_root` as fallback when no path provided
- [ ] Existing tests pass
- [ ] New test verifies handler uses provided workspace root (not cwd)
- [ ] Committed

---

### Task 2: Remove redundant tool_execution_lock

**Files:**
- Modify: `src/handler.rs:67` (remove field)
- Modify: `src/handler.rs:82` (remove from constructor)
- Modify: `src/handler.rs:284-292` (remove `tool_lock_is_free`)
- Modify: `src/handler.rs:368-522` (remove `_guard` acquisition from all 7 tool handlers)
- Modify: `src/tests/core/handler.rs` (rewrite or remove stubbed test)
- Check: `fast_refs` on `tool_execution_lock` and `tool_lock_is_free` for any other callers

**What to build:** Remove the `tool_execution_lock` field and all lock acquisitions from tool handlers. rmcp's `SinkStreamTransport` already wraps the write sink in `Arc<Mutex<Si>>` (confirmed in `sink_stream.rs`), so stdio writes are already serialized at the transport layer. The lock was preventing concurrent tool execution entirely — removing it allows parallel read-only tool calls.

**Approach:**
- Remove `tool_execution_lock: Arc<tokio::sync::Mutex<()>>` from struct and constructor
- Remove `let _guard = self.tool_execution_lock.lock().await;` from all 7 tool handler methods
- Remove `tool_lock_is_free()` method (check refs first — it may be used by tests or startup)
- Update or remove the stubbed `tool_lock_not_held_during_tool_execution` test
- The comment about "write-interleaving guard" on line 62-66 should be removed since it describes deleted functionality

**Acceptance criteria:**
- [ ] `tool_execution_lock` field removed from `JulieServerHandler`
- [ ] All 7 tool handlers no longer acquire a lock before execution
- [ ] `tool_lock_is_free()` removed (after checking refs)
- [ ] Handler test file updated (stubbed test removed or rewritten)
- [ ] All tests pass
- [ ] Committed

---

### Task 3: Skip embeddings during auto-indexing

**Files:**
- Modify: `src/tools/workspace/commands/index.rs:328-339` (conditionally skip embeddings)
- Modify: `src/handler.rs:295-335` (set env hint or pass flag for auto-index)

**What to build:** During auto-indexing (triggered by `on_initialized`), skip the `spawn_workspace_embedding` call. Embeddings are an expensive, network-dependent operation that should not block the initial indexing path. They can be triggered explicitly via `manage_workspace index` or lazily on first NL-definition search.

**Approach:**
- The simplest approach: `run_auto_indexing` already constructs a `ManageWorkspaceTool` struct with hardcoded fields (line 308-316). We can't easily pass a "skip embeddings" flag through the tool struct without changing the schema.
- Better approach: Check an internal signal. Add a `skip_auto_embeddings: Arc<AtomicBool>` to `JulieServerHandler` (or reuse `IndexingStatus`). Set it true before auto-indexing, check it in `handle_index_command` before calling `spawn_workspace_embedding`, clear it after.
- Simplest viable approach: Move `spawn_workspace_embedding` out of `handle_index_command` and into the callers that want it. Auto-indexing in `run_auto_indexing` skips it. Explicit `manage_workspace index` calls it. But this changes the tool behavior — explicit index should still trigger embeddings.
- **Recommended:** Add `pub auto_mode: bool` (serde default false, skip_serializing) to `ManageWorkspaceTool`. When `run_auto_indexing` constructs the tool, set `auto_mode: true`. In `handle_index_command`, skip `spawn_workspace_embedding` when `self.auto_mode` is true. This keeps the tool schema backward compatible (field defaults to false when omitted by agents).

**Acceptance criteria:**
- [ ] Auto-indexing via `on_initialized` does not trigger `spawn_workspace_embedding`
- [ ] Explicit `manage_workspace index` still triggers embeddings as before
- [ ] Tests pass (especially workspace and indexing tests)
- [ ] Committed

---

### Task 4: Verify and clean up

**Files:**
- Review all changes
- Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`

**What to build:** Final verification that all changes integrate correctly. Run the fast test tier, check for compilation warnings, verify no regressions.

**Acceptance criteria:**
- [ ] `cargo build` clean (no warnings from our changes)
- [ ] Fast test tier passes
- [ ] No dead code warnings from removed items
- [ ] All 4 findings addressed
- [ ] Final commit if any cleanup needed
