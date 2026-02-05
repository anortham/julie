//! Query building for Tantivy search.
//!
//! Constructs boosted boolean queries for code symbol and file content search.
//! Field boosting ensures name matches rank higher than body matches:
//! - name: 5.0x
//! - signature: 3.0x
//! - doc_comment: 2.0x
//! - code_body: 1.0x

use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::{Field, IndexRecordOption};
use tantivy::Term;

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
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    // Must match doc_type = "symbol"
    let type_term = Term::from_field_text(doc_type_field, "symbol");
    let type_query = TermQuery::new(type_term, IndexRecordOption::Basic);
    subqueries.push((Occur::Must, Box::new(type_query)));

    // Apply optional filters
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

    // Build term-matching clauses as a nested BooleanQuery with Should.
    // This nested query becomes a Must clause, ensuring at least one search
    // term must match in at least one field. Without this nesting, Tantivy's
    // BooleanQuery treats Should as optional when Must clauses are present,
    // which would return ALL documents matching the doc_type filter.
    let mut term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    for term in terms {
        let term_lower = term.to_lowercase();

        let name_term = Term::from_field_text(name_field, &term_lower);
        term_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(name_term, IndexRecordOption::Basic)),
                5.0,
            )),
        ));

        let sig_term = Term::from_field_text(sig_field, &term_lower);
        term_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(sig_term, IndexRecordOption::Basic)),
                3.0,
            )),
        ));

        let doc_term = Term::from_field_text(doc_field, &term_lower);
        term_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(doc_term, IndexRecordOption::Basic)),
                2.0,
            )),
        ));

        let body_term = Term::from_field_text(body_field, &term_lower);
        term_clauses.push((
            Occur::Should,
            Box::new(TermQuery::new(body_term, IndexRecordOption::Basic)),
        ));
    }

    // At least one term must match somewhere
    subqueries.push((Occur::Must, Box::new(BooleanQuery::new(term_clauses))));

    BooleanQuery::new(subqueries)
}

/// Build a file content search query with optional language filter.
///
/// Requires `doc_type = "file"` (Must) and matches terms in the `content` field.
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

    // Build term-matching clauses as a nested BooleanQuery (same pattern as symbol query).
    // Ensures at least one term must match in content.
    let mut term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    for term in terms {
        let term_lower = term.to_lowercase();
        let content_term = Term::from_field_text(content_field, &term_lower);
        term_clauses.push((
            Occur::Should,
            Box::new(TermQuery::new(content_term, IndexRecordOption::Basic)),
        ));
    }

    subqueries.push((Occur::Must, Box::new(BooleanQuery::new(term_clauses))));

    BooleanQuery::new(subqueries)
}
