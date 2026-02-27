# Embeddings Hardening and Backend Foundation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix correctness and reliability gaps in semantic/embedding search for both primary and reference workspaces, then lay the minimum architecture needed to add Candle cleanly.

**Architecture:** First, close correctness bugs that can return wrong results or stale data (filtering, indexing/embedding lifecycle, orphan cleanup safety, incremental vector hygiene). Second, add defensive checks around vector serialization/deserialization. Third, introduce backend selection seams (factory + config + feature flags) without changing current runtime behavior by default.

**Tech Stack:** Rust, Tantivy, sqlite-vec, fastembed/ORT, tokio, rusqlite, existing Julie workspace registry + indexing pipeline

---

### Task 1: Enforce Search Filters on Semantic Hybrid Results

**Files:**
- Modify: `src/search/hybrid.rs`
- Test: `src/tests/tools/hybrid_search_tests.rs`

**Step 1: Write the failing test**

Add an orchestrator test that seeds Tantivy + DB with mixed-language/mixed-path symbols, runs `hybrid_search` with `SearchFilter { language: Some("rust"), file_pattern: Some("src/**/*.rs") }`, and asserts every returned result matches both constraints.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib hybrid_search_tests::orchestrator_tests::test_hybrid_search_applies_filter_to_semantic_results 2>&1 | tail -20`

Expected: FAIL because semantic KNN results currently bypass filter checks.

**Step 3: Write minimal implementation**

Apply filter validation to semantic results before RRF merge (or immediately after KNN-to-symbol conversion), reusing existing file-pattern matching + language checks.

**Step 4: Run test to verify it passes**

Run the same command; expected PASS.

**Step 5: Commit**

`git add src/search/hybrid.rs src/tests/tools/hybrid_search_tests.rs && git commit -m "fix: apply SearchFilter to semantic hybrid candidates"`

---

### Task 2: Ensure Primary `index`/`refresh` Also Trigger Embedding Pipeline

**Files:**
- Modify: `src/tools/workspace/commands/index.rs`
- Modify: `src/tools/workspace/commands/registry/refresh_stats.rs`
- Test: `src/tests/integration/reference_workspace.rs`

**Step 1: Write the failing test**

Add an integration test that performs primary workspace indexing/refresh and asserts embedding pipeline is scheduled when an embedding provider exists (same expectation currently used for reference workspaces).

**Step 2: Run test to verify it fails**

Run: `cargo test --lib reference_workspace::test_primary_index_triggers_embedding_pipeline 2>&1 | tail -20`

Expected: FAIL because only reference paths trigger embedding today.

**Step 3: Write minimal implementation**

Trigger embedding scheduling for successful primary `index` and `refresh` paths, keeping behavior non-fatal and backgrounded.

**Step 4: Run test to verify it passes**

Run the same command; expected PASS.

**Step 5: Commit**

`git add src/tools/workspace/commands/index.rs src/tools/workspace/commands/registry/refresh_stats.rs src/tests/integration/reference_workspace.rs && git commit -m "fix: run embeddings after primary workspace index and refresh"`

---

### Task 3: Protect Primary Index from Orphan Cleanup

**Files:**
- Modify: `src/workspace/registry_service.rs`
- Test: `src/tests/tools/workspace/registry_service.rs`

**Step 1: Write the failing test**

Add a test for `detect_orphaned_indexes` proving the primary workspace index directory is never reported as orphan when present in registry.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib registry_service::test_detect_orphaned_indexes_excludes_primary 2>&1 | tail -20`

Expected: FAIL under current logic.

**Step 3: Write minimal implementation**

Update orphan detection to skip both registered primary workspace ID and all reference workspace IDs.

**Step 4: Run test to verify it passes**

Run the same command; expected PASS.

**Step 5: Commit**

`git add src/workspace/registry_service.rs src/tests/tools/workspace/registry_service.rs && git commit -m "fix: exclude primary workspace index from orphan detection"`

---

### Task 4: Prevent Stale Embeddings on File Modify/Create

**Files:**
- Modify: `src/watcher/mod.rs`
- Test: `src/tests/integration/embedding_incremental.rs`

**Step 1: Write the failing test**

Add/extend incremental embedding test to simulate symbol replacement in a modified file and assert old symbol vectors are removed before re-embedding.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_incremental::tests::test_modified_file_replaces_embeddings_without_stale_rows 2>&1 | tail -20`

Expected: FAIL with stale/extra embedding count.

**Step 3: Write minimal implementation**

For created/modified watcher path, delete existing file embeddings before `embed_symbols_for_file` (same ordering principle already used for delete/rename).

**Step 4: Run test to verify it passes**

Run the same command; expected PASS.

**Step 5: Commit**

`git add src/watcher/mod.rs src/tests/integration/embedding_incremental.rs && git commit -m "fix: clear old file embeddings before incremental re-embed"`

---

### Task 5: Add Vector Safety Checks (Write/Read Paths)

**Files:**
- Modify: `src/embeddings/pipeline.rs`
- Modify: `src/database/vectors.rs`
- Test: `src/tests/integration/embedding_incremental.rs`
- Test: `src/tests/core/vector_storage.rs`

**Step 1: Write the failing tests**

1) Add test ensuring `embed_symbols_for_file` errors on embedding length mismatch.
2) Add test ensuring `get_embedding` rejects malformed blob length (non-multiple-of-4 bytes).

**Step 2: Run tests to verify they fail**

Run:
- `cargo test --lib embedding_incremental::tests::test_embed_symbols_for_file_errors_on_vector_count_mismatch 2>&1 | tail -20`
- `cargo test --lib vector_storage::test_get_embedding_rejects_malformed_blob 2>&1 | tail -20`

Expected: FAIL.

**Step 3: Write minimal implementation**

- In `embed_symbols_for_file`, assert `vectors.len() == prepared.len()` before zip/store.
- In `get_embedding`, validate byte length and return explicit error when malformed.

**Step 4: Run tests to verify they pass**

Run the same commands; expected PASS.

**Step 5: Commit**

`git add src/embeddings/pipeline.rs src/database/vectors.rs src/tests/integration/embedding_incremental.rs src/tests/core/vector_storage.rs && git commit -m "fix: harden embedding vector read/write validation"`

---

### Task 6: Add Embedding Provider Factory + Config Seam (No Behavior Change)

**Files:**
- Create: `src/embeddings/factory.rs`
- Modify: `src/embeddings/mod.rs`
- Modify: `src/workspace/mod.rs`
- Modify: `src/tests/core/embedding_provider.rs`

**Step 1: Write the failing test**

Add unit tests around provider selection (default ORT, unknown provider rejected, provider unavailable degrades to `None`).

**Step 2: Run test to verify it fails**

Run: `cargo test --lib embedding_provider::test_provider_factory_selection 2>&1 | tail -20`

Expected: FAIL (factory does not exist yet).

**Step 3: Write minimal implementation**

Introduce `EmbeddingProviderFactory` and `EmbeddingConfig` (provider name + cache dir). Wire workspace initialization through factory while keeping default ORT behavior unchanged.

**Step 4: Run test to verify it passes**

Run the same command; expected PASS.

**Step 5: Commit**

`git add src/embeddings/factory.rs src/embeddings/mod.rs src/workspace/mod.rs src/tests/core/embedding_provider.rs && git commit -m "refactor: introduce embedding provider factory for backend pluggability"`

---

### Task 7: Add Cargo Feature Gates for Backend Isolation

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/embeddings/mod.rs`
- Test: `src/tests/core/embedding_deps.rs`

**Step 1: Write the failing test/build check**

Add/update compile-time checks so embeddings build cleanly with explicit feature sets.

**Step 2: Run checks to verify failure first**

Run:
- `cargo test --lib embedding_deps 2>&1 | tail -20`
- `cargo check --no-default-features 2>&1 | tail -20`

Expected: at least one check fails before gating is added.

**Step 3: Write minimal implementation**

Add `[features]` with default ORT path and conditional module exports for backend providers.

**Step 4: Run checks to verify pass**

Run the same checks; expected PASS.

**Step 5: Commit**

`git add Cargo.toml src/embeddings/mod.rs src/tests/core/embedding_deps.rs && git commit -m "build: add feature-gated embedding backend wiring"`

---

### Task 8: End-to-End Verification (Primary + Reference)

**Files:**
- Modify (if needed): `src/tests/tools/search/primary_workspace_bug.rs`
- Modify (if needed): `src/tests/integration/workspace_isolation_smoke.rs`

**Step 1: Run targeted integration checks**

Run:
- `cargo test --lib hybrid_search_tests 2>&1 | tail -20`
- `cargo test --lib embedding_incremental 2>&1 | tail -20`
- `cargo test --lib reference_workspace 2>&1 | tail -20`
- `cargo test --lib registry_service 2>&1 | tail -20`

**Step 2: Run fast-tier regression suite**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -20`

Expected: all pass.

**Step 3: Commit final verification adjustments**

`git add src/tests/tools/search/primary_workspace_bug.rs src/tests/integration/workspace_isolation_smoke.rs && git commit -m "test: verify semantic embedding behavior across primary and reference workspaces"`
