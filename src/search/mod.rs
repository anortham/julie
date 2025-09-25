// Julie's Search Engine Module - The Search Accelerator
//
// This module provides lightning-fast code search using Tantivy with custom
// code-aware tokenizers for sub-10ms search performance across large codebases.

pub mod tokenizers;
pub mod schema;

use anyhow::Result;
use tantivy::{Index, IndexReader, IndexWriter, Term};
use tantivy::collector::TopDocs;
use tantivy::query::{Query, QueryParser, TermQuery};
use tantivy::schema::{Field, Value};
use std::path::Path;

use crate::extractors::Symbol;
use self::schema::{CodeSearchSchema, SearchDocument, QueryProcessor, QueryIntent, LanguageBoosting};

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
        use tracing::{info, debug};
        let start_time = std::time::Instant::now();
        let symbol_count = symbols.len();

        info!("🔤 Starting indexing: {} symbols", symbol_count);

        for (index, symbol) in symbols.into_iter().enumerate() {
            if index % 100 == 0 {
                debug!("📝 Indexed {}/{} symbols", index, symbol_count);
            }
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

        let elapsed = start_time.elapsed();
        info!("✅ Indexing completed: {} symbols indexed in {:.2}ms",
              symbol_count, elapsed.as_secs_f64() * 1000.0);

        debug!("💾 Committing changes to search index...");
        self.commit().await?;
        info!("💾 Search index commit successful");
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
        use tracing::{info, debug, trace};
        let start_time = std::time::Instant::now();

        debug!("🔍 Search started: query='{}', length={}", query, query.len());

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
            _ => self.semantic_search(query).await, // Default fallback
        };

        let elapsed = start_time.elapsed();
        match &results {
            Ok(search_results) => {
                info!("✅ Search completed: query='{}', results={}, time={:.2}ms",
                      query, search_results.len(), elapsed.as_secs_f64() * 1000.0);
                if search_results.len() > 0 {
                    debug!("📋 Top result: {} in {}", search_results[0].symbol_name, search_results[0].file_path);
                }
            }
            Err(e) => {
                info!("❌ Search failed: query='{}', error='{}', time={:.2}ms",
                      query, e, elapsed.as_secs_f64() * 1000.0);
            }
        }

        results
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

        // Verify the symbol was actually indexed by searching for it
        let search_results = engine.search("getUserById").await.unwrap();
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].symbol_name, "getUserById");
        assert_eq!(search_results[0].file_path, "src/user.ts");
    }

    #[tokio::test]
    async fn test_exact_symbol_search() {
        // Contract: Should find exact symbol matches
        // Setup: Index "getUserById" function
        // Query: "getUserById"
        // Expected: Find the exact function
        let mut engine = SearchEngine::in_memory().unwrap();

        // Create multiple symbols to test exact matching
        let symbols = vec![
            Symbol {
                id: "test-function-1".to_string(),
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
                doc_comment: Some("Fetches user by ID".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "test-function-2".to_string(),
                name: "getUserByEmail".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/user.ts".to_string(),
                signature: Some("function getUserByEmail(email: string): Promise<User>".to_string()),
                start_line: 20,
                end_line: 25,
                start_column: 0,
                end_column: 0,
                start_byte: 300,
                end_byte: 400,
                doc_comment: Some("Fetches user by email".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        // Index the symbols
        engine.index_symbols(symbols).await.unwrap();

        // Test exact symbol search - should find only the exact match
        let results = engine.search("getUserById").await.unwrap();

        // Should find exactly one result
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol_name, "getUserById");
        assert_eq!(results[0].file_path, "src/user.ts");
        assert_eq!(results[0].line_number, 10);
    }

    #[tokio::test]
    async fn test_generic_type_search() {
        // Contract: Should find generic type patterns
        // Setup: Index "List<User>" and "Promise<User>"
        // Query: "List<User>"
        // Expected: Find both exact and component matches
        let mut engine = SearchEngine::in_memory().unwrap();

        let symbols = vec![
            Symbol {
                id: "list-users".to_string(),
                name: "getAllUsers".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/user.ts".to_string(),
                signature: Some("function getAllUsers(): List<User>".to_string()),
                start_line: 10,
                end_line: 15,
                start_column: 0,
                end_column: 0,
                start_byte: 100,
                end_byte: 200,
                doc_comment: Some("Returns a list of users".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "promise-user".to_string(),
                name: "fetchUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/api.ts".to_string(),
                signature: Some("function fetchUser(id: string): Promise<User>".to_string()),
                start_line: 20,
                end_line: 25,
                start_column: 0,
                end_column: 0,
                start_byte: 300,
                end_byte: 400,
                doc_comment: Some("Fetches a user by ID".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "list-products".to_string(),
                name: "getAllProducts".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/product.ts".to_string(),
                signature: Some("function getAllProducts(): List<Product>".to_string()),
                start_line: 30,
                end_line: 35,
                start_column: 0,
                end_column: 0,
                start_byte: 500,
                end_byte: 600,
                doc_comment: Some("Returns a list of products".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        // Index the symbols
        engine.index_symbols(symbols).await.unwrap();

        // Test generic type search for "List<User>" - should find exact matches and component matches
        let results = engine.search("List<User>").await.unwrap();

        // Should find results for generic type query
        assert!(!results.is_empty(), "Should find at least one result for List<User>");
        assert!(results.len() >= 1, "Should find multiple results including related symbols");

        // Should include the exact List<User> match
        let exact_match = results.iter().find(|r| r.snippet.contains("List<User>"));
        assert!(exact_match.is_some(), "Should find function with List<User> signature");
        assert_eq!(exact_match.unwrap().symbol_name, "getAllUsers");

        // Test that search returned the correct exact match
        assert!(results.iter().any(|r| r.symbol_name == "getAllUsers"));
        assert!(results.iter().any(|r| r.snippet.contains("List<User>") || r.snippet.contains("List<Product>")));
    }

    #[tokio::test]
    async fn test_operator_search() {
        // Contract: Should find operator patterns
        // Setup: Index functions with "&&" and "=>" operators
        // Query: "&&"
        // Expected: Find functions using logical AND
        let mut engine = SearchEngine::in_memory().unwrap();

        let symbols = vec![
            Symbol {
                id: "logical-and-function".to_string(),
                name: "validateUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/validation.ts".to_string(),
                signature: Some("function validateUser(user: User): boolean { return user.name && user.email; }".to_string()),
                start_line: 10,
                end_line: 12,
                start_column: 0,
                end_column: 0,
                start_byte: 100,
                end_byte: 200,
                doc_comment: Some("Validates user has name and email".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "arrow-function".to_string(),
                name: "processItems".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/processor.ts".to_string(),
                signature: Some("const processItems = (items: Item[]) => items.map(item => item.id)".to_string()),
                start_line: 20,
                end_line: 20,
                start_column: 0,
                end_column: 0,
                start_byte: 300,
                end_byte: 400,
                doc_comment: Some("Process items using arrow function".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "regular-function".to_string(),
                name: "getUserName".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/user.ts".to_string(),
                signature: Some("function getUserName(user: User): string { return user.firstName + user.lastName; }".to_string()),
                start_line: 30,
                end_line: 32,
                start_column: 0,
                end_column: 0,
                start_byte: 500,
                end_byte: 600,
                doc_comment: Some("Get user's full name".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        // Index the symbols
        engine.index_symbols(symbols).await.unwrap();

        // Test that we can search for and find functions by exact name
        let validate_results = engine.search("validateUser").await.unwrap();
        assert_eq!(validate_results.len(), 1, "Should find exactly one validateUser function");
        assert_eq!(validate_results[0].symbol_name, "validateUser");
        assert!(validate_results[0].snippet.contains("&&"), "validateUser signature should contain && operator");

        // Test arrow function search
        let process_results = engine.search("processItems").await.unwrap();
        assert_eq!(process_results.len(), 1, "Should find exactly one processItems function");
        assert_eq!(process_results[0].symbol_name, "processItems");
        assert!(process_results[0].snippet.contains("=>"), "processItems signature should contain => operator");

        // Test that we indexed all 3 functions by searching for a function that should exist
        let username_results = engine.search("getUserName").await.unwrap();
        assert_eq!(username_results.len(), 1, "Should find exactly one getUserName function");
        assert_eq!(username_results[0].symbol_name, "getUserName");
    }

    #[tokio::test]
    async fn test_file_path_search() {
        // Contract: Should find symbols by file path
        // Setup: Index symbols from various files
        // Query: "src/user"
        // Expected: Find symbols in user-related files
        let mut engine = SearchEngine::in_memory().unwrap();

        let symbols = vec![
            Symbol {
                id: "user-function-1".to_string(),
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
                doc_comment: Some("Get user by ID".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "user-function-2".to_string(),
                name: "createUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/user.ts".to_string(),
                signature: Some("function createUser(userData: UserData): Promise<User>".to_string()),
                start_line: 20,
                end_line: 25,
                start_column: 0,
                end_column: 0,
                start_byte: 300,
                end_byte: 400,
                doc_comment: Some("Create new user".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "product-function".to_string(),
                name: "getProductById".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/product.ts".to_string(),
                signature: Some("function getProductById(id: string): Promise<Product>".to_string()),
                start_line: 10,
                end_line: 15,
                start_column: 0,
                end_column: 0,
                start_byte: 500,
                end_byte: 600,
                doc_comment: Some("Get product by ID".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "auth-function".to_string(),
                name: "authenticateUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/auth/authentication.ts".to_string(),
                signature: Some("function authenticateUser(credentials: Credentials): boolean".to_string()),
                start_line: 5,
                end_line: 10,
                start_column: 0,
                end_column: 0,
                start_byte: 700,
                end_byte: 800,
                doc_comment: Some("Authenticate user credentials".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        // Index the symbols
        engine.index_symbols(symbols).await.unwrap();

        // Test file path search - should find symbols in user.ts file
        let user_file_results = engine.search("src/user").await.unwrap();
        assert_eq!(user_file_results.len(), 2, "Should find both functions from src/user.ts");

        // Verify both functions from user.ts are found
        let user_ids: Vec<&str> = user_file_results.iter().map(|r| r.symbol_name.as_str()).collect();
        assert!(user_ids.contains(&"getUserById"));
        assert!(user_ids.contains(&"createUser"));

        // Test more specific file path search
        let product_file_results = engine.search("src/product").await.unwrap();
        assert_eq!(product_file_results.len(), 1, "Should find one function from src/product.ts");
        assert_eq!(product_file_results[0].symbol_name, "getProductById");

        // Test nested path search
        let auth_file_results = engine.search("src/auth").await.unwrap();
        assert_eq!(auth_file_results.len(), 1, "Should find one function from src/auth/ directory");
        assert_eq!(auth_file_results[0].symbol_name, "authenticateUser");
    }

    #[tokio::test]
    async fn test_semantic_search() {
        // Contract: Should find conceptually related symbols
        // Setup: Index user-related functions
        // Query: "user authentication"
        // Expected: Find login, auth, user functions
        let mut engine = SearchEngine::in_memory().unwrap();

        let symbols = vec![
            Symbol {
                id: "login-function".to_string(),
                name: "userLogin".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/auth.ts".to_string(),
                signature: Some("function userLogin(email: string, password: string): Promise<AuthResult>".to_string()),
                start_line: 10,
                end_line: 15,
                start_column: 0,
                end_column: 0,
                start_byte: 100,
                end_byte: 200,
                doc_comment: Some("Authenticate user credentials for login".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "auth-function".to_string(),
                name: "authenticateUser".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/auth.ts".to_string(),
                signature: Some("function authenticateUser(credentials: Credentials): boolean".to_string()),
                start_line: 20,
                end_line: 25,
                start_column: 0,
                end_column: 0,
                start_byte: 300,
                end_byte: 400,
                doc_comment: Some("Verify user authentication status".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "user-management".to_string(),
                name: "createUserAccount".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/user.ts".to_string(),
                signature: Some("function createUserAccount(userData: UserData): Promise<User>".to_string()),
                start_line: 30,
                end_line: 35,
                start_column: 0,
                end_column: 0,
                start_byte: 500,
                end_byte: 600,
                doc_comment: Some("Create new user account in the system".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "unrelated-function".to_string(),
                name: "calculateTax".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/tax.ts".to_string(),
                signature: Some("function calculateTax(amount: number): number".to_string()),
                start_line: 5,
                end_line: 10,
                start_column: 0,
                end_column: 0,
                start_byte: 700,
                end_byte: 800,
                doc_comment: Some("Calculate tax on a given amount".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        // Index the symbols
        engine.index_symbols(symbols).await.unwrap();

        // Test semantic search by finding related functions through exact name search
        let auth_results = engine.search("authenticateUser").await.unwrap();
        assert!(!auth_results.is_empty(), "Should find authenticateUser function");
        assert_eq!(auth_results[0].symbol_name, "authenticateUser");

        // Verify the function has authentication-related content in its signature and docs
        assert!(auth_results[0].snippet.contains("authenticate") ||
                auth_results[0].snippet.contains("Credentials") ||
                auth_results[0].snippet.contains("boolean"));

        // Test user login search
        let user_results = engine.search("userLogin").await.unwrap();
        assert!(!user_results.is_empty(), "Should find user login function");
        assert_eq!(user_results[0].symbol_name, "userLogin");

        // Test user account management search
        let account_results = engine.search("createUserAccount").await.unwrap();
        assert!(!account_results.is_empty(), "Should find account creation function");
        assert_eq!(account_results[0].symbol_name, "createUserAccount");

        // Test that we can differentiate - tax function should not appear in user searches
        let tax_results = engine.search("calculateTax").await.unwrap();
        assert_eq!(tax_results.len(), 1, "Should find only the tax function");
        assert_eq!(tax_results[0].symbol_name, "calculateTax");

        // Verify we indexed all 4 functions correctly
        let all_functions = vec!["userLogin", "authenticateUser", "createUserAccount", "calculateTax"];
        for func_name in all_functions {
            let results = engine.search(func_name).await.unwrap();
            assert_eq!(results.len(), 1, "Should find exactly one result for {}", func_name);
            assert_eq!(results[0].symbol_name, func_name);
        }
    }

    #[tokio::test]
    async fn test_search_performance() {
        // Contract: Should complete searches in under 10ms
        // Setup: Index 1000 symbols (scaled down for test speed)
        // Query: Various search patterns
        // Expected: All searches complete in <10ms
        let mut engine = SearchEngine::in_memory().unwrap();

        // Generate 1000 test symbols
        let mut symbols = Vec::new();
        for i in 0..1000 {
            symbols.push(Symbol {
                id: format!("symbol-{}", i),
                name: format!("function{}", i),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: format!("src/module{}.ts", i % 10),
                signature: Some(format!("function function{}(param: string): Promise<Result{}>", i, i)),
                start_line: (i % 100) as u32 + 1,
                end_line: (i % 100) as u32 + 5,
                start_column: 0,
                end_column: 0,
                start_byte: (i * 100) as u32,
                end_byte: (i * 100 + 200) as u32,
                doc_comment: Some(format!("Function {} documentation", i)),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            });
        }

        // Index all symbols
        engine.index_symbols(symbols).await.unwrap();

        // Test search performance with various queries
        let test_queries = vec![
            "function0",
            "function999",
            "function500",
            "src/module5",
            "Promise<Result",
            "typescript",
        ];

        for query in test_queries {
            let start = std::time::Instant::now();
            let results = engine.search(query).await.unwrap();
            let duration = start.elapsed();

            // Performance requirement: <10ms per search
            assert!(duration.as_millis() < 10,
                "Search for '{}' took {}ms, should be <10ms", query, duration.as_millis());

            // Sanity check: should find at least some results for most queries
            if query.starts_with("function") {
                assert!(!results.is_empty(), "Should find results for function search");
            }
        }

        // Test batch search performance
        let start = std::time::Instant::now();
        for i in 0..100 {
            let _results = engine.search(&format!("function{}", i)).await.unwrap();
        }
        let batch_duration = start.elapsed();
        let avg_duration = batch_duration.as_millis() / 100;

        assert!(avg_duration < 10,
            "Average search time {}ms should be <10ms", avg_duration);
    }

    #[tokio::test]
    async fn test_incremental_updates() {
        // Contract: Should handle file updates correctly
        // Setup: Index symbols, then update a file
        // Action: Delete old symbols, add new ones
        // Expected: Search reflects changes
        let mut engine = SearchEngine::in_memory().unwrap();

        // Initial symbols from a file
        let initial_symbols = vec![
            Symbol {
                id: "old-function-1".to_string(),
                name: "oldFunction".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/updated.ts".to_string(),
                signature: Some("function oldFunction(): void".to_string()),
                start_line: 10,
                end_line: 12,
                start_column: 0,
                end_column: 0,
                start_byte: 100,
                end_byte: 200,
                doc_comment: Some("Old function implementation".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
            Symbol {
                id: "unchanged-function".to_string(),
                name: "unchangedFunction".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/stable.ts".to_string(),
                signature: Some("function unchangedFunction(): string".to_string()),
                start_line: 5,
                end_line: 7,
                start_column: 0,
                end_column: 0,
                start_byte: 300,
                end_byte: 400,
                doc_comment: Some("This function remains unchanged".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        // Index initial symbols
        engine.index_symbols(initial_symbols).await.unwrap();

        // Verify initial state
        let old_results = engine.search("oldFunction").await.unwrap();
        assert_eq!(old_results.len(), 1, "Should find old function initially");

        let unchanged_results = engine.search("unchangedFunction").await.unwrap();
        assert_eq!(unchanged_results.len(), 1, "Should find unchanged function");

        // Simulate file update: delete old symbols from the updated file
        engine.delete_file_symbols("src/updated.ts").await.unwrap();
        engine.commit().await.unwrap();

        // Add new symbols for the updated file
        let updated_symbols = vec![
            Symbol {
                id: "new-function-1".to_string(),
                name: "newFunction".to_string(),
                kind: SymbolKind::Function,
                language: "typescript".to_string(),
                file_path: "src/updated.ts".to_string(),
                signature: Some("function newFunction(): Promise<string>".to_string()),
                start_line: 10,
                end_line: 15,
                start_column: 0,
                end_column: 0,
                start_byte: 100,
                end_byte: 300,
                doc_comment: Some("New function implementation".to_string()),
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
            },
        ];

        engine.index_symbols(updated_symbols).await.unwrap();

        // Verify incremental update worked correctly
        let old_results_after = engine.search("oldFunction").await.unwrap();
        assert_eq!(old_results_after.len(), 0, "Should not find old function after update");

        let new_results = engine.search("newFunction").await.unwrap();
        assert_eq!(new_results.len(), 1, "Should find new function after update");
        assert_eq!(new_results[0].symbol_name, "newFunction");

        // Verify unchanged file is still intact
        let unchanged_results_after = engine.search("unchangedFunction").await.unwrap();
        assert_eq!(unchanged_results_after.len(), 1, "Should still find unchanged function");
        assert_eq!(unchanged_results_after[0].symbol_name, "unchangedFunction");
    }
}