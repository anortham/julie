# Indexing & Embedding Pipeline Bugfixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 bugs and 2 enhancements in the indexing/embedding pipeline found during code review.

**Architecture:** Surgical fixes across 5 files. No new modules. Bugs 1-4 and Enhancement 5 are fully independent (parallelizable). Enhancement 6 is coupled with Bug 2 (shared watcher provider). All fixes follow TDD: write failing test, implement fix, verify green.

**Tech Stack:** Rust, Tantivy, SQLite, tree-sitter, tokio async

---

## File Map

| Fix | Files Modified | Test File |
|-----|---------------|-----------|
| Bug 1: Force-index ref clears primary embeddings | `src/tools/workspace/commands/index.rs` | `src/tests/tools/workspace/index_embedding_tests.rs` (new) |
| Bug 2: Watcher snapshots None provider | `src/watcher/mod.rs`, `src/workspace/mod.rs` | `src/tests/tools/workspace/index_embedding_tests.rs` |
| Bug 3: vector_count under-reports | `src/tools/workspace/indexing/embeddings.rs` | `src/tests/tools/workspace/index_embedding_tests.rs` |
| Bug 4: matches_filter ignores exclude_tests | `src/search/hybrid.rs` | `src/tests/tools/hybrid_search_tests.rs` |
| Enh 5: No-op index spawns embeddings | `src/tools/workspace/commands/index.rs` | (verified through Bug 1 test + manual) |
| Enh 6: Per-file embeddings miss lang_configs | `src/watcher/mod.rs` | (verified through Bug 2 test) |

---

## Task 1: Bug 4 -- Add exclude_tests to hybrid search matches_filter

**Severity:** Search recall degradation. Semantic results from test files consume RRF merge slots, reducing real results before `filter_test_symbols` runs in the caller.

**Files:**
- Modify: `src/search/hybrid.rs:286-306` (the `matches_filter` function)
- Modify: `src/tests/tools/hybrid_search_tests.rs` (add test to `orchestrator_tests` module)

- [ ] **Step 1: Write failing test**

Add to `src/tests/tools/hybrid_search_tests.rs` inside the `orchestrator_tests` module, after `test_hybrid_search_sidecar_timeout_degrades_to_keyword_results`:

```rust
#[test]
fn test_hybrid_search_exclude_tests_filters_semantic_results() {
    // Setup: index with a test-file symbol and a non-test symbol
    let (mut index, mut db, _tantivy_dir, _db_dir) = setup_index_and_db();

    // Add a non-test symbol
    index.add_document(SymbolDocument {
        id: "prod_fn".to_string(),
        name: "process_data".to_string(),
        signature: "fn process_data()".to_string(),
        doc_comment: String::new(),
        file_path: "src/pipeline.rs".to_string(),
        kind: "function".to_string(),
        language: "rust".to_string(),
        start_line: 10,
        reference_score: 1.0,
    });

    // Add a test symbol (lives in a test path)
    index.add_document(SymbolDocument {
        id: "test_fn".to_string(),
        name: "test_process_data".to_string(),
        signature: "fn test_process_data()".to_string(),
        doc_comment: String::new(),
        file_path: "src/tests/pipeline_tests.rs".to_string(),
        kind: "function".to_string(),
        language: "rust".to_string(),
        start_line: 5,
        reference_score: 1.0,
    });
    index.commit().unwrap();

    // Store embeddings so semantic search has results from test file
    let dim = 4;
    db.store_embeddings(&[
        ("test_fn".to_string(), vec![0.9, 0.1, 0.0, 0.0]),
        ("prod_fn".to_string(), vec![0.8, 0.2, 0.0, 0.0]),
    ]).unwrap();

    // Provider that returns a vector close to both stored embeddings
    struct TestProvider;
    impl EmbeddingProvider for TestProvider {
        fn embed_query(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.85, 0.15, 0.0, 0.0])
        }
        fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.85, 0.15, 0.0, 0.0]).collect())
        }
        fn dimensions(&self) -> usize { 4 }
        fn device_info(&self) -> DeviceInfo {
            DeviceInfo { device: "cpu".into(), model_name: "test".into(), accelerated: false }
        }
    }

    let filter = SearchFilter {
        exclude_tests: true,
        ..Default::default()
    };

    let results = hybrid_search(
        "process data",
        &filter,
        10,
        &index,
        &db,
        Some(&TestProvider),
        None,
    ).unwrap();

    // The test symbol should have been filtered out by matches_filter
    // BEFORE the RRF merge, not after
    for r in &results.results {
        assert!(
            !r.file_path.contains("tests/"),
            "Test file result '{}' in '{}' should have been filtered from semantic candidates",
            r.name, r.file_path
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_hybrid_search_exclude_tests_filters_semantic_results 2>&1 | tail -10`
Expected: FAIL -- the test symbol from `src/tests/pipeline_tests.rs` appears in results because `matches_filter` doesn't check `exclude_tests`.

- [ ] **Step 3: Implement the fix**

In `src/search/hybrid.rs`, modify `matches_filter` to check `exclude_tests`:

```rust
fn matches_filter(result: &SymbolSearchResult, filter: &SearchFilter) -> bool {
    if let Some(language) = &filter.language {
        if result.language != *language {
            return false;
        }
    }

    if let Some(kind) = &filter.kind {
        if result.kind != *kind {
            return false;
        }
    }

    if let Some(file_pattern) = &filter.file_pattern {
        if !matches_glob_pattern(&result.file_path, file_pattern) {
            return false;
        }
    }

    if filter.exclude_tests && crate::search::scoring::is_test_path(&result.file_path) {
        return false;
    }

    true
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_hybrid_search_exclude_tests_filters_semantic_results 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/hybrid.rs src/tests/tools/hybrid_search_tests.rs
git commit -m "fix(search): filter test symbols from semantic candidates before RRF merge

matches_filter() checked language, kind, and file_pattern but not
exclude_tests. Semantic results from test files consumed RRF merge
slots, reducing recall for real results even when exclude_tests=true."
```

---

## Task 2: Bug 3 -- Fix vector_count to use actual DB embedding count

**Severity:** Dashboard cosmetic. `vector_count` in daemon.db drifts from reality after partial re-embeds.

**Files:**
- Modify: `src/tools/workspace/indexing/embeddings.rs:170-180` (the completion handler in `spawn_workspace_embedding`)
- Test: `src/tests/tools/workspace/index_embedding_tests.rs` (new file, shared with Task 1 and Task 5)

- [ ] **Step 1: Write failing test**

Create `src/tests/tools/workspace/index_embedding_tests.rs`:

```rust
//! Tests for indexing and embedding pipeline fixes.

use crate::database::SymbolDatabase;
use tempfile::TempDir;

/// Verify that embedding_count() returns the actual row count from symbol_vectors,
/// not just "symbols embedded this run". This is the ground-truth value that
/// spawn_workspace_embedding should write to daemon.db.
#[test]
fn test_embedding_count_reflects_total_vectors_not_run_count() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Insert some symbols
    db.insert_symbol(&crate::database::Symbol {
        id: "sym_a".into(),
        name: "alpha".into(),
        kind: crate::database::SymbolKind::Function,
        language: "rust".into(),
        file_path: "src/lib.rs".into(),
        start_line: 1,
        end_line: 10,
        ..Default::default()
    }).unwrap();
    db.insert_symbol(&crate::database::Symbol {
        id: "sym_b".into(),
        name: "beta".into(),
        kind: crate::database::SymbolKind::Function,
        language: "rust".into(),
        file_path: "src/lib.rs".into(),
        start_line: 12,
        end_line: 20,
        ..Default::default()
    }).unwrap();

    // Store embeddings for both
    let dim = 4;
    db.store_embeddings(&[
        ("sym_a".to_string(), vec![0.1; dim]),
        ("sym_b".to_string(), vec![0.2; dim]),
    ]).unwrap();

    // embedding_count should reflect total stored, not any pipeline stat
    let count = db.embedding_count().unwrap();
    assert_eq!(count, 2, "embedding_count should return total vectors in DB");

    // Simulate a partial re-embed that only touches sym_a
    // (delete + re-store for one symbol)
    db.delete_embeddings_for_file("src/lib.rs").unwrap();
    db.store_embeddings(&[
        ("sym_a".to_string(), vec![0.3; dim]),
    ]).unwrap();

    // Now only 1 vector exists. A pipeline would report symbols_embedded=1.
    // The correct vector_count to write to daemon.db is 1, not 2.
    let count = db.embedding_count().unwrap();
    assert_eq!(count, 1, "embedding_count should reflect current state after partial re-embed");
}
```

Register the test file: ensure `src/tests/tools/workspace/mod.rs` includes `mod index_embedding_tests;`.

- [ ] **Step 2: Run test to verify it passes (this is a characterization test)**

Run: `cargo test --lib test_embedding_count_reflects_total_vectors_not_run_count 2>&1 | tail -10`
Expected: PASS (this validates the ground-truth function; the actual bug is in the caller).

- [ ] **Step 3: Fix spawn_workspace_embedding to use embedding_count()**

In `src/tools/workspace/indexing/embeddings.rs`, in the `Ok(Ok(stats))` match arm (around line 170-180), replace:

```rust
// OLD:
if let Some(ref db) = daemon_db {
    let _ = db.update_vector_count(&workspace_id, stats.symbols_embedded as i64);
    let _ = db.update_embedding_model(&workspace_id, &model_name);
}
```

With:

```rust
// NEW: Read actual vector count from workspace DB (not pipeline run stats)
if let Some(ref daemon) = daemon_db {
    let actual_count = {
        let db_lock = db_arc.lock().unwrap_or_else(|p| p.into_inner());
        db_lock.embedding_count().unwrap_or(stats.symbols_embedded as i64)
    };
    let _ = daemon.update_vector_count(&workspace_id, actual_count);
    let _ = daemon.update_embedding_model(&workspace_id, &model_name);
}
```

Note: `db_arc` is the `Arc<Mutex<SymbolDatabase>>` for the workspace, still in scope after `spawn_blocking` completes (only `db_clone` was moved into the closure).

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add src/tools/workspace/indexing/embeddings.rs src/tests/tools/workspace/index_embedding_tests.rs src/tests/tools/workspace/mod.rs
git commit -m "fix(embeddings): report actual vector count to daemon.db, not pipeline run stats

spawn_workspace_embedding wrote stats.symbols_embedded (vectors stored
this run) to daemon.db vector_count. Partial re-embeds, no-op runs, and
incremental updates made this drift from reality. Now reads
embedding_count() from the workspace DB after pipeline completion."
```

---

## Task 3: Enhancement 5 -- Skip embedding spawn on no-op index

**Severity:** Performance waste. Repeated `manage_workspace index` calls spawn full embedding pipelines even when zero files changed.

**Files:**
- Modify: `src/tools/workspace/commands/index.rs` (gate embedding spawn on `db_mutated || force`)

- [ ] **Step 1: Identify the change location**

In `handle_index_command`, inside the `Ok(result)` branch after indexing succeeds, the embedding spawn is unconditional:

```rust
// Current (unconditional):
let embed_count =
    crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
        handler, ws_id,
    ).await;
```

The `handle_refresh_command` already has the correct gate pattern:

```rust
let db_mutated = result.files_processed > 0 || result.orphans_cleaned > 0;
let embed_count = if db_mutated || force { /* spawn */ } else { 0 };
```

- [ ] **Step 2: Apply the same gate to handle_index_command**

In `src/tools/workspace/commands/index.rs`, replace the unconditional embedding spawn block (inside `if let Some(ws_id) = indexed_workspace_id`) with:

```rust
if let Some(ws_id) = indexed_workspace_id {
    if skip_embeddings {
        info!(
            "Skipping embeddings in auto-index mode (use explicit `manage_workspace index` to embed)"
        );
    } else {
        // Only run embedding pipeline when the DB actually mutated.
        // Matches the gate in handle_refresh_command.
        let db_mutated =
            result.files_processed > 0 || result.orphans_cleaned > 0;

        if db_mutated || force {
            // Force re-index: clear embeddings so the new pipeline
            // re-embeds everything with the latest enrichment text.
            if force {
                if let Ok(Some(workspace)) = handler.get_workspace().await {
                    if let Some(ref db) = workspace.db {
                        let mut db_lock =
                            db.lock().unwrap_or_else(|p| p.into_inner());
                        match db_lock.clear_all_embeddings() {
                            Ok(()) => {
                                info!(
                                    "🗑️ Cleared all embeddings for force re-embed"
                                )
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to clear embeddings: {e}"
                                )
                            }
                        }
                    }
                }
            }

            let embed_count =
                crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                    handler,
                    ws_id,
                )
                .await;
            if embed_count > 0 {
                message.push_str(&format!(
                    "\nEmbedding {} symbols in background...",
                    embed_count
                ));
            }
        } else {
            debug!("No files changed, skipping embedding pipeline");
        }
    }
}
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 4: Commit**

```bash
git add src/tools/workspace/commands/index.rs
git commit -m "perf(index): skip embedding spawn when no files changed

Repeated manage_workspace index calls spawned the full embedding
pipeline even with zero changed files. Now gates on
files_processed > 0 || orphans_cleaned > 0 || force, matching
the existing gate in handle_refresh_command."
```

---

## Task 4: Bug 1 -- Route force-clear embeddings to correct workspace DB

**Severity:** Data loss. Force-indexing a reference workspace clears the primary workspace's embeddings because `handler.get_workspace()` returns the primary, not the reference.

**Files:**
- Modify: `src/tools/workspace/commands/index.rs` (the `if force { clear_all_embeddings }` block)

**Note:** This task modifies the same block as Task 3. If running in parallel, coordinate or rebase. The `if force` block from Task 3 needs the reference workspace routing added here.

- [ ] **Step 1: Write failing test**

Add to `src/tests/tools/workspace/index_embedding_tests.rs`:

```rust
/// Verify that clear_all_embeddings targets the correct database.
/// This is a unit-level test for the routing logic, not a full integration test.
/// The bug: handle_index_command called handler.get_workspace().db.clear_all_embeddings()
/// which always targets the primary workspace, even when force-indexing a reference.
#[test]
fn test_clear_embeddings_on_separate_db_does_not_affect_other() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    let primary_path = dir1.path().join("primary.db");
    let reference_path = dir2.path().join("reference.db");

    let mut primary_db = SymbolDatabase::new(&primary_path).unwrap();
    let mut reference_db = SymbolDatabase::new(&reference_path).unwrap();

    // Both have symbols and embeddings
    for (db, prefix) in [(&mut primary_db, "pri"), (&mut reference_db, "ref")] {
        db.insert_symbol(&crate::database::Symbol {
            id: format!("{prefix}_sym"),
            name: format!("{prefix}_func"),
            kind: crate::database::SymbolKind::Function,
            language: "rust".into(),
            file_path: "src/lib.rs".into(),
            start_line: 1,
            end_line: 10,
            ..Default::default()
        }).unwrap();
        db.store_embeddings(&[
            (format!("{prefix}_sym"), vec![0.1, 0.2, 0.3, 0.4]),
        ]).unwrap();
    }

    assert_eq!(primary_db.embedding_count().unwrap(), 1);
    assert_eq!(reference_db.embedding_count().unwrap(), 1);

    // Clear embeddings on the REFERENCE db only
    reference_db.clear_all_embeddings().unwrap();

    // Primary should be untouched
    assert_eq!(
        primary_db.embedding_count().unwrap(), 1,
        "Primary embeddings should not be affected by clearing reference DB"
    );
    assert_eq!(reference_db.embedding_count().unwrap(), 0);
}
```

- [ ] **Step 2: Run test to verify it passes (characterization test)**

Run: `cargo test --lib test_clear_embeddings_on_separate_db_does_not_affect_other 2>&1 | tail -10`
Expected: PASS (this validates that separate DBs are independent; the bug is in which DB the code opens).

- [ ] **Step 3: Fix the routing in handle_index_command**

In `src/tools/workspace/commands/index.rs`, replace the `if force` block inside the embedding section (after Task 3's changes) with reference-aware routing:

```rust
if db_mutated || force {
    if force {
        if is_reference_workspace {
            // Open the REFERENCE workspace's DB to clear its embeddings.
            // handler.get_workspace() returns the PRIMARY, which we must NOT touch.
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                let ref_db_path = workspace.workspace_db_path(&ws_id);
                if ref_db_path.exists() {
                    let path = ref_db_path;
                    let clear_result = tokio::task::spawn_blocking(move || {
                        let mut ref_db = crate::database::SymbolDatabase::new(path)?;
                        ref_db.clear_all_embeddings()
                    }).await;
                    match clear_result {
                        Ok(Ok(())) => info!("🗑️ Cleared reference workspace embeddings for force re-embed"),
                        Ok(Err(e)) => tracing::warn!("Failed to clear reference embeddings: {e}"),
                        Err(e) => tracing::warn!("Reference embedding clear task panicked: {e}"),
                    }
                }
            }
        } else {
            // Primary workspace: clear from handler's workspace DB
            if let Ok(Some(workspace)) = handler.get_workspace().await {
                if let Some(ref db) = workspace.db {
                    let mut db_lock = db.lock().unwrap_or_else(|p| p.into_inner());
                    match db_lock.clear_all_embeddings() {
                        Ok(()) => info!("🗑️ Cleared all embeddings for force re-embed"),
                        Err(e) => tracing::warn!("Failed to clear embeddings: {e}"),
                    }
                }
            }
        }
    }

    let embed_count =
        crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
            handler, ws_id,
        ).await;
    if embed_count > 0 {
        message.push_str(&format!(
            "\nEmbedding {} symbols in background...",
            embed_count
        ));
    }
} else {
    debug!("No files changed, skipping embedding pipeline");
}
```

- [ ] **Step 4: Build to verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 5: Commit**

```bash
git add src/tools/workspace/commands/index.rs src/tests/tools/workspace/index_embedding_tests.rs
git commit -m "fix(index): route force-clear embeddings to reference workspace DB

handle_index_command called handler.get_workspace().db.clear_all_embeddings()
which always targets the primary workspace. When force-indexing a reference
workspace, this wiped the primary's embeddings instead of the reference's.
Now opens the reference workspace's DB directly via workspace_db_path()."
```

---

## Task 5: Bug 2 + Enhancement 6 -- Shared watcher embedding provider + lang_configs

**Severity:** Moderate. Incremental file changes never update semantic embeddings because the watcher snapshots `None` at construction time. Enhancement 6 (passing lang_configs) is blocked on this fix.

**Files:**
- Modify: `src/watcher/mod.rs` (shared provider type, update method, lang_configs field)
- Modify: `src/workspace/mod.rs` (propagate provider to watcher after lazy init)

### Part A: Shared embedding provider (Bug 2)

- [ ] **Step 1: Write failing test**

Add to `src/tests/tools/workspace/index_embedding_tests.rs`:

```rust
use std::sync::{Arc, RwLock};
use crate::embeddings::EmbeddingProvider;

/// Verify that a shared provider container (the fix pattern) propagates
/// updates to readers, unlike a cloned Option<Arc<...>> which snapshots.
#[test]
fn test_shared_provider_container_propagates_updates() {
    type SharedProvider = Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>;

    let shared: SharedProvider = Arc::new(RwLock::new(None));

    // Simulate watcher cloning the Arc at construction time
    let watcher_ref = shared.clone();

    // At this point, provider is None
    assert!(watcher_ref.read().unwrap().is_none());

    // Simulate lazy initialization updating the shared container
    struct DummyProvider;
    impl EmbeddingProvider for DummyProvider {
        fn embed_query(&self, _: &str) -> anyhow::Result<Vec<f32>> { Ok(vec![]) }
        fn embed_batch(&self, _: &[String]) -> anyhow::Result<Vec<Vec<f32>>> { Ok(vec![]) }
        fn dimensions(&self) -> usize { 4 }
        fn device_info(&self) -> crate::embeddings::DeviceInfo {
            crate::embeddings::DeviceInfo {
                device: "cpu".into(), model_name: "test".into(), accelerated: false,
            }
        }
    }

    *shared.write().unwrap() = Some(Arc::new(DummyProvider));

    // Watcher's ref should now see the provider
    assert!(
        watcher_ref.read().unwrap().is_some(),
        "Watcher should see the provider after lazy init updates the shared container"
    );
}
```

- [ ] **Step 2: Run test to verify it passes (validates the design pattern)**

Run: `cargo test --lib test_shared_provider_container_propagates_updates 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 3: Change IncrementalIndexer to use shared provider**

In `src/watcher/mod.rs`:

**3a. Add type alias** near the top of the file (after imports):

```rust
/// Shared embedding provider that can be updated after construction.
/// The workspace writes to this after lazy initialization; the watcher reads from it.
pub(crate) type SharedEmbeddingProvider = Arc<std::sync::RwLock<Option<Arc<dyn crate::embeddings::EmbeddingProvider>>>>;
```

**3b. Change the field** in `IncrementalIndexer` struct (line ~43):

Replace:
```rust
/// Embedding provider for incremental semantic updates (None if unavailable)
embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
```
With:
```rust
/// Shared embedding provider, updated by workspace after lazy initialization.
embedding_provider: SharedEmbeddingProvider,
```

**3c. Update `new()`** constructor. Change the `embedding_provider` parameter and wrapping:

Replace:
```rust
pub fn new(
    workspace_root: PathBuf,
    db: Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: Arc<ExtractorManager>,
    search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    embedding_provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
) -> Result<Self> {
```
With:
```rust
pub fn new(
    workspace_root: PathBuf,
    db: Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: Arc<ExtractorManager>,
    search_index: Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    embedding_provider: SharedEmbeddingProvider,
) -> Result<Self> {
```

The struct initializer already uses `embedding_provider` directly, so the field assignment is unchanged.

**3d. Add `update_embedding_provider` method** to `impl IncrementalIndexer`:

```rust
/// Update the shared embedding provider after lazy initialization.
/// Called by the workspace when `initialize_embedding_provider()` runs.
pub fn update_embedding_provider(&self, provider: Option<Arc<dyn crate::embeddings::EmbeddingProvider>>) {
    let mut guard = self.embedding_provider.write().unwrap_or_else(|p| p.into_inner());
    *guard = provider;
}
```

**3e. Update `start_watching()`** -- the background queue processor task.

The line `let embedding_provider = self.embedding_provider.clone();` already clones, but now it clones the `Arc<RwLock<...>>` (shared handle) instead of `Option<Arc<...>>` (snapshot). Good.

In the spawned queue processor task, where it calls `dispatch_file_event`, read-lock the provider before each dispatch. Replace:

```rust
dispatch_file_event(
    event,
    &db,
    &extractor_manager,
    &search_index,
    &embedding_provider,
    &workspace_root,
    Some(clear_dedup_on_delete),
)
.await;
```

With:

```rust
let provider_snapshot = embedding_provider
    .read()
    .unwrap_or_else(|p| p.into_inner())
    .clone();
dispatch_file_event(
    event,
    &db,
    &extractor_manager,
    &search_index,
    &provider_snapshot,
    &workspace_root,
    Some(clear_dedup_on_delete),
)
.await;
```

**3f. Update `process_pending_changes()`** -- same pattern. Replace:

```rust
dispatch_file_event(
    event,
    &self.db,
    &self.extractor_manager,
    &self.search_index,
    &self.embedding_provider,
    &self.workspace_root,
    None::<fn(&std::path::Path)>,
)
.await;
```

With:

```rust
let provider_snapshot = self.embedding_provider
    .read()
    .unwrap_or_else(|p| p.into_inner())
    .clone();
dispatch_file_event(
    event,
    &self.db,
    &self.extractor_manager,
    &self.search_index,
    &provider_snapshot,
    &self.workspace_root,
    None::<fn(&std::path::Path)>,
)
.await;
```

- [ ] **Step 4: Update workspace to create shared container and propagate**

In `src/workspace/mod.rs`:

**4a. Update `initialize_file_watcher()`** to wrap the provider. Replace:

```rust
let file_watcher = IncrementalIndexer::new(
    self.root.clone(),
    self.db.as_ref().unwrap().clone(),
    extractor_manager,
    self.search_index.clone(),
    self.embedding_provider.clone(),
)?;
```

With:

```rust
let shared_provider = Arc::new(std::sync::RwLock::new(self.embedding_provider.clone()));
let file_watcher = IncrementalIndexer::new(
    self.root.clone(),
    self.db.as_ref().unwrap().clone(),
    extractor_manager,
    self.search_index.clone(),
    shared_provider,
)?;
```

**4b. Update `initialize_embedding_provider()`** to propagate to watcher. Replace:

```rust
pub fn initialize_embedding_provider(&mut self) {
    let (provider, runtime_status) = crate::embeddings::create_embedding_provider();
    self.embedding_provider = provider;
    self.embedding_runtime_status = runtime_status;
}
```

With:

```rust
pub fn initialize_embedding_provider(&mut self) {
    let (provider, runtime_status) = crate::embeddings::create_embedding_provider();
    self.embedding_provider = provider.clone();
    self.embedding_runtime_status = runtime_status;
    // Propagate to file watcher so incremental updates use the new provider
    if let Some(ref watcher) = self.watcher {
        watcher.update_embedding_provider(provider);
    }
}
```

- [ ] **Step 5: Build to verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 6: Commit Bug 2 fix**

```bash
git add src/watcher/mod.rs src/workspace/mod.rs src/tests/tools/workspace/index_embedding_tests.rs
git commit -m "fix(watcher): propagate lazily-initialized embedding provider to file watcher

IncrementalIndexer snapshotted embedding_provider at construction time
(always None, since embeddings are lazily initialized). The background
queue processor task cloned this snapshot, so live file edits never got
semantic embeddings updated.

Fix: wrap embedding_provider in Arc<RwLock<...>> shared between
workspace and watcher. initialize_embedding_provider() now propagates
to the watcher via update_embedding_provider()."
```

### Part B: Pass lang_configs to per-file embeddings (Enhancement 6)

- [ ] **Step 7: Add lang_configs to IncrementalIndexer and dispatch_file_event**

In `src/watcher/mod.rs`:

**7a. Add field** to `IncrementalIndexer` struct:

```rust
/// Language configs for embedding enrichment (loaded once at construction)
lang_configs: Arc<crate::search::language_config::LanguageConfigs>,
```

**7b. Initialize in `new()`** -- add after existing field initialization:

```rust
lang_configs: Arc::new(crate::search::language_config::LanguageConfigs::load_embedded()),
```

**7c. Add parameter to `dispatch_file_event`** signature:

```rust
async fn dispatch_file_event<F>(
    event: FileChangeEvent,
    db: &Arc<StdMutex<SymbolDatabase>>,
    extractor_manager: &Arc<ExtractorManager>,
    search_index: &Option<Arc<StdMutex<crate::search::SearchIndex>>>,
    embedding_provider: &Option<Arc<dyn crate::embeddings::EmbeddingProvider>>,
    workspace_root: &std::path::Path,
    lang_configs: &crate::search::language_config::LanguageConfigs,
    on_atomic_delete: Option<F>,
) where
    F: FnOnce(&std::path::Path),
```

**7d. Pass lang_configs in the two `reembed_symbols_for_file` calls** inside `dispatch_file_event`:

Replace both instances of:
```rust
reembed_symbols_for_file(db, provider.as_ref(), &rel, None)
```
With:
```rust
reembed_symbols_for_file(db, provider.as_ref(), &rel, Some(lang_configs))
```

**7e. Update callers** of `dispatch_file_event`:

In `start_watching()`, clone lang_configs for the spawned task:
```rust
let lang_configs = self.lang_configs.clone();
```

And pass it in the dispatch call:
```rust
dispatch_file_event(
    event,
    &db,
    &extractor_manager,
    &search_index,
    &provider_snapshot,
    &workspace_root,
    &lang_configs,
    Some(clear_dedup_on_delete),
)
.await;
```

In `process_pending_changes()`:
```rust
dispatch_file_event(
    event,
    &self.db,
    &self.extractor_manager,
    &self.search_index,
    &provider_snapshot,
    &self.workspace_root,
    &self.lang_configs,
    None::<fn(&std::path::Path)>,
)
.await;
```

- [ ] **Step 8: Build to verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles without errors.

- [ ] **Step 9: Commit Enhancement 6**

```bash
git add src/watcher/mod.rs
git commit -m "fix(watcher): pass lang_configs to per-file embedding updates

dispatch_file_event called reembed_symbols_for_file with lang_configs=None,
causing incremental re-embeds to miss language-specific extra kinds in the
embedding text. Now loads LanguageConfigs once at IncrementalIndexer
construction and passes them through to the per-file path."
```

---

## Final Step: Run xtask dev

- [ ] **Run the default test tier**

```bash
cargo xtask test dev
```

Expected: All green. If any failures, investigate (all tiers were green before these changes).

- [ ] **Tag the TODO items as done**

Update `TODO.md`: mark the 4 bug items and 2 enhancement items with `[x]`.
