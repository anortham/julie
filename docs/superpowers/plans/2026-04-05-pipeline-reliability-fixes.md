# Pipeline Reliability Fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all Critical and High severity issues found in the 2026-04-05 pipeline review, hardening the indexing/embedding/watcher/editing pipeline before IU onboarding.

**Context:** 4-reviewer audit (3 Sonnet agents + Codex cross-cutting) produced 27 findings. All findings deduplicated into 13 fix tasks covering every severity level. Nothing deferred.

**Architecture:** Surgical fixes across watcher, indexing, editing, embedding, and handler modules. Tasks 1-3 are the highest priority (data corruption/silent staleness). Tasks 4-8 are important but less likely to cause data loss.

**Tech Stack:** Rust, Tantivy, SQLite, tokio, tree-sitter, blake3, notify

---

## File Map

| Task | Primary Files | Test Files |
|------|--------------|------------|
| 1 | `src/tools/editing/edit_symbol.rs`, `src/tools/editing/edit_file.rs` | `src/tests/tools/editing/edit_symbol_tests.rs` |
| 2 | `src/tools/workspace/commands/index.rs` | `src/tests/tools/workspace/index_embedding_tests.rs` |
| 3 | `src/watcher/events.rs`, `src/watcher/handlers.rs` | `src/tests/integration/watcher.rs` |
| 4 | `src/watcher/mod.rs`, `src/watcher/handlers.rs` | `src/tests/integration/watcher.rs` |
| 5 | `src/watcher/mod.rs` | `src/tests/integration/watcher.rs` |
| 6 | `src/tools/workspace/indexing/embeddings.rs` | `src/tests/tools/workspace/index_embedding_tests.rs` |
| 7 | `src/tools/workspace/indexing/incremental.rs`, `src/watcher/handlers.rs` | existing tests |
| 8 | `src/startup.rs`, `src/watcher/filtering.rs` | `src/tests/integration/stale_index_detection.rs` |

---

## Task 1: CRITICAL -- Stop edit tools from poisoning the watcher hash

**Sources:** Codex cross-cutting #1, embed-edit-reviewer T1, embed-edit-reviewer T2
**Severity:** Critical (silent data corruption, permanent index staleness)

**Problem:** After `edit_symbol` or `edit_file` writes a file, they call `update_file_hash()` which stores the new blake3 hash AND bumps `last_indexed`. The watcher's blake3 comparison in `handle_file_created_or_modified_static` then sees "hash matches" and skips symbol re-extraction entirely. The startup catch-up path also skips the file (mtime vs `last_indexed`). Result: edited files NEVER get their symbols re-extracted. Every subsequent `edit_symbol` on that file operates on stale line numbers.

**Files:**
- `src/tools/editing/edit_symbol.rs:335-347` (update_file_hash call)
- `src/tools/editing/edit_file.rs:355-366` (update_file_hash call)
- `src/watcher/handlers.rs:53` (hash comparison skip)
- `src/tools/editing/edit_symbol.rs:148-169` (freshness check holds DB mutex during I/O)

**Fix approach:** Remove the `update_file_hash` calls from both edit tools. Instead, let the watcher detect the change naturally (hash won't match the pre-edit hash in the DB). The freshness guard in `check_file_freshness` will correctly reject back-to-back edits until the watcher re-extracts symbols, which is the safe behavior. The error message already tells the agent to wait.

Additionally, fix the DB mutex issue in `check_file_freshness`: compute the blake3 hash BEFORE acquiring the DB lock, then pass the pre-computed hash in. This eliminates blocking I/O while holding the mutex.

- [ ] **Step 1: Write failing test** -- Test that after editing a file, the watcher re-extracts symbols (currently it won't because the hash is pre-updated)
- [ ] **Step 2: Remove `update_file_hash` from edit_symbol.rs** (around line 339)
- [ ] **Step 3: Remove `update_file_hash` from edit_file.rs** (around line 359)
- [ ] **Step 4: Refactor `check_file_freshness`** to compute hash before acquiring DB lock
- [ ] **Step 5: Verify test passes** -- watcher now re-extracts symbols after edit
- [ ] **Step 6: Verify freshness guard correctly rejects stale edits** -- back-to-back edit_symbol calls should fail with "file changed since last indexing"

---

## Task 2: CRITICAL -- Fix reference workspace routing in handle_index_command

**Sources:** index-reviewer I1, index-reviewer I2
**Severity:** Critical (data loss: force-reindex ref clears primary embeddings)

**Problem:** `handle_index_command` uses `handler.workspace_id` (always the primary ID) even when indexing a reference workspace. This causes: (a) force-clear embeddings targeting the primary DB instead of the reference DB, (b) daemon.db stats overwritten with reference workspace values, (c) embedding spawn targeting primary instead of reference.

**Files:**
- `src/tools/workspace/commands/index.rs:248-265` (workspace_id derivation)
- `src/tools/workspace/commands/index.rs:299-327` (force-clear path)
- `src/tools/workspace/commands/index.rs:350-354` (embedding spawn)

**Fix approach:** Derive `workspace_id` from the canonical path when `is_reference_workspace` is true, using `generate_workspace_id(&canonical_path_str)`. This single fix at the derivation point corrects all downstream uses (stats, embedding spawn, force-clear).

- [ ] **Step 1: Write failing test** -- Force-index a reference workspace path, assert primary embeddings are untouched
- [ ] **Step 2: Fix workspace_id derivation** to use `generate_workspace_id` for reference paths
- [ ] **Step 3: Verify test passes**
- [ ] **Step 4: Verify daemon.db stats are attributed to reference workspace ID**

---

## Task 3: HIGH -- Fix macOS rename handling (orphaned old-path symbols)

**Sources:** watcher-reviewer W1
**Severity:** High (permanent stale data on macOS, the primary platform for IU users)

**Problem:** On macOS, notify-rs emits `RenameMode::Any` (not `RenameMode::Both`). The current code falls through to `FileChangeType::Modified` for whatever path is provided. The old (now-deleted) path fails `should_index_file`'s `path.is_file()` check and is silently dropped. Its DB symbols, embeddings, and Tantivy docs are never cleaned up.

**Files:**
- `src/watcher/events.rs:104-121` (rename event mapping)
- `src/watcher/handlers.rs` (delete handler)

**Fix approach:** For `RenameMode::Any` events, check if the path exists on disk. If it doesn't exist, emit `FileChangeType::Deleted` (using `should_process_deletion` which doesn't require the file to exist). If it does exist, emit `FileChangeType::Modified` as before.

- [ ] **Step 1: Write failing test** -- Simulate rename event with non-existent path, assert old path symbols are cleaned up
- [ ] **Step 2: Add existence check in RenameMode::Any handler** in events.rs
- [ ] **Step 3: Verify test passes**

---

## Task 4: HIGH -- Fix delete TOCTOU and SQLite/Tantivy atomicity

**Sources:** watcher-reviewer W3, Codex cross-cutting #3
**Severity:** High (inconsistent state between SQLite and Tantivy)

**Problem:** Two related issues:
(a) Delete path has double `path.exists()` check -- embeddings can be deleted while symbols and Tantivy docs survive if the file is recreated between checks.
(b) SQLite commits first, then Tantivy updates are best-effort (warn on failure). Tantivy failures leave it drifting from SQLite with no repair mechanism.

**Files:**
- `src/watcher/mod.rs:129-155` (dispatch_file_event delete path)
- `src/watcher/handlers.rs:229-232` (redundant exists check)
- `src/watcher/handlers.rs:195-204` (Tantivy warn-only failures)

**Fix approach:**
(a) Remove the redundant `path.exists()` check from `handle_file_deleted_static` -- trust the caller's decision.
(b) When Tantivy operations fail after SQLite commit, record the file path in a "dirty" set. On the next successful tick, retry Tantivy operations for dirty files. This prevents silent drift.

- [ ] **Step 1: Write test for TOCTOU** -- Delete event where path is immediately recreated, assert symbols ARE cleaned up
- [ ] **Step 2: Remove redundant exists check** from handle_file_deleted_static
- [ ] **Step 3: Add Tantivy retry mechanism** -- dirty set for failed Tantivy ops, retry on next tick
- [ ] **Step 4: Verify tests pass**

---

## Task 5: HIGH -- Add queue overflow recovery and fix HOL blocking

**Sources:** watcher-reviewer W2, watcher-reviewer W4
**Severity:** High (silent data loss on branch switch or large git operations)

**Problem:** Two related issues:
(a) Queue overflow (>1000 events) drains oldest events with no recovery. Dropped Created/Deleted events permanently break the index.
(b) The re-queue-and-break coalescing pattern blocks all events behind a single deduped event, adding 1-second delays to unrelated files.

**Files:**
- `src/watcher/mod.rs:333-373` (coalescing loop)
- `src/watcher/mod.rs` (queue overflow handling)

**Fix approach:**
(a) On overflow, set a `needs_rescan` flag. After the queue drains, trigger a `filter_changed_files` pass to catch anything that was dropped.
(b) Change the coalescing loop: when an event is within the dedup window, push it to the back but `continue` processing remaining events instead of `break`.

- [ ] **Step 1: Add `needs_rescan` flag** to IncrementalIndexer
- [ ] **Step 2: On queue overflow, set the flag** instead of silently draining
- [ ] **Step 3: After queue drains with flag set, run filter_changed_files** to catch missed changes
- [ ] **Step 4: Fix coalescing loop** -- change `break` to `continue` for dedup events
- [ ] **Step 5: Write tests** for both overflow recovery and non-blocking coalescing

---

## Task 6: HIGH -- Fix stale vectors from cancelled/concurrent embedding pipelines

**Sources:** embed-edit-reviewer E1, embed-edit-reviewer E2, Codex cross-cutting #5
**Severity:** High (stale/orphaned vectors after cancel or concurrent runs)

**Problem:** Three related issues:
(a) `handle.abort()` doesn't stop `spawn_blocking` threads. Old pipeline batches can write stale vectors after force reindex clears the table.
(b) daemon.db vector_count/embedding_model never updated after abort.
(c) Bulk embedding and watcher's per-file embedding can race on the same symbol_vectors rows.

**Files:**
- `src/tools/workspace/indexing/embeddings.rs:161-174` (abort handling)
- `src/tools/workspace/indexing/embeddings.rs:177-194` (stats update)
- `src/embeddings/pipeline.rs:411` (cancel flag check)
- `src/watcher/mod.rs:114` (per-file reembed call)

**Fix approach:**
(a) Add cancel flag check AFTER each batch write (not just at batch start). Don't rely on `abort()` for correctness.
(b) Move vector_count update to an unconditional finally block using `embedding_count()` from the DB.
(c) Skip files in the bulk pipeline whose mtime is newer than the pipeline's start timestamp (they'll be handled by the watcher's per-file path).

- [ ] **Step 1: Add cancel check after batch write** in pipeline.rs
- [ ] **Step 2: Move daemon.db stats update** to unconditional path after spawn_blocking
- [ ] **Step 3: Add mtime filter** to bulk pipeline to skip recently-changed files
- [ ] **Step 4: Write tests** for cancel-after-batch and stats-after-abort

---

## Task 7: HIGH -- Fix catch-up vs watcher race and cleanup gaps

**Sources:** Codex cross-cutting #2, watcher-reviewer W6, Codex cross-cutting #4
**Severity:** High (older catch-up snapshot overwrites newer watcher data)

**Problem:** Three related issues:
(a) Catch-up indexing and the watcher can process the same file concurrently. The catch-up bulk commit can overwrite newer watcher results.
(b) `clean_orphaned_files` in catch-up doesn't remove Tantivy docs or embeddings for deleted files.
(c) Force reindex on reference workspace doesn't stop that workspace's watcher.

**Files:**
- `src/handler.rs:1229` (on_initialized catch-up)
- `src/tools/workspace/indexing/incremental.rs:225,249,343` (clean_orphaned_files)
- `src/tools/workspace/commands/index.rs:157` (reference workspace force path)

**Fix approach:**
(a) Pause the watcher during catch-up indexing. Resume after the bulk pass completes and drain any buffered events.
(b) Add Tantivy remove and embedding delete calls to `clean_orphaned_files`.
(c) Route reference workspace force reindex through the watcher pool to stop/pause the watcher first.

- [ ] **Step 1: Add pause/resume** to IncrementalIndexer
- [ ] **Step 2: Pause watcher before catch-up**, resume after
- [ ] **Step 3: Add Tantivy + embedding cleanup** to clean_orphaned_files
- [ ] **Step 4: Stop reference watcher during force reindex**
- [ ] **Step 5: Write tests** for catch-up/watcher ordering

---

## Task 8: MEDIUM -- Consolidate is_code_file with canonical extension list

**Sources:** index-reviewer I3
**Severity:** Medium (spurious catch-up triggers for Scala/Elixir projects)

**Problem:** `is_code_file` in startup.rs maintains a separate hardcoded extension list that has diverged from the watcher's canonical list in `src/watcher/filtering.rs`. Missing: `.scala`, `.sc`, `.ex`, `.exs`, `.mts`, `.cts`, and others.

**Files:**
- `src/startup.rs:270-326` (is_code_file)
- `src/watcher/filtering.rs` (canonical list)

**Fix approach:** Delete `is_code_file` from startup.rs and delegate to the watcher's filtering module (or extract a shared function).

- [ ] **Step 1: Extract shared `is_indexable_extension` function** from filtering.rs
- [ ] **Step 2: Replace startup.rs `is_code_file`** with call to shared function
- [ ] **Step 3: Verify no extension gaps remain**

---

## Task 9: MEDIUM -- Fix write lock held across await in on_initialized

**Sources:** index-reviewer I4
**Severity:** Medium (serializes reconnect handlers behind first session's backfill)

**Problem:** `on_initialized` holds a tokio `RwLock` write guard on `is_indexed` across `backfill_vector_count().await`, which does DB I/O. This blocks all concurrent `is_indexed` readers/writers.

**Files:**
- `src/handler.rs:1236-1245`

**Fix:** Release the lock before awaiting. Read first, then write separately.

- [ ] **Step 1: Refactor lock usage** -- read lock to check, release, do async work, write lock to set
- [ ] **Step 2: Verify no regression** in session connect behavior

---

## Task 10: MEDIUM -- Fix stop() aborting task without draining

**Sources:** watcher-reviewer W5
**Severity:** Medium (partial state on shutdown: SQLite updated but Tantivy not)

**Problem:** `handle.abort()` cancels the queue task at the next await point. If mid-dispatch (after SQLite write, before Tantivy update), leaves inconsistent state.

**Files:**
- `src/watcher/mod.rs:488-508`

**Fix:** Set the cancel flag and wait for the task to finish its current iteration naturally, rather than using `abort()`.

- [ ] **Step 1: Replace abort() with cancel flag + join** in stop()
- [ ] **Step 2: Verify graceful shutdown** completes current event before exiting

---

## Task 11: MEDIUM -- Wrap reembed_symbols_for_file in spawn_blocking

**Sources:** embed-edit-reviewer E3
**Severity:** Medium (blocks async runtime thread during sidecar IPC)

**Problem:** `reembed_symbols_for_file` is called directly from async context in `dispatch_file_event`. It does synchronous blocking IPC to the Python sidecar via `std::sync::Mutex` and `recv_timeout()`, stalling the watcher queue.

**Files:**
- `src/watcher/mod.rs:115,180`

**Fix:** Wrap the `reembed_symbols_for_file` call in `tokio::task::spawn_blocking`, matching how `spawn_workspace_embedding` handles it.

- [ ] **Step 1: Wrap both call sites** in spawn_blocking
- [ ] **Step 2: Verify watcher responsiveness** is not degraded during embedding

---

## Task 12: MEDIUM -- Fix MultiFileTransaction rollback atomicity

**Sources:** embed-edit-reviewer T3
**Severity:** Medium (crash during rollback can truncate files)

**Problem:** `rollback_partial_commit` uses plain `fs::write` (not atomic temp+rename). Files that didn't originally exist are stored as `""`, so rollback creates empty files instead of deleting them.

**Files:**
- `src/tools/editing/mod.rs:249-260`

**Fix:** Use temp+rename for rollback writes. For files that didn't exist before the transaction, delete them on rollback.

- [ ] **Step 1: Add sentinel for "file did not exist"** in the files map
- [ ] **Step 2: Use atomic temp+rename** in rollback_partial_commit
- [ ] **Step 3: Delete files that didn't exist** instead of writing empty
- [ ] **Step 4: Write test** for rollback after partial commit failure

---

## Task 13: LOW -- Fix remaining small issues (batch)

**Sources:** I5, W7, W8, E4, T4, T5
**Severity:** Low (6 items, all small and independent)

**Fixes:**
1. **I5**: Add `AtomicBool` "catch-up in progress" flag to dedup concurrent `run_auto_indexing()` tasks (`src/handler.rs:1242-1245`)
2. **W7**: Inline the `last_processed.lock().await.remove()` call in `clear_dedup_on_delete` instead of spawning a detached task (`src/watcher/mod.rs:377-385`)
3. **W8**: Replace stub `test_blake3_change_detection_placeholder` with a real test: write file, index, write same content (no re-index), write different content (re-index) (`src/tests/integration/watcher.rs:196-198`)
4. **E4**: Change cancel flag `Ordering::Relaxed` to `Ordering::Release`/`Ordering::Acquire` to match watcher pattern (`src/tools/workspace/indexing/embeddings.rs:143`)
5. **T4**: Add trailing-newline preservation to `insert_near_symbol` matching `replace_symbol_body` logic (`src/tools/editing/edit_symbol.rs:107-144`)
6. **T5**: Add `search_from = end.max(pos + 1)` defensive guard in DMP bitap loop (`src/tools/editing/edit_file.rs:143-154`)

- [ ] **Step 1: Fix all 6 items**
- [ ] **Step 2: Write real blake3 detection test** (W8)
- [ ] **Step 3: Verify all pass**

---

## Execution Notes

- **All 13 tasks are real bugs and will be fixed**
- Tasks 1-8 are independent and parallelizable
- Tasks 9-12 are independent and parallelizable
- Task 13 batches 6 small fixes
- All tasks follow TDD: write failing test, implement fix, verify green
- After all tasks: run `cargo xtask test full` for broad regression check
