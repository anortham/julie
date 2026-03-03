---
id: token-efficiency-cleanup-across-julie-mcp-tool-for
title: Token Efficiency Cleanup Across Julie MCP Tool Formatting
status: active
created: 2026-03-03T21:44:55.859Z
updated: 2026-03-03T21:44:55.859Z
tags:
  - refactoring
  - token-efficiency
  - formatting
  - cleanup
---

# Token Efficiency Cleanup Across Julie MCP Tool Formatting

## Overview
Remove dead code, filler text, and token-wasteful formatting from Julie's MCP tool output. Every token in tool output consumes AI agent context window — this cleanup targets five areas of waste.

## Status: PLANNED

---

## Step 1: Remove dead OptimizedResponse machinery (SAFEST, LOWEST RISK)

### What
- Remove `insights: Option<String>` and `next_actions: Vec<String>` fields from `OptimizedResponse` struct
- Remove `with_insights()` and `with_next_actions()` methods
- Remove `generate_search_insights()` and `suggest_next_actions()` functions from scoring.rs
- Remove the calls in search/mod.rs lines 152-157
- Simplify `optimize_for_tokens()` — always called with `Some(limit)` so the confidence-based dynamic path is dead code. Remove the `Option` param; just take a `usize`.
- **Keep** `confidence` field — it's used by `calculate_search_confidence()` which feeds into the (now-simplified) `optimize_for_tokens`. It provides a meaningful signal even with an explicit limit because it gets stored in the struct. Actually wait — does anything read it after `optimize_for_tokens` runs? No. The formatting functions never read `.confidence`. So it's dead too. **Remove `confidence` field and `calculate_search_confidence()` entirely.**
- **Keep** `tool` field — while formatting.rs doesn't read it, `OptimizedResponse` derives `Serialize` and tests serialize it to JSON for comparison (lean_format_tests.rs:173). The `tool` field is also semantically correct for the struct identity. However, since we're only formatting as text (never outputting JSON), `tool` is also dead weight. **Remove `tool` field too.**

### Files to modify
- `src/tools/shared.rs` — Simplify struct to just `results: Vec<T>` and `total_found: usize`
- `src/tools/search/scoring.rs` — Delete `generate_search_insights()` and `suggest_next_actions()`. Delete `calculate_search_confidence()`.
- `src/tools/search/mod.rs` — Remove lines 149-159 (confidence calc, insights, next_actions, optimize_for_tokens). Replace with direct `.truncate(self.limit)`.
- `src/tools/search/formatting.rs` — Update `OptimizedResponse<Symbol>` parameter types (struct changed)
- `src/tests/tools/search/lean_format_tests.rs` — Remove `insights`, `next_actions`, `tool`, `confidence` from struct literals. Update JSON comparison test.
- `src/tests/tools/search/definition_promotion_tests.rs` — Uses `OptimizedResponse::new()` constructor; update accordingly.

### Risk: VERY LOW
- `insights` and `next_actions` are set but never read by any formatting function
- `confidence` is set but never read after `optimize_for_tokens` (which itself is always called with `Some(limit)`)
- No external consumers (MCP server, not REST API)
- All changes are purely subtractive (removing dead code)

### Tests impacted
- `lean_format_tests.rs` — 6 struct literals need `insights`/`next_actions` removed; 1 JSON comparison test needs updating
- `definition_promotion_tests.rs` — 6 calls to `OptimizedResponse::new()` need signature updated
- No search_quality tests reference these fields

### Test command after change
```bash
cargo test --lib tests::tools::search -- --skip search_quality 2>&1 | tail -5
```

---

## Step 2: Remove health report filler (LOW RISK)

### What
- Remove "Performance Recommendations" section entirely (lines 57-63 of health.rs)
- Remove "Recommended Actions" section from `assess_overall_health()` (lines 252-258)
- Remove `"• System is fully operational - enjoy lightning-fast development!\n"` filler
- Remove `"Search Capabilities: Fast full-text search enabled\n"` and `"Performance: <5ms query response time\n"` from search engine health — these are marketing, not health data
- Compact "Detailed Metrics" section — currently just re-states `db_size_mb` and says "Query performance: Optimized with indexes" which is meaningless
- When `embedding_runtime_status` is `None`, output a single line `"Embedding Status: NOT INITIALIZED\n"` instead of 6 lines of "unavailable"/"false"/"none"

### Files to modify
- `src/tools/workspace/commands/registry/health.rs` — All changes here

### Risk: LOW-MEDIUM
- The health tests assert on exact strings from the embedding section. The `NOT INITIALIZED` case currently outputs 6 lines that are explicitly tested. If we collapse to 1 line, we need to update the test.
- Tests at `src/tests/tools/workspace/mod_tests.rs` lines 568-578 assert `"Embedding Status: NOT INITIALIZED"`, `"Runtime: unavailable"`, `"Backend: unresolved"`, `"Device: unavailable"`, `"Accelerated: false"`, `"Degraded: none"`.
- **Decision**: Keep the "NOT INITIALIZED" case as-is for now to avoid test churn. Only remove the untested filler: Performance Recommendations, Recommended Actions, search engine marketing text, and the detailed metrics re-statement.

### Tests impacted
- `mod_tests.rs` health tests: NO impact if we only remove untested sections (Performance Recommendations is only emitted when `detailed=true`, which no test exercises)

### Test command
```bash
cargo test --lib tests::tools::workspace -- --skip search_quality 2>&1 | tail -5
```

---

## Step 3: Remove workspace clean filler text (TRIVIAL, ZERO RISK)

### What
- Remove `"Cleanup helps maintain optimal performance and storage usage."` from `handle_clean_command()` (list_clean.rs line 216)

### Files to modify
- `src/tools/workspace/commands/registry/list_clean.rs` — Remove one line from format string

### Risk: ZERO
- No tests assert on this string

### Test command
```bash
cargo test --lib tests::tools::workspace -- --skip search_quality 2>&1 | tail -5
```

---

## Step 4: get_context — Drop raw ref_score from output (MEDIUM RISK)

### What
- In **readable** mode: Change `"  Centrality: {} (ref_score: {})\n"` to just `"  Centrality: {}\n"` — the raw numeric score adds no value for the agent
- In **compact** mode: Change `centrality={} ref={}` to just `centrality={}` — same reason
- The `centrality_label()` function stays; it converts the score to "high"/"medium"/"low" which is meaningful

### Files to modify
- `src/tools/get_context/formatting.rs` — Lines 148-151 (readable) and 220-228 (compact)

### Risk: MEDIUM
- Test `test_ref_score_displayed_as_integer` (line 677) explicitly asserts `output.contains("ref_score: 47")` — **this test will break and needs updating**
- Test `test_compact_format_is_token_lean_and_structured` asserts compact output structure — **may need updating** if the compact PIVOT line format changes
- The readable centrality tests (`test_centrality_high`, etc.) assert on `"Centrality: high"` which remains valid since we're keeping the label

### Tests impacted
- `get_context_formatting_tests.rs`:
  - `test_ref_score_displayed_as_integer` — Must change assertion from `"ref_score: 47"` to just checking centrality label
  - `test_compact_format_is_token_lean_and_structured` — May need compact output assertion updated (currently asserts `"PIVOT process_payment src/payment/processor.rs:42"` which should still be fine, but the compact PIVOT line currently includes `ref=25` which will be removed)

### Test command
```bash
cargo test --lib tests::tools::get_context_formatting -- --skip search_quality 2>&1 | tail -5
```

---

## Step 5: get_context — Remove file map from compact mode (MEDIUM RISK)

### What
- Remove the `FILE` section from `format_context_compact()` (lines 248-255)
- The file map is redundant in compact mode because every PIVOT and NEIGHBOR line already includes the file path
- Keep the file map in readable mode (it serves as a summary index)

### Files to modify
- `src/tools/get_context/formatting.rs` — Remove lines 248-255 from `format_context_compact()`

### Risk: MEDIUM
- Test `test_compact_format_is_token_lean_and_structured` asserts `output.contains("FILE src/payment/processor.rs | pivot: process_payment")` — **this test will break and needs updating** (remove that assertion)
- Test `test_compact_reduces_estimated_tokens_by_at_least_20_percent` — token reduction should increase (compact gets even smaller), so this should be fine or better

### Tests impacted
- `get_context_formatting_tests.rs`:
  - `test_compact_format_is_token_lean_and_structured` — Remove FILE assertion
  - `test_compact_output_smaller_than_readable_for_same_context` — Should still pass (compact gets smaller)
  - `test_compact_reduces_estimated_tokens_by_at_least_20_percent` — Reduction increases; assertion still valid

### Test command
```bash
cargo test --lib tests::tools::get_context_formatting -- --skip search_quality 2>&1 | tail -5
```

---

## Step 6: get_context — Change default format to compact (LOW RISK but BEHAVIORAL CHANGE)

### What
- Change `OutputFormat::from_option` default from `Self::Readable` to `Self::Compact`
- Update the tool description from `"readable" (default)` to `"compact" (default)`
- `format_context()` keeps hardcoding `Readable` for backward compat in tests — it's only called by tests

### Files to modify
- `src/tools/get_context/formatting.rs` — Line 37: change `_ => Self::Readable` to `_ => Self::Compact`
- `src/tools/get_context/mod.rs` — Update doc comment on `format` field

### Risk: LOW (but behavioral)
- This is a **user-visible behavioral change**: agents that previously got readable output by default now get compact output
- However, agents never specified `format` explicitly (it's optional and undocumented beyond the schema), so they'll just get a denser format
- The compact format contains all the same information, just with less decorative formatting
- No test breakage: all 20 formatting tests use `format_context()` which hardcodes `Readable`
- The pipeline uses `format_context_with_mode(data, OutputFormat::from_option(format.as_deref()))` so when `format` is `None`, it will now get `Compact`

### Tests impacted
- NONE — tests call `format_context()` directly, which hardcodes `Readable`

### Test command
```bash
cargo test --lib tests::tools::get_context -- --skip search_quality 2>&1 | tail -5
```

---

## Step 7: get_context — Shorten Unicode separators in readable mode (LOW RISK)

### What
- The readable mode uses 43-47 char box-drawing separators like `"── Pivot: {} ───────────────────────────────────────────"` and `"── Neighbors ───────────────────────────────────────────"`
- Shorten to ~20 chars: `"── Pivot: {} ────────"` and `"── Neighbors ────────"`
- Same for the Files section header
- Also shorten the header `"═══ Context: {} ═══"` (currently has trailing ═══)

### Files to modify
- `src/tools/get_context/formatting.rs` — Lines 123, 140, 183, 193

### Risk: LOW
- Tests assert on `output.contains("Context:")`, `"Pivot: process_payment"`, `"Neighbors"`, `"Files"` — none assert on the exact length of the box-drawing chars
- Exception: `test_compact_format_is_token_lean_and_structured` asserts `!output.contains("═══")` for compact mode — this is fine since we're only changing readable mode

### Tests impacted
- NONE expected — no test asserts on exact separator lengths

### Test command
```bash
cargo test --lib tests::tools::get_context_formatting -- --skip search_quality 2>&1 | tail -5
```

---

## Implementation Order (Recommended)

1. **Step 1** — OptimizedResponse cleanup (highest impact, safest, self-contained)
2. **Step 3** — Clean filler text (trivial one-liner)
3. **Step 2** — Health report filler (small, untested sections)
4. **Step 4** — Drop raw ref_score (medium risk, requires test updates)
5. **Step 5** — Remove file map from compact (medium risk, requires test updates)
6. **Step 7** — Shorten Unicode separators (low risk)
7. **Step 6** — Change default format to compact (behavioral change, do last for clean rollback)

### Rationale
- Steps 1-3: Pure dead code / filler removal. Zero behavioral change. Highest confidence.
- Steps 4-5: Format changes with known test breakage. Test updates are mechanical. Group them together since they both touch `formatting.rs`.
- Step 7: Cosmetic but harmless.
- Step 6: The only behavioral change visible to users. Doing it last means if it's controversial, the other savings are already landed.

---

## Workspace list compact format (DEFERRED)

The proposal mentioned changing workspace list to one-line-per-workspace and eliminating duplicated formatting logic. After reviewing the code:

- The duplicated closure (lines 43-87) vs real output (lines 97-136) is genuinely duplicated — the closure exists solely for `ProgressiveReducer` token estimation. This is a real code smell.
- However, refactoring this involves understanding the `ProgressiveReducer` API and is orthogonal to token efficiency in output.
- **Recommendation**: Defer to a separate PR. The formatting change is a design decision (what info to show per workspace) that deserves its own review, and the dedup is a code quality issue, not a token efficiency issue.

---

## Total Expected Token Savings

| Area | Estimated savings per invocation |
|------|----------------------------------|
| OptimizedResponse dead fields (insights/next_actions never appear in output, but the code runs uselessly) | ~0 output tokens, ~50 lines dead code removed |
| Health report filler | ~30-50 tokens per health call |
| Clean filler text | ~10 tokens per clean call |
| Drop raw ref_score | ~5 tokens per pivot per get_context call |
| Remove file map from compact | ~15-30 tokens per get_context call (compact mode) |
| Compact as default | ~50-100 tokens per get_context call (users who never specified format) |
| Shorter Unicode separators | ~10-20 tokens per get_context call (readable mode) |

The biggest win is Step 6 (compact default) — but it's also the only behavioral change.

