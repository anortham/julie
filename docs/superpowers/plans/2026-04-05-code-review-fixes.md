# Code Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all 7 validated code review findings from the 2026-04-05 external review, plus add end-to-end edit_symbol tests.

**Architecture:** Each fix is independent and touches a different subsystem. Tasks are ordered by priority (High first). The freshness guard (Task 1) and deletion detection (Task 2) both leverage existing blake3 hash infrastructure in the `files` table. The watcher dedup fix (Task 3) changes queue processing strategy. Tasks 4-8 are smaller, isolated changes.

**Tech Stack:** Rust, SQLite (rusqlite), Tantivy, tokio, tree-sitter, blake3

---

## File Map

| Task | Files Modified | Files Created | Test Files |
|------|---------------|---------------|------------|
| 1 | `src/tools/editing/edit_symbol.rs` | | `src/tests/tools/editing/edit_symbol_tests.rs` |
| 2 | `src/startup.rs` | | `src/tests/integration/stale_index_detection.rs` |
| 3 | `src/watcher/mod.rs` | | `src/tests/integration/watcher.rs` (or new file) |
| 4 | `src/handler.rs`, `src/tools/workspace/indexing/embeddings.rs` | | `src/tests/tools/editing/edit_symbol_tests.rs` |
| 5 | `.claude/skills/web-research/SKILL.md` | | (manual validation) |
| 6 | `src/handler.rs` | | (no test needed) |
| 7 | `src/tools/editing/validation.rs` | | `src/tests/tools/editing/edit_symbol_tests.rs` |
| 8 | | | `src/tests/tools/editing/edit_symbol_tests.rs` |

---

### Task 1: edit_symbol Freshness Guard (High)

**Problem:** `edit_symbol` resolves `start_line`/`end_line` from the DB and applies them to current file contents with no freshness check. A stale index means edits land on wrong lines.

**Files:**
- Modify: `src/tools/editing/edit_symbol.rs` (the `call_tool` method)
- Test: `src/tests/tools/editing/edit_symbol_tests.rs`

**Approach:** After resolving the symbol from the DB, compute the current file's blake3 hash and compare it against the stored hash in the `files` table. If they differ, return a clear error telling the agent the index is stale for that file, rather than silently applying to wrong lines. We use hash comparison (not mtime) because it's what the rest of Julie uses for change detection and it's immune to filesystem timestamp resolution issues.

- [ ] **Step 1: Write the failing test**

Add to `src/tests/tools/editing/edit_symbol_tests.rs`:

```rust
#[test]
fn test_replace_rejects_when_file_changed() {
    // Simulate stale index: the source has been modified since indexing.
    // start_line/end_line from the DB point at the ORIGINAL content,
    // but the file now has extra lines inserted before the symbol.
    let original = "line1\nfn foo() {\n    bar()\n}\nline5\n";
    let modified_file = "line1\nnew_line_inserted\nfn foo() {\n    bar()\n}\nline5\n";

    // Original index says foo is at lines 2-4.
    // In the modified file, foo is now at lines 3-5.
    // Replacing lines 2-4 in the modified file would hit "new_line_inserted\nfn foo() {\n    bar()"
    // which is wrong.
    let result = replace_symbol_body(modified_file, 2, 4, "fn foo() {\n    baz()\n}");
    // With the current code, this SUCCEEDS (it blindly applies line ranges).
    // After the fix, a freshness check at the call_tool level prevents this scenario.
    // This helper function itself won't have the check (it's pure line manipulation),
    // so this test documents the dangerous behavior.
    assert!(result.is_ok(), "Helper function applies blindly (the guard is in call_tool)");
    let content = result.unwrap();
    // Demonstrate the bug: the replacement hit the wrong lines
    assert!(content.contains("new_line_inserted") == false,
        "BUG DEMONSTRATION: stale line range ate the wrong content");
}
```

Note: This test documents the *existing* dangerous behavior of the helper. The real fix is in `call_tool`, which is integration-tested in Task 8.

- [ ] **Step 2: Run test to verify behavior**

Run: `cargo test --lib test_replace_rejects_when_file_changed 2>&1 | tail -10`
Expected: The `assert!` on the last line should FAIL (demonstrating the bug -- the helper *does* eat the wrong content because it has no guard).

- [ ] **Step 3: Add `check_file_freshness` function to edit_symbol.rs**

Add this function before the `impl EditSymbolTool` block in `src/tools/editing/edit_symbol.rs`:

```rust
/// Check if a file's current content matches what was indexed.
/// Returns Ok(()) if fresh, Err with a descriptive message if stale.
fn check_file_freshness(
    db: &std::sync::MutexGuard<'_, crate::database::SymbolDatabase>,
    file_path: &str,
    resolved_path: &std::path::Path,
) -> Result<()> {
    let current_hash = crate::database::files::calculate_file_hash(resolved_path)
        .map_err(|e| anyhow!("Cannot hash file '{}': {}", file_path, e))?;

    match db.get_file_hash(file_path)? {
        Some(indexed_hash) if indexed_hash == current_hash => Ok(()),
        Some(_) => Err(anyhow!(
            "File '{}' has changed since last indexing. \
             Run manage_workspace(operation=\"index\") or wait for the file watcher to catch up, \
             then retry.",
            file_path
        )),
        None => Err(anyhow!(
            "File '{}' is not in the index. \
             Run manage_workspace(operation=\"index\") first.",
            file_path
        )),
    }
}
```

- [ ] **Step 4: Integrate freshness check into `call_tool`**

In the `call_tool` method, after resolving the file path and before reading the file, add the freshness check. Insert this block after the `let resolved_str = ...` line and before the `let original_content = std::fs::read_to_string(...)` line:

```rust
        // Freshness guard: verify the file hasn't changed since it was indexed.
        // If the index is stale, the start_line/end_line from find_symbol may point
        // at wrong content. Refuse rather than silently corrupt.
        {
            let db = db_arc
                .lock()
                .map_err(|e| anyhow!("Database lock error: {}", e))?;
            if let Err(e) = check_file_freshness(&db, symbol_file, &resolved_path) {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Error: {}", e
                ))]));
            }
        }
```

This requires cloning `db_arc` earlier. Move the `db_arc` clone up from the `spawn_blocking` closure. The `spawn_blocking` block already clones it internally, so extract `db_arc` as a local before that block:

The existing code does:
```rust
        let db_arc = workspace.db.as_ref().ok_or_else(...)?.clone();
```

This is already a local. After the `spawn_blocking` resolves the symbol, we need a second lock. Add the freshness check between resolving the symbol and reading the file. The `db_arc` is available from the earlier `let db_arc = workspace.db...` line, but it was moved into the `spawn_blocking` closure. Fix: clone it before the closure:

```rust
        let db_arc_for_freshness = db_arc.clone();
```

Place this line right before the `let matches = tokio::task::spawn_blocking(move || -> ...` block. Then use `db_arc_for_freshness` in the freshness check.

- [ ] **Step 5: Run tests to verify the fix compiles**

Run: `cargo test --lib test_replace_symbol_body 2>&1 | tail -10`
Expected: PASS (existing tests still work)

- [ ] **Step 6: Update the test to document the guard location**

Update the test from Step 1 to be a documentation test that passes:

```rust
#[test]
fn test_replace_helper_is_unguarded() {
    // replace_symbol_body is a pure line-manipulation helper with no freshness check.
    // The freshness guard lives in EditSymbolTool::call_tool (blake3 hash comparison).
    // This test documents that the helper applies blindly -- callers must verify freshness.
    let modified_file = "line1\nnew_line_inserted\nfn foo() {\n    bar()\n}\nline5\n";
    let result = replace_symbol_body(modified_file, 2, 4, "fn foo() {\n    baz()\n}");
    assert!(result.is_ok());
    let content = result.unwrap();
    // The helper replaces lines 2-4 regardless of what's there.
    // In a stale-index scenario, this produces wrong output.
    // call_tool's freshness check prevents this from happening in practice.
    assert!(!content.contains("fn foo() {\n    bar()"), "Old foo body should be replaced");
}
```

- [ ] **Step 7: Run test**

Run: `cargo test --lib test_replace_helper_is_unguarded 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/tools/editing/edit_symbol.rs src/tests/tools/editing/edit_symbol_tests.rs
git commit -m "fix(edit_symbol): add blake3 freshness guard before applying indexed line ranges

Compares current file hash against indexed hash before applying edits.
Returns a clear error if the index is stale, preventing silent corruption
when start_line/end_line point at wrong content."
```

---

### Task 2: Session-Connect Deletion Detection (High)

**Problem:** `check_if_indexing_needed()` detects new and modified files but never checks for files that were deleted while the daemon was down. Dead symbols persist in search results.

**Files:**
- Modify: `src/startup.rs` (`check_if_indexing_needed` function)
- Test: `src/tests/integration/stale_index_detection.rs`

**Approach:** After the existing new-files check, compute `indexed_files.difference(&workspace_files)`. If deleted files are found, clean them up directly (delete their symbols and file records from DB, remove from Tantivy) rather than triggering a full reindex. This is a lightweight cleanup, not a heavy operation.

- [ ] **Step 1: Write the failing test**

Add to `src/tests/integration/stale_index_detection.rs`:

```rust
/// Given: A workspace with 3 indexed files, then 1 file is deleted from disk
/// When: check_if_indexing_needed() is called
/// Expected: Returns true (cleanup needed for deleted file)
#[tokio::test]
async fn test_deleted_file_detected_on_reconnect() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create 3 source files
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    fs::write(src_dir.join("a.rs"), "fn a() {}\n")?;
    fs::write(src_dir.join("b.rs"), "fn b() {}\n")?;
    fs::write(src_dir.join("c.rs"), "fn c() {}\n")?;

    // Index the workspace
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Delete one file (simulating deletion while daemon was down)
    fs::remove_file(src_dir.join("b.rs"))?;

    // check_if_indexing_needed should detect the deleted file
    let needs_indexing = crate::startup::check_if_indexing_needed(&handler).await?;
    assert!(needs_indexing, "Should detect deleted file b.rs needs cleanup");

    Ok(())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_deleted_file_detected_on_reconnect 2>&1 | tail -10`
Expected: FAIL -- currently returns `false` because deletions aren't checked.

- [ ] **Step 3: Add deletion detection to `check_if_indexing_needed`**

In `src/startup.rs`, in the `check_if_indexing_needed` function, after the block that checks for new files (`if !new_files.is_empty() { ... }`), add:

```rust
                // Check for deleted files (indexed but no longer on disk)
                let deleted_files: Vec<_> = indexed_files.difference(&workspace_files).collect();

                if !deleted_files.is_empty() {
                    info!(
                        "📊 Found {} deleted files still in database - cleanup needed",
                        deleted_files.len()
                    );
                    debug!("Deleted files: {:?}", deleted_files);
                    return Ok(true);
                }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_deleted_file_detected_on_reconnect 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Add cleanup logic for deleted files on reconnect**

The detection now returns `true`, which triggers a full reindex via `index_workspace_files`. That path already calls `filter_changed_files` which calls `clean_orphaned_files`. So deleted files get cleaned up during the reindex.

However, this is heavier than needed -- a full reindex when only deletions occurred. For now this is acceptable because:
1. `filter_changed_files` uses hash comparison, so unchanged files are skipped quickly
2. `clean_orphaned_files` handles the actual deletion cleanup
3. A lighter dedicated cleanup path is an optimization, not a correctness fix

Add a comment documenting this:

```rust
                // Note: returning true triggers index_workspace_files, which calls
                // filter_changed_files -> clean_orphaned_files. This cleans up the
                // deleted files' symbols and DB records. A lighter dedicated cleanup
                // path could avoid the hash-comparison pass for unchanged files,
                // but correctness is more important than startup speed here.
```

- [ ] **Step 6: Run test**

Run: `cargo test --lib test_deleted_file_detected_on_reconnect 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/startup.rs src/tests/integration/stale_index_detection.rs
git commit -m "fix(startup): detect deleted files during session-connect catch-up

check_if_indexing_needed now computes indexed_files - workspace_files
to find files deleted while the daemon was down. Returns true so the
subsequent reindex path cleans up orphaned symbols and search hits."
```

---

### Task 3: Watcher Dedup Coalescing (High)

**Problem:** The watcher's dedup logic drops events for the same path within 1 second, rather than processing the latest state. A real second save within the window is silently ignored.

**Files:**
- Modify: `src/watcher/mod.rs` (the queue processor loop in `start_watching`)
- Test: `src/tests/integration/watcher.rs` or a new dedicated test

**Approach:** Instead of skipping the event entirely when a recent processing is detected, re-queue the event to be processed on the next tick. This ensures the latest state is always eventually processed. The re-queued event will be picked up ~1 second later on the next tick, at which point the dedup window has expired.

- [ ] **Step 1: Write the failing test**

This is tricky to unit test because the dedup is deep inside an async loop. Instead, write a focused test for the dedup logic extracted as a helper. But the current code has the dedup inline. The simplest approach: test the observable behavior via the `process_pending_changes` method.

Add a test to `src/tests/integration/watcher.rs` (or create `src/tests/integration/watcher_dedup.rs`):

```rust
/// Verify that rapid file changes within the dedup window still result in
/// the latest content being indexed (not the first-save content).
#[tokio::test]
async fn test_rapid_saves_coalesce_to_latest() -> Result<()> {
    use std::time::Duration;
    use tempfile::TempDir;

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    let src_dir = workspace_path.join("src");
    fs::create_dir_all(&src_dir)?;
    let file_path = src_dir.join("rapid.rs");

    // Initial content
    fs::write(&file_path, "fn version1() {}\n")?;

    // Set up handler and index
    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    // Simulate two rapid saves by queuing two events for the same path
    let workspace = handler.get_workspace().await?.unwrap();
    let watcher = workspace.watcher.as_ref().unwrap();

    // Queue first event
    {
        let mut queue = watcher.index_queue.lock().await;
        queue.push_back(crate::watcher::FileChangeEvent {
            path: file_path.clone(),
            change_type: crate::watcher::FileChangeType::Modified,
        });
    }

    // Write updated content (simulating second save)
    fs::write(&file_path, "fn version2() {}\n")?;

    // Queue second event for same path
    {
        let mut queue = watcher.index_queue.lock().await;
        queue.push_back(crate::watcher::FileChangeEvent {
            path: file_path.clone(),
            change_type: crate::watcher::FileChangeType::Modified,
        });
    }

    // Process both events
    watcher.process_pending_changes().await?;

    // The indexed content should reflect the LATEST save (version2), not version1
    let db = workspace.db.as_ref().unwrap().lock().unwrap();
    let hash = db.get_file_hash("src/rapid.rs")?;
    let expected_hash = crate::database::files::calculate_file_hash(&file_path)?;
    assert_eq!(hash, Some(expected_hash), "Index should reflect latest file content");

    Ok(())
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_rapid_saves_coalesce_to_latest 2>&1 | tail -10`
Expected: May pass or fail depending on timing. The key behavioral issue is in the background loop, not `process_pending_changes` (which doesn't have dedup). Let's verify.

Actually, looking at the code more carefully: `process_pending_changes()` does NOT have the dedup logic -- only the background loop in `start_watching` does. So the test above would pass even without the fix. We need a different approach.

- [ ] **Step 2 (revised): Change approach -- fix the dedup inline**

The dedup bug is in the background queue processor loop. Rather than testing it end-to-end (which requires timing-sensitive async coordination), the fix is straightforward enough to apply directly and verify via code review + existing watcher tests.

The fix: when the dedup check fires (same path within 1s), instead of `continue` (dropping the event), push it back to the end of the queue:

In `src/watcher/mod.rs`, in the `start_watching` method, find the dedup block:

```rust
                    if should_skip {
                        continue;
                    }
```

Replace with:

```rust
                    if should_skip {
                        // Re-queue so the latest state gets processed on the next tick.
                        // The event will be picked up ~1s later when the dedup window expires.
                        let mut queue = queue_for_processing.lock().await;
                        queue.push_back(event);
                        continue;
                    }
```

- [ ] **Step 3: Verify no infinite loop risk**

The re-queued event will be dequeued on the next `tick.tick().await` iteration (1 second later). By then, the dedup window (1s) will have expired, so it will be processed normally. No infinite loop.

Edge case: if the same file generates continuous events faster than 1s, the queue could accumulate. But the dedup eviction (entries >2s are cleaned) prevents unbounded growth, and the re-queue only adds one event per skip. This is safe.

- [ ] **Step 4: Run existing watcher tests**

Run: `cargo test --lib tests::integration::watcher 2>&1 | tail -20`
Expected: All existing tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/watcher/mod.rs
git commit -m "fix(watcher): coalesce rapid saves instead of dropping them

When the dedup check fires for the same path within 1 second, re-queue
the event instead of dropping it. The re-queued event is processed on
the next tick when the dedup window has expired, ensuring the latest
file state is always indexed."
```

---

### Task 4: Per-Workspace Embedding Cancellation (Medium)

**Problem:** `handler.embedding_task` is a single slot. Starting embeddings for workspace B cancels workspace A's in-progress pipeline.

**Files:**
- Modify: `src/handler.rs` (field type + initialization)
- Modify: `src/tools/workspace/indexing/embeddings.rs` (cancel + store logic)

**Approach:** Change `embedding_task` from `Option<(AtomicBool, JoinHandle)>` to `HashMap<String, (AtomicBool, JoinHandle)>` keyed by workspace_id. Only cancel the matching workspace's task, not all of them.

- [ ] **Step 1: Change the field type in handler.rs**

In `src/handler.rs`, change the `embedding_task` field from:

```rust
    pub(crate) embedding_task: Arc<
        tokio::sync::Mutex<
            Option<(
                Arc<std::sync::atomic::AtomicBool>,
                tokio::task::JoinHandle<()>,
            )>,
        >,
    >,
```

To:

```rust
    /// Per-workspace embedding pipeline: cancellation flag + task handle.
    /// Keyed by workspace_id so concurrent workspaces don't cancel each other.
    pub(crate) embedding_tasks: Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<
                String,
                (
                    Arc<std::sync::atomic::AtomicBool>,
                    tokio::task::JoinHandle<()>,
                ),
            >,
        >,
    >,
```

- [ ] **Step 2: Update initialization sites**

Change both initialization sites (lines ~212 and ~281 in `handler.rs`) from:

```rust
            embedding_task: Arc::new(tokio::sync::Mutex::new(None)),
```

To:

```rust
            embedding_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
```

- [ ] **Step 3: Update embeddings.rs cancel logic**

In `src/tools/workspace/indexing/embeddings.rs`, the cancel block currently does:

```rust
        let mut task_guard = handler.embedding_task.lock().await;
        if let Some((cancel_flag, handle)) = task_guard.take() {
            info!("Cancelling previous embedding pipeline before starting new one");
            cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            handle.abort();
        }
```

Replace with:

```rust
        let mut tasks = handler.embedding_tasks.lock().await;
        if let Some((cancel_flag, handle)) = tasks.remove(&workspace_id) {
            info!("Cancelling previous embedding pipeline for workspace {workspace_id}");
            cancel_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            handle.abort();
        }
```

- [ ] **Step 4: Update the store logic**

The current store logic:

```rust
        let mut task_guard = handler.embedding_task.lock().await;
        *task_guard = Some((cancel_flag, handle));
```

Replace with:

```rust
        let mut tasks = handler.embedding_tasks.lock().await;
        tasks.insert(workspace_id.clone(), (cancel_flag, handle));
```

Note: `workspace_id` is used inside the spawned async block. You'll need a `let workspace_id_for_store = workspace_id.clone();` before the `tokio::spawn` and use it in the store logic after.

- [ ] **Step 5: Update the self-cleanup inside the spawned task**

The spawned task currently does:

```rust
        let mut slot = embedding_task_slot.lock().await;
        if let Some((ref stored_flag, _)) = *slot {
            if Arc::ptr_eq(stored_flag, &self_cancel_flag) {
                *slot = None;
            }
        }
```

Replace with:

```rust
        let mut tasks = embedding_task_slot.lock().await;
        if let Some((ref stored_flag, _)) = tasks.get(&workspace_id) {
            if Arc::ptr_eq(stored_flag, &self_cancel_flag) {
                tasks.remove(&workspace_id);
            }
        }
```

The `workspace_id` variable is already captured by the async block (it's used in the log messages). The `embedding_task_slot` variable needs renaming to `embedding_tasks_slot` for clarity but functionally works the same.

- [ ] **Step 6: Fix compilation and run tests**

Run: `cargo test --lib test_refresh_no_changes_skips_embedding_pipeline 2>&1 | tail -10`
Expected: PASS

Also check for any other references to `embedding_task` (singular):

Search for `embedding_task` in the codebase and update any remaining references to `embedding_tasks`.

- [ ] **Step 7: Commit**

```bash
git add src/handler.rs src/tools/workspace/indexing/embeddings.rs
git commit -m "fix(embeddings): per-workspace cancellation instead of global slot

Changed embedding_task from a single Option slot to a HashMap keyed by
workspace_id. Starting embeddings for one workspace no longer cancels
another workspace's in-progress pipeline."
```

---

### Task 5: Fix web-research Skill Context Leak (Medium)

**Problem:** Step 1 of the skill prints full page markdown to stdout (captured into conversation context) before Step 2 saves it to disk. This defeats the token-efficiency pitch.

**Files:**
- Modify: `.claude/skills/web-research/SKILL.md`

- [ ] **Step 1: Rewrite Step 1 to pipe directly to file**

Replace the current Step 1 content (lines 29-51 approximately) with a combined fetch-and-save step. The new Step 1 should:
1. Fetch via browser39 batch mode
2. Extract markdown from JSONL and write directly to the target file
3. Never print the full content to stdout

New Step 1 + Step 2 combined:

````markdown
### Step 1: Fetch and Save

Determine the target file path from the URL. The directory structure mirrors the URL:

```
docs/web/
  docs.rs/axum/latest.md
  developer.mozilla.org/Web/API/Fetch_API.md
```

Fetch the page and save directly to the target file (never print full content to stdout):

```bash
# Fetch
echo '{"id":"1","action":"fetch","v":1,"seq":1,"url":"THE_URL","options":{"selector":"article","strip_nav":true,"include_links":true}}' > /tmp/b39-cmd.jsonl
browser39 batch /tmp/b39-cmd.jsonl --output /tmp/b39-out.jsonl

# Extract markdown directly to file (no stdout)
mkdir -p docs/web/TARGET_DOMAIN/TARGET_PATH_DIR
python3 -c "
import sys,json
with open('/tmp/b39-out.jsonl') as f:
    for line in f:
        d=json.loads(line)
        md = d.get('markdown','')
        if md:
            with open('docs/web/TARGET_DOMAIN/TARGET_PATH.md','w') as out:
                out.write(md)
            print(f'Saved {len(md)} chars to docs/web/TARGET_DOMAIN/TARGET_PATH.md')
"
```

If the page content is empty or too short, retry without the `selector` option, or try `"main"`, `".content"`, or `"body"`.

The filewatcher automatically indexes the file within 1-2 seconds. If `get_symbols` returns no results, wait a moment and retry, or fall back to `Read` for the specific section you need.
````

Remove the old separate Step 2 ("Save to docs/web/") since it's now combined into Step 1. Renumber subsequent steps (old Step 3 becomes Step 2, etc.).

- [ ] **Step 2: Update Step numbering**

After combining Steps 1+2:
- New Step 1: Fetch and Save (combined)
- New Step 2: Explore the Content (was Step 3)
- New Step 3: Read Selectively (was Step 4)
- New Step 4: Follow Links (was Step 5)
- New Step 5: Clean Up (was Step 6)

- [ ] **Step 3: Verify the skill reads correctly**

Read through the modified skill file to confirm no full-content print remains.

- [ ] **Step 4: Commit**

```bash
git add .claude/skills/web-research/SKILL.md
git commit -m "fix(web-research): pipe fetched content directly to file, skip stdout

Combined the fetch and save steps so browser39 output goes directly to
docs/web/ without printing full page markdown into conversation context.
This fixes the token-efficiency bypass where the entire page entered the
context window before being saved."
```

---

### Task 6: Document edit_symbol Line-Based Limitation (Medium)

**Problem:** The tool description doesn't mention that edits are line-granularity, which can surprise agents working with same-line symbols.

**Files:**
- Modify: `src/handler.rs` (tool description at line ~1171)

- [ ] **Step 1: Update the tool description**

Change the `edit_symbol` tool description from:

```
"Edit a symbol by name without reading the file. Operations: replace (swap entire definition), insert_after, insert_before. The symbol is looked up from Julie's index. Combine with deep_dive or get_symbols for zero-read editing workflows. Always dry_run=true first to preview, then dry_run=false to apply."
```

To:

```
"Edit a symbol by name without reading the file. Operations: replace (swap entire definition), insert_after, insert_before. The symbol is resolved from Julie's index by its line range. Edits operate at line granularity (not byte/column), so same-line symbols or tightly formatted code may need manual adjustment. Combine with deep_dive or get_symbols for zero-read editing workflows. Always dry_run=true first to preview, then dry_run=false to apply."
```

- [ ] **Step 2: Commit**

```bash
git add src/handler.rs
git commit -m "docs(edit_symbol): clarify line-granularity limitation in tool description"
```

---

### Task 7: Bracket Balance -- Downgrade to Warning (Low/Medium)

**Problem:** `check_bracket_balance` counts raw characters including those in strings and comments, causing false rejects on valid edits.

**Files:**
- Modify: `src/tools/editing/validation.rs` (`check_bracket_balance` return type/behavior)
- Modify: `src/tools/editing/edit_symbol.rs` (call site)
- Modify: `src/tools/editing/edit_file.rs` (call site)
- Test: `src/tests/tools/editing/edit_symbol_tests.rs`

**Approach:** Change `check_bracket_balance` from returning `Err` (hard reject) to returning `Ok(Option<String>)` where `Some(warning)` is a non-blocking warning appended to the tool output. The agent sees the warning but the edit proceeds. This eliminates false rejects while still flagging suspicious balance changes.

- [ ] **Step 1: Write the failing test**

Add to the edit tests:

```rust
#[test]
fn test_bracket_in_string_does_not_reject() {
    // A valid edit that adds a bracket inside a string literal should not be rejected.
    let before = "fn foo() {\n    println!(\"hello\");\n}\n";
    let after = "fn foo() {\n    println!(\"hello {\");\n}\n";

    // Currently this returns Err because raw bracket count changed.
    // After the fix, it should return Ok with a warning.
    let result = check_bracket_balance(before, after);
    // With current code: Err (false reject). After fix: Ok(Some(warning)).
    assert!(result.is_ok(), "Bracket in string should not hard-reject the edit");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_bracket_in_string_does_not_reject 2>&1 | tail -10`
Expected: FAIL (currently returns Err)

- [ ] **Step 3: Change `check_bracket_balance` return type**

In `src/tools/editing/validation.rs`, change:

```rust
pub fn check_bracket_balance(before: &str, after: &str) -> Result<()> {
```

To:

```rust
/// Check if an edit changes bracket balance. Returns a warning string if
/// the balance changed (possible syntax issue), or None if balanced.
/// This is advisory, not a hard reject, because the check cannot distinguish
/// brackets in code from brackets in strings/comments.
pub fn check_bracket_balance(before: &str, after: &str) -> Option<String> {
```

Change the body from:

```rust
    if bb != ba || kb != ka || pb != pa {
        let mut issues = Vec::new();
        if bb != ba {
            issues.push(format!("braces {{}} changed by {}", ba - bb));
        }
        if kb != ka {
            issues.push(format!("brackets [] changed by {}", ka - kb));
        }
        if pb != pa {
            issues.push(format!("parens () changed by {}", pa - pb));
        }
        return Err(anyhow::anyhow!(
            "Edit changes bracket balance ({}) -- may create invalid syntax",
            issues.join(", ")
        ));
    }

    Ok(())
```

To:

```rust
    if bb != ba || kb != ka || pb != pa {
        let mut issues = Vec::new();
        if bb != ba {
            issues.push(format!("braces {{}} changed by {}", ba - bb));
        }
        if kb != ka {
            issues.push(format!("brackets [] changed by {}", ka - kb));
        }
        if pb != pa {
            issues.push(format!("parens () changed by {}", pa - pb));
        }
        return Some(format!(
            "Warning: edit changes bracket balance ({}) -- verify this is intentional",
            issues.join(", ")
        ));
    }

    None
```

- [ ] **Step 4: Update call sites in edit_symbol.rs**

In `src/tools/editing/edit_symbol.rs`, change the bracket check from:

```rust
        if should_check_balance(symbol_file) {
            if let Err(e) = check_bracket_balance(&original_content, &modified_content) {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Edit rejected: {}. Review the content and try again.",
                    e
                ))]));
            }
        }
```

To:

```rust
        let balance_warning = if should_check_balance(symbol_file) {
            check_bracket_balance(&original_content, &modified_content)
        } else {
            None
        };
```

Then append the warning to the output. In the dry_run response:

```rust
        if self.dry_run {
            let mut msg = format!("Dry run preview (set dry_run=false to apply):\n\n{}", diff);
            if let Some(ref warning) = balance_warning {
                msg.push_str(&format!("\n\n{}", warning));
            }
            return Ok(CallToolResult::text_content(vec![Content::text(msg)]));
        }
```

And in the apply response:

```rust
        let mut msg = format!("Applied {} on '{}' in {}:\n\n{}", self.operation, self.symbol, symbol_file, diff);
        if let Some(warning) = balance_warning {
            msg.push_str(&format!("\n\n{}", warning));
        }
        Ok(CallToolResult::text_content(vec![Content::text(msg)]))
```

- [ ] **Step 5: Update call site in edit_file.rs**

Apply the same pattern change in `src/tools/editing/edit_file.rs`. Find the `check_bracket_balance` call and change from hard-reject to warning-append. The exact code depends on the current structure of that file, but the pattern is identical.

- [ ] **Step 6: Update the test to match new return type**

```rust
#[test]
fn test_bracket_in_string_does_not_reject() {
    let before = "fn foo() {\n    println!(\"hello\");\n}\n";
    let after = "fn foo() {\n    println!(\"hello {\");\n}\n";

    let result = check_bracket_balance(before, after);
    // Should be a warning, not a rejection
    assert!(result.is_some(), "Should warn about bracket change");
    assert!(result.unwrap().contains("Warning"), "Should be advisory, not an error");
}

#[test]
fn test_balanced_edit_no_warning() {
    let before = "fn foo() {\n    bar();\n}\n";
    let after = "fn foo() {\n    baz();\n}\n";

    let result = check_bracket_balance(before, after);
    assert!(result.is_none(), "Balanced edit should produce no warning");
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test --lib test_bracket_in_string 2>&1 | tail -10`
Run: `cargo test --lib test_balanced_edit_no_warning 2>&1 | tail -10`
Expected: Both PASS

- [ ] **Step 8: Commit**

```bash
git add src/tools/editing/validation.rs src/tools/editing/edit_symbol.rs src/tools/editing/edit_file.rs src/tests/tools/editing/edit_symbol_tests.rs
git commit -m "fix(validation): downgrade bracket balance from hard reject to warning

The bracket counter operates on raw characters without parsing strings
or comments, causing false rejects on valid edits. Changed from Err
(blocking) to an advisory warning appended to tool output."
```

---

### Task 8: End-to-End edit_symbol Integration Tests (Opportunity)

**Problem:** Current tests only cover helper functions (`replace_symbol_body`, `insert_near_symbol`), not the full indexed `call_tool` flow.

**Files:**
- Modify: `src/tests/tools/editing/edit_symbol_tests.rs`

**Approach:** Add integration tests that index a temp workspace, then exercise `edit_symbol` through the handler. This tests the full path: index lookup, freshness check, line resolution, edit application.

- [ ] **Step 1: Add integration test imports and helper**

Add to `src/tests/tools/editing/edit_symbol_tests.rs`:

```rust
// Integration tests for the full edit_symbol flow (index -> resolve -> apply)
#[cfg(test)]
mod integration {
    use anyhow::Result;
    use std::fs;
    use tempfile::TempDir;

    use crate::handler::JulieServerHandler;
    use crate::tools::workspace::ManageWorkspaceTool;

    async fn setup_indexed_workspace(content: &str) -> Result<(TempDir, JulieServerHandler, String)> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path();
        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;
        let file_path = src_dir.join("test.rs");
        fs::write(&file_path, content)?;

        let handler = JulieServerHandler::new(workspace_path.to_path_buf()).await?;
        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            path: None,
            force: false,
            deregister: false,
        };
        index_tool.call_tool(&handler).await?;

        Ok((temp_dir, handler, "src/test.rs".to_string()))
    }

    #[tokio::test]
    async fn test_edit_symbol_replace_via_index() -> Result<()> {
        let source = "fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";
        let (_temp, handler, _file) = setup_indexed_workspace(source).await?;

        let tool = crate::tools::editing::edit_symbol::EditSymbolTool {
            symbol: "hello".to_string(),
            operation: "replace".to_string(),
            content: "fn hello() {\n    println!(\"updated\");\n}".to_string(),
            file_path: None,
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = result.content.first().and_then(|c| c.as_text()).unwrap_or_default();
        assert!(text.contains("Applied replace"), "Should apply successfully: {}", text);

        // Verify the file was actually changed
        let content = fs::read_to_string(_temp.path().join("src/test.rs"))?;
        assert!(content.contains("println!(\"updated\")"), "File should contain updated content");
        assert!(content.contains("fn world()"), "Other functions should be preserved");

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_symbol_rejects_stale_index() -> Result<()> {
        let source = "fn original() {\n    v1();\n}\n";
        let (temp, handler, _file) = setup_indexed_workspace(source).await?;

        // Modify the file after indexing (simulating stale index)
        let file_path = temp.path().join("src/test.rs");
        fs::write(&file_path, "// new comment\nfn original() {\n    v1();\n}\n")?;

        let tool = crate::tools::editing::edit_symbol::EditSymbolTool {
            symbol: "original".to_string(),
            operation: "replace".to_string(),
            content: "fn original() {\n    v2();\n}".to_string(),
            file_path: None,
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = result.content.first().and_then(|c| c.as_text()).unwrap_or_default();
        assert!(text.contains("changed since last indexing"),
            "Should reject stale index: {}", text);

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_symbol_insert_after() -> Result<()> {
        let source = "fn existing() {\n    body();\n}\n";
        let (_temp, handler, _file) = setup_indexed_workspace(source).await?;

        let tool = crate::tools::editing::edit_symbol::EditSymbolTool {
            symbol: "existing".to_string(),
            operation: "insert_after".to_string(),
            content: "\nfn new_function() {\n    added();\n}".to_string(),
            file_path: None,
            dry_run: true, // dry run first
        };

        let result = tool.call_tool(&handler).await?;
        let text = result.content.first().and_then(|c| c.as_text()).unwrap_or_default();
        assert!(text.contains("Dry run preview"), "Should show dry run: {}", text);
        assert!(text.contains("new_function"), "Preview should contain inserted content");

        Ok(())
    }

    #[tokio::test]
    async fn test_edit_symbol_not_found() -> Result<()> {
        let source = "fn real() {}\n";
        let (_temp, handler, _file) = setup_indexed_workspace(source).await?;

        let tool = crate::tools::editing::edit_symbol::EditSymbolTool {
            symbol: "nonexistent".to_string(),
            operation: "replace".to_string(),
            content: "fn fake() {}".to_string(),
            file_path: None,
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = result.content.first().and_then(|c| c.as_text()).unwrap_or_default();
        assert!(text.contains("not found"), "Should report not found: {}", text);

        Ok(())
    }
}
```

- [ ] **Step 2: Run the integration tests**

Run: `cargo test --lib edit_symbol_tests::integration 2>&1 | tail -20`
Expected: All 4 tests PASS (including `test_edit_symbol_rejects_stale_index` which validates Task 1's freshness guard)

- [ ] **Step 3: Commit**

```bash
git add src/tests/tools/editing/edit_symbol_tests.rs
git commit -m "test(edit_symbol): add end-to-end integration tests

Tests the full indexed flow: index a workspace, resolve symbol from DB,
apply edit, verify file contents. Includes stale-index rejection test,
insert_after dry-run test, and not-found error test."
```

---

## Execution Order

Tasks can be parallelized in groups:

**Group 1 (independent, can be parallel):**
- Task 1 (edit_symbol freshness guard)
- Task 2 (session-connect deletions)
- Task 3 (watcher dedup)

**Group 2 (independent, can be parallel):**
- Task 4 (per-workspace embeddings)
- Task 5 (web-research skill)
- Task 6 (tool description)
- Task 7 (bracket balance)

**Group 3 (depends on Task 1 + Task 7):**
- Task 8 (end-to-end tests)

**Final:** Run `cargo xtask test dev` to verify no regressions.
