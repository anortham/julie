# Content Search Quality Overhaul

**Date:** 2026-02-18
**Status:** Design approved, pending implementation

## Problem Statement

`fast_search` content mode (line-level search) fails to find identifiers and multi-word queries that provably exist in the codebase. Root cause: a semantic mismatch between Tantivy's token-based file discovery and the line-level substring matching that follows.

### Failure Cases

1. **Single identifiers** — `files_by_language` (exists 4x in processor.rs) returns zero results
   - CodeTokenizer splits to `["files", "by", "language"]`
   - `filter_compound_tokens` strips the original compound
   - Tantivy searches for generic terms, returns irrelevant files in top N
   - Line matching never sees processor.rs

2. **Multi-word cross-line** — `spawn_blocking statistics` returns zero results
   - Both terms exist in the same file but on different lines
   - Line matching requires ALL tokens on the SAME line
   - No single line contains both terms → zero matches

3. **Generic term flooding** — queries with common sub-terms (like "by", "files") cause BM25 to return files where those words appear incidentally, pushing the relevant file out of the fetch limit

## Design

### Change 1: Compound Token Handling

**File:** `src/search/index.rs`, `src/search/query.rs`

Remove `filter_compound_tokens` entirely. Modify `build_content_query` to distinguish compound tokens from atomic tokens:

- **Compound tokens** (contain `_`): `Occur::Should` with boost — ranks exact identifier matches higher
- **Atomic sub-parts**: `Occur::Must` — ensures file contains all relevant words

Example for query `files_by_language`:
```
MUST doc_type = "file"
MUST content = "files"
MUST content = "by"
MUST content = "language"
SHOULD content = "files_by_language" (boosted)
```

Files containing the literal identifier score higher. Files with scattered "files", "by", "language" still match but rank lower.

### Change 2: Cross-Line Matching for Multi-Word Queries

**Files:** `src/tools/search/line_mode.rs`, `src/tools/search/query.rs`, `src/tools/search/types.rs`

Add a new `LineMatchStrategy` variant for file-level matching:

```rust
enum LineMatchStrategy {
    Substring(String),                              // existing: single token
    Tokens { required: Vec<String>, excluded: Vec<String> }, // existing: same-line AND
    FileLevel { terms: Vec<String> },               // new: cross-line OR
}
```

**Query routing logic in `line_match_strategy`:**
- Single token (no whitespace): `Substring` — find lines containing that exact identifier
- Multi-word query: `FileLevel` — Tantivy guarantees file-level AND, line matching uses OR to show where each term appears

**Output format for FileLevel matches:**
```
📄 File-level search in [workspace]: 'query' (found N lines across M files)
```

### Change 3: Dynamic Fetch Limit

**File:** `src/tools/search/line_mode.rs`

Bump the default fetch_limit multiplier from `5x` to `20x`. This is a safety net for multi-word queries with common terms. Cost is negligible (Tantivy returns file paths and scores, not content).

```rust
// Before
base_limit.saturating_mul(5)
// After
base_limit.saturating_mul(20)
```

### Files Touched

| File | Change |
|------|--------|
| `src/search/index.rs` | Remove `filter_compound_tokens`, pass compound info to query builder |
| `src/search/query.rs` | `build_content_query` with SHOULD for compounds; `FileLevel` strategy variant; update `line_match_strategy` and `line_matches` |
| `src/tools/search/line_mode.rs` | Bump fetch_limit, update `collect_line_matches` for FileLevel mode, update output formatting |
| `src/tools/search/types.rs` | Add `FileLevel` variant to `LineMatchStrategy` |

## Test Plan

### Dogfood Tests (`src/tests/tools/search_quality/dogfood_tests.rs`)

- `test_single_identifier_snake_case` — search `files_by_language`, verify processor.rs found
- `test_single_identifier_camel_case` — search `LanguageParserPool`, regression guard
- `test_multiword_cross_line` — search `spawn_blocking statistics`, verify file-level match
- `test_multiword_same_line` — search `incremental update atomic`, regression guard

### Unit Tests (`src/tests/tools/search/`)

- `test_compound_tokens_boost_ranking` — verify SHOULD boost on compound, MUST on sub-parts
- `test_line_match_strategy_single_token` — verify Substring strategy for single identifiers
- `test_line_match_strategy_multi_word` — verify FileLevel strategy for multi-word queries
- `test_filter_compound_tokens_removed` — verify old behavior no longer strips compounds
