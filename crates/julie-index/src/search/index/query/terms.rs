use tantivy::Term;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, TermQuery};
use tantivy::schema::IndexRecordOption;

use super::super::{SearchFilter, SearchIndex};
use crate::search::schema::SchemaFields;
use crate::search::tokenizer::split_camel_case;

const ANNOTATION_ORIGINAL_GROUP_WEIGHT: f32 = 5.0;
const ANNOTATION_ALIAS_GROUP_WEIGHT: f32 = 3.5;
const ANNOTATION_NORMALIZED_GROUP_WEIGHT: f32 = 2.5;
const ANNOTATION_NAME_FIELD_BOOST: f32 = 5.0;
const ANNOTATION_SIGNATURE_FIELD_BOOST: f32 = 3.0;
const ANNOTATION_DOC_FIELD_BOOST: f32 = 2.0;
const ANNOTATION_BODY_FIELD_BOOST: f32 = 1.0;
const ANNOTATION_OWNER_FIELD_BOOST: f32 = 4.0;

impl SearchIndex {
    fn tokenize_query(&self, query_str: &str) -> Vec<String> {
        use std::collections::HashSet;

        let mut tokenizer = self
            .index
            .tokenizers()
            .get("code")
            .expect("code tokenizer not registered");

        let mut stream = tokenizer.token_stream(query_str);
        let mut terms = Vec::new();
        let mut seen = HashSet::new();
        while stream.advance() {
            let token = stream.token().text.clone();
            if seen.insert(token.clone()) {
                terms.push(token);
            }
        }
        terms
    }

    /// Public wrapper around `tokenize_query` for the debug search module.
    ///
    /// Shows how the CodeTokenizer splits a query string into individual
    /// search terms (CamelCase splitting, snake_case splitting, stemming, etc.).
    pub fn tokenize_query_public(&self, query_str: &str) -> Vec<String> {
        self.tokenize_query(query_str)
    }

    pub(super) fn tokenize_terms(&self, terms: &[String]) -> Vec<String> {
        use std::collections::HashSet;

        let mut tokenized_terms = Vec::new();
        let mut seen = HashSet::new();
        for term in terms {
            for token in self.tokenize_query(term) {
                if seen.insert(token.clone()) {
                    tokenized_terms.push(token);
                }
            }
        }
        tokenized_terms
    }

    pub(super) fn annotation_context_terms(&self, query: &str) -> Vec<String> {
        use std::collections::HashSet;

        let terms = query
            .split_whitespace()
            .map(|term| {
                term.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
            })
            .filter(|term| !term.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let tokenized_terms = self.tokenize_terms(&terms);
        let token_set: HashSet<String> = tokenized_terms.iter().cloned().collect();
        let mut compound_tokens_to_drop = HashSet::new();

        for term in &terms {
            let camel_parts = split_camel_case(term);
            if camel_parts.len() <= 1 {
                continue;
            }

            let part_tokens = camel_parts
                .iter()
                .flat_map(|part| self.tokenize_query(part))
                .collect::<HashSet<_>>();
            if part_tokens.is_empty() || !part_tokens.iter().all(|part| token_set.contains(part)) {
                continue;
            }

            let term_lower = term.to_ascii_lowercase();
            if token_set.contains(&term_lower) {
                compound_tokens_to_drop.insert(term_lower);
            }
        }

        Self::filter_compound_tokens(
            tokenized_terms
                .into_iter()
                .filter(|token| !compound_tokens_to_drop.contains(token))
                .collect(),
        )
    }

    /// Remove compound tokens whose snake_case sub-parts are all present in the list.
    ///
    /// The CodeTokenizer emits the full form plus atomic sub-parts, but never
    /// partial compounds. For example, `search_term_one` produces tokens
    /// `[search_term_one, search, term, one]` — there is no `search_term` token.
    ///
    /// When a query like `"search_term"` tokenizes to `[search_term, search, term]`,
    /// requiring ALL tokens via AND would fail because `search_term` doesn't exist
    /// in documents indexed as `search_term_one`. By filtering out `search_term`
    /// (whose parts `search` and `term` are already present), we get clean AND
    /// semantics on just the atomic parts.
    pub(super) fn filter_compound_tokens(tokens: Vec<String>) -> Vec<String> {
        use std::collections::HashSet;
        let token_set: HashSet<String> = tokens.iter().cloned().collect();
        tokens
            .into_iter()
            .filter(|token| {
                let parts: Vec<&str> = token.split('_').collect();
                if parts.len() <= 1 {
                    return true; // Not a snake_case compound, keep it
                }
                // Keep if any sub-part is missing from the token set
                !parts
                    .iter()
                    .all(|part| !part.is_empty() && token_set.contains(*part))
            })
            .collect()
    }
}

pub(super) fn build_annotation_symbol_query(
    original_terms: &[String],
    alias_terms: &[String],
    normalized_terms: &[String],
    annotation_keys: &[String],
    f: &SchemaFields,
    filter: &SearchFilter,
    require_all_terms: bool,
) -> BooleanQuery {
    let mut subqueries: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    let type_term = Term::from_field_text(f.doc_type, "symbol");
    subqueries.push((
        Occur::Must,
        Box::new(TermQuery::new(type_term, IndexRecordOption::Basic)),
    ));

    if let Some(language) = filter.language.as_deref() {
        let lang_term = Term::from_field_text(f.language, language);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(lang_term, IndexRecordOption::Basic)),
        ));
    }
    if let Some(kind) = filter.kind.as_deref() {
        let kind_term = Term::from_field_text(f.kind, kind);
        subqueries.push((
            Occur::Must,
            Box::new(TermQuery::new(kind_term, IndexRecordOption::Basic)),
        ));
    }
    for key in annotation_keys {
        let key = key.trim().to_ascii_lowercase();
        if !key.is_empty() {
            let annotation_term = Term::from_field_text(f.annotations_exact, &key);
            subqueries.push((
                Occur::Must,
                Box::new(TermQuery::new(annotation_term, IndexRecordOption::Basic)),
            ));
        }
    }

    let mut term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
    let grouped_terms = [
        (original_terms, ANNOTATION_ORIGINAL_GROUP_WEIGHT, true),
        (alias_terms, ANNOTATION_ALIAS_GROUP_WEIGHT, false),
        (normalized_terms, ANNOTATION_NORMALIZED_GROUP_WEIGHT, false),
    ];

    for (terms, group_weight, is_original_group) in grouped_terms {
        let group_factor = group_weight / ANNOTATION_ORIGINAL_GROUP_WEIGHT;
        let mut group_term_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

        for term in terms {
            let term_lower = term.to_lowercase();
            let mut field_clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
            push_boosted_term(
                &mut field_clauses,
                f.name,
                &term_lower,
                ANNOTATION_NAME_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.signature,
                &term_lower,
                ANNOTATION_SIGNATURE_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.doc_comment,
                &term_lower,
                ANNOTATION_DOC_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.code_body,
                &term_lower,
                ANNOTATION_BODY_FIELD_BOOST * group_factor,
            );
            push_boosted_term(
                &mut field_clauses,
                f.owner_names_text,
                &term_lower,
                ANNOTATION_OWNER_FIELD_BOOST * group_factor,
            );

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

    if term_clauses.is_empty() {
        return BooleanQuery::new(subqueries);
    }
    if require_all_terms {
        subqueries.extend(term_clauses);
    } else {
        subqueries.push((Occur::Must, Box::new(BooleanQuery::new(term_clauses))));
    }

    BooleanQuery::new(subqueries)
}

fn push_boosted_term(
    clauses: &mut Vec<(Occur, Box<dyn tantivy::query::Query>)>,
    field: tantivy::schema::Field,
    term: &str,
    boost: f32,
) {
    let term = Term::from_field_text(field, term);
    clauses.push((
        Occur::Should,
        Box::new(BoostQuery::new(
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
            boost,
        )),
    ));
}
