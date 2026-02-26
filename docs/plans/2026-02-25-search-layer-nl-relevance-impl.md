# Search-Layer NL Relevance Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve natural-language search retrieval so concept queries return actionable production code more reliably without regressing identifier searches.

**Architecture:** Add deterministic query expansion in the search layer with bounded, weighted term groups (original, phrase aliases, normalized variants), then apply a mild path prior only for NL-like multi-word queries. Keep existing Tantivy query structure, field boosts, and AND->OR fallback behavior intact.

**Tech Stack:** Rust, Tantivy, existing `src/search/*` query builders, existing Rust test harness in `src/tests/tools/search/*`.

---

### Task 1: Add failing tests for NL expansion primitives

**Files:**
- Create: `src/tests/tools/search/tantivy_query_expansion_tests.rs`
- Modify: `src/tests/tools/search/mod.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_phrase_alias_expands_workspace_routing() {
    let expanded = crate::search::expansion::expand_query_terms("workspace routing");
    assert!(expanded.alias_terms.iter().any(|t| t == "router"));
    assert!(expanded.alias_terms.iter().any(|t| t == "registry"));
}
```

Add tests for:
- phrase alias expansion exists for known phrase
- dedup of alias + original terms
- max-added-term cap respected

**Step 2: Run test to verify it fails**

Run: `cargo test --lib tests::tools::search::tantivy_query_expansion_tests 2>&1 | tail -20`
Expected: FAIL due to missing `search::expansion` module/functions.

**Step 3: Wire test module**

Add `mod tantivy_query_expansion_tests;` in `src/tests/tools/search/mod.rs`.

**Step 4: Run test to verify failure remains targeted**

Run: `cargo test --lib tests::tools::search::tantivy_query_expansion_tests 2>&1 | tail -20`
Expected: FAIL only for new expansion tests.

**Step 5: Commit**

```bash
git add src/tests/tools/search/mod.rs src/tests/tools/search/tantivy_query_expansion_tests.rs
git commit -m "test: add failing tests for deterministic query expansion"
```

### Task 2: Implement deterministic expansion module (minimal green)

**Files:**
- Create: `src/search/expansion.rs`
- Modify: `src/search/mod.rs`
- Test: `src/tests/tools/search/tantivy_query_expansion_tests.rs`

**Step 1: Write minimal implementation**

Create `expand_query_terms(query: &str) -> ExpandedQueryTerms` with:
- `original_terms: Vec<String>`
- `alias_terms: Vec<String>`
- `normalized_terms: Vec<String>`
- hardcoded phrase map for initial phrases (`"workspace routing"` etc.)
- dedup + cap logic

```rust
pub struct ExpandedQueryTerms {
    pub original_terms: Vec<String>,
    pub alias_terms: Vec<String>,
    pub normalized_terms: Vec<String>,
}
```

**Step 2: Export module**

Add `pub mod expansion;` to `src/search/mod.rs`.

**Step 3: Run tests to verify green**

Run: `cargo test --lib tests::tools::search::tantivy_query_expansion_tests 2>&1 | tail -20`
Expected: PASS for expansion primitive tests.

**Step 4: Refactor for readability**

Extract helpers:
- `is_nl_like_query(...)`
- `apply_aliases(...)`
- `apply_normalization(...)`

Keep file under project size limit.

**Step 5: Commit**

```bash
git add src/search/expansion.rs src/search/mod.rs src/tests/tools/search/tantivy_query_expansion_tests.rs
git commit -m "feat: add deterministic query expansion primitives"
```

### Task 3: Add failing tests for query-builder weighting behavior

**Files:**
- Create: `src/tests/tools/search/tantivy_query_weighting_tests.rs`
- Modify: `src/tests/tools/search/mod.rs`
- Test target: `src/search/query.rs`

**Step 1: Write failing test**

Use query debug rendering checks (or equivalent stable behavior assertions) to verify:
- original term group included with highest boost
- alias/normalized groups included with lower boosts
- `doc_type` and filter MUST clauses preserved

```rust
#[test]
fn test_weighted_symbol_query_preserves_doc_type_and_filters() {
    // Build weighted query and assert required clauses remain mandatory.
    // Assert boosts for original > alias > normalized.
}
```

**Step 2: Run to confirm fail**

Run: `cargo test --lib tests::tools::search::tantivy_query_weighting_tests 2>&1 | tail -20`
Expected: FAIL because weighted builder does not yet exist.

**Step 3: Wire test module**

Add `mod tantivy_query_weighting_tests;` to `src/tests/tools/search/mod.rs`.

**Step 4: Re-run target test**

Run: `cargo test --lib tests::tools::search::tantivy_query_weighting_tests 2>&1 | tail -20`
Expected: FAIL only for new weighting tests.

**Step 5: Commit**

```bash
git add src/tests/tools/search/mod.rs src/tests/tools/search/tantivy_query_weighting_tests.rs
git commit -m "test: add failing weighted query builder tests"
```

### Task 4: Implement weighted query builders in search/query.rs

**Files:**
- Modify: `src/search/query.rs`
- Test: `src/tests/tools/search/tantivy_query_weighting_tests.rs`

**Step 1: Implement minimal weighted builders**

Add (or overload with new names):
- `build_symbol_query_weighted(...)`
- `build_content_query_weighted(...)`

Inputs include grouped terms (original/alias/normalized) and produce boosted SHOULD groups while preserving existing MUST semantics.

**Step 2: Preserve compatibility**

Keep existing `build_symbol_query`/`build_content_query` call paths available by adapting through weighted APIs with empty alias/normalized groups.

**Step 3: Run weighting tests**

Run: `cargo test --lib tests::tools::search::tantivy_query_weighting_tests 2>&1 | tail -20`
Expected: PASS.

**Step 4: Run nearby query tests**

Run: `cargo test --lib tests::integration::query_preprocessor_tests 2>&1 | tail -20`
Expected: PASS (no query-type regressions).

**Step 5: Commit**

```bash
git add src/search/query.rs src/tests/tools/search/tantivy_query_weighting_tests.rs
git commit -m "feat: add weighted query builders for expanded terms"
```

### Task 5: Add failing tests for NL-only path prior

**Files:**
- Create: `src/tests/tools/search/tantivy_path_prior_tests.rs`
- Modify: `src/tests/tools/search/mod.rs`
- Test target: `src/search/scoring.rs`

**Step 1: Write failing tests**

Add tests for:
- NL-like query: `src/**` gets small boost over equivalent docs/tests match.
- Identifier query: path prior does not trigger.
- Penalty paths include `docs/**`, `src/tests/**`, `fixtures/**`.

**Step 2: Run to confirm fail**

Run: `cargo test --lib tests::tools::search::tantivy_path_prior_tests 2>&1 | tail -20`
Expected: FAIL because path-prior function absent.

**Step 3: Wire module**

Add `mod tantivy_path_prior_tests;` to `src/tests/tools/search/mod.rs`.

**Step 4: Re-run target test**

Run: `cargo test --lib tests::tools::search::tantivy_path_prior_tests 2>&1 | tail -20`
Expected: FAIL only in new tests.

**Step 5: Commit**

```bash
git add src/tests/tools/search/mod.rs src/tests/tools/search/tantivy_path_prior_tests.rs
git commit -m "test: add failing tests for NL-only path prior"
```

### Task 6: Implement NL-only path prior scoring

**Files:**
- Modify: `src/search/scoring.rs`
- Test: `src/tests/tools/search/tantivy_path_prior_tests.rs`

**Step 1: Implement small scoring adjustment**

Add pure function (example):

```rust
pub fn apply_nl_path_prior(results: &mut [SymbolSearchResult], query: &str) { /* ... */ }
```

Behavior:
- early return unless `is_nl_like_query(query)`
- modest multiplier for `src/`
- modest penalties for `docs/`, `src/tests/`, `fixtures/`
- re-sort descending by score

**Step 2: Keep prior conservative**

Document multiplier constants and rationale in comments near constants.

**Step 3: Run tests**

Run: `cargo test --lib tests::tools::search::tantivy_path_prior_tests 2>&1 | tail -20`
Expected: PASS.

**Step 4: Run existing scoring tests**

Run: `cargo test --lib tests::tools::search::tantivy_scoring_tests 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/search/scoring.rs src/tests/tools/search/tantivy_path_prior_tests.rs
git commit -m "feat: add conservative NL-only path prior scoring"
```

### Task 7: Integrate expansion + weighted queries + path prior into SearchIndex

**Files:**
- Modify: `src/search/index.rs`
- Modify (if needed): `src/search/query.rs`
- Modify (if needed): `src/search/scoring.rs`
- Test: `src/tests/tools/search/tantivy_integration_tests.rs`

**Step 1: Write failing integration test first**

Add test case that creates mixed docs/code corpus and queries NL phrase (for example: `"workspace routing"`), asserting top-K includes production symbol from `src/**`.

**Step 2: Run single test to confirm fail**

Run: `cargo test --lib tests::tools::search::tantivy_integration_tests::test_nl_query_prefers_code_over_docs 2>&1 | tail -20`
Expected: FAIL with current ranking.

**Step 3: Implement minimal integration**

In `search_symbols` and `search_content`:
- build expanded terms from query
- use weighted query builders
- keep AND->OR fallback logic
- apply existing boosts + NL path prior

**Step 4: Run integration tests**

Run: `cargo test --lib tests::tools::search::tantivy_integration_tests 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/search/index.rs src/tests/tools/search/tantivy_integration_tests.rs
git commit -m "feat: integrate query expansion into search index"
```

### Task 8: Add regression test for identifier-query stability

**Files:**
- Modify: `src/tests/tools/search/tantivy_variants_tests.rs`
- Modify or create: `src/tests/tools/search/quality.rs`

**Step 1: Write failing test**

Add case asserting exact identifier query (`get_reference_scores`-style) still ranks exact symbol/file near top.

**Step 2: Run to confirm fail (if regressed)**

Run: `cargo test --lib tests::tools::search::tantivy_variants_tests 2>&1 | tail -20`
Expected: either FAIL (if regression exists) or PASS (then keep as guard test).

**Step 3: Adjust thresholds/weights minimally**

Tune alias/path prior constants only if needed to satisfy both NL and identifier tests.

**Step 4: Re-run relevant module**

Run: `cargo test --lib tests::tools::search 2>&1 | tail -20`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/tests/tools/search/tantivy_variants_tests.rs src/tests/tools/search/quality.rs src/search/*.rs
git commit -m "test: guard identifier query relevance against NL expansion regressions"
```

### Task 9: Verify broader no-regression baseline

**Files:**
- Modify (if needed): `TODO.md`
- Optional docs note: `docs/SEARCH_FLOW.md`

**Step 1: Run focused search + get_context checks**

Run:
- `cargo test --lib tests::tools::search 2>&1 | tail -20`
- `cargo test --lib tests::tools::get_context 2>&1 | tail -20`

Expected: both PASS.

**Step 2: Run fast-tier suite**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -20`
Expected: PASS summary.

**Step 3: Update docs/todo notes**

Document what shipped and remaining gaps (if any) in `TODO.md` and/or `docs/SEARCH_FLOW.md`.

**Step 4: Sanity-check git diff scope**

Run: `git status -s` and `git diff --stat`
Expected: only intended search/tests/docs changes.

**Step 5: Commit**

```bash
git add src/search/*.rs src/tests/tools/search/*.rs TODO.md docs/SEARCH_FLOW.md
git commit -m "chore: document and verify deterministic NL relevance improvements"
```

## Notes for the implementing engineer

- Keep each implementation file under 500 lines.
- Keep each test file under 1000 lines.
- Prefer small, frequent commits per task.
- If any task reveals unexpected behavior, stop and run superpowers:systematic-debugging before continuing.
- Do not run full dogfood (`search_quality`) unless explicitly preparing merge gate.
