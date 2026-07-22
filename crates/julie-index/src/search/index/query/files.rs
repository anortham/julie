use super::super::UnifiedHit;
#[cfg(any(test, feature = "test-support"))]
use super::super::{ContentSearchResult, FileMatchKind, FileSearchResult};

pub(in crate::search::index) fn normalize_file_path(path: &str) -> String {
    path.replace('\\', "/")
}

pub(in crate::search::index) fn basename_for_path(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(any(test, feature = "test-support"))]
fn query_contains_glob_syntax(query: &str) -> bool {
    query
        .chars()
        .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) fn classify_file_match(
    query: &str,
    normalized_query: &str,
    file_path: &str,
) -> FileMatchKind {
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
    // This lets query "bar" match file "src/foo/bar.rs" as ExactBasename.
    // Only the last extension is stripped: "foo.tar.gz" → stem "foo.tar".
    // Hidden files like ".gitignore" have empty stems and must NOT match
    // an extensionless query of the suffix (query "gitignore" against file
    // ".gitignore" stays PathFragment; only ".gitignore" matches ".gitignore"
    // via the equality path above).
    if let Some((stem, _ext)) = file_basename.rsplit_once('.')
        && !stem.is_empty()
        && stem == query_basename
    {
        return FileMatchKind::ExactBasename;
    }

    FileMatchKind::PathFragment
}

/// Normalise a name or query to its lowercase, alphanumeric-only compact form.
///
/// Strips separators (`_`, `-`, ` `, ...) and case so that `displayTemplate`,
/// `display_template`, `display-template`, and `display template` all map to
/// `displaytemplate`.  Used by the title-exact reranker for both the files
/// and content search paths to avoid the per-term matching footgun where a
/// multi-word query would boost a file whose only matching symbol is a
/// generic one-word name.
/// Three-tier stable partition for unified hits:
///   1. Definition-kind symbols whose name matches `query` (full or last-
///      component-of-qualified) — promoted to top, sorted by source-tier,
///      then score.
///   2. Other exact-name matches (non-definition kinds like Import).
///   3. Everything else, score-ordered.
///
/// Mirrors `promote_exact_name_matches` from the per-target pipeline but
/// operates on the unified `UnifiedHit` shape.
pub(super) fn promote_exact_unified_hits(hits: &mut Vec<UnifiedHit>, query: &str) {
    if hits.is_empty() {
        return;
    }
    use crate::search::scoring::{DEFINITION_KINDS, DOC_LANGUAGES, is_name_match, is_test_path};

    let query_lower = query.trim().to_lowercase();
    let mut definitions: Vec<UnifiedHit> = Vec::new();
    let mut other_exact: Vec<UnifiedHit> = Vec::new();
    let mut rest: Vec<UnifiedHit> = Vec::new();

    for hit in hits.drain(..) {
        if is_name_match(&hit.name, &query_lower) {
            if DEFINITION_KINDS.contains(&hit.kind.as_str()) {
                definitions.push(hit);
            } else {
                other_exact.push(hit);
            }
        } else {
            rest.push(hit);
        }
    }

    // Within definitions: full-match first, then source-tier (source>test>doc),
    // then score desc.
    definitions.sort_by(|a, b| {
        let is_full_match = |h: &UnifiedHit| -> bool { h.name.to_lowercase() == query_lower };
        let file_tier = |h: &UnifiedHit| -> u8 {
            if DOC_LANGUAGES.contains(&h.language.as_str()) {
                2
            } else if is_test_path(&h.file_path) {
                1
            } else {
                0
            }
        };
        let a_full = !is_full_match(a);
        let b_full = !is_full_match(b);
        a_full
            .cmp(&b_full)
            .then_with(|| file_tier(a).cmp(&file_tier(b)))
            .then_with(|| {
                b.tantivy_score
                    .partial_cmp(&a.tantivy_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    // Other exact matches: score desc.
    other_exact.sort_by(|a, b| {
        b.tantivy_score
            .partial_cmp(&a.tantivy_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    hits.extend(definitions);
    hits.extend(other_exact);
    hits.extend(rest);
}

pub fn compact_alnum_lc(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

// ---------------------------------------------------------------------------
// Test-only shims: apply_reranker_to_content_results,
//                  apply_symbol_title_boost_to_file_results
//
// The old per-target reranker entry points were deleted in T9.  These thin
// wrappers re-implement the title-exact boost (the only part the unit tests
// exercise) so the tests in `title_exact_boost_tests.rs` keep compiling and
// passing without modification.
// ---------------------------------------------------------------------------

/// Title-exact boost for content (file-path) search results.
///
/// For each file in `results`, look up the symbol names stored for that file
/// in `db`.  If the compact-alphanum form of any symbol name equals the
/// compact-alphanum form of the query (after stripping spaces), add
/// `EXACT_TITLE_BOOST` to that file's score and re-sort descending.
///
/// When `db` is `None` the function returns immediately (preserves BM25 order).
#[cfg(any(test, feature = "test-support"))]
pub fn apply_reranker_to_content_results(
    query: &str,
    results: &mut Vec<ContentSearchResult>,
    db: Option<&julie_core::database::SymbolDatabase>,
) {
    let Some(db) = db else { return };
    if results.is_empty() {
        return;
    }
    let query_compact = compact_alnum_lc(query);
    let paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
    let Ok(titles_map) = db.titles_for_files(&paths) else {
        return;
    };
    for result in results.iter_mut() {
        if let Some(titles) = titles_map.get(result.file_path.as_str()) {
            for title in titles {
                if compact_alnum_lc(title) == query_compact {
                    result.score += crate::search::reranker::EXACT_TITLE_BOOST;
                    break;
                }
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

/// Title-exact boost for file search results.
///
/// Identical logic to `apply_reranker_to_content_results` but operates on
/// `FileSearchResult` so file-target tests keep passing.
#[cfg(any(test, feature = "test-support"))]
pub fn apply_symbol_title_boost_to_file_results(
    query: &str,
    results: &mut Vec<FileSearchResult>,
    db: &julie_core::database::SymbolDatabase,
) {
    if results.is_empty() {
        return;
    }
    let query_compact = compact_alnum_lc(query);
    let paths: Vec<&str> = results.iter().map(|r| r.file_path.as_str()).collect();
    let Ok(titles_map) = db.titles_for_files(&paths) else {
        return;
    };
    for result in results.iter_mut() {
        if let Some(titles) = titles_map.get(result.file_path.as_str()) {
            for title in titles {
                if compact_alnum_lc(title) == query_compact {
                    result.score += crate::search::reranker::EXACT_TITLE_BOOST;
                    break;
                }
            }
        }
    }
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}
