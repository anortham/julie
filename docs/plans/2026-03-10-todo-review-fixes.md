# TODO Review Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.

**Goal:** Fix the 9 non-Windows findings from the GPT code review (TODO.md), plus mark 3 Windows items and 1 uncertain item for later.

**Architecture:** Each task is a self-contained fix — no dependencies between tasks except Task 1 (ref-workspace parity) which unblocks Task 6 (federated refs ranking). Tasks can be parallelized in groups.

**Tech Stack:** Rust, tokio, serde_json

---

## Deferred Items (not in this plan)

- **Windows `stop_service()` self-kill** — WINDOWS-ONLY, deferred to Windows session
- **Windows uninstall active executable** — WINDOWS-ONLY, deferred to Windows session
- **Windows UNC `display_path`** — WINDOWS-ONLY, deferred to Windows session
- **Embedding provider cross-workspace** — UNCERTAIN (possibly stale post-Tantivy), needs re-evaluation
- **CORS + unauthenticated endpoints** — Design discussion, not a code fix

---

## Task 1: Restore reference-workspace `fast_refs` parity

**Files:**
- Modify: `src/tools/navigation/reference_workspace.rs:17` — `find_references_in_reference_workspace()`
- Modify: `src/tools/navigation/fast_refs.rs:414-425` — `database_find_references_in_reference()`
- Test: `src/tests/tools/federated_refs_tests.rs` or new file

**What to build:** The reference workspace path ignores `limit` and `reference_kind` params. It also lacks Strategy 3 (identifier-based refs) that the primary path has. Pass `limit` and `reference_kind` through `database_find_references_in_reference` → `find_references_in_reference_workspace`, and apply them.

**Approach:**
1. Add `limit: u32` and `reference_kind: Option<&str>` params to `find_references_in_reference_workspace()`
2. Inside the `spawn_blocking` block: when `reference_kind` is set, use `get_relationships_to_symbols_filtered_by_kind()` instead of `get_relationships_to_symbols()` (same pattern as primary path)
3. Add identifier-based discovery (Strategy 3) using `get_identifiers_by_names` / `get_identifiers_by_names_and_kind` — same logic as primary path at `fast_refs.rs:280-370`
4. Apply `truncate(limit)` after sorting references by confidence
5. Update `database_find_references_in_reference()` to pass `self.limit` and `self.reference_kind.as_deref()`
6. Write regression tests

**Acceptance criteria:**
- [ ] `find_references_in_reference_workspace` accepts and applies `limit` and `reference_kind`
- [ ] Identifier-based refs (Strategy 3) included for reference workspaces
- [ ] References sorted by confidence then truncated to limit
- [ ] `database_find_references_in_reference` passes `self.limit` and `self.reference_kind`
- [ ] Tests verify filtering and limiting behavior
- [ ] Fast-tier tests pass

---

## Task 2: Auto-queue indexing for Registered/Stale projects on daemon startup

**Files:**
- Modify: `src/server.rs:119-131` — after spawning indexing worker and setting sender
- Modify: `src/daemon_state.rs` — possibly add helper method

**What to build:** After `load_registered_projects()` runs and the indexing worker is spawned, iterate over workspaces with `Registered` or `Stale` status and send `IndexRequest` for each.

**Approach:**
1. After `ds.set_indexing_sender(indexing_sender.clone())` at `server.rs:130`, read daemon state and collect workspace IDs with `Registered` or `Stale` status
2. For each, send an `IndexRequest` via the `indexing_sender` channel
3. Log the auto-queue action: "Auto-queuing indexing for N Registered/Stale projects"
4. This is fire-and-forget — the indexing worker processes them sequentially in the background

**Acceptance criteria:**
- [ ] `Registered` and `Stale` projects get `IndexRequest`s sent after daemon startup
- [ ] Logging indicates which projects were auto-queued
- [ ] No blocking — uses the existing async indexing channel
- [ ] Fast-tier tests pass

---

## Task 3: Preserve JSON-RPC request id in connect bridge errors

**Files:**
- Modify: `src/connect.rs:468-484` — `write_jsonrpc_error()`
- Modify: `src/connect.rs` — all call sites of `write_jsonrpc_error`

**What to build:** Extract the `id` field from the incoming JSON-RPC request line and pass it to `write_jsonrpc_error` so error responses match the request they correspond to.

**Approach:**
1. Change `write_jsonrpc_error` signature to accept `id: Option<&serde_json::Value>`
2. Use `id.unwrap_or(&serde_json::Value::Null)` in the error response JSON
3. In `run_stdio_bridge`, after reading `trimmed` (the request line), parse `id` with `serde_json::from_str::<serde_json::Value>(trimmed).ok().and_then(|v| v.get("id").cloned())`
4. Pass extracted id to all `write_jsonrpc_error` calls
5. This is a lightweight parse — we only extract `id`, not the full request

**Acceptance criteria:**
- [ ] `write_jsonrpc_error` accepts and uses the request ID
- [ ] All call sites pass the extracted ID
- [ ] Error responses contain the original request's `id` field
- [ ] Fast-tier tests pass

---

## Task 4: Fix federated refs alphabetical starvation

**Files:**
- Modify: `src/tools/navigation/federated_refs.rs:100-130` — post-collection sorting and truncation
- Test: `src/tests/tools/federated_refs_tests.rs`

**What to build:** Instead of sorting by project name and then truncating (which starves later-alphabet projects), merge all references across projects, sort globally by confidence, truncate to the limit, then re-group by project for display.

**Approach:**
1. After collecting `per_project` results, flatten all `(project_name, relationship)` pairs into a single vec
2. Sort by confidence descending (matching the primary path's sort order)
3. Truncate to `global_limit`
4. Re-group by project name for the `ProjectTaggedResult` formatting
5. Keep definitions untruncated (they're typically 0-1 per project)
6. Write a test with multiple projects where a later-alphabet project has higher-confidence refs

**Acceptance criteria:**
- [ ] Global limit applied after confidence-based sorting, not after alphabetical project ordering
- [ ] High-confidence refs from any project survive regardless of project name
- [ ] Definitions still included regardless of limit
- [ ] Test verifies a "z-project" with high confidence isn't starved by "a-project" with low confidence
- [ ] Fast-tier tests pass

---

## Task 5: Stop line-mode workspace re-resolution

**Files:**
- Modify: `src/tools/search/mod.rs:130-135` — pass resolved target to line_mode
- Modify: `src/tools/search/line_mode.rs:20-64` — accept `WorkspaceTarget` instead of raw `Option<String>`

**What to build:** `line_mode_search` currently re-resolves the workspace string independently. Instead, pass the already-resolved `WorkspaceTarget` from `fast_search`'s `call_tool`.

**Approach:**
1. Change `line_mode_search` signature: replace `workspace: &Option<String>` with a workspace target enum or the resolved workspace ID string
2. Since `line_mode_search` only needs the target workspace ID (not the full `WorkspaceTarget` enum), the simplest approach is passing the resolved ID: `target_workspace_id: &str` and `is_primary: bool` (or just the ID, since comparing to primary is cheap)
3. Actually, simplest: pass `WorkspaceTarget` directly. Import it in line_mode.rs. Match on it to decide primary vs reference, same as current logic but without re-resolution
4. Remove the `WorkspaceRegistryService` creation and re-resolution block at lines 41-64
5. Update `call_tool` in `mod.rs` to pass `workspace_target` instead of `&self.workspace`

**Acceptance criteria:**
- [ ] `line_mode_search` accepts `WorkspaceTarget` instead of `Option<String>`
- [ ] No `WorkspaceRegistryService` created inside `line_mode_search`
- [ ] Workspace resolution happens exactly once in the call chain
- [ ] Fast-tier tests pass

---

## Task 6: Fail project registration when registry persistence fails

**Files:**
- Modify: `src/daemon_state.rs:119-213` — `register_project()`
- Modify: `src/api/projects.rs:162-203` — `create_project()`
- Possibly: `src/registry.rs` — check if `save()` returns Result

**What to build:** When `register_project` updates the in-memory registry but the file write fails, the API currently returns success. It should return an error or degraded status.

**Approach:**
1. Check how `GlobalRegistry` persists — find the `save()` method and ensure it returns `Result`
2. In `register_project()`, after modifying the registry, check the persist result
3. If persist fails: either roll back the in-memory change and return error, or return a response with `status: "degraded"` indicating memory-only state
4. Propagate the error through the API response in `create_project()`
5. Same treatment for `deregister_project` / `delete_project`

**Acceptance criteria:**
- [ ] Registry persistence failure propagates as an error to the API caller
- [ ] In-memory state is consistent (either rolled back or explicitly degraded)
- [ ] API returns appropriate HTTP status code on failure
- [ ] Fast-tier tests pass

---

## Task 7: UI asset 404 instead of SPA fallback for missing files

**Files:**
- Modify: `src/ui.rs:31-50` — `serve_embedded_file()`

**What to build:** The SPA fallback (serve `index.html` for unknown paths) should only apply to navigation routes, not to static asset requests. Requests for files with known static extensions (`.js`, `.css`, `.png`, `.svg`, `.woff2`, etc.) should return 404 when not found.

**Approach:**
1. In `serve_embedded_file`, before the SPA fallback, check if the path has a static asset extension
2. If it does (`.js`, `.css`, `.png`, `.jpg`, `.svg`, `.ico`, `.woff`, `.woff2`, `.map`, `.json`), return 404 instead of falling back to `index.html`
3. Only fall back to `index.html` for extensionless paths or `.html` paths (actual SPA navigation)
4. This prevents browsers from getting HTML when they expect JS/CSS, which causes confusing parse errors

**Acceptance criteria:**
- [ ] Requests for missing `.js`/`.css`/`.png` etc. return 404
- [ ] SPA navigation routes (no extension or `.html`) still get `index.html` fallback
- [ ] Fast-tier tests pass

---

## Task 8: PID-file atomic locking

**Files:**
- Modify: `src/daemon.rs:185-236` — `daemon_start()`
- Modify: `src/daemon.rs:78-88` — `write_pid_file()`
- Test: `src/tests/daemon_tests.rs`

**What to build:** Use file locking (`flock`/`LockFile`) on the PID file to prevent TOCTOU race between check and write. This is defense-in-depth since port binding already catches duplicates.

**Approach:**
1. Use `fs2` crate (already common in Rust ecosystem) or `file-lock` for cross-platform file locking
2. In `daemon_start`, open the PID file with exclusive lock before writing
3. Hold the lock for the lifetime of the daemon process (lock released on process exit)
4. Second daemon start attempt will fail to acquire lock → clean error message
5. Check if `fs2` is already a dependency; if not, add it (`fs2` is lightweight)

**Acceptance criteria:**
- [ ] PID file is exclusively locked during daemon lifetime
- [ ] Second concurrent `daemon_start` fails with clear error before port binding
- [ ] Lock is released on daemon exit (including crash — OS releases flock)
- [ ] Fast-tier tests pass

---

## Execution Order

No strict dependencies except: Task 1 should complete before Task 4 (both touch navigation code).

**Suggested batches:**
- Batch 1 (independent): Tasks 2, 3, 7, 8
- Batch 2 (independent): Tasks 1, 5, 6
- Batch 3 (after Task 1): Task 4
