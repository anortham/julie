# Phase 3: Semantic Similarity in deep_dive — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a "Semantically Similar" section to `deep_dive` at `full` depth, using stored embeddings to find conceptually related symbols within the same workspace.

**Architecture:** Look up a symbol's stored embedding vector from `symbol_vectors`, run KNN search against the same table, convert results to `SimilarEntry` structs in `SymbolContext`, and render them in the formatting layer. Graceful degradation when no embeddings exist.

**Tech Stack:** sqlite-vec (KNN), zerocopy (vector serialization), existing `SymbolDatabase` infrastructure

---

### Task 1: Add `get_embedding` to SymbolDatabase

**Files:**
- Modify: `src/database/vectors.rs` (add method after `knn_search`, ~line 99)
- Test: `src/tests/core/vector_storage.rs`

**Step 1: Write the failing tests**

Add two tests to `src/tests/core/vector_storage.rs`:

```rust
#[test]
fn test_get_embedding_returns_stored_vector() {
    let (_tmp, mut db) = setup_db_with_embeddings();
    // Store a known embedding for symbol "sym-1"
    let embedding = vec![0.1_f32, 0.2, 0.3]; // short for test; real = 384 dims
    db.store_embeddings(&[("sym-1".to_string(), embedding.clone())]).unwrap();

    let result = db.get_embedding("sym-1").unwrap();
    assert!(result.is_some());
    let stored = result.unwrap();
    assert_eq!(stored.len(), 3);
    assert!((stored[0] - 0.1).abs() < 1e-5);
    assert!((stored[1] - 0.2).abs() < 1e-5);
    assert!((stored[2] - 0.3).abs() < 1e-5);
}

#[test]
fn test_get_embedding_returns_none_for_missing() {
    let (_tmp, db) = setup_db_with_embeddings();
    let result = db.get_embedding("nonexistent-symbol").unwrap();
    assert!(result.is_none());
}
```

Note: Check what `setup_db_with_embeddings` looks like in the existing test file — reuse it. If it doesn't exist, create a helper that initializes a DB with the `symbol_vectors` virtual table.

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_get_embedding 2>&1 | tail -10`
Expected: FAIL — `get_embedding` method doesn't exist yet.

**Step 3: Implement `get_embedding`**

Add to `src/database/vectors.rs` after the `knn_search` method (~line 99):

```rust
/// Retrieve a symbol's stored embedding vector.
///
/// Returns `None` if no embedding exists for this symbol_id.
/// The vector is deserialized from the raw bytes stored by sqlite-vec.
pub fn get_embedding(&self, symbol_id: &str) -> Result<Option<Vec<f32>>> {
    let mut stmt = self
        .conn
        .prepare(
            "SELECT embedding FROM symbol_vectors WHERE symbol_id = ?",
        )
        .context("Failed to prepare get_embedding query")?;

    let result = stmt
        .query_row(rusqlite::params![symbol_id], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            Ok(bytes)
        })
        .optional()
        .context("Failed to execute get_embedding query")?;

    match result {
        Some(bytes) => {
            // Convert raw bytes back to Vec<f32>
            // sqlite-vec stores floats as little-endian IEEE 754
            let floats: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();
            Ok(Some(floats))
        }
        None => Ok(None),
    }
}
```

Add `use rusqlite::OptionalExtension;` to the imports at the top of `vectors.rs` if not already present (needed for `.optional()`).

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_get_embedding 2>&1 | tail -10`
Expected: 2 passed, 0 failed.

**Step 5: Commit**

```bash
git add src/database/vectors.rs src/tests/core/vector_storage.rs
git commit -m "feat: add get_embedding to SymbolDatabase for vector retrieval"
```

---

### Task 2: Add `SimilarEntry` and `similar` field to `SymbolContext`

**Files:**
- Modify: `src/tools/deep_dive/data.rs` (add struct + field)
- Modify: `src/tools/deep_dive/data.rs:build_symbol_context` (add similarity lookup)
- Test: `src/tests/tools/deep_dive_tests.rs`

**Step 1: Write the failing tests**

Add a new test module section in `src/tests/tools/deep_dive_tests.rs` inside `data_tests`:

```rust
// === semantic similarity tests ===

#[test]
fn test_similar_symbols_at_full_depth() {
    let (_tmp, mut db) = setup_db();

    // Create two symbols
    let sym1 = make_symbol("sym-auth", "authenticate_user", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None);
    let sym2 = make_symbol("sym-verify", "verify_credentials", SymbolKind::Function, "src/handler.rs", 20, None, None, None, None);
    db.store_symbols(&[sym1, sym2]).unwrap();

    // Store similar embeddings (close in vector space)
    db.store_embeddings(&[
        ("sym-auth".to_string(), vec![1.0_f32; 384]),
        ("sym-verify".to_string(), vec![0.99_f32; 384]),
    ]).unwrap();

    let symbols = find_symbol(&db, "authenticate_user", None).unwrap();
    let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

    assert!(!ctx.similar.is_empty(), "similar should be populated at full depth");
    assert_eq!(ctx.similar[0].symbol.name, "verify_credentials");
    assert!(ctx.similar[0].score > 0.0);
}

#[test]
fn test_similar_symbols_skipped_when_no_embeddings() {
    let (_tmp, mut db) = setup_db();

    let sym = make_symbol("sym-1", "process", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None);
    db.store_symbols(&[sym]).unwrap();
    // No embeddings stored

    let symbols = find_symbol(&db, "process", None).unwrap();
    let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

    assert!(ctx.similar.is_empty(), "similar should be empty when no embeddings exist");
}

#[test]
fn test_similar_symbols_excludes_self() {
    let (_tmp, mut db) = setup_db();

    let sym1 = make_symbol("sym-a", "alpha", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None);
    let sym2 = make_symbol("sym-b", "beta", SymbolKind::Function, "src/handler.rs", 20, None, None, None, None);
    db.store_symbols(&[sym1, sym2]).unwrap();

    db.store_embeddings(&[
        ("sym-a".to_string(), vec![1.0_f32; 384]),
        ("sym-b".to_string(), vec![0.5_f32; 384]),
    ]).unwrap();

    let symbols = find_symbol(&db, "alpha", None).unwrap();
    let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

    // Self should not appear in similar results
    for entry in &ctx.similar {
        assert_ne!(entry.symbol.id, "sym-a", "self should be excluded from similar");
    }
}

#[test]
fn test_similar_symbols_not_at_context_depth() {
    let (_tmp, mut db) = setup_db();

    let sym1 = make_symbol("sym-a", "alpha", SymbolKind::Function, "src/engine.rs", 10, None, None, None, None);
    let sym2 = make_symbol("sym-b", "beta", SymbolKind::Function, "src/handler.rs", 20, None, None, None, None);
    db.store_symbols(&[sym1, sym2]).unwrap();

    db.store_embeddings(&[
        ("sym-a".to_string(), vec![1.0_f32; 384]),
        ("sym-b".to_string(), vec![0.99_f32; 384]),
    ]).unwrap();

    let symbols = find_symbol(&db, "alpha", None).unwrap();

    // Context depth should NOT include similar
    let ctx = build_symbol_context(&db, &symbols[0], "context", 10, 10).unwrap();
    assert!(ctx.similar.is_empty(), "similar should be empty at context depth");

    // Overview depth should NOT include similar
    let ctx = build_symbol_context(&db, &symbols[0], "overview", 10, 10).unwrap();
    assert!(ctx.similar.is_empty(), "similar should be empty at overview depth");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_similar_symbols 2>&1 | tail -10`
Expected: FAIL — `similar` field doesn't exist on `SymbolContext`, `SimilarEntry` doesn't exist.

**Step 3: Implement the data layer changes**

In `src/tools/deep_dive/data.rs`:

**3a. Add `SimilarEntry` struct** (after `RefEntry`):

```rust
/// Entry for a semantically similar symbol
#[derive(Debug)]
pub struct SimilarEntry {
    pub symbol: Symbol,
    /// Similarity score: 0.0..1.0, higher = more similar (1.0 - cosine_distance)
    pub score: f32,
}
```

**3b. Add `similar` field to `SymbolContext`**:

```rust
pub struct SymbolContext {
    // ... existing fields unchanged ...
    /// Semantically similar symbols (populated at "full" depth only)
    pub similar: Vec<SimilarEntry>,
}
```

**3c. Add similarity lookup in `build_symbol_context`** — after the test_refs block, before the final `Ok(SymbolContext { ... })`:

```rust
// === Semantically similar symbols (full depth only) ===
let similar = if depth == "full" {
    build_similar(db, &symbol)?
} else {
    vec![]
};
```

Add `similar` to the `Ok(SymbolContext { ... })` return.

**3d. Add `build_similar` helper function** (private, at the end of the file):

```rust
/// Find semantically similar symbols via KNN on stored embeddings.
/// Returns empty Vec if the symbol has no embedding (graceful degradation).
fn build_similar(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<SimilarEntry>> {
    const SIMILAR_LIMIT: usize = 5;

    // Step 1: Get the symbol's own embedding
    let embedding = match db.get_embedding(&symbol.id)? {
        Some(vec) => vec,
        None => return Ok(vec![]),
    };

    // Step 2: KNN search (fetch limit+1 to account for self)
    let knn_results = db.knn_search(&embedding, SIMILAR_LIMIT + 1)?;

    // Step 3: Filter out self and convert to SimilarEntry
    let symbol_ids: Vec<String> = knn_results
        .iter()
        .filter(|(id, _)| id != &symbol.id)
        .take(SIMILAR_LIMIT)
        .map(|(id, _)| id.clone())
        .collect();

    let distances: std::collections::HashMap<String, f64> = knn_results
        .into_iter()
        .collect();

    if symbol_ids.is_empty() {
        return Ok(vec![]);
    }

    // Step 4: Fetch full symbols
    let symbols = db.get_symbols_by_ids(&symbol_ids)?;

    // Step 5: Build entries in KNN order (ascending distance = descending similarity)
    let mut entries: Vec<SimilarEntry> = Vec::new();
    for id in &symbol_ids {
        if let Some(sym) = symbols.iter().find(|s| &s.id == id) {
            let distance = distances.get(id).copied().unwrap_or(1.0);
            entries.push(SimilarEntry {
                symbol: sym.clone(),
                score: (1.0 - distance) as f32,
            });
        }
    }

    Ok(entries)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_similar_symbols 2>&1 | tail -10`
Expected: 4 passed, 0 failed.

**Step 5: Run fast tier to check for regressions**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All pass. Some existing tests that construct `SymbolContext` directly may need `similar: vec![]` added — fix any compilation errors.

**Step 6: Commit**

```bash
git add src/tools/deep_dive/data.rs src/tests/tools/deep_dive_tests.rs
git commit -m "feat: add semantic similarity to SymbolContext at full depth"
```

---

### Task 3: Add formatting for similar symbols

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs` (add `format_similar_section`, call from `format_symbol_context`)
- Test: `src/tests/tools/deep_dive_tests.rs` (formatting_tests module)

**Step 1: Write the failing test**

Add to the `formatting_tests` module in `src/tests/tools/deep_dive_tests.rs`:

```rust
#[test]
fn test_format_similar_section() {
    let ctx = SymbolContext {
        symbol: make_symbol(SymbolKind::Function, "search_symbols", Some("pub fn search_symbols(query: &str) -> Vec<Symbol>"), "src/search.rs", 10, None),
        incoming: vec![],
        incoming_total: 0,
        outgoing: vec![],
        outgoing_total: 0,
        children: vec![],
        implementations: vec![],
        test_refs: vec![],
        similar: vec![
            SimilarEntry {
                symbol: Symbol {
                    id: "s1".to_string(),
                    name: "find_symbols".to_string(),
                    kind: SymbolKind::Function,
                    language: "rust".to_string(),
                    file_path: "src/query.rs".to_string(),
                    start_line: 42,
                    end_line: 52,
                    start_column: 0,
                    end_column: 0,
                    start_byte: 0,
                    end_byte: 0,
                    parent_id: None,
                    signature: Some("pub fn find_symbols(q: &str) -> Vec<Symbol>".to_string()),
                    doc_comment: None,
                    visibility: Some(Visibility::Public),
                    metadata: None,
                    semantic_group: None,
                    confidence: None,
                    code_context: None,
                    content_type: None,
                },
                score: 0.92,
            },
        ],
    };

    let output = format_symbol_context(&ctx, "full");
    assert!(output.contains("Semantically Similar"), "should have similar section header");
    assert!(output.contains("find_symbols"), "should show similar symbol name");
    assert!(output.contains("0.92"), "should show similarity score");
    assert!(output.contains("src/query.rs:42"), "should show file:line");
}

#[test]
fn test_format_no_similar_section_when_empty() {
    let ctx = SymbolContext {
        symbol: make_symbol(SymbolKind::Function, "search_symbols", Some("pub fn search_symbols(query: &str) -> Vec<Symbol>"), "src/search.rs", 10, None),
        incoming: vec![],
        incoming_total: 0,
        outgoing: vec![],
        outgoing_total: 0,
        children: vec![],
        implementations: vec![],
        test_refs: vec![],
        similar: vec![],
    };

    let output = format_symbol_context(&ctx, "full");
    assert!(!output.contains("Semantically Similar"), "should NOT have similar section when empty");
}
```

Note: Adapt the `make_symbol` call to match the exact helper signature used in `formatting_tests` — it differs from `data_tests`. Check the existing formatting tests for the correct helper.

**Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_format_similar 2>&1 | tail -10`
Expected: FAIL — compilation errors (similar field missing from test SymbolContext construction) or assertion failures.

**Step 3: Implement the formatting**

In `src/tools/deep_dive/formatting.rs`:

**3a. Import `SimilarEntry`:**

```rust
use super::data::{RefEntry, SimilarEntry, SymbolContext};
```

**3b. Add `format_similar_section` function** (at the end of the file, before the last `}`):

```rust
fn format_similar_section(out: &mut String, similar: &[SimilarEntry]) {
    if similar.is_empty() {
        return;
    }

    out.push_str(&format!("\nSemantically Similar ({}):\n", similar.len()));

    for entry in similar {
        let kind = format!("{:?}", entry.symbol.kind).to_lowercase();
        let vis = entry.symbol.visibility
            .as_ref()
            .map(|v| format!("{:?}", v).to_lowercase())
            .unwrap_or_default();
        let kind_vis = if vis.is_empty() {
            kind
        } else {
            format!("{}, {}", kind, vis)
        };

        out.push_str(&format!(
            "  {:<25} {:.2}  {}:{} ({})\n",
            entry.symbol.name,
            entry.score,
            entry.symbol.file_path,
            entry.symbol.start_line,
            kind_vis,
        ));
    }
}
```

**3c. Call from `format_symbol_context`** — add after the kind-specific match block, before `out.trim_end().to_string()`:

```rust
// === Semantic similarity (full depth only) ===
format_similar_section(&mut out, &ctx.similar);

out.trim_end().to_string()
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_format_similar 2>&1 | tail -10`
Expected: 2 passed, 0 failed.

**Step 5: Run fast tier**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All pass. Any existing formatting tests that construct `SymbolContext` without `similar` will need `similar: vec![]` added.

**Step 6: Commit**

```bash
git add src/tools/deep_dive/formatting.rs src/tests/tools/deep_dive_tests.rs
git commit -m "feat: add Semantically Similar section to deep_dive formatting"
```

---

### Task 4: Update MCP tool description

**Files:**
- Modify: `src/handler.rs:468-470`

**Step 1: Update the deep_dive tool description**

Change line 470 in `src/handler.rs` from:

```rust
description = "Understand a symbol before modifying it. Returns definition, callers, callees, children, and type info in one call — replaces chaining fast_search → get_symbols → fast_refs → Read.",
```

To:

```rust
description = "Investigate a symbol with progressive depth. Returns definition, references, children, and type info in a single call — tailored to the symbol's kind.\n\n**Always use BEFORE modifying or extending a symbol.** Replaces the common chain of fast_search → get_symbols → fast_refs → Read with a single call.",
```

Note: The description already covers the tool well. The semantically similar section is self-explanatory in the output — no need to bloat the description with it. The LLM instructions (Julie's MCP server instructions) will be updated separately if needed.

**Step 2: Commit**

```bash
git add src/handler.rs
git commit -m "docs: update deep_dive tool description"
```

---

### Task 5: Dogfood integration test

**Files:**
- Create: `src/tests/tools/search_quality/semantic_similarity_dogfood.rs`
- Modify: `src/tests/tools/search_quality/mod.rs` (register module)

**Step 1: Write the integration test**

This test uses Julie's own fixture database (same pattern as `hybrid_search_dogfood.rs`):

```rust
//! Dogfood test: verify semantic similarity in deep_dive on Julie's own codebase.
//!
//! Uses the Julie fixture database (~27K symbols) with embeddings to verify that
//! deep_dive at full depth returns meaningful semantically similar symbols.

use crate::database::SymbolDatabase;
use crate::embeddings::pipeline::run_embedding_pipeline;
use crate::embeddings::OrtEmbeddingProvider;
use crate::tools::deep_dive::data::{build_symbol_context, find_symbol};

/// Load the Julie fixture DB path (same helper used by other dogfood tests).
fn fixture_db_path() -> std::path::PathBuf {
    // Check existing dogfood tests for the exact fixture path helper
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("julie_fixture.db")
}

#[test]
fn test_deep_dive_full_shows_similar_on_real_codebase() {
    // Step 1: Copy fixture to temp dir and open
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("symbols.db");
    std::fs::copy(fixture_db_path(), &db_path).unwrap();
    let mut db = SymbolDatabase::new(&db_path).unwrap();

    // Step 2: Run embedding pipeline to populate vectors
    let provider = OrtEmbeddingProvider::new().expect("embedding model should load");
    run_embedding_pipeline(&mut db, &provider).unwrap();

    // Step 3: Deep dive a well-known symbol at full depth
    let symbols = find_symbol(&db, "hybrid_search", None).unwrap();
    assert!(!symbols.is_empty(), "hybrid_search should exist in fixture");

    let ctx = build_symbol_context(&db, &symbols[0], "full", 10, 10).unwrap();

    // Step 4: Verify similar symbols exist and are search-related
    assert!(
        !ctx.similar.is_empty(),
        "hybrid_search should have semantically similar symbols"
    );

    // At least one similar symbol should be search-related
    let search_related = ctx.similar.iter().any(|s| {
        let name = s.symbol.name.to_lowercase();
        name.contains("search") || name.contains("knn") || name.contains("rrf")
            || name.contains("query") || name.contains("tantivy")
    });
    assert!(
        search_related,
        "at least one similar symbol should be search-related, got: {:?}",
        ctx.similar.iter().map(|s| &s.symbol.name).collect::<Vec<_>>()
    );

    // Scores should be in valid range
    for entry in &ctx.similar {
        assert!(entry.score >= 0.0 && entry.score <= 1.0,
            "score {} should be in [0, 1]", entry.score);
    }

    // Max 5 results
    assert!(ctx.similar.len() <= 5, "should return at most 5 similar symbols");
}
```

Note: Check how `hybrid_search_dogfood.rs` sets up the fixture DB — copy that exact pattern for the temp dir + DB copy + embedding pipeline. The fixture path and setup may differ slightly.

**Step 2: Register the module**

In `src/tests/tools/search_quality/mod.rs`, add:

```rust
mod semantic_similarity_dogfood;
```

**Step 3: Run the test**

Run: `cargo test --lib test_deep_dive_full_shows_similar_on_real_codebase 2>&1 | tail -20`
Expected: PASS (this test will be slow — ~30-60s for embedding pipeline).

**Step 4: Run fast tier for regressions**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All pass.

**Step 5: Commit**

```bash
git add src/tests/tools/search_quality/semantic_similarity_dogfood.rs src/tests/tools/search_quality/mod.rs
git commit -m "test: add dogfood test for semantic similarity in deep_dive"
```

---

### Task 6: Final verification and cleanup

**Step 1: Run full test suite**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: All tests pass (including dogfood search quality tests).

**Step 2: Verify no file exceeds 500 lines**

Run: `wc -l src/tools/deep_dive/data.rs src/tools/deep_dive/formatting.rs src/database/vectors.rs`
Expected: All under 500 lines.

**Step 3: Verify the feature works end-to-end**

Ask the user to rebuild Julie (`cargo build --release`) and test `deep_dive(symbol="hybrid_search", depth="full")` in a live MCP session. The output should include a "Semantically Similar" section.

**Step 4: Commit any final fixups**

```bash
git add -A
git commit -m "chore: Phase 3 final cleanup"
```
