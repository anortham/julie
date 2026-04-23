//! Query building for Tantivy search.
//!
//! Constructs boosted boolean queries for code symbol and file content search.
//! Field boosting ensures name matches rank higher than body matches:
//! - name: 5.0x
//! - signature: 3.0x
//! - doc_comment: 2.0x
//! - code_body: 1.0x

use std::collections::HashSet;

use tantivy::Term;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::{Field, IndexRecordOption};

use crate::extractors::normalize_annotations;

const ORIGINAL_GROUP_WEIGHT: f32 = 5.0;
const ALIAS_GROUP_WEIGHT: f32 = 3.5;
const NORMALIZED_GROUP_WEIGHT: f32 = 2.5;

const NAME_FIELD_BOOST: f32 = 5.0;
const SIGNATURE_FIELD_BOOST: f32 = 3.0;
const DOC_FIELD_BOOST: f32 = 2.0;
const BODY_FIELD_BOOST: f32 = 1.0;
const FILE_PATH_EXACT_BOOST: f32 = 40.0;
const BASENAME_EXACT_BOOST: f32 = 25.0;
const PATH_TEXT_TERM_BOOST: f32 = 3.0;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ParsedAnnotationQuery {
    pub annotation_keys: Vec<String>,
    pub remaining_query: String,
}

impl ParsedAnnotationQuery {
    pub(crate) fn has_annotation_filters(&self) -> bool {
        !self.annotation_keys.is_empty()
    }
}

/// Split annotation-prefixed terms from normal definition search text.
pub(crate) fn parse_annotation_query(query: &str) -> ParsedAnnotationQuery {
    let mut annotation_keys = Vec::new();
    let mut seen_annotation_keys = HashSet::new();
    let mut remaining_terms = Vec::new();

    for token in query.split_whitespace() {
        if let Some((raw_annotation, language)) = annotation_token(token) {
            let markers = normalize_annotations(&[raw_annotation], language);
            if markers.is_empty() {
                remaining_terms.push(token.to_string());
            } else {
                for marker in markers {
                    let key = marker.annotation_key.trim().to_ascii_lowercase();
                    if !key.is_empty() && seen_annotation_keys.insert(key.clone()) {
                        annotation_keys.push(key);
                    }
                }
            }
            continue;
        }

        remaining_terms.push(token.to_string());
    }

    ParsedAnnotationQuery {
        annotation_keys,
        remaining_query: remaining_terms.join(" "),
    }
}

fn annotation_token(token: &str) -> Option<(&str, &'static str)> {
    let token = token.trim_matches(|ch| matches!(ch, ',' | ';'));
    if token.starts_with("#[") && token.ends_with(']') {
        return Some((token, "rust"));
    }
    if token.starts_with("[[") && token.ends_with("]]") {
        return Some((token, "cpp"));
    }
    if token.starts_with('[') && token.ends_with(']') {
        return Some((token, "csharp"));
    }
    token.starts_with('@').then_some((token, "python"))
}

/// Build a boosted symbol search query with optional filters.
///
/// Requires `doc_type = "symbol"` (Must) and boosts term matches across fields:
/// - `name` field at 5.0x (highest priority)
/// - `signature` field at 3.0x
/// - `doc_comment` field at 2.0x
/// - `code_body` field at 1.0x (baseline)
///
/// Optional `language` and `kind` filters are applied as Must clauses.
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
    require_all_terms: bool,
) -> BooleanQuery {
    build_symbol_query_weighted(
        terms,
        &[],
        &[],
        name_field,
        sig_field,
        doc_field,
        body_field,
        doc_type_field,
        language_field,
        kind_field,
        language_filter,
        kind_filter,
        require_all_terms,
    )
}

/// Build a boosted symbol search query with weighted term groups.
///
/// Group weights default to:
/// - original terms: 5.0
/// - alias terms: 3.5
/// - normalized terms: 2.5
pub fn build_symbol_query_weighted(
    original_terms: &[String],
    alias_terms: &[String],
    normalized_terms: &[String],
    name_field: Field,
    sig_field: Field,
    doc_field: Field,
    body_field: Field,
    doc_type_field: Field,
    language_field: Field,
    kind_field: Field,
    language_filter: Option<&str>,
    kind_filter: Option<&str>,
    require_all_terms: bool,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    // Must match doc_type = "symbol" — always required regardless of mode
    let type_term = Term::from_field_text(doc_type_field, "symbol");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    // Apply optional filters — always Must regardless of mode
    if let Some(lang) = language_filter {
        let lang_term = Term::from_field_text(language_field, lang);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lang_term, IndexRecordOption::Basic)),
        ));
    }
    if let Some(k) = kind_filter {
        let kind_term = Term::from_field_text(kind_field, k);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(kind_term, IndexRecordOption::Basic)),
        ));
    }

    // Build per-term field sub-queries.
    // Within each term, the field variants are OR'd (Should) so "select" can match
    // in name OR signature OR doc OR body.
    let mut term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
    let grouped_terms = [
        (original_terms, ORIGINAL_GROUP_WEIGHT, true),
        (alias_terms, ALIAS_GROUP_WEIGHT, false),
        (normalized_terms, NORMALIZED_GROUP_WEIGHT, false),
    ];

    for (terms, group_weight, is_original_group) in grouped_terms {
        let group_factor = group_weight / ORIGINAL_GROUP_WEIGHT;
        let mut group_term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        for term in terms {
            let term_lower = term.to_lowercase();

            let mut field_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

            let name_term = Term::from_field_text(name_field, &term_lower);
            field_clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(name_term, IndexRecordOption::Basic)),
                    NAME_FIELD_BOOST * group_factor,
                )),
            ));

            let sig_term = Term::from_field_text(sig_field, &term_lower);
            field_clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(sig_term, IndexRecordOption::Basic)),
                    SIGNATURE_FIELD_BOOST * group_factor,
                )),
            ));

            let doc_term = Term::from_field_text(doc_field, &term_lower);
            field_clauses.push((
                Occur::Should,
                Box::new(BoostQuery::new(
                    Box::new(TermQuery::new(doc_term, IndexRecordOption::Basic)),
                    DOC_FIELD_BOOST * group_factor,
                )),
            ));

            let body_term = Term::from_field_text(body_field, &term_lower);
            let body_query = BoostQuery::new(
                Box::new(TermQuery::new(body_term, IndexRecordOption::Basic)),
                BODY_FIELD_BOOST * group_factor,
            );
            field_clauses.push((Occur::Should, Box::new(body_query)));

            // In AND mode, each term is Must (all terms required).
            // In OR mode, each term is Should (any term can match).
            let term_occur = if require_all_terms && is_original_group {
                Occur::Must
            } else {
                Occur::Should
            };
            group_term_clauses.push((term_occur, Box::new(BooleanQuery::new(field_clauses))));
        }

        if !group_term_clauses.is_empty() {
            let group_occur = if require_all_terms && is_original_group {
                Occur::Must
            } else {
                Occur::Should
            };
            term_clauses.push((group_occur, Box::new(BooleanQuery::new(group_term_clauses))));
        }
    }

    if require_all_terms {
        // AND mode: add term clauses directly — each is Must so all are required
        subqueries.extend(term_clauses);
    } else {
        // OR mode: wrap all Should term clauses in their own BooleanQuery, then
        // add that wrapper as Must. This ensures at least one term must match
        // (Tantivy treats Should clauses as optional when Must clauses exist,
        // so without wrapping, every symbol document would match).
        let terms_query = BooleanQuery::new(term_clauses);
        subqueries.push((Occur::Must, Box::new(terms_query)));
    }

    BooleanQuery::new(subqueries)
}

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
    require_all_terms: bool,
) -> BooleanQuery {
    build_content_query_weighted(
        terms,
        &[],
        &[],
        content_field,
        doc_type_field,
        language_field,
        language_filter,
        require_all_terms,
    )
}

/// Build a file content search query with weighted term groups.
///
/// Group weights default to:
/// - original terms: 5.0
/// - alias terms: 3.5
/// - normalized terms: 2.5
pub fn build_content_query_weighted(
    original_terms: &[String],
    alias_terms: &[String],
    normalized_terms: &[String],
    content_field: Field,
    doc_type_field: Field,
    language_field: Field,
    language_filter: Option<&str>,
    require_all_terms: bool,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    // Must match doc_type = "file" — always required regardless of mode
    let type_term = Term::from_field_text(doc_type_field, "file");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    // Apply optional language filter — always Must regardless of mode
    if let Some(lang) = language_filter {
        let lang_term = Term::from_field_text(language_field, lang);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lang_term, IndexRecordOption::Basic)),
        ));
    }

    let mut term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
    let grouped_terms = [
        (original_terms, ORIGINAL_GROUP_WEIGHT, true),
        (alias_terms, ALIAS_GROUP_WEIGHT, false),
        (normalized_terms, NORMALIZED_GROUP_WEIGHT, false),
    ];

    for (terms, group_weight, is_original_group) in grouped_terms {
        let group_factor = group_weight / ORIGINAL_GROUP_WEIGHT;

        for term in terms {
            let term_lower = term.to_lowercase();
            let content_term = Term::from_field_text(content_field, &term_lower);
            let term_query = TermQuery::new(content_term, IndexRecordOption::Basic);

            // Heuristic: underscores indicate snake_case compound tokens from CodeTokenizer.
            // CamelCase compounds are lowercased without underscores, so they pass through as atomic.
            if term.contains('_') {
                // Compound token → SHOULD with boost (promotes exact identifier matches)
                term_clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(term_query), 5.0 * group_factor)),
                ));
            } else if require_all_terms && is_original_group {
                // AND mode: atomic sub-part → MUST (ensures file contains the word)
                term_clauses.push((
                    Occur::Must,
                    Box::new(BoostQuery::new(Box::new(term_query), group_factor)),
                ));
            } else {
                // OR mode: atomic sub-part → SHOULD (partial matches allowed)
                term_clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(term_query), group_factor)),
                ));
            }
        }
    }

    if require_all_terms {
        // AND mode: add term clauses directly — Must clauses ensure all terms required
        subqueries.extend(term_clauses);
    } else {
        // OR mode: wrap all Should term clauses in their own BooleanQuery, then
        // add that wrapper as Must. This ensures at least one term must match
        // (Tantivy treats Should clauses as optional when Must clauses exist,
        // so without wrapping, every file document would match).
        let terms_query = BooleanQuery::new(term_clauses);
        subqueries.push((Occur::Must, Box::new(terms_query)));
    }

    BooleanQuery::new(subqueries)
}

pub fn build_file_query(
    path_terms: &[String],
    file_path_field: Field,
    basename_field: Field,
    path_text_field: Field,
    doc_type_field: Field,
    language_field: Field,
    language_filter: Option<&str>,
    exact_path: Option<&str>,
    exact_basename: Option<&str>,
    require_all_terms: bool,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    let type_term = Term::from_field_text(doc_type_field, "file");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    if let Some(lang) = language_filter {
        let lang_term = Term::from_field_text(language_field, lang);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lang_term, IndexRecordOption::Basic)),
        ));
    }

    let mut ranking_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
    if let Some(path) = exact_path {
        let path_term = Term::from_field_text(file_path_field, path);
        ranking_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(path_term, IndexRecordOption::Basic)),
                FILE_PATH_EXACT_BOOST,
            )),
        ));
    }
    if let Some(basename) = exact_basename {
        let basename_term = Term::from_field_text(basename_field, basename);
        ranking_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(basename_term, IndexRecordOption::Basic)),
                BASENAME_EXACT_BOOST,
            )),
        ));
    }

    let mut path_term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
    for term in path_terms {
        let path_term = Term::from_field_text(path_text_field, &term.to_lowercase());
        let occur = if require_all_terms {
            Occur::Must
        } else {
            Occur::Should
        };
        path_term_clauses.push((
            occur,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(path_term, IndexRecordOption::Basic)),
                PATH_TEXT_TERM_BOOST,
            )),
        ));
    }

    if require_all_terms {
        if path_term_clauses.is_empty() {
            if !ranking_clauses.is_empty() {
                subqueries.push((Occur::Must, Box::new(BooleanQuery::new(ranking_clauses))));
            }
        } else {
            subqueries.extend(path_term_clauses);
            if !ranking_clauses.is_empty() {
                subqueries.push((Occur::Should, Box::new(BooleanQuery::new(ranking_clauses))));
            }
        }
    } else {
        let mut any_match_clauses = ranking_clauses;
        any_match_clauses.extend(path_term_clauses);
        if !any_match_clauses.is_empty() {
            subqueries.push((Occur::Must, Box::new(BooleanQuery::new(any_match_clauses))));
        }
    }

    BooleanQuery::new(subqueries)
}
