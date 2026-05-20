# Fast Search Quality & Match Heuristics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `razorback:executing-plans` for sequential execution, adhering strictly to the TDD cycle (Red, Green, Refactor).

**Goal:** Improve search result quality and match relevance in the `fast_search` pipeline by resolving three specific limitations:
1. **camelCase / snake_case Cross-Matching (Finding A):** Bridge the mismatch in line-level substring matching when a user searches using one case style but code lines use the other.
2. **Implicit Same-Line AND Density Boosting (Finding B):** Rank lines within each file descending by query term density for multi-word `FileLevel` queries.
3. **Extension-Blind Exact Basename Matching (Finding C):** Classify file-target searches (e.g. query `bar` matching `src/foo/bar.rs`) as `ExactBasename` rather than `PathFragment`, dramatically boosting their ranking.

---

## Technical Specifications

### 1. camelCase / snake_case Cross-Matching (Finding A)

**Problem:** A user searching `workspaceIsPrimary` (camelCase) against code `workspace_is_primary` (snake_case) tokenizes fine in Tantivy but fails exact line-level filtering. `line_match_strategy()` builds `LineMatchStrategy::Substring("workspaceisprimary")`. With casing markers erased and `normalized_literal_patterns` only doing `_`↔`-` swaps, the variant generator never produces `workspace_is_primary`.

The same erasure happens on multi-word paths: `LineMatchStrategy::FileLevel { terms }` and `Tokens { required, excluded }` lowercase each word before storage (`query.rs:230, 232`). When `term_matches_line` detects a compound term it falls through to `line_matches_literal(line, &term.to_lowercase())` (`query.rs:474`) — so multi-word camelCase queries hit the same dead variant generator.

**Implementation contract:**

1. **Preserve casing at strategy-construction time, everywhere.**
   - **All five** `LineMatchStrategy::Substring(trimmed.to_lowercase())` call sites in `line_match_strategy()` (`query.rs:206, 214, 221, 243`, plus the empty-required fallback) → `LineMatchStrategy::Substring(trimmed.to_string())`.
   - The `Tokens` branch's `excluded.push(word[1..].to_lowercase())` and `required.push(word.to_lowercase())` → store the original-case slices.
   - The `FileLevel` `terms` vector retains original casing.
   - The `clean_or_disjunction_terms()` helper at `query.rs:249` currently lowercases every branch (`.map(|branch| branch.to_lowercase())`). Preserve original case here too.

2. **Lowercase at compare time, not before.** In `line_matches_literal` (`query.rs:328`):
   - Compute `let pattern_lower = pattern.to_lowercase();` locally.
   - Use `pattern_lower` for the direct `line_lower.contains(...)` check **and** for the punctuation-escape unwrap path's `line_lower.contains(...)` checks.
   - **Pass the original-case `pattern`** to `normalized_literal_patterns(pattern)` so it can detect camelCase boundaries. The function itself returns lowercase variants; compare those against `line_lower`.

3. **Drop the lowercase in the compound `term_matches_line` path.** `term_matches_line` (`query.rs:472`) currently calls `line_matches_literal(line, &term.to_lowercase())`. Change to `line_matches_literal(line, term)`. With `Tokens`/`FileLevel` terms now stored in original case, compound camelCase terms reach the new case-aware literal path intact.

4. **Verify the non-compound token path still works.** `term_matches_tokens` (`query.rs:498`) feeds the term through `CodeTokenizer.token_stream(term)`. The tokenizer normalizes case during tokenization, so storing original-case terms in `Tokens`/`FileLevel` does not break token matching. Confirm via the existing `test_file_level_line_matches_or_logic` (`line_match_strategy_tests.rs:177`) which has a case-insensitivity assertion at line 198.

5. **Extend `normalized_literal_patterns`** (`query.rs:345`) to yield case-shape variants. Reuse the existing `has_camel_case_boundary` helper at `query.rs:485`. Algorithm:
   - All variants pushed are **lowercase strings** (matched against `line_lower`).
   - Always push `pattern.to_lowercase().replace('-', "_")` and `pattern.to_lowercase().replace('_', "-")` (replaces the current calls).
   - If `has_camel_case_boundary(pattern)`: split into components on case boundary, then push:
     - `snake_case`: components joined by `_`, all lowercase.
     - `kebab-case`: components joined by `-`, all lowercase.
     - `flat-lowercase`: components concatenated without separators.
   - Keep the existing punctuation-escape unwrap branch, but feed its variants through the same lowercasing.
   - `push_unique_variant` already guards against duplicates and equality with the original; no change there.

**Test contract** (`src/tests/tools/search/line_match_strategy_tests.rs`):

Six existing assertions break under the new contract — **all** must be updated together:
- Line 25: `"languageparserpool"` → `"LanguageParserPool"`
- Line 104: `"insert or replace symbols"` → `"INSERT OR REPLACE symbols"`
- Line 116: `"is not null"` → `"IS NOT NULL"`
- Line 128: `"do not edit"` → `"DO NOT EDIT"`
- Line 140: `"\"insert or replace\""` → `"\"INSERT OR REPLACE\""`
- Line 152: `"insert or replace"` → `"INSERT OR REPLACE"`

Three `FileLevel` term assertions also break (terms now preserve case):
- Line 68-70: `vec!["logging.basicconfig", "datefmt"]` → `vec!["logging.basicConfig", "datefmt"]`
- Line 87-90: `command::search/refs/tool` → restore original `Command::Search/Refs/Tool`
- Line 39-41: multi-word `spawn_blocking statistics` is already lowercase input — unchanged.

Other test files that may assert on lowercased strategy payloads: grep `Substring(s)` and `terms:` assertions across `src/tests/tools/search/` before pushing. Any failing assertion on a previously-lowercased value updates to the original-case value the test fed in.

New tests (add to `line_match_strategy_tests.rs`):
- `test_camel_case_query_matches_snake_case_line`: strategy from `"workspaceIsPrimary"`, `line_matches` true against `"let workspace_is_primary = true;"`.
- `test_snake_case_query_matches_camel_case_line`: strategy from `"workspace_is_primary"`, `line_matches` true against `"let workspaceIsPrimary = true;"`.
- `test_kebab_case_query_matches_snake_case_line`: strategy from `"workspace-is-primary"`, `line_matches` true against `"workspace_is_primary"`.
- `test_filelevel_camel_case_compound_term_matches_snake_case_line`: `FileLevel { terms: vec!["workspaceIsPrimary".into()] }`, `line_matches` true against `"workspace_is_primary"`. This is the multi-word camelCase regression guard.

### 2. Same-Line AND Density Boosting (Finding B)

**Problem:** In multi-word `FileLevel` queries (e.g. `workspace_is_primary edit_file`), Tantivy ensures both words are present *somewhere* in the file, but `collect_line_matches` (`line_mode.rs:742`) uses OR logic via `line_matches` → `terms.iter().any(...)` (`query.rs:321-324`). In large files, dozens of single-term lines drown out the few same-line co-occurrences a user actually wants.

**Implementation contract:**

1. **Scope: `LineMatchStrategy::FileLevel` only.** `Substring` and `Tokens` paths retain current behavior (preserve source order, early-break at `max_results`). Regression-guard tests must verify this.

2. **Density definition: count of distinct query terms matched on the line.** Range `1..=terms.len()`. Implementation: dedupe `terms` once at function entry, then `density = deduped_terms.iter().filter(|t| term_matches_line(t, line, &line_tokens)).count()`. No IDF, no weighting, no occurrence multiplication. Rejected because: line-mode has no IDF context, occurrence weighting would reward repeated noisy terms over actual co-occurrence, and the goal is "show me lines that hit more of my query" — distinct-count says exactly that.

3. **Collect-then-rank flow for `FileLevel`.**
   - Iterate **all** lines in the file. **No early break on `max_results`.**
   - For each matched line: push `(density: usize, line_number: usize, LineMatch)` into a per-file scratch `Vec`.
   - After the loop: `scratch.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)))` — primary density descending, secondary line number ascending. Stable sort.
   - Truncate to the remaining global budget (`max_results - destination.len()`) and extend `destination`.

4. **Signature stable.** Keep `collect_line_matches` signature unchanged. Branch on `LineMatchStrategy::FileLevel` inside the function body. Extract `terms` via pattern match.

5. **Cross-file budget caveat (known limitation, by design).** `collect_matches_from_file_results` (`line_mode.rs:167-229`) iterates `file_results` in Tantivy rank order and stops at `base_limit` once total `matches.len() >= base_limit`. A high-density line in a Tantivy-low-rank file can still be missed if earlier files exhaust the budget with their own (possibly low-density) lines. This plan **does not address** cross-file re-ranking; it improves within-file ordering only. A follow-up that collects across files then re-ranks globally would address the cross-file case, but is out of scope here. State this explicitly in any release notes.

6. **Memory note.** For very large files the scratch vector now holds every matched line before truncation. Bounded by file length; acceptable because (a) files reaching this function already passed Tantivy filtering, and (b) per-file matched lines in real code are typically dozens-to-hundreds.

**Test additions** (`line_match_strategy_tests.rs` or new sibling for line_mode internals if `collect_line_matches` is private — use a `#[cfg(test)]` shim if needed):
- `test_filelevel_density_sort_promotes_multi_term_line`: file content with one line containing both `alpha` and `beta`, two single-term lines; assert the dense line is first in returned `LineMatch` order.
- `test_filelevel_ties_preserve_source_order`: two lines each matching one distinct term; returned order matches line numbers ascending.
- `test_filelevel_density_dedupes_repeated_query_terms`: query terms `["alpha", "alpha", "beta"]` (deduped to `["alpha","beta"]`); a line with `alpha alpha` does NOT outrank a line with `alpha beta`.
- `test_substring_strategy_preserves_source_order`: non-`FileLevel` regression guard.
- `test_tokens_strategy_preserves_source_order`: non-`FileLevel` regression guard.

### 3. Extension-Blind Exact Basename Matching (Finding C)

**Problem:** Query `bar` matches `src/foo/bar.rs`. `classify_file_match` (`index.rs:1569`) compares basenames `"bar.rs"` vs `"bar"` → falls to `PathFragment` (rank 2) instead of `ExactBasename` (rank 1).

**Implementation contract:**

1. **One-sided extension stripping. File side only.** Stripping both sides would cause `bar.py` (query) to match `bar.rs` (file) — wrong. Keep query basename verbatim.

2. **Last extension only.** `rsplit_once('.')` on the file basename. `foo.tar.gz` → stem `"foo.tar"`. Multi-extension files are intentionally treated as if `.gz` is the extension (consistent with editor and shell conventions). State this in the function's doc comment.

3. **Hidden-file rules.**
   - `.gitignore` → `rsplit_once('.')` yields `("", "gitignore")`. **Skip stem comparison when stem is empty.** Otherwise query `gitignore` would falsely match `.gitignore` as ExactBasename. (Equality path at step 1 still handles `.gitignore` matching `.gitignore` correctly.)
   - `.env.local` → `rsplit_once('.')` yields `(".env", "local")`. Query `.env` matches stem `.env` → classifies as ExactBasename. Intended: dotfile with a sub-extension is a real case.
   - `Makefile` (no dot) → `rsplit_once('.')` returns `None`; falls through to existing equality path. Query `Makefile` against file `Makefile` still classifies as ExactBasename via that path.

4. **Existing equality path preserved.** Keep the current `basename_for_path(file_path) == basename_for_path(normalized_query)` check first. Only when it fails, try the stem comparison. Guarantees no regression for queries that include the extension.

5. **Glob short-circuit unchanged.** The leading `query_contains_glob_syntax(query)` check still wins for glob queries.

**Reference algorithm:**

```rust
fn classify_file_match(query: &str, normalized_query: &str, file_path: &str) -> FileMatchKind {
    if query_contains_glob_syntax(query) {
        return FileMatchKind::Glob;
    }
    if file_path == normalized_query {
        return FileMatchKind::ExactPath;
    }

    let file_basename = basename_for_path(file_path);
    let query_basename = basename_for_path(normalized_query);

    if file_basename == query_basename {
        return FileMatchKind::ExactBasename;
    }

    // Extension-blind: strip the *last* extension from the file basename only.
    // Hidden files like ".gitignore" have empty stems and must NOT match
    // an extensionless query of the suffix.
    if let Some((stem, _ext)) = file_basename.rsplit_once('.')
        && !stem.is_empty()
        && stem == query_basename
    {
        return FileMatchKind::ExactBasename;
    }

    FileMatchKind::PathFragment
}
```

6. **Out of scope: retrieval-side boost.** `build_file_query()` (`index.rs:~400`) constructs the Tantivy query before classification runs. An extensionless basename query like `bar` still has to be retrieved by the existing query builder for `classify_file_match` to even see it. Empirically this works because basename indexing + path-fragment matching surfaces `bar.rs` as a candidate; classification then promotes it. If candidates are getting culled pre-classification, that's a separate (larger) change. Document this assumption.

**Test additions** (`src/tests/tools/search/file_mode_index_tests.rs`):
- `test_search_files_promotes_extensionless_query_to_exact_basename`: index `src/foo/bar.rs` and `src/baz/bar_helper.rs`; query `bar` returns `bar.rs` ranked ahead of `bar_helper.rs`.
- `test_search_files_does_not_match_wrong_extension`: index `bar.rs` and `bar.py`; query `bar.py` returns `bar.py` first; assert that the underlying classification of `bar.rs` is NOT `ExactBasename` (use `assert_eq!` on the classified kind if possible, otherwise verify via rank ordering).
- `test_search_files_extensionless_file_still_matches`: index `Makefile`; query `Makefile` classifies as ExactBasename (regression guard).
- `test_search_files_hidden_file_suffix_not_promoted`: index `.gitignore`; query `gitignore` does **not** classify as ExactBasename (does not match equality, stem is empty so skipped). Query `.gitignore` still matches via the equality path.
- `test_search_files_dotted_extension_file_classifies_correctly`: index `.env.local`; query `.env` promotes to ExactBasename via stem comparison.
- Existing `test_search_files_prefers_exact_basename_over_fragment_matches` (extension included) must continue to pass — regression guard.

**Test instrumentation note.** The current public `search_files` API returns ranked results without the `FileMatchKind` label. To assert on `match_kind` directly, either:
- Add a `#[cfg(test)]` accessor that exposes `classify_file_match` for unit-level assertions; or
- Verify behavior via rank ordering (ExactBasename outranks PathFragment).
The first is preferred — direct assertions are less brittle than rank inference.

---

## Proposed Changes

### Component 1: Search Tools & Query Processing

#### [MODIFY] `src/tools/search/query.rs`
- Five `LineMatchStrategy::Substring(trimmed.to_lowercase())` → `LineMatchStrategy::Substring(trimmed.to_string())`.
- `Tokens` branch: stop lowercasing pushes into `required`/`excluded`.
- `FileLevel` branch: stop lowercasing terms.
- `clean_or_disjunction_terms`: stop lowercasing branches.
- `line_matches_literal`: compute `pattern_lower` locally; pass original-case `pattern` to `normalized_literal_patterns`.
- `term_matches_line`: drop the `.to_lowercase()` on the compound-term literal fallback.
- `normalized_literal_patterns`: reuse `has_camel_case_boundary`; yield snake/kebab/flat-lowercase variants from camelCase patterns; preserve existing `_`↔`-` swap behavior; ensure all returned variants are lowercase.

#### [MODIFY] `src/tools/search/line_mode.rs`
- `collect_line_matches`: add `FileLevel` branch that collects all matches with density, sorts `(Reverse(density), line_number)`, truncates to remaining budget. Other branches unchanged.

#### [MODIFY] `src/tests/tools/search/line_match_strategy_tests.rs`
- Update six `Substring(s)` assertions and three term-list assertions per the inventory above.
- Add four new camelCase / kebab / FileLevel-compound tests.
- Add four density tests.
- Add two source-order regression guards.

### Component 2: File Search & Indexing

#### [MODIFY] `src/search/index.rs`
- `classify_file_match`: insert stem comparison after the basename-equality check; skip when stem is empty; document last-extension semantics in a doc comment.

#### [MODIFY] `src/tests/tools/search/file_mode_index_tests.rs`
- Add five new file-classification tests (extensionless promotion, wrong-extension non-match, hidden-file no-promote, dotted-extension promote, Makefile regression).
- Add `#[cfg(test)]` accessor for `classify_file_match` if not already exposed.

---

## Verification Plan

### TDD discipline

Each finding starts RED: write the test, confirm failure via `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`, then implement, then GREEN via the same command. Do NOT batch test edits across findings.

### Per-batch gates (lead-only, after all three findings land)

In order:
1. `cargo nextest run --lib tests::tools::search` — focused module sweep.
2. `cargo xtask test changed` — bucket-scoped regression on the actual diff.
3. `cargo xtask test dogfood` — **required**; the `search_quality` bucket is the canonical guard for ranking/scoring regressions per CLAUDE.md. Density sort and ExactBasename promotion both affect ranking.

`nano` is redundant once the above pass.

### Worker rules

Per CLAUDE.md subagent contract: workers run **only** the narrowest test by exact name. Workers MUST NOT run `cargo xtask test changed`, `cargo xtask test dev`, or any tier — the lead handles those gates.

### Known limitations to document in release notes

- Finding B: within-file ranking only. Cross-file budget exhaustion (a low-density-but-high-Tantivy-rank file filling the budget before a high-density-but-lower-rank file gets a chance) is not addressed. Tracked as a possible follow-up.
- Finding C: depends on existing retrieval surfacing the candidate. If `build_file_query` does not retrieve `bar.rs` for query `bar`, classification never runs. Empirically retrieval works; a retrieval-side extension-blind boost is out of scope.

---

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Baseline search unit tests pass before modification | `cargo nextest run --lib tests::tools::search` | worker-exact | 0817b4d1371264e654d5326fc4ebaeb0a697b814 | pass | 2026-05-20T12:41:05Z | no |
