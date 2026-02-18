# Content Search Quality Overhaul — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix `fast_search` content mode so compound identifiers (`files_by_language`) and multi-word cross-line queries (`spawn_blocking statistics`) return correct results.

**Architecture:** Three changes to the existing two-stage search pipeline (Tantivy file discovery → line matching): (1) keep compound tokens as boosted SHOULD terms instead of stripping them, (2) add cross-line file-level matching for multi-word queries, (3) increase fetch limit safety net.

**Tech Stack:** Rust, Tantivy (BooleanQuery/BoostQuery), tree-sitter tokenizer

---

## Task 1: Add `FileLevel` variant to `LineMatchStrategy`

**Files:**
- Modify: `src/tools/search/types.rs:1-18`

**Step 1: Add the new variant**

In `src/tools/search/types.rs`, add `FileLevel` to the enum:

```rust
pub enum LineMatchStrategy {
    /// Simple substring matching (case-insensitive)
    Substring(String),
    /// Token-based matching with required and excluded terms
    Tokens {
        required: Vec<String>,
        excluded: Vec<String>,
    },
    /// File-level matching: Tantivy guarantees all terms in file,
    /// line matching uses OR to show where each term appears
    FileLevel {
        terms: Vec<String>,
    },
}
```

**Step 2: Run build to verify it compiles**

Run: `cargo check 2>&1 | tail -20`

Expected: Warnings about non-exhaustive match patterns in `line_matches` (query.rs). This is expected — we'll fix those in Task 3.

**Step 3: Commit**

```bash
git add src/tools/search/types.rs
git commit -m "feat(search): add FileLevel variant to LineMatchStrategy"
```

---

## Task 2: Update `line_match_strategy` to route multi-word queries

**Files:**
- Modify: `src/tools/search/query.rs:74-104` (`line_match_strategy` function)
- Modify: `src/tools/search/query.rs:107-125` (`line_matches` function)

**Step 1: Write failing tests**

Create new test functions in a new file `src/tests/tools/search/line_match_strategy_tests.rs`:

```rust
#[cfg(test)]
mod line_match_strategy_tests {
    use crate::tools::search::query::line_match_strategy;
    use crate::tools::search::types::LineMatchStrategy;

    #[test]
    fn test_single_identifier_produces_substring() {
        // Single snake_case identifier → Substring strategy
        let strategy = line_match_strategy("files_by_language");
        match strategy {
            LineMatchStrategy::Substring(s) => {
                assert_eq!(s, "files_by_language");
            }
            other => panic!("Expected Substring, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn test_single_camel_case_produces_substring() {
        let strategy = line_match_strategy("LanguageParserPool");
        match strategy {
            LineMatchStrategy::Substring(s) => {
                assert_eq!(s, "languageparserpool");
            }
            other => panic!("Expected Substring, got {:?}", std::mem::discriminant(&other)),
        }
    }

    #[test]
    fn test_multi_word_produces_file_level() {
        // Multi-word query → FileLevel strategy
        let strategy = line_match_strategy("spawn_blocking statistics");
        match &strategy {
            LineMatchStrategy::FileLevel { terms } => {
                assert_eq!(terms.len(), 2);
                assert!(terms.contains(&"spawn_blocking".to_string()));
                assert!(terms.contains(&"statistics".to_string()));
            }
            other => panic!("Expected FileLevel, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn test_multi_word_with_exclusion_keeps_tokens_strategy() {
        // Exclusion with '-' prefix → stays as Tokens (existing behavior)
        let strategy = line_match_strategy("spawn_blocking -test");
        match &strategy {
            LineMatchStrategy::Tokens { required, excluded } => {
                assert!(required.contains(&"spawn_blocking".to_string()));
                assert!(excluded.contains(&"test".to_string()));
            }
            other => panic!("Expected Tokens, got {:?}", std::mem::discriminant(other)),
        }
    }
}
```

Register this module in `src/tests/tools/search/mod.rs`.

**Step 2: Run tests to verify they fail**

Run: `cargo test line_match_strategy_tests 2>&1 | tail -20`

Expected: `test_multi_word_produces_file_level` FAILS (current code produces `Tokens`, not `FileLevel`). The single-identifier tests may pass or fail depending on current routing.

**Step 3: Update `line_match_strategy` in `src/tools/search/query.rs`**

Replace the function body (lines 74-104):

```rust
pub fn line_match_strategy(query: &str) -> LineMatchStrategy {
    let trimmed = query.trim();

    // Special syntax → fall back to substring (preserves existing behavior for quoted, wildcard, boolean)
    if trimmed.is_empty()
        || trimmed.contains('"')
        || trimmed.contains('\'')
        || trimmed.contains('*')
        || trimmed.contains(" AND ")
        || trimmed.contains(" OR ")
        || trimmed.contains(" NOT ")
    {
        return LineMatchStrategy::Substring(trimmed.to_lowercase());
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();

    // Single word (possibly compound like files_by_language) → substring match
    if words.len() == 1 {
        return LineMatchStrategy::Substring(trimmed.to_lowercase());
    }

    // Multi-word: check for exclusion tokens
    let mut required = Vec::new();
    let mut excluded = Vec::new();

    for word in &words {
        if word.starts_with('-') && word.len() > 1 {
            excluded.push(word[1..].to_lowercase());
        } else if !word.is_empty() {
            required.push(word.to_lowercase());
        }
    }

    // If there are exclusions, use Tokens strategy (same-line AND with exclusions)
    if !excluded.is_empty() {
        return LineMatchStrategy::Tokens { required, excluded };
    }

    // Multi-word without exclusions → FileLevel (cross-line OR, Tantivy guarantees file-level AND)
    if required.is_empty() {
        LineMatchStrategy::Substring(trimmed.to_lowercase())
    } else {
        LineMatchStrategy::FileLevel { terms: required }
    }
}
```

**Step 4: Update `line_matches` to handle `FileLevel`**

In `src/tools/search/query.rs`, update the `line_matches` function (lines 107-125):

```rust
pub fn line_matches(strategy: &LineMatchStrategy, line: &str) -> bool {
    let line_lower = line.to_lowercase();

    match strategy {
        LineMatchStrategy::Substring(pattern) => {
            !pattern.is_empty() && line_lower.contains(pattern)
        }
        LineMatchStrategy::Tokens { required, excluded } => {
            let required_ok =
                required.is_empty() || required.iter().all(|token| line_lower.contains(token));
            let excluded_ok = excluded.iter().all(|token| !line_lower.contains(token));
            required_ok && excluded_ok
        }
        LineMatchStrategy::FileLevel { terms } => {
            // Match lines containing ANY term (OR logic)
            // Tantivy already guarantees all terms exist in the file
            terms.iter().any(|term| line_lower.contains(term))
        }
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test line_match_strategy_tests 2>&1 | tail -20`

Expected: ALL PASS

**Step 6: Commit**

```bash
git add src/tools/search/query.rs src/tests/tools/search/line_match_strategy_tests.rs src/tests/tools/search/mod.rs
git commit -m "feat(search): route multi-word queries to FileLevel strategy"
```

---

## Task 3: Compound token boost in `build_content_query`

**Files:**
- Modify: `src/search/index.rs:298-310` (`search_content` method)
- Modify: `src/search/index.rs:448-464` (`filter_compound_tokens` — remove)
- Modify: `src/search/query.rs:110-144` (`build_content_query` function)

**Step 1: Write failing test**

Add test to `src/tests/tools/search/tantivy_index_tests.rs`:

```rust
#[test]
fn test_compound_token_finds_exact_identifier() {
    // Regression test: searching for a snake_case identifier should find it
    // even when its sub-parts are very common words
    let temp_dir = TempDir::new().unwrap();
    let index = SearchIndex::create(temp_dir.path()).unwrap();

    // File that contains the exact identifier
    index.add_file_content(&FileDocument {
        file_path: "src/processor.rs".into(),
        content: "let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();".into(),
        language: "rust".into(),
    }).unwrap();

    // File that contains the sub-parts scattered (should also match but rank lower)
    index.add_file_content(&FileDocument {
        file_path: "src/utils.rs".into(),
        content: "// process files for each language detected by the scanner".into(),
        language: "rust".into(),
    }).unwrap();

    index.commit().unwrap();

    let filter = crate::search::SearchFilter {
        language: None,
        kind: None,
        file_pattern: None,
    };

    let results = index.search_content("files_by_language", &filter, 10).unwrap();

    // Must find at least the file with the exact identifier
    assert!(!results.is_empty(), "Should find files matching compound identifier");

    // The file with the exact identifier should rank first
    assert_eq!(
        results[0].file_path, "src/processor.rs",
        "File with exact identifier should rank higher. Got: {:?}",
        results.iter().map(|r| &r.file_path).collect::<Vec<_>>()
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_compound_token_finds_exact_identifier 2>&1 | tail -20`

Expected: FAIL — either empty results (compound stripped) or wrong ranking.

**Step 3: Update `build_content_query` signature and body**

In `src/search/query.rs`, replace `build_content_query` (lines 110-144):

```rust
/// Build a file content search query with optional language filter.
///
/// Compound tokens (containing `_`) are added as boosted SHOULD clauses to
/// promote files containing the exact identifier. Atomic sub-parts remain
/// as MUST clauses for baseline matching.
pub fn build_content_query(
    terms: &[String],
    content_field: Field,
    doc_type_field: Field,
    language_field: Field,
    language_filter: Option<&str>,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    // Must match doc_type = "file"
    let type_term = Term::from_field_text(doc_type_field, "file");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    // Apply optional language filter
    if let Some(lang) = language_filter {
        let lang_term = Term::from_field_text(language_field, lang);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lang_term, IndexRecordOption::Basic)),
        ));
    }

    for term in terms {
        let term_lower = term.to_lowercase();
        let content_term = Term::from_field_text(content_field, &term_lower);
        let term_query = TermQuery::new(content_term, IndexRecordOption::Basic);

        if term.contains('_') {
            // Compound token → SHOULD with boost (promotes exact identifier matches)
            subqueries.push((
                Occur::Should,
                Box::new(BoostQuery::new(Box::new(term_query), 5.0)),
            ));
        } else {
            // Atomic sub-part → MUST (ensures file contains the word)
            subqueries.push((Occur::Must, Box::new(term_query)));
        }
    }

    BooleanQuery::new(subqueries)
}
```

**Step 4: Remove `filter_compound_tokens` from `search_content`**

In `src/search/index.rs`, change line 306 from:
```rust
let terms = Self::filter_compound_tokens(self.tokenize_query(query_str));
```
to:
```rust
let terms = self.tokenize_query(query_str);
```

Then delete the `filter_compound_tokens` method entirely (lines 448-464). Check for other callers first — it should only be called from `search_content`.

**Step 5: Run test to verify it passes**

Run: `cargo test test_compound_token_finds_exact_identifier 2>&1 | tail -20`

Expected: PASS

**Step 6: Run all search tests for regressions**

Run: `cargo test search 2>&1 | tail -20`

Expected: ALL PASS (no regressions)

**Step 7: Commit**

```bash
git add src/search/index.rs src/search/query.rs src/tests/tools/search/tantivy_index_tests.rs
git commit -m "feat(search): boost compound tokens instead of stripping them"
```

---

## Task 4: Bump fetch limit and update `collect_line_matches` for FileLevel

**Files:**
- Modify: `src/tools/search/line_mode.rs:74` (fetch_limit)
- Modify: `src/tools/search/line_mode.rs:292-312` (`collect_line_matches`)
- Modify: `src/tools/search/line_mode.rs:250-256` (output formatting)

**Step 1: Bump fetch limit**

In `src/tools/search/line_mode.rs`, change line 74 from:
```rust
base_limit.saturating_mul(5)
```
to:
```rust
base_limit.saturating_mul(20)
```

**Step 2: Update output header for FileLevel matches**

In `src/tools/search/line_mode.rs`, update the output formatting (around line 250-256). Replace the single format string with strategy-aware formatting:

```rust
let header = match &match_strategy {
    LineMatchStrategy::FileLevel { .. } => {
        let file_count = filtered_matches
            .iter()
            .map(|m| &m.file_path)
            .collect::<std::collections::HashSet<_>>()
            .len();
        format!(
            "📄 File-level search in [{}]: '{}' (found {} lines across {} files)",
            target_workspace_id, query, filtered_matches.len(), file_count
        )
    }
    _ => format!(
        "📄 Line-level search in [{}]: '{}' (found {} lines)",
        target_workspace_id, query, filtered_matches.len()
    ),
};
let mut lines = vec![header];
```

**Step 3: Run all search tests**

Run: `cargo test search 2>&1 | tail -20`

Expected: ALL PASS

**Step 4: Commit**

```bash
git add src/tools/search/line_mode.rs
git commit -m "feat(search): bump fetch limit 5x→20x, FileLevel output formatting"
```

---

## Task 5: Dogfood integration tests

**Files:**
- Modify: `src/tests/tools/search_quality/dogfood_tests.rs` (add new tests at end)

These tests use the fixture database (`fixtures/databases/julie-snapshot/`) which indexes the Julie codebase itself.

**Step 1: Add dogfood tests**

Append to `src/tests/tools/search_quality/dogfood_tests.rs`:

```rust
// ============================================================================
// Content Search Quality: Compound Identifiers & Cross-Line Matching
// ============================================================================

#[tokio::test(flavor = "multi_thread")]
async fn test_content_single_identifier_snake_case() {
    let handler = setup_handler_with_fixture().await;

    // "files_by_language" exists in processor.rs as a local variable
    // Previously returned zero results due to compound token stripping
    let results = search_content(&handler, "files_by_language", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
    assert_contains_path(&results, "processor.rs");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_content_single_identifier_camel_case() {
    let handler = setup_handler_with_fixture().await;

    // Regression guard: camelCase identifiers should still work
    let results = search_content(&handler, "LanguageParserPool", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_content_multiword_cross_line() {
    let handler = setup_handler_with_fixture().await;

    // "spawn_blocking" and "statistics" both exist in handler.rs/processor.rs
    // but not necessarily on the same line
    let results = search_content(&handler, "spawn_blocking statistics", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_content_multiword_same_line_regression() {
    let handler = setup_handler_with_fixture().await;

    // Regression guard: multi-word queries that DO appear on same lines
    // should still work
    let results = search_content(&handler, "incremental update atomic", 10)
        .await
        .expect("Search failed");

    assert_min_results(&results, 1);
}
```

**Step 2: Run dogfood tests**

Run: `cargo test dogfood_tests 2>&1 | tail -30`

Expected: ALL PASS (including the new ones)

**Step 3: Run full test suite for regressions**

Run: `cargo test 2>&1 | tail -20`

Expected: ALL PASS

**Step 4: Commit**

```bash
git add src/tests/tools/search_quality/dogfood_tests.rs
git commit -m "test(search): dogfood tests for compound identifiers and cross-line matching"
```

---

## Task 6: Cleanup and verification

**Step 1: Verify `filter_compound_tokens` is fully removed**

Run: `grep -r "filter_compound_tokens" src/`

Expected: Zero results.

**Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`

Expected: No new warnings.

**Step 3: Manual smoke test**

If Julie is running in this session, test these directly:
- `fast_search(query="files_by_language")` — should find processor.rs lines
- `fast_search(query="spawn_blocking statistics")` — should find results across lines
- `fast_search(query="LanguageParserPool")` — regression check, should still work
- `fast_search(query="incremental_update_atomic")` — regression check

**Step 4: Final commit if any cleanup needed**

---

## Notes for Implementer

- **`search_content` helper in dogfood tests** goes through `text_search_impl`, not `line_mode_search`. The dogfood tests verify the Tantivy ranking changes (Tasks 1-3) but not the line-mode formatting changes (Task 4). The line-mode changes are covered by the unit tests in `src/tests/tools/search/line_mode.rs` and `line_match_strategy_tests.rs`.
- **Existing dogfood tests** may break if `filter_compound_tokens` removal changes ranking for tests like `test_tokenization_underscore_splitting`. Check them after Task 3.
- **The `BoostQuery` import** is already in `src/search/query.rs` line 10: `use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};`
- **`filter_compound_tokens` is only called from `search_content`** (line 306 of `index.rs`). Safe to delete.
- **`LineMatchStrategy` does not derive `Debug`** — you may need to add `#[derive(Debug)]` or use `std::mem::discriminant` in test assertions.
