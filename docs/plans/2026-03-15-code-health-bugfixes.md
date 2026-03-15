# Code Health Intelligence — Bug Fixes & Quality Improvements

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 7 validated bugs and 3 tech debt items from the Code Health Intelligence review.

**Architecture:** All fixes are localized — no schema changes, no new modules. Most are 1-5 line changes with corresponding test additions. The bugs fall into three categories: output formatting (double-print, wrong format), search correctness (truncation order, missing parameter threading), and metadata accuracy (empty labels, mislabeled counts).

**Tech Stack:** Rust, Tantivy, SQLite, tree-sitter extractors

**Validated findings source:** GPT deep review of Code Health Intelligence work (2026-03-14/15), validated by code inspection + dogfooding.

---

## Chunk 1: Output Formatting Fixes (Bugs 6, 8, 7)

These are the easiest wins — each fix is 1-5 lines and immediately improves output quality.

---

### Task 1: Fix Change Risk double-print in deep_dive (Bug 6)

**Problem:** `format_header()` calls `format_change_risk_info()`, and then every kind-specific formatter (`format_callable`, `format_class_or_struct`, etc.) calls it again. Change Risk appears twice in ALL deep_dive output. (Security Risk is correctly only in kind-specific formatters.)

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs:69` — remove the `format_change_risk_info` call from `format_header`
- Test: `src/tests/tools/deep_dive_tests.rs` — add regression test

- [ ] **Step 1: Write failing test**

Add to `src/tests/tools/deep_dive_tests.rs`:

```rust
#[test]
fn test_change_risk_not_duplicated_in_output() {
    // Build a callable symbol context with change_risk metadata
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("change_risk".to_string(), serde_json::json!({
        "score": 0.75,
        "label": "HIGH",
        "factors": {
            "centrality": 0.6,
            "visibility": "public",
            "kind": "function",
            "test_weakness": 0.8
        }
    }));

    let symbol = Symbol {
        id: "test-id".to_string(),
        name: "process_payment".to_string(),
        kind: SymbolKind::Function,
        file_path: "src/payments.rs".to_string(),
        start_line: 42,
        end_line: 80,
        language: "rust".to_string(),
        signature: Some("pub fn process_payment(amount: f64) -> Result<()>".to_string()),
        visibility: Some(crate::extractors::base::Visibility::Public),
        metadata: Some(metadata),
        code_context: Some("fn process_payment(amount: f64) -> Result<()> { Ok(()) }".to_string()),
        ..Default::default()
    };

    let ctx = SymbolContext {
        symbol,
        incoming: vec![],
        incoming_total: 5,
        outgoing: vec![],
        outgoing_total: 2,
        children: vec![],
        implementations: vec![],
        test_refs: vec![],
        similar: vec![],
    };

    let output = format_symbol_context(&ctx, "context");
    let change_risk_count = output.matches("Change Risk:").count();
    assert_eq!(change_risk_count, 1, "Change Risk should appear exactly once, but appeared {} times in:\n{}", change_risk_count, output);
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test --lib test_change_risk_not_duplicated -- --nocapture 2>&1 | tail -20`
Expected: FAIL — "Change Risk should appear exactly once, but appeared 2 times"

- [ ] **Step 3: Fix — remove change_risk call from format_header**

In `src/tools/deep_dive/formatting.rs`, line 69, remove:
```rust
    format_change_risk_info(out, s, ctx.incoming_total);
```

The `format_header` function should end with just `format_test_quality_info(out, s);` before the closing brace.

- [ ] **Step 4: Run test, verify it passes**

Run: `cargo test --lib test_change_risk_not_duplicated -- --nocapture 2>&1 | tail -5`
Expected: PASS

- [ ] **Step 5: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: No new failures

- [ ] **Step 6: Commit**

```bash
git add src/tools/deep_dive/formatting.rs src/tests/tools/deep_dive_tests.rs
git commit -m "fix(deep_dive): remove duplicate Change Risk from format_header

format_header called format_change_risk_info, and every kind-specific
formatter also called it, causing Change Risk to appear twice in all
deep_dive output."
```

---

### Task 2: Fix definition search truncation before exclude_tests (Bug 8)

**Problem:** In `definition_search_with_index`, `.truncate(limit)` runs BEFORE `filter_test_symbols()`. Test symbols consume top-N slots, get removed after truncation, leaving underfilled results.

**Files:**
- Modify: `src/tools/search/text_search.rs:273,316` — move `filter_test_symbols` before `truncate`
- Test: `src/tests/tools/search/` — add regression test

- [ ] **Step 1: Write failing test**

Add a test that verifies when `exclude_tests=true`, the result count equals `limit` even when test symbols exist in the top results. The test should create symbols where some are marked `is_test=true`, run definition search with exclude_tests, and verify the returned count equals the limit (not limit minus filtered tests).

Note: This may need to be an integration-level test using the fixture DB, since `definition_search_with_index` is a private function. If testing through the public API is too complex, a unit test that calls the function directly with `pub(crate)` visibility is acceptable.

- [ ] **Step 2: Run test, verify it fails**

Expected: FAIL — fewer results than limit because test symbols consumed slots

- [ ] **Step 3: Fix — move filter before truncate in both paths**

In `src/tools/search/text_search.rs`, **hybrid path** (around line 273):

Change from:
```rust
        promote_exact_name_matches(&mut hybrid_results.results, query);
        hybrid_results.results.truncate(limit);

        let mut symbols: Vec<Symbol> = hybrid_results
            .results
            .into_iter()
            .map(tantivy_symbol_to_symbol)
            .collect();
        enrich_symbols_from_db(&mut symbols, db);

        // Filter out test symbols when exclude_tests is set
        filter_test_symbols(&mut symbols, filter.exclude_tests);
```

To:
```rust
        promote_exact_name_matches(&mut hybrid_results.results, query);

        let mut symbols: Vec<Symbol> = hybrid_results
            .results
            .into_iter()
            .map(tantivy_symbol_to_symbol)
            .collect();
        enrich_symbols_from_db(&mut symbols, db);

        // Filter BEFORE truncating so test symbols don't consume limit slots
        filter_test_symbols(&mut symbols, filter.exclude_tests);
        symbols.truncate(limit);
```

**Keyword path** (around line 315-327):

Change from:
```rust
        promote_exact_name_matches(&mut filtered_results, query);
        filtered_results.truncate(limit);

        let mut symbols: Vec<Symbol> = filtered_results
            .into_iter()
            .map(tantivy_symbol_to_symbol)
            .collect();
        if let Some(db) = db {
            enrich_symbols_from_db(&mut symbols, db);
        }

        // Filter out test symbols when exclude_tests is set
        filter_test_symbols(&mut symbols, filter.exclude_tests);
```

To:
```rust
        promote_exact_name_matches(&mut filtered_results, query);

        let mut symbols: Vec<Symbol> = filtered_results
            .into_iter()
            .map(tantivy_symbol_to_symbol)
            .collect();
        if let Some(db) = db {
            enrich_symbols_from_db(&mut symbols, db);
        }

        // Filter BEFORE truncating so test symbols don't consume limit slots
        filter_test_symbols(&mut symbols, filter.exclude_tests);
        symbols.truncate(limit);
```

- [ ] **Step 4: Run test, verify it passes**

- [ ] **Step 5: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add src/tools/search/text_search.rs src/tests/tools/search/
git commit -m "fix(search): filter test symbols before truncating results

truncate(limit) ran before filter_test_symbols, so test symbols could
consume top-N slots and leave underfilled production results."
```

---

### Task 3: Fix get_context no-results format (Bug 7)

**Problem:** `run_pipeline` returns early with a boxed `═══` readable format when no results are found, ignoring the `format` parameter. Both `format_context_compact` and `format_context_readable` already handle empty pivots correctly.

**Files:**
- Modify: `src/tools/get_context/pipeline.rs:155-166` — route through formatter instead of early return
- Test: `src/tests/tools/get_context_formatting_tests.rs` — add regression test

- [ ] **Step 1: Write failing test**

Add to the get_context formatting tests:

```rust
#[test]
fn test_no_results_respects_compact_format() {
    let data = ContextData {
        query: "nonexistent_query".to_string(),
        pivots: vec![],
        neighbors: vec![],
        allocation: super::super::allocation::TokenBudget::new(2000).allocate(0, 0),
    };

    let output = format_context_with_mode(&data, OutputFormat::Compact);
    // Compact format should NOT use ═══ borders
    assert!(!output.contains("═══"), "Compact no-results should not use readable borders, got:\n{}", output);
    // Should use the compact format
    assert!(output.contains("no relevant symbols"), "Should contain no-results message");
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test --lib test_no_results_respects_compact_format -- --nocapture 2>&1 | tail -20`
Expected: FAIL — the test currently can't fail because the early return in `run_pipeline` means `format_context_with_mode` is never called with empty pivots from the pipeline. But this test validates the formatter behavior is correct. The real fix is in step 3.

Actually, the formatter test above should PASS (the formatters already handle it). The bug is that `run_pipeline` never reaches the formatter. Write a pipeline-level test instead:

```rust
#[test]
fn test_run_pipeline_no_results_respects_format() {
    // This test needs a real SearchIndex + DB with no matching results.
    // Use the fixture DB and search for a nonsense query.
    let db = /* load fixture db */;
    let index = /* load fixture index */;
    let result = run_pipeline(
        "xyzzy_nonexistent_gibberish_query_12345",
        None, None, None,
        Some("compact".to_string()),
        &db, &index, None,
    ).unwrap();

    assert!(!result.contains("═══"), "Compact format should not have readable borders");
}
```

- [ ] **Step 3: Fix — replace early return with ContextData flow-through**

In `src/tools/get_context/pipeline.rs`, replace the early return (lines 155-166):

```rust
    if search_results.results.is_empty() {
        return Ok(format!(
            "\u{2550}\u{2550}\u{2550} Context: \"{}\" \u{2550}\u{2550}\u{2550}\nNo relevant symbols found.\n\
            💡 Try fast_search(query=\"{}\") for exact matches, or verify the workspace is indexed",
            query, query
        ));
    }
```

With:
```rust
    if search_results.results.is_empty() {
        let empty_data = ContextData {
            query: query.to_string(),
            pivots: vec![],
            neighbors: vec![],
            allocation: TokenBudget::new(0).allocate(0, 0),
        };
        return Ok(format_context_with_mode(
            &empty_data,
            super::formatting::OutputFormat::from_option(format.as_deref()),
        ));
    }
```

Note: `TokenBudget` and `ContextData`/`format_context_with_mode` are already imported in this function's scope (lines 138-139).

- [ ] **Step 4: Run test, verify it passes**

- [ ] **Step 5: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add src/tools/get_context/pipeline.rs src/tests/tools/
git commit -m "fix(get_context): no-results output now respects format parameter

run_pipeline returned a hardcoded readable format on empty results,
ignoring the format parameter. Now routes through format_context_with_mode
which already handles empty pivots in both compact and readable modes."
```

---

## Chunk 2: Metadata & Label Accuracy (Bugs 5, 9)

---

### Task 4: Fix get_context SignatureOnly dropping risk/security labels (Bug 5)

**Problem:** `fetch_pivot_batch_data` sets `full_symbols = HashMap::new()` when `pivot_mode` is `SignatureOnly`. Labels are extracted from `full_symbols` in `build_pivot_entries`, so they're always `None` in SignatureOnly mode. The code_context body isn't used in SignatureOnly anyway — only the metadata matters.

**Files:**
- Modify: `src/tools/get_context/pipeline.rs:241-245` — always fetch full_symbols
- Test: `src/tests/tools/get_context_formatting_tests.rs` — add regression test

- [ ] **Step 1: Write failing test**

Write a test that builds a `ContextData` with a pivot that has `risk_label` and `security_label` set, formats it in compact mode with a SignatureOnly allocation, and verifies labels appear. This tests the formatting path.

For the pipeline path, write a test that calls `run_pipeline` with a high `max_tokens` budget that triggers SignatureOnly mode, and verifies labels appear on pivots that have change_risk/security_risk metadata.

- [ ] **Step 2: Run test, verify it fails**

Expected: FAIL — labels are None because full_symbols is empty in SignatureOnly

- [ ] **Step 3: Fix — always fetch full_symbols**

In `src/tools/get_context/pipeline.rs`, replace lines 241-247:

```rust
    // 1. Full symbol bodies (skip if we only need signatures)
    let full_symbols: HashMap<String, Symbol> = if matches!(pivot_mode, PivotMode::SignatureOnly) {
        HashMap::new()
    } else {
        db.get_symbols_by_ids(pivot_ids)?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };
```

With:
```rust
    // Always fetch full symbols — even in SignatureOnly mode we need metadata
    // for risk/security labels. The code_context body is only used in
    // FullBody/SignatureAndKey modes (handled by build_pivot_entries).
    let full_symbols: HashMap<String, Symbol> = db.get_symbols_by_ids(pivot_ids)?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
```

- [ ] **Step 4: Run test, verify it passes**

- [ ] **Step 5: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`

- [ ] **Step 6: Commit**

```bash
git add src/tools/get_context/pipeline.rs src/tests/tools/
git commit -m "fix(get_context): always fetch full_symbols for risk/security labels

SignatureOnly mode skipped full_symbols fetch, but labels are extracted
from symbol metadata in full_symbols. Code bodies are not used in
SignatureOnly — only metadata matters."
```

---

### Task 5: Fix deep_dive caller/blast-radius count mislabeling (Bug 9)

**Problem:** `incoming_total` includes ALL incoming relationship types (calls, uses, references, type_usage, member_access) PLUS identifier fallback refs. But `format_change_risk_info` labels it as `"{} callers"` and `"{} direct callers"`, and `format_security_risk_info` labels it as `"{} callers"`. This misrepresents the count.

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs:224,232-233,293` — relabel to "dependents"/"incoming refs"
- Test: `src/tests/tools/deep_dive_tests.rs` — update any tests checking these labels

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn test_change_risk_labels_say_dependents_not_callers() {
    // Build a symbol with change_risk metadata
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("change_risk".to_string(), serde_json::json!({
        "score": 0.65,
        "label": "MEDIUM",
        "factors": {
            "centrality": 0.5,
            "visibility": "public",
            "kind": "function",
            "test_weakness": 0.3
        }
    }));

    let symbol = Symbol {
        id: "test-id".to_string(),
        name: "my_func".to_string(),
        kind: SymbolKind::Function,
        file_path: "src/lib.rs".to_string(),
        start_line: 10,
        end_line: 20,
        language: "rust".to_string(),
        visibility: Some(crate::extractors::base::Visibility::Public),
        metadata: Some(metadata),
        ..Default::default()
    };

    let mut output = String::new();
    format_change_risk_info(&mut output, &symbol, 8);

    // Should NOT say "callers" — incoming_total includes all ref types
    assert!(!output.contains("callers"), "Should not use 'callers' label for mixed incoming refs, got:\n{}", output);
    // Should say "dependents" or "incoming refs"
    assert!(output.contains("dependents"), "Should label as 'dependents', got:\n{}", output);
}
```

- [ ] **Step 2: Run test, verify it fails**

Expected: FAIL — currently says "callers"

- [ ] **Step 3: Fix — relabel in both formatting functions**

In `src/tools/deep_dive/formatting.rs`:

**`format_change_risk_info`** — change the summary line (around line 224):
```rust
// Before:
"\nChange Risk: {} ({:.2}) — {} callers, {}, {}\n"
// After:
"\nChange Risk: {} ({:.2}) — {} dependents, {}, {}\n"
```

And the detail line (around line 232):
```rust
// Before:
"  centrality: {:.2} ({} direct callers)\n"
// After:
"  centrality: {:.2} ({} incoming refs)\n"
```

**`format_security_risk_info`** — change the blast radius line (around line 293):
```rust
// Before: uses "callers" in the blast radius detail
// After: use "dependents" or "incoming refs"
```

Check exact wording in `format_security_risk_info` and update similarly.

- [ ] **Step 4: Run test, verify it passes**

- [ ] **Step 5: Run dev tier, fix any tests that assert old label text**

Run: `cargo xtask test dev 2>&1 | tail -20`
If tests fail due to hardcoded "callers" assertions, update them to "dependents".

- [ ] **Step 6: Commit**

```bash
git add src/tools/deep_dive/formatting.rs src/tests/tools/deep_dive_tests.rs
git commit -m "fix(deep_dive): relabel caller counts as dependents

incoming_total includes all relationship types (calls, uses, type_usage,
member_access) plus identifier fallback, not just callers. Relabel to
'dependents' for accuracy."
```

---

## Chunk 3: Search Parameter Threading (Bug 3)

---

### Task 6: Thread exclude_tests through line_mode_search (Bug 3)

**Problem:** `line_mode_search()` hardcodes `exclude_tests: false` in its `SearchFilter` (lines 56 and 168 of `line_mode.rs`). The `FastSearchTool` has `exclude_tests` on its struct but never passes it to `line_mode_search`. Content searches can't filter test results.

Since line_mode operates on file-level text (not symbols), the best we can do is filter out matches from test files using path-based heuristics (same approach as `is_test_path`).

**Files:**
- Modify: `src/tools/search/line_mode.rs:24` — add `exclude_tests` parameter
- Modify: `src/tools/search/mod.rs:128` — pass `self.exclude_tests` to line_mode_search
- Test: `src/tests/tools/search/line_mode.rs` — add regression test

- [ ] **Step 1: Write failing test**

Add a test that calls `line_mode_search` with `exclude_tests=true` and verifies that results from test files (paths matching test patterns) are excluded.

- [ ] **Step 2: Run test, verify it fails**

- [ ] **Step 3: Add exclude_tests parameter to line_mode_search**

In `src/tools/search/line_mode.rs`, add `exclude_tests: Option<bool>` parameter:

```rust
pub async fn line_mode_search(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    exclude_tests: Option<bool>,  // NEW
    workspace_target: &WorkspaceTarget,
    handler: &JulieServerHandler,
) -> Result<CallToolResult> {
```

Resolve the smart default early:
```rust
    let exclude_test_files = exclude_tests.unwrap_or(false);
```

Add a filter in the line match collection loop (after the language filter, before content retrieval):
```rust
    // Skip test files when exclude_tests is set
    if exclude_test_files && crate::search::scoring::is_test_path(&file_result.file_path) {
        continue;
    }
```

Apply the same filter in the reference workspace path and in the post-filter section.

Update both `SearchFilter` constructions to pass the resolved value:
```rust
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
        exclude_tests: exclude_test_files,
    };
```

- [ ] **Step 4: Update the call site in mod.rs**

In `src/tools/search/mod.rs:128`, pass `self.exclude_tests`:

```rust
        if use_line_mode {
            return line_mode::line_mode_search(
                &self.query,
                &self.language,
                &self.file_pattern,
                self.limit,
                self.exclude_tests,  // NEW
                &workspace_target,
                handler,
            )
            .await;
        }
```

- [ ] **Step 5: Run test, verify it passes**

- [ ] **Step 6: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`

- [ ] **Step 7: Commit**

```bash
git add src/tools/search/line_mode.rs src/tools/search/mod.rs src/tests/tools/search/
git commit -m "fix(search): thread exclude_tests through line_mode_search

line_mode_search hardcoded exclude_tests: false, ignoring the parameter
from FastSearchTool. Now accepts and applies the filter using
is_test_path for file-level filtering."
```

---

## Chunk 4: Data Quality Improvements (Bug 4, Tech Debt)

Lower priority — address after the bugs above are fixed.

---

### Task 7: Add language filter to test coverage name-match fallback (Bug 4)

**Problem:** Step 2b (identifier name-match fallback) in `compute_test_coverage` joins `s_prod.name = i.name` without filtering by language. In polyglot repos, a Python test could link to a Rust production symbol with the same name.

**Files:**
- Modify: `src/analysis/test_coverage.rs` — add `AND s_test.language = s_prod.language` to Step 2b SQL
- Test: `src/tests/analysis/` — add polyglot test case

- [ ] **Step 1: Write failing test**

Create a test with symbols in different languages sharing the same name (e.g., `process` in both Python and Rust). Verify that a Python test only links to the Python production symbol.

- [ ] **Step 2: Run test, verify it fails**

- [ ] **Step 3: Fix — add language filter to SQL**

In `src/analysis/test_coverage.rs`, the Step 2b query (around line 116), add:
```sql
AND s_test.language = s_prod.language
```

After the existing `AND s_prod.kind NOT IN (...)` clause.

- [ ] **Step 4: Run test, verify it passes**

- [ ] **Step 5: Run dev tier**

- [ ] **Step 6: Commit**

```bash
git add src/analysis/test_coverage.rs src/tests/analysis/
git commit -m "fix(test_coverage): add language filter to name-match fallback

The identifier name-match fallback could link a Python test to a Rust
symbol with the same name in polyglot repos."
```

---

### Task 8: Add Go Fuzz/Example test detection (Tech Debt)

**Problem:** `detect_go()` only recognizes `TestXxx` entry points. Go also uses `FuzzXxx` and `ExampleXxx` as test entry points in `_test.go` files.

**Files:**
- Modify: `crates/julie-extractors/src/test_detection.rs:128-132` — add Fuzz/Example
- Test: existing test file for test_detection

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn test_go_fuzz_detected() {
    assert!(is_test_symbol("go", "FuzzParseInput", "parser_test.go", &SymbolKind::Function, &[], &[], None));
}

#[test]
fn test_go_example_detected() {
    assert!(is_test_symbol("go", "ExampleParseInput", "parser_test.go", &SymbolKind::Function, &[], &[], None));
}
```

- [ ] **Step 2: Run tests, verify they fail**

- [ ] **Step 3: Fix detect_go**

```rust
fn detect_go(name: &str, file_path: &str) -> bool {
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    (name.starts_with("Test") || name.starts_with("Fuzz") || name.starts_with("Example"))
        && file_name.ends_with("_test.go")
}
```

- [ ] **Step 4: Run tests, verify they pass**

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/test_detection.rs
git commit -m "feat(test_detection): detect Go Fuzz and Example test entry points

detect_go only recognized TestXxx. Go also uses FuzzXxx and ExampleXxx
as test entry points in _test.go files."
```

---

### Task 9: Cap and deduplicate deep_dive test locations (Tech Debt)

**Problem:** `build_test_refs()` returns unbounded results from raw name matching. For common symbol names, this can produce many noisy entries.

**Files:**
- Modify: `src/tools/deep_dive/data.rs:254-288` — add dedup + cap
- Test: `src/tests/tools/deep_dive_tests.rs`

- [ ] **Step 1: Write failing test**

Write a test that creates many duplicate test refs (same file, different lines) and verifies the output is capped and deduped.

- [ ] **Step 2: Run test, verify it fails**

- [ ] **Step 3: Fix — add dedup by file + cap**

In `build_test_refs()`, before returning:

```rust
    // Deduplicate by (file_path, containing_symbol name) — keep first occurrence
    let mut seen = HashSet::new();
    test_refs.retain(|r| {
        let key = (
            r.file_path.clone(),
            r.symbol.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
        );
        seen.insert(key)
    });

    // Cap at 10 to prevent output bloat
    test_refs.truncate(10);

    Ok(test_refs)
```

- [ ] **Step 4: Run test, verify it passes**

- [ ] **Step 5: Run dev tier**

- [ ] **Step 6: Commit**

```bash
git add src/tools/deep_dive/data.rs src/tests/tools/deep_dive_tests.rs
git commit -m "fix(deep_dive): cap and deduplicate test locations

build_test_refs returned unbounded raw name matches. Now deduplicates
by (file_path, symbol_name) and caps at 10 entries."
```

---

## Items NOT in this plan (tracked in TODO.md)

These are valid findings but lower priority or require separate design work:

- **Watcher doesn't respect .gitignore** — design debt, needs `ignore` crate integration at event-filter level
- **Test quality regexes match comments/strings** — needs AST-aware analysis, significant rework
- **Deep-dive test-location lookup not linkage-based** — future improvement to use test_coverage data instead of raw name matching
- **Cross-language test detection normalization** — broader audit of all 31 extractors
- **Missing regression test coverage** — addressed partially by tests in this plan
- **Windows Python launcher probing** — enhancement, not a bug
- **Embedding KNN smoke test** — pre-existing
- **workspace_init pathological** — pre-existing

---

## Execution Order

1. **Chunk 1** (Tasks 1-3): Quick wins, immediate output quality improvement
2. **Chunk 2** (Tasks 4-5): Metadata accuracy
3. **Chunk 3** (Task 6): Search parameter threading
4. **Chunk 4** (Tasks 7-9): Data quality — can be done in any order

Tasks within each chunk are independent and can be parallelized with subagents.
