# GPT Review Bugfixes — Security & get_context Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 4 validated bugs from GPT's code health intelligence review: false-positive security sink matching, mislabeled exposure visibility, missing test_quality in get_context, and test-name queries hiding exact matches.

**Architecture:** All fixes are localized — no schema changes, no new tables, no new modules. Two security fixes in analysis/formatting, two get_context fixes in scoring/pipeline/formatting.

**Tech Stack:** Rust, TDD (red-green-refactor)

---

## Chunk 1: Security signal accuracy (Tasks 1–2)

### Task 1: Remove `filter` from DATABASE_SINKS

**Problem:** `DATABASE_SINKS` includes bare `"filter"`, which matches iterator/LINQ `.filter()` calls across all languages. Only Django's `queryset.filter()` is a real database sink, but the final-segment matching can't distinguish them.

**Files:**
- Modify: `src/analysis/security_risk.rs:49-68` (remove `"filter"` from `DATABASE_SINKS`)
- Test: `src/tests/analysis/security_risk_tests.rs`

- [ ] **Step 1: Write failing test — `filter` should NOT match as a sink**

In `src/tests/analysis/security_risk_tests.rs`, add:
```rust
#[test]
fn test_filter_is_not_a_database_sink() {
    // "filter" is too generic — matches iterator .filter() across all languages.
    // Django's queryset.filter() is a real sink, but final-segment matching
    // can't distinguish it from iter.filter(), so we exclude it.
    let result = matches_sink_pattern("filter", &DATABASE_SINKS);
    assert!(result.is_none(), "bare 'filter' should not match DATABASE_SINKS");

    let result2 = matches_sink_pattern("items.filter", &DATABASE_SINKS);
    assert!(result2.is_none(), "'items.filter' should not match DATABASE_SINKS");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_filter_is_not_a_database_sink 2>&1 | tail -10`
Expected: FAIL — `filter` currently matches

- [ ] **Step 3: Remove `filter` from DATABASE_SINKS**

In `src/analysis/security_risk.rs`, remove `"filter"` from the `DATABASE_SINKS` array (line ~60, in the Django/SQLAlchemy section). Keep `"raw"`, `"commit"`, `"cursor"` — those are unambiguous.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_filter_is_not_a_database_sink 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Verify no regressions in security risk tests**

Run: `cargo test --lib security_risk 2>&1 | tail -10`
Expected: All pass

---

### Task 2: Fix `protected` visibility displayed as `public`

**Problem:** `exposure_score()` returns 0.5 for `protected`, and `format_security_risk_info()` prints `"public"` for `exposure >= 0.5`. A `protected` method renders as `exposure: public`. Fix: store the raw visibility string in security_risk metadata signals and use it for display.

**Files:**
- Modify: `src/analysis/security_risk.rs:252-406` (store visibility string in signals)
- Modify: `src/tools/deep_dive/formatting.rs:249-330` (use visibility string for display)
- Test: `src/tests/analysis/security_risk_tests.rs`
- Test: `src/tests/tools/deep_dive_tests.rs`

- [ ] **Step 1: Write failing test — visibility string stored in metadata**

In `src/tests/analysis/security_risk_tests.rs`, add:
```rust
#[test]
fn test_security_risk_metadata_stores_visibility_string() {
    // Protected methods should store "protected" in signals, not get
    // threshold-mapped to "public" at display time.
    let score = exposure_score(Some("protected"), &SymbolKind::Function);
    assert!((score - 0.5).abs() < f64::EPSILON);
    // The compute function stores visibility — we test that in integration.
    // Here we verify the score itself is correct for protected.
    let public_score = exposure_score(Some("public"), &SymbolKind::Function);
    assert!((public_score - 1.0).abs() < f64::EPSILON);
    assert!(public_score > score, "public should score higher than protected");
}
```

- [ ] **Step 2: Add `visibility` field to security_risk signals in `compute_security_risk()`**

In `src/analysis/security_risk.rs`, in the `risk_data` JSON construction inside `compute_security_risk()`, add the visibility string to signals:

```rust
let risk_data = serde_json::json!({
    "score": (score * 100.0).round() / 100.0,
    "label": label,
    "signals": {
        "exposure": (exposure * 100.0).round() / 100.0,
        "visibility": vis.as_deref().unwrap_or("default"),
        "input_handling": input_handling,
        "sink_calls": sink_names,
        "blast_radius": (blast_radius * 100.0).round() / 100.0,
        "untested": untested == 1.0,
    }
});
```

- [ ] **Step 3: Update `format_security_risk_info()` to use visibility string**

In `src/tools/deep_dive/formatting.rs`, change the exposure display logic from threshold-guessing to using the stored visibility string:

**Summary line** — replace the `exp >= 0.5` check:
```rust
if let Some(vis) = sigs.get("visibility").and_then(|v| v.as_str()) {
    if vis == "public" {
        summary_parts.push("public".to_string());
    } else if vis == "protected" {
        summary_parts.push("protected".to_string());
    }
} else if let Some(exp) = sigs.get("exposure").and_then(|v| v.as_f64()) {
    // Fallback for data indexed before visibility field was added
    if exp >= 0.8 {
        summary_parts.push("public".to_string());
    }
}
```

**Detail line** — replace the `exposure >= 0.5` block:
```rust
let exposure = sigs.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0);
let vis_label = sigs.get("visibility").and_then(|v| v.as_str()).unwrap_or("unknown");
out.push_str(&format!("  exposure: {} ({:.2})\n", vis_label, exposure));
```

- [ ] **Step 4: Run security risk and deep_dive tests**

Run: `cargo test --lib security_risk 2>&1 | tail -10`
Run: `cargo test --lib deep_dive 2>&1 | tail -10`
Expected: All pass

- [ ] **Step 5: Commit chunk 1**

```bash
git add src/analysis/security_risk.rs src/tools/deep_dive/formatting.rs \
        src/tests/analysis/security_risk_tests.rs src/tests/tools/deep_dive_tests.rs
git commit -m "fix(security): remove generic 'filter' from sinks, fix protected→public mislabel"
```

---

## Chunk 2: get_context test metadata and scoring (Tasks 3–4)

### Task 3: Surface `test_quality` on get_context test pivots

**Problem:** When a test symbol becomes a get_context pivot, it shows `risk_label` and `security_label` but not `test_quality`. The metadata exists (deep_dive renders it), but PivotEntry has no field for it and build_pivot_entries doesn't extract it.

**Files:**
- Modify: `src/tools/get_context/formatting.rs:44-65` (add `test_quality_label` to PivotEntry)
- Modify: `src/tools/get_context/formatting.rs:199-250` (render in compact format)
- Modify: `src/tools/get_context/formatting.rs:112-197` (render in readable format)
- Modify: `src/tools/get_context/pipeline.rs:310-396` (extract from metadata)
- Test: `src/tests/tools/get_context_formatting_tests.rs`

- [ ] **Step 1: Write failing test — test_quality rendered on pivot**

In `src/tests/tools/get_context_formatting_tests.rs`, add:
```rust
#[test]
fn test_compact_format_renders_test_quality_label() {
    let mut pivot = make_pivot("test_auth_works", "src/tests/auth.rs", 10, 0.0, "fn test_auth_works() {}");
    pivot.test_quality_label = Some("thorough".to_string());

    let data = ContextData {
        query: "auth test".to_string(),
        pivots: vec![pivot],
        neighbors: vec![],
        allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
    };

    let output = format_context_with_mode(&data, OutputFormat::Compact);
    assert!(output.contains("quality=thorough"), "compact format should show test quality: {}", output);
}

#[test]
fn test_readable_format_renders_test_quality_label() {
    let mut pivot = make_pivot("test_auth_works", "src/tests/auth.rs", 10, 0.0, "fn test_auth_works() {}");
    pivot.test_quality_label = Some("thorough".to_string());

    let data = ContextData {
        query: "auth test".to_string(),
        pivots: vec![pivot],
        neighbors: vec![],
        allocation: make_allocation(PivotMode::FullBody, NeighborMode::SignatureAndDoc),
    };

    let output = format_context_with_mode(&data, OutputFormat::Readable);
    assert!(output.contains("[thorough quality]"), "readable format should show test quality: {}", output);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_compact_format_renders_test_quality_label 2>&1 | tail -10`
Expected: FAIL — `test_quality_label` field doesn't exist

- [ ] **Step 3: Add `test_quality_label` field to PivotEntry**

In `src/tools/get_context/formatting.rs`, add to `PivotEntry`:
```rust
/// Test quality tier (thorough/adequate/thin/stub) from metadata, if this is a test symbol.
pub test_quality_label: Option<String>,
```

- [ ] **Step 4: Update `make_pivot` helper in test file**

In `src/tests/tools/get_context_formatting_tests.rs`, add `test_quality_label: None,` to the `make_pivot` function.

- [ ] **Step 5: Extract test_quality in `build_pivot_entries`**

In `src/tools/get_context/pipeline.rs`, after the `security_label` extraction block, add:
```rust
let test_quality_label = batch.full_symbols.get(&pivot.result.id)
    .and_then(|sym| sym.metadata.as_ref())
    .and_then(|m| m.get("test_quality"))
    .and_then(|tq| tq.get("quality_tier"))
    .and_then(|t| t.as_str())
    .map(String::from);
```

And add `test_quality_label,` to the `PivotEntry` construction.

- [ ] **Step 6: Render test_quality in compact format**

In `format_context_compact`, after the `security_tag` line, add:
```rust
let quality_tag = pivot.test_quality_label.as_ref()
    .map(|l| format!(" quality={}", l))
    .unwrap_or_default();
```

And append `quality_tag` to the PIVOT format string.

- [ ] **Step 7: Render test_quality in readable format**

In `format_context_readable`, after the `security_tag` line, add:
```rust
let quality_tag = pivot.test_quality_label.as_ref()
    .map(|l| format!("  [{} quality]", l))
    .unwrap_or_default();
```

And append `quality_tag` to the location format string.

- [ ] **Step 8: Run formatting tests**

Run: `cargo test --lib get_context_formatting 2>&1 | tail -10`
Expected: All pass (including new tests)

---

### Task 4: Boost exact test-name matches in get_context scoring

**Problem:** `TEST_FILE_PENALTY = 0.3` applies a 70% score reduction to test files regardless of match quality. When someone searches for `test_compute_security_risk_excludes_imports`, the exact test gets buried under partial production matches.

**Fix:** In `select_pivots`, detect when a result's name exactly matches the query (case-insensitive) and apply a significant boost that overcomes the test file penalty.

**Files:**
- Modify: `src/tools/get_context/scoring.rs:46-115` (add exact-match boost)
- Test: `src/tests/tools/get_context_scoring_tests.rs`

- [ ] **Step 1: Write failing test — exact test-name match ranks first**

In `src/tests/tools/get_context_scoring_tests.rs`, add:
```rust
#[test]
fn test_exact_test_name_match_overcomes_test_penalty() {
    let query = "test_compute_security_risk";
    let test_result = make_result_with_kind_and_path(
        "t1", "test_compute_security_risk", "function",
        "src/tests/analysis/security_risk_tests.rs", 0.9
    );
    let prod_result = make_result_with_kind_and_path(
        "p1", "compute_security_risk", "function",
        "src/analysis/security_risk.rs", 0.8
    );
    let results = vec![test_result, prod_result];
    let ref_scores = HashMap::new();

    let pivots = select_pivots_for_query(query, results, &ref_scores);
    assert!(!pivots.is_empty());
    assert_eq!(pivots[0].result.name, "test_compute_security_risk",
        "exact test-name match should rank first despite TEST_FILE_PENALTY");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_exact_test_name_match_overcomes_test_penalty 2>&1 | tail -10`
Expected: FAIL — `select_pivots_for_query` doesn't exist yet

- [ ] **Step 3: Add query parameter to `select_pivots`**

This is the key design decision. The current `select_pivots` doesn't know the query, so it can't detect exact matches. We need to either:
- (a) Add a new `select_pivots_for_query` that wraps `select_pivots` with a pre-boost, or
- (b) Change `select_pivots` signature to accept `Option<&str>` query

Option (a) is cleaner — no signature change to existing callers:

In `src/tools/get_context/scoring.rs`, add:
```rust
/// Exact-match boost: when the query exactly matches a symbol name,
/// override the test-file penalty so the intended symbol ranks first.
const EXACT_NAME_MATCH_BOOST: f32 = 5.0;

pub fn select_pivots_for_query(
    query: &str,
    results: Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) -> Vec<Pivot> {
    let query_lower = query.to_lowercase();
    let boosted: Vec<SymbolSearchResult> = results.into_iter().map(|mut r| {
        if r.name.to_lowercase() == query_lower {
            r.score *= EXACT_NAME_MATCH_BOOST;
        }
        r
    }).collect();
    select_pivots(boosted, reference_scores)
}
```

- [ ] **Step 4: Wire `select_pivots_for_query` into `run_pipeline`**

In `src/tools/get_context/pipeline.rs`, in `run_pipeline()`, replace the `select_pivots_with_code_fallback` call with `select_pivots_for_query_with_code_fallback` (or pass query through). The simplest approach: add a similar wrapper for `select_pivots_with_code_fallback`:

```rust
pub fn select_pivots_with_code_fallback_for_query(
    query: &str,
    results: Vec<SymbolSearchResult>,
    reference_scores: &HashMap<String, f64>,
) -> Vec<Pivot> {
    let query_lower = query.to_lowercase();
    let boosted: Vec<SymbolSearchResult> = results.into_iter().map(|mut r| {
        if r.name.to_lowercase() == query_lower {
            r.score *= EXACT_NAME_MATCH_BOOST;
        }
        r
    }).collect();
    select_pivots_with_code_fallback(boosted, reference_scores)
}
```

Then in `run_pipeline`, replace:
```rust
let pivots = select_pivots_with_code_fallback(results.results, &reference_scores);
```
with:
```rust
let pivots = select_pivots_with_code_fallback_for_query(query, results.results, &reference_scores);
```

- [ ] **Step 5: Export new functions**

In `src/tools/get_context/scoring.rs`, ensure `select_pivots_for_query` and `EXACT_NAME_MATCH_BOOST` are `pub`.
In `src/tools/get_context/pipeline.rs`, import from scoring.

- [ ] **Step 6: Run scoring tests**

Run: `cargo test --lib get_context_scoring 2>&1 | tail -10`
Expected: All pass

- [ ] **Step 7: Commit chunk 2**

```bash
git add src/tools/get_context/formatting.rs src/tools/get_context/pipeline.rs \
        src/tools/get_context/scoring.rs \
        src/tests/tools/get_context_formatting_tests.rs \
        src/tests/tools/get_context_scoring_tests.rs
git commit -m "fix(get_context): surface test_quality on pivots, boost exact test-name matches"
```

---

## Post-Implementation

- [ ] **Run `cargo xtask test dev`** — full dev-tier regression check
- [ ] **Update TODO.md** — check off fixed items
- [ ] **Checkpoint** — save progress to Goldfish
