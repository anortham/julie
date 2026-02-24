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

    for term in terms {
        let term_lower = term.to_lowercase();

        let mut field_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        let name_term = Term::from_field_text(name_field, &term_lower);
        field_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(name_term, IndexRecordOption::Basic)),
                5.0,
            )),
        ));

        let sig_term = Term::from_field_text(sig_field, &term_lower);
        field_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(sig_term, IndexRecordOption::Basic)),
                3.0,
            )),
        ));

        let doc_term = Term::from_field_text(doc_field, &term_lower);
        field_clauses.push((
            Occur::Should,
            Box::new(BoostQuery::new(
                Box::new(TermQuery::new(doc_term, IndexRecordOption::Basic)),
                2.0,
            )),
        ));

        let body_term = Term::from_field_text(body_field, &term_lower);
        field_clauses.push((
            Occur::Should,
            Box::new(TermQuery::new(body_term, IndexRecordOption::Basic)),
        ));

        // In AND mode, each term is Must (all terms required).
        // In OR mode, each term is Should (any term can match).
        let term_occur = if require_all_terms {
            Occur::Must
        } else {
            Occur::Should
        };
        term_clauses.push((term_occur, Box::new(BooleanQuery::new(field_clauses))));
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
                Box::new(BoostQuery::new(Box::new(term_query), 5.0)),
            ));
        } else if require_all_terms {
            // AND mode: atomic sub-part → MUST (ensures file contains the word)
            term_clauses.push((Occur::Must, Box::new(term_query)));
        } else {
            // OR mode: atomic sub-part → SHOULD (partial matches allowed)
            term_clauses.push((Occur::Should, Box::new(term_query)));
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
