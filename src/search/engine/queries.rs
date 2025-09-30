use super::result::SearchResult;
use super::utils::{capitalize_first_letter, to_camel_case, to_pascal_case};
use super::SearchEngine;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::Term;
use tracing::{debug, info, trace};

use super::super::schema::QueryIntent;

impl SearchEngine {
    /// Perform intelligent search with intent detection
    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let start_time = std::time::Instant::now();

        debug!(
            "🔍 Search started: query='{}', length={}",
            query,
            query.len()
        );

        let intent = self.query_processor.detect_intent(query);
        debug!("🎯 Intent detected: {:?}", intent);

        let processed_query = self.query_processor.transform_query(query, &intent);
        trace!("📝 Processed query: '{}' -> '{}'", query, processed_query);

        let results = match intent {
            QueryIntent::ExactSymbol => self.exact_symbol_search(&processed_query).await,
            QueryIntent::GenericType => self.generic_type_search(&processed_query).await,
            QueryIntent::OperatorSearch => self.operator_search(&processed_query).await,
            QueryIntent::FilePath => self.file_path_search(&processed_query).await,
            QueryIntent::SemanticConcept => self.semantic_search(&processed_query).await,
            QueryIntent::Mixed(intents) => self.mixed_search(query, &intents).await,
            _ => self.semantic_search(query).await,
        };

        let elapsed = start_time.elapsed();
        match &results {
            Ok(search_results) => {
                info!(
                    "✅ Search completed: query='{}', results={}, time={:.2}ms",
                    query,
                    search_results.len(),
                    elapsed.as_secs_f64() * 1000.0
                );
                if let Some(top) = search_results.first() {
                    debug!(
                        "📋 Top result: {} in {}",
                        top.symbol.name, top.symbol.file_path
                    );
                }
            }
            Err(error) => {
                info!(
                    "❌ Search failed: query='{}', error='{}', time={:.2}ms",
                    query,
                    error,
                    elapsed.as_secs_f64() * 1000.0
                );
            }
        }

        results
    }

    pub async fn exact_symbol_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let clean_query = query.trim_matches('"');

        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let term = Term::from_field_text(fields.symbol_name_exact, clean_query);
        let term_query = TermQuery::new(term, tantivy::schema::IndexRecordOption::WithFreqs);

        let mut top_docs = searcher.search(&term_query, &TopDocs::with_limit(50))?;

        if top_docs.is_empty() {
            let variations = vec![
                clean_query.to_lowercase(),
                clean_query.to_uppercase(),
                capitalize_first_letter(clean_query),
                to_pascal_case(clean_query),
                to_camel_case(clean_query),
            ];

            for variation in variations {
                if variation != clean_query {
                    let term = Term::from_field_text(fields.symbol_name_exact, &variation);
                    let variation_query =
                        TermQuery::new(term, tantivy::schema::IndexRecordOption::WithFreqs);
                    let variation_docs =
                        searcher.search(&variation_query, &TopDocs::with_limit(50))?;

                    if !variation_docs.is_empty() {
                        top_docs = variation_docs;
                        break;
                    }
                }
            }
        }

        if top_docs.is_empty() {
            debug!("🔍 Exact match failed, trying camelCase-aware wildcard search");

            let camelcase_variants = self.expand_camelcase_query(clean_query);
            debug!("🐪 Generated camelCase variants: {:?}", camelcase_variants);

            let query_parser = QueryParser::for_index(&self.index, vec![fields.symbol_name, fields.code_context]);

            for variant in camelcase_variants {
                match query_parser.parse_query(&variant) {
                    Ok(wildcard_query) => {
                        let wildcard_docs =
                            searcher.search(&*wildcard_query, &TopDocs::with_limit(50))?;
                        if !wildcard_docs.is_empty() {
                            debug!(
                                "🎯 CamelCase wildcard found {} results with pattern: '{}'",
                                wildcard_docs.len(),
                                variant
                            );
                            top_docs = wildcard_docs;
                            break;
                        }
                    }
                    Err(error) => {
                        debug!(
                            "❌ Failed to parse camelCase pattern '{}': {}",
                            variant, error
                        );
                    }
                }
            }
        }

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let result = self.document_to_search_result(&doc)?;
            results.push(result);
        }

        Ok(results)
    }

    async fn generic_type_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let base_type = self.extract_generic_base(query);
        let inner_types = self.extract_generic_types(query);

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![
                fields.signature,
                fields.signature_exact,
                fields.symbol_name,
                fields.all_text,
                fields.code_context,  // Support FILE_CONTENT in generic type search
            ],
        );

        let mut search_terms = vec![query.to_string()];
        search_terms.push(base_type.clone());
        search_terms.extend(inner_types);

        let combined_query = search_terms.join(" OR ");
        let parsed_query = query_parser.parse_query(&combined_query)?;

        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let mut result = self.document_to_search_result(&doc)?;

            if result.snippet.contains(query) || result.symbol.name.contains(query) {
                result.score = score * 1.5;
            } else {
                result.score = score;
            }

            results.push(result);
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(results)
    }

    fn extract_generic_base(&self, query: &str) -> String {
        if let Some(position) = query.find('<') {
            query[..position].to_string()
        } else {
            query.to_string()
        }
    }

    fn extract_generic_types(&self, query: &str) -> Vec<String> {
        if let Some(start) = query.find('<') {
            if let Some(end) = query.rfind('>') {
                let inner = &query[start + 1..end];
                return inner
                    .split(',')
                    .map(|segment| segment.trim().to_string())
                    .filter(|segment| !segment.is_empty())
                    .collect();
            }
        }
        vec![]
    }

    async fn operator_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let term = Term::from_field_text(fields.signature, query);
        let term_query = TermQuery::new(term, tantivy::schema::IndexRecordOption::WithFreqs);

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![fields.signature, fields.signature_exact, fields.all_text, fields.code_context],
        );

        let escaped_query = format!("\"{}\"", query);
        let parsed_query = match query_parser.parse_query(&escaped_query) {
            Ok(parsed) => parsed,
            Err(_) => Box::new(term_query) as Box<dyn Query>,
        };

        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let mut result = self.document_to_search_result(&doc)?;

            if result.snippet.contains(query) {
                result.score = score * 2.0;
            } else {
                result.score = score;
            }

            results.push(result);
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(results)
    }

    async fn file_path_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let query_parser = QueryParser::for_index(&self.index, vec![fields.file_path]);
        let parsed_query = query_parser.parse_query(query)?;

        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let result = self.document_to_search_result(&doc)?;
            results.push(result);
        }

        Ok(results)
    }

    async fn semantic_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let query_parser = QueryParser::for_index(
            &self.index,
            vec![
                fields.all_text,
                fields.symbol_name,
                fields.signature,
                fields.doc_comment,
                fields.code_context,  // Query directly for FILE_CONTENT with standard tokenizer
            ],
        );

        let raw_terms: Vec<&str> = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .collect();
        let simple_term_count = raw_terms
            .iter()
            .filter(|term| {
                term.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            })
            .count();

        let use_or_expansion = raw_terms.len() > 1 && simple_term_count == raw_terms.len();

        debug!(
            "🔍 Query analysis: terms={:?}, use_or_expansion={}",
            raw_terms, use_or_expansion
        );

        let parsed_query: Box<dyn Query> = if use_or_expansion {
            let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

            if let Ok(phrase_query) = query_parser.parse_query(query) {
                clauses.push((Occur::Should, phrase_query));
            }

            let mut seen_terms = HashSet::new();
            for term in &raw_terms {
                let normalized = term.to_lowercase();
                if !seen_terms.insert(normalized) {
                    continue;
                }

                let camelcase_variants = self.expand_camelcase_query(term);
                debug!(
                    "🐪 Expanding OR term '{}' to variants: {:?}",
                    term, camelcase_variants
                );

                if let Ok(original_query) = query_parser.parse_query(term) {
                    clauses.push((Occur::Should, original_query));
                }

                for variant in camelcase_variants {
                    if let Ok(parsed_variant) = query_parser.parse_query(&variant) {
                        clauses.push((Occur::Should, parsed_variant));
                    }
                }
            }

            Box::new(BooleanQuery::new(clauses))
        } else {
            query_parser.parse_query(query)?
        };

        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(100))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let mut result = self.document_to_search_result(&doc)?;
            result.score = score;
            results.push(result);
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(results)
    }

    async fn mixed_search(
        &self,
        query: &str,
        intents: &[QueryIntent],
    ) -> Result<Vec<SearchResult>> {
        let mut all_results = Vec::new();

        for intent in intents {
            let results = match intent {
                QueryIntent::ExactSymbol => {
                    let symbols: Vec<String> = query
                        .split_whitespace()
                        .filter(|word| {
                            word.chars().all(|c| c.is_alphanumeric() || c == '_') && word.len() > 2
                        })
                        .map(|word| word.to_string())
                        .collect();

                    if let Some(symbol) = symbols.first() {
                        self.exact_symbol_search(symbol).await?
                    } else {
                        vec![]
                    }
                }
                QueryIntent::FilePath => {
                    let paths: Vec<&str> = query
                        .split_whitespace()
                        .filter(|word| word.contains('/') || word.contains('.'))
                        .collect();

                    if let Some(path) = paths.first() {
                        self.file_path_search(path).await?
                    } else {
                        vec![]
                    }
                }
                QueryIntent::GenericType => {
                    if let Some(_start) = query.find('<') {
                        if let Some(end) = query.find('>') {
                            let generic_part = &query[..=end];
                            self.generic_type_search(generic_part).await?
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    }
                }
                _ => self.semantic_search(query).await?,
            };

            all_results.extend(results);
        }

        let mut unique_results: HashMap<String, SearchResult> = HashMap::new();

        for result in all_results {
            let key = format!("{}:{}", result.symbol.id, result.symbol.file_path);
            match unique_results.get_mut(&key) {
                Some(existing) => {
                    existing.score = existing.score.max(result.score);
                }
                None => {
                    unique_results.insert(key, result);
                }
            }
        }

        let mut final_results: Vec<SearchResult> = unique_results.into_values().collect();
        final_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        Ok(final_results)
    }

    fn expand_camelcase_query(&self, query: &str) -> Vec<String> {
        let mut variants = Vec::new();

        let lowercase = query.to_lowercase();
        let capitalized = capitalize_first_letter(&lowercase);

        variants.push(format!("*{}*", lowercase));
        variants.push(format!("*{}*", capitalized));

        if query.chars().any(|c| c.is_uppercase()) && query != capitalized {
            variants.push(format!("*{}*", query));
        }

        variants
    }
}
