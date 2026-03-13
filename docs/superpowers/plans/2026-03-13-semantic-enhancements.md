# Semantic Enhancements Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface semantic similarity in `deep_dive` (context depth) and `fast_refs` (zero-ref fallback) to enable cross-language symbol discovery without adding new tools.

**Architecture:** Extract the existing `build_similar` KNN logic from `deep_dive/data.rs` into a shared `src/search/similarity.rs` module. Both `deep_dive` and `fast_refs` call the shared function. Add a distance threshold (score < 0.5 filtered out) to avoid surfacing garbage matches.

**Tech Stack:** Rust, SQLite (sqlite-vec for KNN), existing embedding infrastructure (ORT/fastembed).

**Spec:** `docs/superpowers/specs/2026-03-13-semantic-enhancements-design.md`

---

## Chunk 1: Foundation — shared similarity module + deep_dive enhancement

### Task 1: Fix broken `test_lean_refs_no_results` from v5.0.4

The v5.0.4 error message improvements changed `format_lean_refs_results` to include
a recovery hint, but `test_lean_refs_no_results` still asserts the old exact string.

**Files:**
- Modify: `src/tests/tools/formatting_tests.rs:73-76`

- [ ] **Step 1: Update the test assertion**

```rust
#[test]
fn test_lean_refs_no_results() {
    let output = format_lean_refs_results("Unknown", &[], &[], &HashMap::new());
    assert!(
        output.contains("No references found for \"Unknown\""),
        "Should contain 'No references found' message, got: {}",
        output
    );
    assert!(
        output.contains("fast_search"),
        "Should contain recovery hint suggesting fast_search, got: {}",
        output
    );
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test test_lean_refs_no_results 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/tests/tools/formatting_tests.rs
git commit -m "fix(tests): update formatting test for v5.0.4 error message change"
```

---

### Task 2: Create shared similarity module

Extract the KNN similarity logic into `src/search/similarity.rs` so both `deep_dive`
and `fast_refs` can use it. This is a new file with a single public function.

**Files:**
- Create: `src/search/similarity.rs`
- Modify: `src/search/mod.rs` — add `pub mod similarity;`

- [ ] **Step 1: Write the shared function and tests**

Create `src/search/similarity.rs` with the function, struct, and inline tests.

Note on test setup: `SymbolDatabase::new()` takes `&Path` and handles all initialization
internally — do NOT call `db.initialize()`. The database requires file records before
storing symbols (FK constraint), so call `store_file_info` for each file path used in
test symbols. Follow the pattern in `src/tests/tools/deep_dive_tests.rs:1045-1068`.

```rust
//! Shared semantic similarity search via KNN on stored embeddings.
//!
//! Used by `deep_dive` (similar symbols section) and `fast_refs` (zero-ref fallback).

use anyhow::Result;
use std::collections::HashMap;

use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;

/// Minimum similarity score (1.0 - cosine_distance) to include in results.
/// Below this threshold, matches are likely noise.
pub const MIN_SIMILARITY_SCORE: f32 = 0.5;

/// Entry for a semantically similar symbol.
#[derive(Debug)]
pub struct SimilarEntry {
    pub symbol: Symbol,
    /// Similarity score: 0.0..1.0, higher = more similar (1.0 - cosine_distance)
    pub score: f32,
}

/// Find symbols semantically similar to `symbol` via KNN on stored embeddings.
///
/// Returns empty Vec if the symbol has no embedding (graceful degradation).
/// Filters out self-matches and entries below `min_score`.
pub fn find_similar_symbols(
    db: &SymbolDatabase,
    symbol: &Symbol,
    limit: usize,
    min_score: f32,
) -> Result<Vec<SimilarEntry>> {
    // Step 1: Get the symbol's own embedding
    let embedding = match db.get_embedding(&symbol.id)? {
        Some(vec) => vec,
        None => return Ok(vec![]),
    };

    // Step 2: KNN search (fetch extra to account for self + threshold filtering)
    let knn_results = db.knn_search(&embedding, limit + 1)?;

    // Step 3: Filter out self, apply threshold, collect IDs
    let filtered: Vec<(String, f64)> = knn_results
        .into_iter()
        .filter(|(id, _)| id != &symbol.id)
        .filter(|(_, distance)| (1.0 - distance) as f32 >= min_score)
        .take(limit)
        .collect();

    if filtered.is_empty() {
        return Ok(vec![]);
    }

    let distances: HashMap<String, f64> = filtered.iter().cloned().collect();
    let symbol_ids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();

    // Step 4: Fetch full symbols
    let symbols = db.get_symbols_by_ids(&symbol_ids)?;

    // Step 5: Build entries in KNN order
    let mut entries = Vec::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::SymbolDatabase;
    use crate::database::files::FileInfo;
    use crate::extractors::base::{SymbolKind, Visibility};
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, SymbolDatabase) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // FK constraint requires file records before symbols
        for file in &["src/a.rs", "src/b.rs"] {
            db.store_file_info(&FileInfo {
                path: file.to_string(),
                language: "rust".to_string(),
                hash: format!("hash_{}", file),
                size: 500,
                last_modified: 1000000,
                last_indexed: 0,
                symbol_count: 2,
                content: None,
            }).unwrap();
        }

        (tmp, db)
    }

    fn make_symbol(id: &str, name: &str, kind: SymbolKind, file: &str, line: u32) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            file_path: file.to_string(),
            line_number: line,
            end_line_number: Some(line + 10),
            parent_id: None,
            signature: Some(format!("fn {}()", name)),
            visibility: Some(Visibility::Public),
            doc_comment: None,
            language: Some("rust".to_string()),
            content_type: None,
            confidence: None,
            semantic_group: None,
            metadata: None,
        }
    }

    #[test]
    fn test_find_similar_returns_results_above_threshold() {
        let (_tmp, db) = setup_db();

        let sym_a = make_symbol("sym-a", "process_data", SymbolKind::Function, "src/a.rs", 10);
        let sym_b = make_symbol("sym-b", "handle_data", SymbolKind::Function, "src/b.rs", 20);
        db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

        // Close embeddings → high similarity score
        let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        let mut emb_b = emb_a.clone();
        emb_b[0] += 0.001;
        db.store_embeddings(&[
            ("sym-a".to_string(), emb_a),
            ("sym-b".to_string(), emb_b),
        ]).unwrap();

        let results = find_similar_symbols(&db, &sym_a, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol.name, "handle_data");
        assert!(results[0].score >= MIN_SIMILARITY_SCORE);
    }

    #[test]
    fn test_find_similar_filters_below_threshold() {
        let (_tmp, db) = setup_db();

        let sym_a = make_symbol("sym-a", "process_data", SymbolKind::Function, "src/a.rs", 10);
        let sym_b = make_symbol("sym-b", "totally_unrelated", SymbolKind::Function, "src/b.rs", 20);
        db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

        // Distant embeddings → low similarity score → should be filtered out
        let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        let emb_b: Vec<f32> = (0..384).map(|i| ((383 - i) as f32) * 0.01).collect();
        db.store_embeddings(&[
            ("sym-a".to_string(), emb_a),
            ("sym-b".to_string(), emb_b),
        ]).unwrap();

        let results = find_similar_symbols(&db, &sym_a, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert!(
            results.is_empty(),
            "Distant embeddings should be filtered out by threshold, got {} results",
            results.len()
        );
    }

    #[test]
    fn test_find_similar_no_embedding_returns_empty() {
        let (_tmp, db) = setup_db();

        let sym = make_symbol("sym-a", "lonely", SymbolKind::Function, "src/a.rs", 10);
        db.store_symbols(&[sym.clone()]).unwrap();
        // No embeddings stored

        let results = find_similar_symbols(&db, &sym, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_similar_excludes_self() {
        let (_tmp, db) = setup_db();

        let sym = make_symbol("sym-a", "only_one", SymbolKind::Function, "src/a.rs", 10);
        db.store_symbols(&[sym.clone()]).unwrap();

        let emb: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
        db.store_embeddings(&[("sym-a".to_string(), emb)]).unwrap();

        let results = find_similar_symbols(&db, &sym, 5, MIN_SIMILARITY_SCORE).unwrap();
        assert!(results.is_empty(), "Should not include self");
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src/search/mod.rs`, add:
```rust
pub mod similarity;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test similarity 2>&1 | tail -10`
Expected: 4 tests PASS

- [ ] **Step 4: Commit**

```bash
git add src/search/similarity.rs src/search/mod.rs
git commit -m "feat(search): add shared similarity module for KNN symbol lookup"
```

---

### Task 3: Refactor deep_dive to use shared similarity + lower depth gate

Replace `deep_dive/data.rs::build_similar` with a call to the shared function,
lower the gate from "full" to "context+full", and update the formatting comment.

**Files:**
- Modify: `src/tools/deep_dive/data.rs:37-43` — re-export `SimilarEntry` from shared module
- Modify: `src/tools/deep_dive/data.rs:205-210` — change depth gate
- Modify: `src/tools/deep_dive/data.rs:400-444` — replace `build_similar` body
- Modify: `src/tools/deep_dive/formatting.rs:43` — update stale comment
- Modify: `src/tests/tools/deep_dive_tests.rs:1964-2009` — update test assertions

- [ ] **Step 1: Update the test to assert context depth DOES include similar**

In `src/tests/tools/deep_dive_tests.rs`, rename `test_similar_symbols_not_at_context_depth`
to `test_similar_symbols_at_context_depth` and update assertions:

```rust
#[test]
fn test_similar_symbols_at_context_depth() {
    let (_tmp, mut db) = setup_db();

    let sym_a = make_symbol(
        "sym-c", "func_alpha", SymbolKind::Function, "src/engine.rs", 10,
        None, Some("fn func_alpha()"), Some(Visibility::Public), None,
    );
    let sym_b = make_symbol(
        "sym-d", "func_beta", SymbolKind::Function, "src/handler.rs", 20,
        None, Some("fn func_beta()"), Some(Visibility::Public), None,
    );
    db.store_symbols(&[sym_a.clone(), sym_b.clone()]).unwrap();

    let emb_a: Vec<f32> = (0..384).map(|i| (i as f32) * 0.01).collect();
    let mut emb_b = emb_a.clone();
    emb_b[0] += 0.001;
    db.store_embeddings(&[("sym-c".to_string(), emb_a), ("sym-d".to_string(), emb_b)])
        .unwrap();

    // At "context" depth, similar SHOULD be populated
    let ctx_context = build_symbol_context(&db, &sym_a, "context", 10, 10).unwrap();
    assert!(
        !ctx_context.similar.is_empty(),
        "similar should be populated at context depth"
    );

    // At "overview" depth, similar should NOT be populated
    let ctx_overview = build_symbol_context(&db, &sym_a, "overview", 10, 10).unwrap();
    assert!(
        ctx_overview.similar.is_empty(),
        "similar should be empty at overview depth"
    );
}
```

- [ ] **Step 2: Run the test — verify it fails**

Run: `cargo test test_similar_symbols_at_context_depth 2>&1 | tail -10`
Expected: FAIL (context depth still returns empty similar)

- [ ] **Step 3: Replace `SimilarEntry` in data.rs with re-export from shared module**

In `src/tools/deep_dive/data.rs`, replace the `SimilarEntry` struct definition (lines 37-43)
with a re-export:

```rust
// Re-export SimilarEntry from shared similarity module
pub use crate::search::similarity::SimilarEntry;
```

Remove the old struct definition. The `SymbolContext` struct keeps its `similar: Vec<SimilarEntry>`
field unchanged.

- [ ] **Step 4: Replace `build_similar` function body with delegation**

Replace the `build_similar` function (lines 400-444) with:

```rust
/// Find semantically similar symbols via KNN on stored embeddings.
/// Delegates to the shared similarity module.
fn build_similar(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<SimilarEntry>> {
    use crate::search::similarity::{self, MIN_SIMILARITY_SCORE};
    const SIMILAR_LIMIT: usize = 5;
    similarity::find_similar_symbols(db, symbol, SIMILAR_LIMIT, MIN_SIMILARITY_SCORE)
}
```

- [ ] **Step 5: Lower the depth gate**

In `build_symbol_context` (line 205-210), change:

```rust
// === Semantically similar symbols (context and full depth) ===
let similar = if depth == "full" || depth == "context" {
    build_similar(db, &symbol)?
} else {
    vec![]
};
```

- [ ] **Step 6: Update the formatting comment**

In `src/tools/deep_dive/formatting.rs:43`, change:
```rust
// === Semantic similarity (context and full depth) ===
```

- [ ] **Step 7: Run the tests**

Run: `cargo test deep_dive 2>&1 | tail -20`
Expected: All pass including `test_similar_symbols_at_context_depth`

Note: `test_similar_symbols_at_full_depth` uses very close embeddings (nudge of 0.001
on 2 dimensions of 384), producing a score well above 0.5 — the threshold won't filter it.

- [ ] **Step 8: Commit**

```bash
git add src/tools/deep_dive/data.rs src/tools/deep_dive/formatting.rs \
        src/tests/tools/deep_dive_tests.rs
git commit -m "feat(deep_dive): show similar symbols at context depth, add distance threshold"
```

---

## Chunk 2: fast_refs semantic fallback

### Task 4: Add formatting for semantic fallback

Add the formatting function and its tests before wiring it into `fast_refs`.

**Files:**
- Modify: `src/tools/navigation/formatting.rs` — add `format_semantic_fallback` function
- Modify: `src/tests/tools/formatting_tests.rs` — add formatting tests

- [ ] **Step 1: Write the formatting function tests**

In `src/tests/tools/formatting_tests.rs`, add:

```rust
#[test]
fn test_format_semantic_fallback_with_results() {
    use crate::search::similarity::SimilarEntry;

    let entries = vec![
        SimilarEntry {
            symbol: Symbol {
                id: "s1".to_string(),
                name: "UserDto".to_string(),
                kind: SymbolKind::Class,
                file_path: "src/api/models.cs".to_string(),
                line_number: 45,
                end_line_number: Some(80),
                parent_id: None,
                signature: None,
                visibility: Some(Visibility::Public),
                doc_comment: None,
                language: Some("csharp".to_string()),
                content_type: None,
                confidence: None,
                semantic_group: None,
                metadata: None,
            },
            score: 0.82,
        },
    ];

    let output = format_semantic_fallback("IUser", &entries);
    assert!(output.contains("Related symbols (semantic)"));
    assert!(output.contains("UserDto"));
    assert!(output.contains("0.82"));
    assert!(output.contains("src/api/models.cs:45"));
}

#[test]
fn test_format_semantic_fallback_empty() {
    let output = format_semantic_fallback("IUser", &[]);
    assert!(output.is_empty(), "Should return empty string for no results");
}
```

- [ ] **Step 2: Write the `format_semantic_fallback` function**

In `src/tools/navigation/formatting.rs`, add:

```rust
use crate::search::similarity::SimilarEntry;

/// Format semantic similarity results for the zero-ref fallback in fast_refs.
pub fn format_semantic_fallback(symbol: &str, similar: &[SimilarEntry]) -> String {
    if similar.is_empty() {
        return String::new();
    }

    let mut out = String::from("\nRelated symbols (semantic):\n");

    for entry in similar {
        let kind = entry.symbol.kind.to_string();
        let vis = entry
            .symbol
            .visibility
            .as_ref()
            .map(|v| v.to_string().to_lowercase())
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
            entry.symbol.line_number,
            kind_vis,
        ));
    }

    out.push_str(&format!(
        "\n💡 These are semantically similar to \"{}\", not exact references",
        symbol
    ));

    out
}
```

- [ ] **Step 3: Run formatting tests**

Run: `cargo test formatting_tests 2>&1 | tail -10`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/tools/navigation/formatting.rs src/tests/tools/formatting_tests.rs
git commit -m "feat(fast_refs): add semantic fallback formatting function"
```

---

### Task 5: Wire semantic fallback into fast_refs call_tool

When `fast_refs` finds zero references AND the query is for the primary workspace,
attempt a semantic similarity lookup before returning.

**Files:**
- Modify: `src/tools/navigation/fast_refs.rs:69-97` — add semantic fallback in `call_tool`

- [ ] **Step 1: Add `try_semantic_fallback` method**

Add this method to the `impl FastRefsTool` block in `src/tools/navigation/fast_refs.rs`:

```rust
/// When zero references are found, try semantic similarity as a fallback.
/// Returns formatted semantic results or empty string.
/// Skips for reference workspace queries (may lack embeddings).
async fn try_semantic_fallback(&self, handler: &JulieServerHandler) -> String {
    use crate::search::similarity::{self, MIN_SIMILARITY_SCORE};
    use super::formatting::format_semantic_fallback;

    // Skip for reference workspace queries
    if self.workspace.is_some() && self.workspace.as_deref() != Some("primary") {
        return String::new();
    }

    let workspace = match handler.get_workspace().await {
        Ok(Some(w)) => w,
        _ => return String::new(),
    };

    let db = match workspace.db.as_ref() {
        Some(db) => db,
        None => return String::new(),
    };

    let db_guard = match db.lock() {
        Ok(guard) => guard,
        Err(_) => return String::new(),
    };

    // Find the symbol by name to get its ID for embedding lookup
    let symbols = match db_guard.find_symbols_by_name(&self.symbol) {
        Ok(syms) => syms,
        Err(_) => return String::new(),
    };

    // Filter out imports, take first definition match
    let symbol = match symbols.iter().find(|s| {
        s.kind != crate::extractors::base::SymbolKind::Import
    }) {
        Some(s) => s.clone(),
        None => return String::new(),
    };

    let similar = match similarity::find_similar_symbols(
        &db_guard, &symbol, 5, MIN_SIMILARITY_SCORE,
    ) {
        Ok(results) => results,
        Err(_) => return String::new(),
    };

    format_semantic_fallback(&self.symbol, &similar)
}
```

- [ ] **Step 2: Modify the zero-ref early return to use the fallback**

Replace lines 80-83 in `call_tool`:

```rust
if definitions.is_empty() && references.is_empty() {
    // Attempt semantic fallback (primary workspace only)
    let semantic_section = self.try_semantic_fallback(handler).await;

    let empty_names = HashMap::new();
    let mut result_text = format_lean_refs_results(
        &self.symbol, &[], &[], &empty_names,
    );
    result_text.push_str(&semantic_section);
    return Ok(CallToolResult::text_content(vec![Content::text(result_text)]));
}
```

- [ ] **Step 3: Run fast_refs tests**

Run: `cargo test fast_refs 2>&1 | tail -15`
Expected: All existing tests pass

- [ ] **Step 4: Commit**

```bash
git add src/tools/navigation/fast_refs.rs
git commit -m "feat(fast_refs): add semantic similarity fallback on zero references"
```

---

### Task 6: Integration validation

Validate the changes work on Julie's own codebase with real embeddings.

**Files:** None (manual testing)

- [ ] **Step 1: Build Julie**

Run: `cargo build 2>&1 | tail -3`
Expected: Clean build

- [ ] **Step 2: Test deep_dive context depth shows similar**

Test via MCP: `deep_dive(symbol="EmbeddingProvider", depth="context")`
Expected: Should include a "Semantically Similar" section with related
provider/embedding types (if embeddings are indexed for the workspace).

- [ ] **Step 3: Test deep_dive overview depth does NOT show similar**

Test via MCP: `deep_dive(symbol="EmbeddingProvider", depth="overview")`
Expected: No "Semantically Similar" section.

- [ ] **Step 4: Test fast_refs semantic fallback**

Test via MCP: `fast_refs(symbol="SomeRareSymbolName")`
Expected: If no exact references found, should show semantic relatives
(or clean "No references found" with hint if no embeddings available).

- [ ] **Step 5: Commit**

```bash
git commit --allow-empty -m "chore: semantic enhancements validated"
```
