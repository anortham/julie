# Phase 3, Priority 1: fast_search Improvements

**Date:** 2026-02-06
**Status:** Draft
**Depends on:** Phase 2 (Tool Categorization) — Complete

## Audit Summary

4-agent parallel audit completed covering data utilization, output format, code quality, and dogfood testing.

**Overall Grade: B+** — Definition search excellent, content search has a critical bug, significant dead code and untapped capabilities.

### Key Findings

| # | Finding | Severity | Category |
|---|---------|----------|----------|
| 1 | Content search (symbol-mode) returns false positives | HIGH | Bug |
| 2 | `search_method` "semantic"/"hybrid" are dead code | MEDIUM | Dead code |
| 3 | `context_lines` parameter is completely unused | LOW | Dead code |
| 4 | `code_context` always None (code_body indexed but never returned) | MEDIUM | Underutilization |
| 5 | AUTO output mode never selects LEAN (80% smaller than JSON) | MEDIUM | Token efficiency |
| 6 | No fuzzy matching despite Tantivy support | LOW | Enhancement |
| 7 | No phrase search despite Tantivy support | MEDIUM | Enhancement |
| 8 | `call_tool()` is 228-line god method | LOW | Code quality |
| 9 | Stringly-typed parameters (no enum validation) | LOW | Code quality |
| 10 | Duplicated ToonResponse creation code | LOW | Code quality |

---

## Root Cause: Content Search False Positives (#1)

**The Problem:**
Searching for `"Blake3 hash"` in content returns files that DON'T contain this string.

**Why:**
- CodeTokenizer splits `"Blake3 hash"` → `["blake", "3", "hash"]`
- AND-per-term requires all tokens present in the file
- `"3"` matches almost every code file (numbers everywhere)
- `"hash"` matches many files (HashMap, hash functions, etc.)
- Result: high recall, terrible precision

**Line-mode is NOT affected** because it post-verifies via regex against actual file content.

**Fix:** Route symbol-mode content search through the same verification path line_mode uses, OR deprecate symbol-mode content search in favor of always returning line-level results for content queries.

---

## Implementation Plan

### Task 1: Fix content search false positives (HIGH)

**Problem:** symbol-mode content search returns garbage results.

**Approach:** When `search_target == "content"`, always use line_mode regardless of output format. The current symbol-mode content search creates fake Symbol objects with `kind: Module` and `name: file_path` — this is a code smell. Line-mode produces superior results.

**Changes:**
- `src/tools/search/mod.rs` — Route content searches to `line_mode::line_mode_search()` regardless of output format
- `src/tools/search/text_search.rs` — Remove the content search branch (or mark it for internal use only by line_mode)
- If output format is NOT "lines" for content search, format line_mode results appropriately

**Test:**
- New test: content search for "Blake3 hash" should find only files containing that literal string
- New test: content search for "BooleanQuery" should rank `query.rs` first

### Task 2: Remove dead search_method values (MEDIUM)

**Problem:** "semantic" and "hybrid" silently route to text search, misleading the MCP tool schema.

**Changes:**
- `src/tools/search/mod.rs:44-46` — Update doc comment to only document "text" and "auto"
- `src/tools/search/mod.rs:162-168` — Remove the debug log block (or convert to a warning that rejects invalid values)
- Update JULIE_AGENT_INSTRUCTIONS.md if it references semantic/hybrid

**Test:**
- Existing tests should still pass (they don't use semantic/hybrid)

### Task 3: Clean up unused context_lines parameter (LOW)

**Problem:** `_context_lines: Option<u32>` is underscore-prefixed and never used.

**Approach:** Keep the parameter in the tool schema (agents send it), but document it as applying only to line-mode results. The `truncate_code_context` function already exists — it just needs populated code_context to work on.

**Changes:**
- `src/tools/search/text_search.rs:30` — Remove underscore prefix if we populate code_context (Task 4)
- OR document clearly that context_lines only affects line-mode output

### Task 4: Populate code_context from Tantivy (MEDIUM)

**Problem:** `code_body` is indexed in Tantivy but never returned in search results.

**Analysis:** The Tantivy schema stores `code_body` with `STORED` flag, so we CAN retrieve it. Currently `code_context: None` always.

**Changes:**
- `src/search/schema.rs` — Verify code_body field has STORED option (it should already)
- `src/search/index.rs` — Add `code_body` to `SymbolSearchResult` struct and populate it from doc
- `src/tools/search/text_search.rs` — Map `code_body` → `code_context` in Symbol conversion
- `src/tools/search/formatting.rs` — `truncate_code_context` will now work properly
- Wire `context_lines` parameter to control truncation

**Test:**
- New test: search results should include code_context when available
- New test: context_lines=0 should truncate to 1 line

**Risk:** Token explosion — without truncation, code_body could be hundreds of lines. The truncation function MUST work correctly before we populate this field.

### Task 5: Make LEAN default in AUTO mode (MEDIUM)

**Problem:** AUTO mode uses JSON for <10 results, TOON for ≥10. LEAN (80% smaller) is never selected.

**Changes:**
- `src/tools/search/mod.rs:259-289` — Change AUTO logic:
  - Default to LEAN for all result counts (best for AI agent consumption)
  - Only use JSON when explicitly requested
  - Keep TOON as a fallback for LEAN encoding failures (if any)

**Test:**
- New test: AUTO mode returns LEAN format by default
- Verify existing LEAN format tests still pass

### Task 6: Refactor call_tool() god method (LOW)

**Problem:** 228 lines, 9 responsibilities.

**Changes:**
- Extract output formatting (lines 216-322) to `fn format_response()`
- Extract workspace readiness check (lines 114-146) to `fn check_readiness()`
- Keep core flow in `call_tool()`

**Test:**
- All existing tests should pass unchanged (pure refactor)

---

## Out of Scope (Deferred)

These were identified but deferred for future work:

| Enhancement | Why Deferred |
|------------|--------------|
| Fuzzy matching (`FuzzyTermQuery`) | Requires new parameter + schema change + testing matrix |
| Phrase search (`PhraseQuery`) | Requires query parser for quoted strings + schema changes |
| Stringly-typed enum conversion | Low impact, many downstream changes |
| Identifiers table integration | Belongs to `fast_refs` deep-dive (Phase 3 Priority 2) |
| Reference workspace parity | Infrastructure issue, not fast_search specific |
| Highlighting/snippets | Complex Tantivy API, unclear agent value |

---

## Task Order & Dependencies

```
Task 1 (content search fix) ← highest impact, no dependencies
Task 2 (dead search_method) ← independent, quick cleanup
Task 3 (context_lines) ← depends on Task 4
Task 4 (code_context) ← depends on schema verification
Task 5 (LEAN default) ← independent
Task 6 (refactor) ← do last, after all other changes
```

**Recommended order:** 1 → 2 → 4 → 3 → 5 → 6

**Estimated effort:** 1 session (Tasks 1-5), +0.5 session for Task 6

---

## Success Criteria

- [ ] Content search returns only files containing the actual query string
- [ ] No references to "semantic" or "hybrid" in tool schema docs
- [ ] Search results include code_context (truncated by context_lines)
- [ ] AUTO output mode defaults to LEAN format
- [ ] call_tool() reduced to <100 lines
- [ ] All existing tests pass + new tests for each change
