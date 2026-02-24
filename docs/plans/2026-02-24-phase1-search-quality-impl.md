# Phase 1: Search Quality & Cleanup — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix zero-result searches by adding OR-fallback, and clean up 26 stale todo!() test stubs.

**Architecture:** Add a `require_all_terms` parameter to `build_symbol_query` in `src/search/query.rs`. When AND-per-term returns zero results in `search_symbols`, automatically retry with OR semantics. BM25 scoring naturally ranks documents matching more terms higher. Delete the stale test file.

**Tech Stack:** Tantivy (BooleanQuery with Occur::Should), existing CodeTokenizer, rusqlite

---

### Task 1: Delete Stale Test File

**Files:**
- Delete: `src/tests/integration/intelligence_tools.rs`
- Modify: `src/tests/mod.rs:111`

**Step 1: Delete the stale test file**

```bash
rm src/tests/integration/intelligence_tools.rs
```

**Step 2: Remove the commented-out mod declaration**

In `src/tests/mod.rs` line 111, delete the line:
```rust
    // pub mod intelligence_tools;      // Intelligence tools integration tests - DISABLED
```

**Step 3: Verify clean build**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass, no mention of intelligence_tools

**Step 4: Commit**

```bash
git add -u
git commit -m "chore: delete 26 stale todo!() test stubs in intelligence_tools.rs"
```

---

### Task 2: Add MatchMode to build_symbol_query

**Files:**
- Modify: `src/search/query.rs:23-105`
- Test: `src/tests/tools/search/tantivy_index_tests.rs` (append new test)

**Step 1: Write the failing test**

Append to `src/tests/tools/search/tantivy_index_tests.rs`:

```rust
#[test]
fn test_or_fallback_returns_partial_matches() {
    // When searching for multiple terms where no single symbol contains ALL of them,
    // OR mode should still return symbols that match SOME terms, ranked by match count.
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // Symbol that matches "ranking" and "score" (2 of 4 terms)
    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "apply_ranking_score".into(),
            signature: "pub fn apply_ranking_score(results: &mut Vec<SearchResult>)".into(),
            doc_comment: "Apply ranking scores to search results".into(),
            code_body: "fn apply_ranking_score(results: &mut Vec<SearchResult>) { /* impl */ }".into(),
            file_path: "src/search/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();

    // Symbol that matches "centrality" and "boost" (2 of 4 terms)
    index
        .add_symbol(&SymbolDocument {
            id: "2".into(),
            name: "apply_centrality_boost".into(),
            signature: "pub fn apply_centrality_boost(results: &mut Vec<SearchResult>)".into(),
            doc_comment: "Boost results by graph centrality".into(),
            code_body: "fn apply_centrality_boost(results: &mut Vec<SearchResult>) { /* impl */ }".into(),
            file_path: "src/search/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 30,
        })
        .unwrap();

    // Symbol that matches only "score" (1 of 4 terms)
    index
        .add_symbol(&SymbolDocument {
            id: "3".into(),
            name: "calculate_score".into(),
            signature: "pub fn calculate_score(input: &str) -> f32".into(),
            doc_comment: "Calculate a score".into(),
            code_body: "fn calculate_score(input: &str) -> f32 { 0.0 }".into(),
            file_path: "src/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 50,
        })
        .unwrap();

    index.commit().unwrap();

    // AND mode: "ranking score boost centrality" — no symbol has ALL four tokens
    let and_results = index
        .search_symbols("ranking score boost centrality", &SearchFilter::default(), 10)
        .unwrap();
    assert!(
        and_results.is_empty(),
        "AND mode should return nothing — no symbol contains all four terms"
    );

    // OR mode: should return partial matches, ranked by how many terms match
    let or_results = index
        .search_symbols_relaxed("ranking score boost centrality", &SearchFilter::default(), 10)
        .unwrap();
    assert!(
        !or_results.is_empty(),
        "OR mode should return partial matches"
    );
    // Both 2-term matches should appear before the 1-term match
    assert!(
        or_results.len() >= 2,
        "Should find at least the two 2-term matches, got {}",
        or_results.len()
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_or_fallback_returns_partial_matches 2>&1 | tail -20`
Expected: FAIL — `search_symbols_relaxed` method does not exist

**Step 3: Implement MatchMode and search_symbols_relaxed**

In `src/search/query.rs`, add the `require_all_terms` parameter to `build_symbol_query`:

```rust
pub fn build_symbol_query(
    terms: &[String],
    name_field: Field,
    sig_field: Field,
    doc_field: Field,
    body_field: Field,
    doc_type_field: Field,
    language_field: Field,
    kind_field: Field,
    language_filter: Option<&str>,
    kind_filter: Option<&str>,
    require_all_terms: bool,  // NEW: false = OR mode
) -> BooleanQuery {
    // ... existing subqueries for doc_type, language, kind filters (always Must) ...

    // Term occurrence depends on mode
    let term_occur = if require_all_terms {
        Occur::Must   // AND: every term required
    } else {
        Occur::Should // OR: any term contributes to score
    };

    for term in terms {
        // ... existing per-field clause building ...
        subqueries.push((term_occur, Box::new(BooleanQuery::new(field_clauses))));
    }

    BooleanQuery::new(subqueries)
}
```

In `src/search/index.rs`, update the existing `search_symbols` call to pass `true`, and add `search_symbols_relaxed`:

```rust
pub fn search_symbols_relaxed(
    &self,
    query_str: &str,
    filter: &SearchFilter,
    limit: usize,
) -> Result<Vec<SymbolSearchResult>> {
    let f = &self.schema_fields;
    let terms = Self::filter_compound_tokens(self.tokenize_query(query_str));
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let query = build_symbol_query(
        &terms, f.name, f.signature, f.doc_comment, f.code_body,
        f.doc_type, f.language, f.kind,
        filter.language.as_deref(), filter.kind.as_deref(),
        false,  // OR mode
    );

    let searcher = self.reader.searcher();
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

    let mut results = Vec::with_capacity(top_docs.len());
    for (score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        results.push(SymbolSearchResult {
            id: Self::get_text_field(&doc, f.id),
            name: Self::get_text_field(&doc, f.name),
            signature: Self::get_text_field(&doc, f.signature),
            doc_comment: Self::get_text_field(&doc, f.doc_comment),
            file_path: Self::get_text_field(&doc, f.file_path),
            kind: Self::get_text_field(&doc, f.kind),
            language: Self::get_text_field(&doc, f.language),
            start_line: Self::get_u64_field(&doc, f.start_line) as u32,
            score,
        });
    }

    if let Some(configs) = &self.language_configs {
        apply_important_patterns_boost(&mut results, configs);
    }

    Ok(results)
}
```

Also update the existing `search_symbols` to pass `require_all_terms: true` to `build_symbol_query`.

**Step 4: Run test to verify it passes**

Run: `cargo test test_or_fallback_returns_partial_matches 2>&1 | tail -20`
Expected: PASS

**Step 5: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass — existing behavior unchanged since `search_symbols` still passes `true`

**Step 6: Commit**

```bash
git add src/search/query.rs src/search/index.rs src/tests/tools/search/tantivy_index_tests.rs
git commit -m "feat: add OR-mode search for partial term matching"
```

---

### Task 3: Integrate OR Fallback into search_symbols

**Files:**
- Modify: `src/search/index.rs:244-299` (search_symbols method)
- Test: `src/tests/tools/search/tantivy_index_tests.rs` (append new test)

**Step 1: Write the failing test**

Append to `src/tests/tools/search/tantivy_index_tests.rs`:

```rust
#[test]
fn test_search_symbols_auto_fallback_to_or() {
    // search_symbols should automatically fall back to OR when AND returns zero results
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "apply_ranking_score".into(),
            signature: "pub fn apply_ranking_score()".into(),
            doc_comment: "Ranking scores".into(),
            code_body: "fn apply_ranking_score() {}".into(),
            file_path: "src/scoring.rs".into(),
            kind: "function".into(),
            language: "rust".into(),
            start_line: 10,
        })
        .unwrap();

    index.commit().unwrap();

    // Query with terms that partially match — AND would fail, OR should succeed
    let results = index
        .search_symbols("ranking boost centrality", &SearchFilter::default(), 10)
        .unwrap();

    assert!(
        !results.is_empty(),
        "search_symbols should auto-fallback to OR when AND returns nothing"
    );
    assert_eq!(results[0].name, "apply_ranking_score");
}

#[test]
fn test_search_symbols_prefers_and_when_available() {
    // When AND produces results, OR fallback should NOT be used
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    index
        .add_symbol(&SymbolDocument {
            id: "1".into(),
            name: "UserService".into(),
            signature: "pub struct UserService".into(),
            doc_comment: "".into(),
            code_body: "pub struct UserService {}".into(),
            file_path: "src/user.rs".into(),
            kind: "class".into(),
            language: "rust".into(),
            start_line: 1,
        })
        .unwrap();

    index.commit().unwrap();

    // Single-term query — AND works fine
    let results = index
        .search_symbols("UserService", &SearchFilter::default(), 10)
        .unwrap();
    assert_eq!(results[0].name, "UserService");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_search_symbols_auto_fallback_to_or 2>&1 | tail -20`
Expected: FAIL — search_symbols still uses AND-only and returns empty

**Step 3: Modify search_symbols to auto-fallback**

In `src/search/index.rs`, modify `search_symbols` (lines 244-299):

```rust
pub fn search_symbols(
    &self,
    query_str: &str,
    filter: &SearchFilter,
    limit: usize,
) -> Result<Vec<SymbolSearchResult>> {
    let f = &self.schema_fields;
    let terms = Self::filter_compound_tokens(self.tokenize_query(query_str));
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    // Try AND-per-term first (strict matching)
    let query = build_symbol_query(
        &terms, f.name, f.signature, f.doc_comment, f.code_body,
        f.doc_type, f.language, f.kind,
        filter.language.as_deref(), filter.kind.as_deref(),
        true,
    );

    let searcher = self.reader.searcher();
    let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

    // Auto-fallback: if AND returned nothing and we have multiple terms, try OR
    let top_docs = if top_docs.is_empty() && terms.len() > 1 {
        let or_query = build_symbol_query(
            &terms, f.name, f.signature, f.doc_comment, f.code_body,
            f.doc_type, f.language, f.kind,
            filter.language.as_deref(), filter.kind.as_deref(),
            false,
        );
        searcher.search(&or_query, &TopDocs::with_limit(limit))?
    } else {
        top_docs
    };

    let mut results = Vec::with_capacity(top_docs.len());
    for (score, doc_address) in top_docs {
        let doc: TantivyDocument = searcher.doc(doc_address)?;
        results.push(SymbolSearchResult { /* same as current */ });
    }

    if let Some(configs) = &self.language_configs {
        apply_important_patterns_boost(&mut results, configs);
    }

    Ok(results)
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test test_search_symbols_auto_fallback 2>&1 | tail -20`
Expected: PASS

Run: `cargo test test_search_symbols_prefers_and 2>&1 | tail -20`
Expected: PASS

**Step 5: Run full test suite for regressions**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass

**Step 6: Commit**

```bash
git add src/search/index.rs src/tests/tools/search/tantivy_index_tests.rs
git commit -m "feat: auto-fallback to OR matching when AND returns zero results"
```

---

### Task 4: Add Relaxed-Match Indicator to Tool Output

**Files:**
- Modify: `src/tools/search/text_search.rs` (where search results are formatted for MCP output)
- Test: Verify manually after build

**Step 1: Find where search results are formatted**

Look at `src/tools/search/text_search.rs` — the `text_search_impl` function that calls `search_symbols`. This is where the MCP tool output is assembled.

**Step 2: Track whether OR fallback was used**

Modify `search_symbols` to return a tuple or a struct indicating whether fallback was used:

```rust
pub struct SearchResult {
    pub results: Vec<SymbolSearchResult>,
    pub relaxed: bool,  // true if OR fallback was used
}
```

Or, simpler: add a `relaxed_search: bool` field to `SymbolSearchResult`. Decide during implementation which approach is cleaner.

**Step 3: Prepend hint to tool output when relaxed**

In the tool output formatting, if OR fallback was used, prepend:
```
⚡ Relaxed search (showing partial matches — no results matched all terms)
```

**Step 4: Commit**

```bash
git add src/search/index.rs src/tools/search/text_search.rs
git commit -m "feat: show relaxed-search indicator when OR fallback is used"
```

---

### Task 5: Apply OR Fallback to Content Search

**Files:**
- Modify: `src/search/query.rs` — `build_content_query` (lines 107-141)
- Modify: `src/search/index.rs` — `search_content` method
- Test: `src/tests/tools/search/tantivy_index_tests.rs`

**Step 1: Audit `build_content_query`**

Read `src/search/query.rs` lines 107-141. Note: content search already uses a mix of Must/Should for compound vs atomic tokens. Determine if it has the same zero-result problem. If it does, apply the same `require_all_terms` pattern.

**Step 2: Write failing test if needed**

If content search has the same problem, write a test similar to Task 2's test but using `search_content` instead of `search_symbols`.

**Step 3: Implement fix if needed**

Apply same pattern: AND first, OR fallback on zero results.

**Step 4: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All pass

**Step 5: Commit**

```bash
git add src/search/query.rs src/search/index.rs src/tests/tools/search/tantivy_index_tests.rs
git commit -m "feat: apply OR fallback to content search"
```

---

### Task 6: Search Quality Verification

**Step 1: Build debug binary**

Run: `cargo build 2>&1 | tail -5`

**Step 2: Test against Julie's own codebase**

After restarting with the new build, test these queries via the MCP tool:

**Must still work (AND precision):**
- `fast_search("UserService")` → exact match
- `fast_search("process_payment")` → exact match
- `fast_search("build_symbol_query")` → exact match

**Must now work (OR fallback):**
- `fast_search("ranking score boost centrality")` → finds scoring-related symbols
- `fast_search("graph centrality")` → finds related code
- `fast_search("token budget limit")` → finds TokenEstimator-related code
- `fast_search("zero results fallback relaxed query")` → finds search-related code

**Must not regress:**
- `fast_search("UserService", search_target="definitions")` → definition promoted
- `fast_search("process", language="rust")` → language filter works
- `fast_search("process", file_pattern="src/search/**")` → file filter works

**Step 3: Document results and commit any tuning**

If any tests fail, adjust the implementation and add regression tests.

```bash
git commit -m "test: verify search quality with OR fallback"
```
