// Julie's Search Engine Module - The Search Accelerator
//
// This module provides lightning-fast code search using Tantivy with custom
// code-aware tokenizers for sub-10ms search performance across large codebases.

pub mod tokenizers;
pub mod schema;

use anyhow::Result;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{Field, Value};
use std::path::Path;
use std::sync::Arc;

use crate::extractors::Symbol;
use self::schema::{CodeSearchSchema, SearchDocument, QueryProcessor, QueryIntent, LanguageBoosting};
use self::tokenizers::CodeAwareTokenizer;

/// Main search engine implementing the Search Accelerator pillar
pub struct SearchEngine {
    /// Tantivy index for fast text search
    index: Index,
    /// Schema for structured search
    schema: CodeSearchSchema,
    /// Index reader for queries
    reader: IndexReader,
    /// Index writer for updates
    writer: IndexWriter,
    /// Query processor for intent detection
    query_processor: QueryProcessor,
    /// Language-specific boosting
    language_boosting: LanguageBoosting,
}

impl SearchEngine {
    /// Create a new search engine with the given index path
    pub fn new<P: AsRef<Path>>(index_path: P) -> Result<Self> {
        let schema = CodeSearchSchema::new()?;
        let index = Index::create_in_dir(&index_path, schema.schema().clone())?;

        let reader = index.reader()?;

        let writer = index.writer(50_000_000)?; // 50MB heap
        let query_processor = QueryProcessor::new()?;
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            index,
            schema,
            reader,
            writer,
            query_processor,
            language_boosting,
        })
    }

    /// Create a search engine in RAM for testing
    pub fn in_memory() -> Result<Self> {
        let schema = CodeSearchSchema::new()?;
        let index = Index::create_in_ram(schema.schema().clone());

        let reader = index.reader()?;

        let writer = index.writer(15_000_000)?; // 15MB heap minimum for testing
        let query_processor = QueryProcessor::new()?;
        let language_boosting = LanguageBoosting::new();

        Ok(Self {
            index,
            schema,
            reader,
            writer,
            query_processor,
            language_boosting,
        })
    }

    /// Index a batch of symbols
    pub async fn index_symbols(&mut self, symbols: Vec<Symbol>) -> Result<()> {
        for symbol in symbols {
            let doc = SearchDocument {
                symbol_id: symbol.id.clone(),
                symbol_name: symbol.name.clone(),
                symbol_kind: symbol.kind.to_string(),
                language: symbol.language.clone(),
                file_path: symbol.file_path.clone(),
                signature: symbol.signature.clone(),
                doc_comment: symbol.doc_comment.clone(),
                code_context: None, // TODO: Extract from source
                start_line: symbol.start_line,
                end_line: symbol.end_line,
                metadata: symbol.metadata.as_ref()
                    .map(|m| serde_json::to_string(m).unwrap_or_default()),
                semantic_group: symbol.semantic_group.clone(),
                confidence: symbol.confidence.map(|c| c as f64),
            };

            self.add_document(doc)?;
        }

        self.commit().await?;
        Ok(())
    }

    /// Add a single document to the index
    fn add_document(&mut self, doc: SearchDocument) -> Result<()> {
        let mut tantivy_doc = tantivy::doc!();
        let fields = self.schema.fields();

        // Add all fields to the document
        tantivy_doc.add_text(fields.symbol_id, &doc.symbol_id);
        tantivy_doc.add_text(fields.symbol_name, &doc.symbol_name);
        tantivy_doc.add_text(fields.symbol_name_exact, &doc.symbol_name);
        tantivy_doc.add_text(fields.symbol_kind, &doc.symbol_kind);
        tantivy_doc.add_text(fields.language, &doc.language);
        tantivy_doc.add_text(fields.file_path, &doc.file_path);
        tantivy_doc.add_text(fields.file_path_exact, &doc.file_path);

        if let Some(sig) = &doc.signature {
            tantivy_doc.add_text(fields.signature, sig);
            tantivy_doc.add_text(fields.signature_exact, sig);
        }

        if let Some(doc_comment) = &doc.doc_comment {
            tantivy_doc.add_text(fields.doc_comment, doc_comment);
        }

        if let Some(context) = &doc.code_context {
            tantivy_doc.add_text(fields.code_context, context);
        }

        tantivy_doc.add_u64(fields.start_line, doc.start_line as u64);
        tantivy_doc.add_u64(fields.end_line, doc.end_line as u64);

        if let Some(metadata) = &doc.metadata {
            tantivy_doc.add_text(fields.metadata, metadata);
        }

        if let Some(semantic_group) = &doc.semantic_group {
            tantivy_doc.add_text(fields.semantic_group, semantic_group);
        }

        if let Some(confidence) = doc.confidence {
            tantivy_doc.add_f64(fields.confidence, confidence);
        }

        // Generate combined text fields
        let all_text = doc.generate_all_text();
        tantivy_doc.add_text(fields.all_text, &all_text);

        let exact_matches = doc.generate_exact_matches();
        tantivy_doc.add_text(fields.exact_matches, &exact_matches);

        let language_boost = self.language_boosting.get_language_boost(&doc.language);
        tantivy_doc.add_f64(fields.language_boost, language_boost);

        self.writer.add_document(tantivy_doc)?;
        Ok(())
    }

    /// Commit pending changes to the index
    pub async fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Perform intelligent search with intent detection
    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let intent = self.query_processor.detect_intent(query);
        let processed_query = self.query_processor.transform_query(query, &intent);

        match intent {
            QueryIntent::ExactSymbol => self.exact_symbol_search(&processed_query).await,
            QueryIntent::GenericType => self.generic_type_search(&processed_query).await,
            QueryIntent::OperatorSearch => self.operator_search(&processed_query).await,
            QueryIntent::FilePath => self.file_path_search(&processed_query).await,
            QueryIntent::SemanticConcept => self.semantic_search(&processed_query).await,
            QueryIntent::Mixed(intents) => self.mixed_search(query, &intents).await,
            _ => self.semantic_search(query).await, // Default fallback
        }
    }

    /// Exact symbol name search
    async fn exact_symbol_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        // Remove quotes from processed query for term search
        let clean_query = query.trim_matches('"');

        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let term = Term::from_field_text(fields.symbol_name_exact, clean_query);
        let term_query = TermQuery::new(term, tantivy::schema::IndexRecordOption::WithFreqs);

        let top_docs = searcher.search(&term_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let result = self.document_to_search_result(&doc)?;
            results.push(result);
        }

        Ok(results)
    }

    /// Generic type search (List<User>, Map<K,V>)
    async fn generic_type_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        // Extract generic components for broader matching
        let base_type = self.extract_generic_base(query);
        let inner_types = self.extract_generic_types(query);

        // Build a boolean query to match both exact and component searches
        let query_parser = QueryParser::for_index(&self.index, vec![
            fields.signature,
            fields.signature_exact,
            fields.symbol_name,
            fields.all_text,
        ]);

        // Search for exact match first, then components
        let mut search_terms = vec![query.to_string()]; // Exact match
        search_terms.push(base_type.clone());           // Base type (List, Map, etc.)
        search_terms.extend(inner_types);               // Inner types (User, String, etc.)

        let combined_query = search_terms.join(" OR ");
        let parsed_query = query_parser.parse_query(&combined_query)?;

        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let mut result = self.document_to_search_result(&doc)?;

            // Boost exact generic matches
            if result.snippet.contains(query) || result.symbol_name.contains(query) {
                result.score = score * 1.5;
            } else {
                result.score = score;
            }

            results.push(result);
        }

        // Sort by relevance
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(results)
    }

    /// Extract base type from generic (List<User> -> List)
    fn extract_generic_base(&self, query: &str) -> String {
        if let Some(pos) = query.find('<') {
            query[..pos].to_string()
        } else {
            query.to_string()
        }
    }

    /// Extract inner types from generic (List<User> -> [User], Map<K,V> -> [K,V])
    fn extract_generic_types(&self, query: &str) -> Vec<String> {
        if let Some(start) = query.find('<') {
            if let Some(end) = query.rfind('>') {
                let inner = &query[start + 1..end];
                return inner.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        vec![]
    }

    /// Operator-based search (&& ||, =>)
    async fn operator_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        // Use signature field for operator search since operators appear in code signatures
        let term = Term::from_field_text(fields.signature, query);
        let term_query = TermQuery::new(term, tantivy::schema::IndexRecordOption::WithFreqs);

        // Also search in all_text field for broader coverage
        let query_parser = QueryParser::for_index(&self.index, vec![
            fields.signature,
            fields.signature_exact,
            fields.all_text,
        ]);

        // Escape the operator for query parsing and use quotes for exact matching
        let escaped_query = format!("\"{}\"", query);
        let parsed_query = match query_parser.parse_query(&escaped_query) {
            Ok(q) => q,
            Err(_) => {
                // Fallback to term query if parsing fails
                Box::new(term_query) as Box<dyn Query>
            }
        };

        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let mut result = self.document_to_search_result(&doc)?;

            // Boost results that contain the exact operator in signature
            if result.snippet.contains(query) {
                result.score = score * 2.0; // High boost for exact operator matches
            } else {
                result.score = score;
            }

            results.push(result);
        }

        // Sort by relevance (highest score first)
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        Ok(results)
    }

    /// File path search
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

    /// Semantic concept search (full-text with ranking)
    async fn semantic_search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let searcher = self.reader.searcher();
        let fields = self.schema.fields();

        let query_parser = QueryParser::for_index(&self.index, vec![
            fields.all_text,
            fields.symbol_name,
            fields.signature,
            fields.doc_comment,
        ]);

        let parsed_query = query_parser.parse_query(query)?;
        let top_docs = searcher.search(&*parsed_query, &TopDocs::with_limit(50))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let doc = searcher.doc(doc_address)?;
            let mut result = self.document_to_search_result(&doc)?;
            result.score = score;
            results.push(result);
        }

        Ok(results)
    }

    /// Mixed search combining multiple intents
    async fn mixed_search(&self, query: &str, intents: &[QueryIntent]) -> Result<Vec<SearchResult>> {
        let mut all_results = Vec::new();

        // Execute search for each intent and combine results
        for intent in intents {
            let results = match intent {
                QueryIntent::ExactSymbol => {
                    // Extract symbol-like words for exact search
                    let symbols: Vec<&str> = query.split_whitespace()
                        .filter(|w| w.chars().all(|c| c.is_alphanumeric() || c == '_') && w.len() > 2)
                        .collect();

                    if let Some(symbol) = symbols.first() {
                        self.exact_symbol_search(symbol).await?
                    } else {
                        vec![]
                    }
                }
                QueryIntent::FilePath => {
                    // Extract path-like words
                    let paths: Vec<&str> = query.split_whitespace()
                        .filter(|w| w.contains('/') || w.contains('.'))
                        .collect();

                    if let Some(path) = paths.first() {
                        self.file_path_search(path).await?
                    } else {
                        vec![]
                    }
                }
                QueryIntent::GenericType => {
                    // Extract generic type patterns
                    if let Some(start) = query.find('<') {
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

        // Deduplicate by symbol_id and merge scores
        let mut unique_results: std::collections::HashMap<String, SearchResult> = std::collections::HashMap::new();

        for result in all_results {
            let key = format!("{}:{}", result.symbol_id, result.file_path);
            match unique_results.get_mut(&key) {
                Some(existing) => {
                    // Merge scores by taking the maximum
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

    /// Convert Tantivy document to search result
    fn document_to_search_result(&self, doc: &tantivy::TantivyDocument) -> Result<SearchResult> {
        let fields = self.schema.fields();

        // Proper field extraction using Tantivy's Value API
        let extract_text = |field: Field| -> String {
            doc.get_first(field)
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string()
        };

        let extract_u64 = |field: Field| -> u32 {
            doc.get_first(field)
                .and_then(|value| value.as_u64())
                .unwrap_or(0) as u32
        };

        let symbol_id = extract_text(fields.symbol_id);
        let symbol_name = extract_text(fields.symbol_name);
        let file_path = extract_text(fields.file_path);
        let line_number = extract_u64(fields.start_line);
        let snippet = extract_text(fields.signature);

        Ok(SearchResult {
            symbol_id,
            symbol_name,
            file_path,
            line_number,
            score: 0.0, // Will be set by caller
            snippet,
        })
    }

    /// Delete symbols for a file (for incremental updates)
    pub async fn delete_file_symbols(&mut self, file_path: &str) -> Result<()> {
        let fields = self.schema.fields();
        let term = Term::from_field_text(fields.file_path_exact, file_path);
        self.writer.delete_term(term);
        Ok(())
    }
}

/// Enhanced search result with more metadata
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub symbol_id: String,
    pub symbol_name: String,
    pub file_path: String,
    pub line_number: u32,
    pub score: f32,
    pub snippet: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::{SymbolKind, base::Symbol};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_basic_search_functionality() {
        // TDD Test: Should index a symbol and find it via search
        let mut engine = SearchEngine::in_memory().unwrap();

        // Create a simple symbol to index
        let symbol = Symbol {
            id: "test-function".to_string(),
            name: "getUserById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 0,
            start_byte: 100,
            end_byte: 200,
            doc_comment: Some("Fetches user by ID from the database".to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        // Index the symbol
        engine.index_symbols(vec![symbol]).await.unwrap();

        // Search for the symbol by name
        let results = engine.search("getUserById").await.unwrap();

        // Should find exactly one result
        assert_eq!(results.len(), 1);

        let result = &results[0];
        assert_eq!(result.symbol_name, "getUserById");
        assert_eq!(result.file_path, "src/user.ts");
        assert_eq!(result.line_number, 10);
        assert!(result.snippet.contains("getUserById"));
    }

    #[tokio::test]
    async fn test_symbol_indexing() {
        // Contract: Should index symbols successfully
        let mut engine = SearchEngine::in_memory().unwrap();

        let symbol = Symbol {
            id: "test-symbol".to_string(),
            name: "getUserById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            start_line: 10,
            end_line: 15,
            // ... other fields
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 0,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        let result = engine.index_symbols(vec![symbol]).await;
        assert!(result.is_ok());
        todo!("Verify symbol was actually indexed");
    }

    #[tokio::test]
    async fn test_exact_symbol_search() {
        // Contract: Should find exact symbol matches
        // Setup: Index "getUserById" function
        // Query: "getUserById"
        // Expected: Find the exact function
        todo!("Implement exact symbol search test");
    }

    #[tokio::test]
    async fn test_generic_type_search() {
        // Contract: Should find generic type patterns
        // Setup: Index "List<User>" and "Promise<User>"
        // Query: "List<User>"
        // Expected: Find both exact and component matches
        todo!("Implement generic type search test");
    }

    #[tokio::test]
    async fn test_operator_search() {
        // Contract: Should find operator patterns
        // Setup: Index functions with "&&" and "=>" operators
        // Query: "&&"
        // Expected: Find functions using logical AND
        todo!("Implement operator search test");
    }

    #[tokio::test]
    async fn test_file_path_search() {
        // Contract: Should find symbols by file path
        // Setup: Index symbols from various files
        // Query: "src/user"
        // Expected: Find symbols in user-related files
        todo!("Implement file path search test");
    }

    #[tokio::test]
    async fn test_semantic_search() {
        // Contract: Should find conceptually related symbols
        // Setup: Index user-related functions
        // Query: "user authentication"
        // Expected: Find login, auth, user functions
        todo!("Implement semantic search test");
    }

    #[tokio::test]
    async fn test_search_performance() {
        // Contract: Should complete searches in under 10ms
        // Setup: Index 10,000 symbols
        // Query: Various search patterns
        // Expected: All searches complete in <10ms
        todo!("Implement performance test");
    }

    #[tokio::test]
    async fn test_incremental_updates() {
        // Contract: Should handle file updates correctly
        // Setup: Index symbols, then update a file
        // Action: Delete old symbols, add new ones
        // Expected: Search reflects changes
        todo!("Implement incremental update test");
    }
}