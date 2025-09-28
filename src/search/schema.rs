// Search Schema Design for Julie's Tantivy Engine
//
// This module defines the multi-field schema that supports both exact matching
// and tokenized search across different types of code elements.

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tantivy::schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions};
use tantivy::Index;

/// The complete search schema for code intelligence
#[derive(Debug, Clone)]
pub struct CodeSearchSchema {
    /// Tantivy schema instance
    schema: Schema,
    /// Field mappings for easy access
    fields: SearchFields,
}

/// All searchable fields in the schema
#[derive(Debug, Clone)]
pub struct SearchFields {
    // Core symbol fields
    pub symbol_id: Field,
    pub symbol_name: Field,
    pub symbol_name_exact: Field,
    pub symbol_kind: Field,
    pub language: Field,
    pub file_path: Field,
    pub file_path_exact: Field,

    // Content fields
    pub signature: Field,
    pub signature_exact: Field,
    pub doc_comment: Field,
    pub code_context: Field,

    // Location fields
    pub start_line: Field,
    pub end_line: Field,
    pub start_column: Field,
    pub end_column: Field,
    pub start_byte: Field,
    pub end_byte: Field,

    // Symbol relationship fields
    pub visibility: Field,
    pub parent_id: Field,

    // Metadata fields
    pub metadata: Field,
    pub semantic_group: Field,
    pub confidence: Field,

    // Search optimization fields
    pub all_text: Field,       // Combined searchable text
    pub exact_matches: Field,  // For exact phrase matching
    pub language_boost: Field, // Language-specific boosting
}

impl CodeSearchSchema {
    /// Create a new search schema optimized for code
    pub fn new() -> Result<Self> {
        let mut schema_builder = Schema::builder();

        // Code-aware text options using our custom tokenizer
        let code_text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("code_aware") // Use our custom code-aware tokenizer
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        // Standard text options for non-code fields
        let text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        let stored_only = TextOptions::default().set_stored();

        // Exact match fields use raw tokenizer for precise matching
        let exact_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("raw") // Use raw tokenizer for exact matches
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();

        let symbol_id = schema_builder.add_text_field("symbol_id", stored_only.clone());
        let symbol_name = schema_builder.add_text_field("symbol_name", code_text_options.clone());
        let symbol_name_exact =
            schema_builder.add_text_field("symbol_name_exact", exact_options.clone());
        let symbol_kind = schema_builder.add_text_field("symbol_kind", text_options.clone());
        let language = schema_builder.add_text_field("language", text_options.clone());
        let file_path = schema_builder.add_text_field("file_path", code_text_options.clone());
        let file_path_exact =
            schema_builder.add_text_field("file_path_exact", exact_options.clone());

        // Content for searching
        let signature = schema_builder.add_text_field("signature", code_text_options.clone());
        let signature_exact =
            schema_builder.add_text_field("signature_exact", exact_options.clone());
        let doc_comment = schema_builder.add_text_field("doc_comment", text_options.clone());
        let code_context = schema_builder.add_text_field("code_context", code_text_options.clone());

        // Location information (numeric fields use different options)
        let start_line = schema_builder.add_u64_field(
            "start_line",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );
        let end_line = schema_builder.add_u64_field(
            "end_line",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );
        let start_column = schema_builder.add_u64_field(
            "start_column",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );
        let end_column = schema_builder.add_u64_field(
            "end_column",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );
        let start_byte = schema_builder.add_u64_field(
            "start_byte",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );
        let end_byte = schema_builder.add_u64_field(
            "end_byte",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );

        // Symbol relationship fields
        let visibility = schema_builder.add_text_field("visibility", text_options.clone());
        let parent_id = schema_builder.add_text_field("parent_id", stored_only.clone());

        // Metadata and semantic information
        let metadata = schema_builder.add_text_field("metadata", text_options.clone());
        let semantic_group = schema_builder.add_text_field("semantic_group", text_options.clone());
        let confidence = schema_builder.add_f64_field(
            "confidence",
            tantivy::schema::INDEXED | tantivy::schema::STORED,
        );

        // Search optimization (index-only for search)
        let code_index_only = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("code_aware") // Use code-aware tokenizer for all_text
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let index_only = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default().set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let all_text = schema_builder.add_text_field("all_text", code_index_only);
        let exact_matches = schema_builder.add_text_field("exact_matches", index_only);
        let language_boost =
            schema_builder.add_f64_field("language_boost", tantivy::schema::INDEXED);

        let schema = schema_builder.build();
        let fields = SearchFields {
            symbol_id,
            symbol_name,
            symbol_name_exact,
            symbol_kind,
            language,
            file_path,
            file_path_exact,
            signature,
            signature_exact,
            doc_comment,
            code_context,
            start_line,
            end_line,
            start_column,
            end_column,
            start_byte,
            end_byte,
            visibility,
            parent_id,
            metadata,
            semantic_group,
            confidence,
            all_text,
            exact_matches,
            language_boost,
        };

        Ok(Self { schema, fields })
    }

    /// Get the Tantivy schema
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Get field mappings
    pub fn fields(&self) -> &SearchFields {
        &self.fields
    }

    /// Create an index with this schema
    pub fn create_index(&self) -> Result<Index> {
        let index = Index::create_in_ram(self.schema.clone());
        Ok(index)
    }
}

/// Document structure for indexing symbols
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchDocument {
    pub symbol_id: String,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
    pub doc_comment: Option<String>,
    pub code_context: Option<String>,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
    pub visibility: Option<String>,
    pub parent_id: Option<String>,
    pub metadata: Option<String>, // JSON string
    pub semantic_group: Option<String>,
    pub confidence: Option<f64>,
}

impl SearchDocument {
    /// Generate combined text for full-text search
    pub fn generate_all_text(&self) -> String {
        let mut parts = vec![
            self.symbol_name.clone(),
            self.symbol_kind.clone(),
            self.file_path.clone(),
        ];

        if let Some(sig) = &self.signature {
            parts.push(sig.clone());
        }

        if let Some(doc) = &self.doc_comment {
            parts.push(doc.clone());
        }

        if let Some(context) = &self.code_context {
            parts.push(context.clone());
        }

        if let Some(group) = &self.semantic_group {
            parts.push(group.clone());
        }

        parts.join(" ")
    }

    /// Generate exact match text for precise queries
    pub fn generate_exact_matches(&self) -> String {
        let mut exact_parts = vec![self.symbol_name.clone()];

        if let Some(sig) = &self.signature {
            exact_parts.push(sig.clone());
        }

        exact_parts.join(" | ")
    }

    /// Calculate language-specific boost score
    pub fn calculate_language_boost(&self, preferred_languages: &[&str]) -> f64 {
        if preferred_languages.contains(&self.language.as_str()) {
            1.5 // Boost preferred languages
        } else {
            1.0
        }
    }
}

/// Query intent detection for intelligent search
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QueryIntent {
    /// Exact symbol name search ("getUserById")
    ExactSymbol,
    /// Type signature search ("function(string): Promise<User>")
    TypeSignature,
    /// Generic type search ("List<User>", "Map<String, T>")
    GenericType,
    /// Operator-based search ("&& ||", "=>")
    OperatorSearch,
    /// File path search ("src/components/")
    FilePath,
    /// Semantic concept search ("user authentication", "data validation")
    SemanticConcept,
    /// Mixed query combining multiple intents
    Mixed(Vec<QueryIntent>),
}

/// Query processor for intelligent search intent detection
#[derive(Debug)]
pub struct QueryProcessor {
    /// Patterns for detecting different query types
    patterns: HashMap<QueryIntent, Vec<regex::Regex>>,
}

impl QueryProcessor {
    pub fn new() -> Result<Self> {
        let mut patterns = HashMap::new();

        // Exact Symbol patterns - restrictive patterns for true code symbols only
        let exact_patterns = vec![
            Regex::new(r"^[a-z][a-zA-Z0-9]*[A-Z][a-zA-Z0-9]*$")?, // camelCase with mixed case (getUserById)
            Regex::new(r"^[A-Z][a-zA-Z0-9]*[A-Z][a-zA-Z0-9]*$")?, // PascalCase with mixed case (UserService)
            Regex::new(r"^[a-z_]+[a-z0-9_]*_[a-z0-9_]+$")?,       // snake_case with underscores (user_repository)
            Regex::new(r"^[A-Z_][A-Z0-9_]*$")?,                   // ALL_CAPS constants (API_KEY, MAX_SIZE)
            Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*::[a-zA-Z_][a-zA-Z0-9_]*$")?, // Qualified names (std::vector)
        ];
        patterns.insert(QueryIntent::ExactSymbol, exact_patterns);

        // Generic Type patterns - List<T>, Map<K,V>, Promise<User>
        let generic_patterns = vec![
            Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*<[^>]+>$")?, // Generic<Type>
            Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*<[^,<>]+,[^,<>]+>$")?, // Generic<T,U>
            Regex::new(r"^Array<[^>]+>$")?,                  // Array<Type>
            Regex::new(r"^Promise<[^>]+>$")?,                // Promise<Type>
            Regex::new(r"^List<[^>]+>$")?,                   // List<Type>
            Regex::new(r"^Map<[^,<>]+,[^,<>]+>$")?,          // Map<K,V>
        ];
        patterns.insert(QueryIntent::GenericType, generic_patterns);

        // Operator patterns - &&, ||, =>, ->, ?., !!, etc.
        let operator_patterns = vec![
            Regex::new(r"^(&&|\|\||=>|->|\?\.|!!)$")?, // Common operators
            Regex::new(r"^(==|!=|<=|>=|===|!==)$")?,   // Comparison operators (including ===)
            Regex::new(r"^[=!<>]=?$")?,                // Basic comparison operators
            Regex::new(r"^[+\-*/&|^]=?$")?,            // Arithmetic/bitwise operators
            Regex::new(r"^(\+\+|--|<<|>>)$")?,         // Increment/shift operators
        ];
        patterns.insert(QueryIntent::OperatorSearch, operator_patterns);

        // File Path patterns - src/components/, lib/utils.ts
        let filepath_patterns = vec![
            Regex::new(r"^[a-zA-Z0-9_\-/]+\.[a-zA-Z0-9]+$")?, // path/file.ext
            Regex::new(r"^[a-zA-Z0-9_\-/]+/$")?,              // path/directory/
            Regex::new(r"^src/")?,                            // src/...
            Regex::new(r"^lib/")?,                            // lib/...
            Regex::new(r"^components?/")?,                    // component(s)/...
            Regex::new(r"^utils?/")?,                         // util(s)/...
        ];
        patterns.insert(QueryIntent::FilePath, filepath_patterns);

        // Type Signature patterns - function(...): ReturnType
        let signature_patterns = vec![
            Regex::new(r"^function\s*\(.*\)\s*:")?, // function(params): Type
            Regex::new(r"^\(.*\)\s*=>")?,           // (params) => Type
            Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*\s*\(.*\)\s*:")?, // methodName(params): Type
        ];
        patterns.insert(QueryIntent::TypeSignature, signature_patterns);

        Ok(Self { patterns })
    }

    /// Analyze query and detect intent using pattern matching
    pub fn detect_intent(&self, query: &str) -> QueryIntent {
        let trimmed = query.trim();

        // Check for mixed queries (containing spaces or multiple patterns)
        if trimmed.contains(' ') {
            return self.detect_mixed_intent(trimmed);
        }

        // Test each intent pattern
        for (intent, patterns) in &self.patterns {
            for pattern in patterns {
                if pattern.is_match(trimmed) {
                    return intent.clone();
                }
            }
        }

        // Default to semantic concept search for natural language queries
        QueryIntent::SemanticConcept
    }

    /// Detect mixed queries that combine multiple intents
    fn detect_mixed_intent(&self, query: &str) -> QueryIntent {
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut detected_intents = Vec::new();

        // Check if query contains file path hints
        if words.iter().any(|w| {
            w.starts_with("src/") || w.starts_with("lib/") || w.contains(".ts") || w.contains(".rs")
        }) {
            detected_intents.push(QueryIntent::FilePath);
        }

        // Check if query contains exact symbols (single identifier-like words)
        let identifier_words: Vec<&str> = words
            .iter()
            .filter(|w| w.chars().all(|c| c.is_alphanumeric() || c == '_') && w.len() > 2)
            .cloned()
            .collect();

        // Treat as ExactSymbol if there's exactly one clear identifier word
        // (even if there are other non-identifier words like "in", "from", etc.)
        if identifier_words.len() == 1 {
            detected_intents.push(QueryIntent::ExactSymbol);

            if words.len() > identifier_words.len()
                && !detected_intents.contains(&QueryIntent::SemanticConcept)
            {
                detected_intents.push(QueryIntent::SemanticConcept);
            }
        }

        // Check for generic type hints
        if query.contains('<') && query.contains('>') {
            detected_intents.push(QueryIntent::GenericType);
        }

        if detected_intents.len() > 1 {
            QueryIntent::Mixed(detected_intents)
        } else if detected_intents.len() == 1 {
            detected_intents.into_iter().next().unwrap()
        } else {
            // Multi-word queries that don't match specific patterns are semantic concept searches
            QueryIntent::SemanticConcept
        }
    }

    /// Transform query based on detected intent
    pub fn transform_query(&self, query: &str, intent: &QueryIntent) -> String {
        match intent {
            QueryIntent::ExactSymbol => {
                // Wrap in quotes for exact matching
                format!("\"{}\"", query)
            }
            QueryIntent::GenericType => {
                // Split generic syntax for component search
                query.to_string() // TODO: Implement generic splitting
            }
            QueryIntent::OperatorSearch => {
                // Preserve operators in search
                query.to_string() // TODO: Implement operator preservation
            }
            _ => query.to_string(),
        }
    }
}

/// Language-specific field boosting configuration
#[derive(Debug, Clone)]
pub struct LanguageBoosting {
    /// Per-language boost multipliers
    language_boosts: HashMap<String, f64>,
    /// Per-field boost multipliers for different languages
    field_boosts: HashMap<String, HashMap<String, f64>>,
}

impl LanguageBoosting {
    pub fn new() -> Self {
        let mut language_boosts = HashMap::new();
        let mut field_boosts = HashMap::new();

        // Default language priorities (can be customized per workspace)
        language_boosts.insert("typescript".to_string(), 1.2);
        language_boosts.insert("javascript".to_string(), 1.2);
        language_boosts.insert("rust".to_string(), 1.1);
        language_boosts.insert("python".to_string(), 1.1);
        language_boosts.insert("java".to_string(), 1.0);

        // Language-specific field boosts
        let mut ts_boosts = HashMap::new();
        ts_boosts.insert("signature".to_string(), 1.3);
        ts_boosts.insert("doc_comment".to_string(), 1.1);
        field_boosts.insert("typescript".to_string(), ts_boosts);

        let mut rust_boosts = HashMap::new();
        rust_boosts.insert("signature".to_string(), 1.2);
        rust_boosts.insert("doc_comment".to_string(), 1.2);
        field_boosts.insert("rust".to_string(), rust_boosts);

        Self {
            language_boosts,
            field_boosts,
        }
    }

    /// Get boost multiplier for a language
    pub fn get_language_boost(&self, language: &str) -> f64 {
        self.language_boosts.get(language).copied().unwrap_or(1.0)
    }

    /// Get field boost for a language and field combination
    pub fn get_field_boost(&self, language: &str, field: &str) -> f64 {
        self.field_boosts
            .get(language)
            .and_then(|fields| fields.get(field))
            .copied()
            .unwrap_or(1.0)
    }

    /// Customize language priorities for a workspace
    pub fn set_language_boost(&mut self, language: String, boost: f64) {
        self.language_boosts.insert(language, boost);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_creation() {
        // Contract: Should create schema without errors
        let schema_result = CodeSearchSchema::new();
        assert!(schema_result.is_ok());

        let code_schema = schema_result.unwrap();
        let schema = code_schema.schema();
        let fields = code_schema.fields();

        // Verify all text fields are properly registered (field_id() always valid)
        assert!(fields.symbol_id.field_id() < u32::MAX);
        assert!(fields.symbol_name.field_id() < u32::MAX);
        assert!(fields.symbol_name_exact.field_id() < u32::MAX);
        assert!(fields.symbol_kind.field_id() < u32::MAX);
        assert!(fields.language.field_id() < u32::MAX);
        assert!(fields.file_path.field_id() < u32::MAX);
        assert!(fields.file_path_exact.field_id() < u32::MAX);
        assert!(fields.signature.field_id() < u32::MAX);
        assert!(fields.signature_exact.field_id() < u32::MAX);
        assert!(fields.doc_comment.field_id() < u32::MAX);
        assert!(fields.code_context.field_id() < u32::MAX);
        assert!(fields.metadata.field_id() < u32::MAX);
        assert!(fields.semantic_group.field_id() < u32::MAX);
        assert!(fields.all_text.field_id() < u32::MAX);
        assert!(fields.exact_matches.field_id() < u32::MAX);

        // Verify numeric fields are created
        // Verify numeric fields are registered (field_id() always valid)
        assert!(fields.start_line.field_id() < u32::MAX);
        assert!(fields.end_line.field_id() < u32::MAX);
        assert!(fields.confidence.field_id() < u32::MAX);
        assert!(fields.language_boost.field_id() < u32::MAX);

        // Verify fields exist in the schema by name
        let field_names: Vec<_> = schema
            .fields()
            .map(|(_, field_entry)| field_entry.name())
            .collect();

        // Core symbol fields
        assert!(field_names.contains(&"symbol_id"));
        assert!(field_names.contains(&"symbol_name"));
        assert!(field_names.contains(&"symbol_name_exact"));
        assert!(field_names.contains(&"symbol_kind"));
        assert!(field_names.contains(&"language"));
        assert!(field_names.contains(&"file_path"));
        assert!(field_names.contains(&"file_path_exact"));

        // Content fields
        assert!(field_names.contains(&"signature"));
        assert!(field_names.contains(&"signature_exact"));
        assert!(field_names.contains(&"doc_comment"));
        assert!(field_names.contains(&"code_context"));

        // Location fields
        assert!(field_names.contains(&"start_line"));
        assert!(field_names.contains(&"end_line"));

        // Metadata fields
        assert!(field_names.contains(&"metadata"));
        assert!(field_names.contains(&"semantic_group"));
        assert!(field_names.contains(&"confidence"));

        // Search optimization fields
        assert!(field_names.contains(&"all_text"));
        assert!(field_names.contains(&"exact_matches"));
        assert!(field_names.contains(&"language_boost"));

        // Total field count should match expected number
        assert_eq!(
            field_names.len(),
            25,
            "Expected 25 fields in schema, found: {:?}",
            field_names
        );
    }

    #[test]
    fn test_document_all_text_generation() {
        // Contract: Should combine all searchable text properly
        let doc = SearchDocument {
            symbol_id: "test".to_string(),
            symbol_name: "getUserById".to_string(),
            symbol_kind: "function".to_string(),
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            doc_comment: Some("Fetches user by ID".to_string()),
            code_context: None,
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 20,
            start_byte: 200,
            end_byte: 300,
            visibility: Some("public".to_string()),
            parent_id: None,
            metadata: None,
            semantic_group: Some("user-data".to_string()),
            confidence: Some(0.95),
        };

        let all_text = doc.generate_all_text();
        assert!(all_text.contains("getUserById"));
        assert!(all_text.contains("function"));
        assert!(all_text.contains("Promise<User>"));
        assert!(all_text.contains("src/user.ts"));
        assert!(all_text.contains("Fetches user by ID"));
        assert!(all_text.contains("user-data")); // semantic group
    }

    #[test]
    fn test_exact_match_generation() {
        // Contract: Should generate exact match text for precise queries
        let doc_with_signature = SearchDocument {
            symbol_id: "test".to_string(),
            symbol_name: "getUserById".to_string(),
            symbol_kind: "function".to_string(),
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            doc_comment: None,
            code_context: None,
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 20,
            start_byte: 200,
            end_byte: 300,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        let exact_matches = doc_with_signature.generate_exact_matches();

        // Should contain symbol name
        assert!(exact_matches.contains("getUserById"));

        // Should contain signature
        assert!(exact_matches.contains("function getUserById(id: string): Promise<User>"));

        // Should join with " | " separator
        assert_eq!(
            exact_matches,
            "getUserById | function getUserById(id: string): Promise<User>"
        );

        // Test document without signature
        let doc_without_signature = SearchDocument {
            symbol_id: "test2".to_string(),
            symbol_name: "UserClass".to_string(),
            symbol_kind: "class".to_string(),
            language: "typescript".to_string(),
            file_path: "src/user.ts".to_string(),
            signature: None,
            doc_comment: None,
            code_context: None,
            start_line: 20,
            end_line: 30,
            start_column: 0,
            end_column: 15,
            start_byte: 400,
            end_byte: 500,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        let exact_matches_no_sig = doc_without_signature.generate_exact_matches();

        // Should only contain symbol name when no signature
        assert_eq!(exact_matches_no_sig, "UserClass");
    }

    #[test]
    fn test_language_boost_calculation() {
        // Contract: Should calculate correct boost scores
        let typescript_doc = SearchDocument {
            symbol_id: "ts-test".to_string(),
            symbol_name: "getTSData".to_string(),
            symbol_kind: "function".to_string(),
            language: "typescript".to_string(),
            file_path: "src/app.ts".to_string(),
            signature: None,
            doc_comment: None,
            code_context: None,
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 10,
            start_byte: 0,
            end_byte: 100,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        let rust_doc = SearchDocument {
            symbol_id: "rust-test".to_string(),
            symbol_name: "get_rust_data".to_string(),
            symbol_kind: "function".to_string(),
            language: "rust".to_string(),
            file_path: "src/main.rs".to_string(),
            signature: None,
            doc_comment: None,
            code_context: None,
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 10,
            start_byte: 0,
            end_byte: 100,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        let java_doc = SearchDocument {
            symbol_id: "java-test".to_string(),
            symbol_name: "getJavaData".to_string(),
            symbol_kind: "method".to_string(),
            language: "java".to_string(),
            file_path: "src/App.java".to_string(),
            signature: None,
            doc_comment: None,
            code_context: None,
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 10,
            start_byte: 0,
            end_byte: 100,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        // Test with preferred languages
        let preferred = &["typescript", "rust"];

        // TypeScript should get boost (is preferred)
        assert_eq!(typescript_doc.calculate_language_boost(preferred), 1.5);

        // Rust should get boost (is preferred)
        assert_eq!(rust_doc.calculate_language_boost(preferred), 1.5);

        // Java should not get boost (not preferred)
        assert_eq!(java_doc.calculate_language_boost(preferred), 1.0);

        // Test with empty preferred languages
        let no_preferred: &[&str] = &[];
        assert_eq!(typescript_doc.calculate_language_boost(no_preferred), 1.0);
        assert_eq!(rust_doc.calculate_language_boost(no_preferred), 1.0);
        assert_eq!(java_doc.calculate_language_boost(no_preferred), 1.0);

        // Test with single preferred language
        let single_preferred = &["java"];
        assert_eq!(
            typescript_doc.calculate_language_boost(single_preferred),
            1.0
        );
        assert_eq!(rust_doc.calculate_language_boost(single_preferred), 1.0);
        assert_eq!(java_doc.calculate_language_boost(single_preferred), 1.5);
    }

    #[test]
    fn test_intent_detection_exact_symbol() {
        // Contract: Should detect exact symbol queries
        let processor = QueryProcessor::new().unwrap();

        // Test camelCase
        assert!(matches!(
            processor.detect_intent("getUserById"),
            QueryIntent::ExactSymbol
        ));

        // Test PascalCase
        assert!(matches!(
            processor.detect_intent("UserService"),
            QueryIntent::ExactSymbol
        ));

        // Test snake_case
        assert!(matches!(
            processor.detect_intent("user_repository"),
            QueryIntent::ExactSymbol
        ));

        // Multi-word queries should be Mixed or SemanticConcept, not ExactSymbol
        let result = processor.detect_intent("get user by id");
        assert!(
            !matches!(result, QueryIntent::ExactSymbol),
            "Multi-word queries should not be detected as ExactSymbol, got: {:?}",
            result
        );
    }

    #[test]
    fn test_intent_detection_generic_type() {
        // Contract: Should detect generic type queries
        let processor = QueryProcessor::new().unwrap();

        // Test simple generics
        assert!(matches!(
            processor.detect_intent("List<User>"),
            QueryIntent::GenericType
        ));

        // Test complex generics
        assert!(matches!(
            processor.detect_intent("Map<String,User>"),
            QueryIntent::GenericType
        ));

        // Test Promise types
        assert!(matches!(
            processor.detect_intent("Promise<Response>"),
            QueryIntent::GenericType
        ));

        // Test Array types
        assert!(matches!(
            processor.detect_intent("Array<number>"),
            QueryIntent::GenericType
        ));
    }

    #[test]
    fn test_intent_detection_operator_search() {
        // Contract: Should detect operator-based queries
        let processor = QueryProcessor::new().unwrap();

        // Test logical operators
        assert!(matches!(
            processor.detect_intent("&&"),
            QueryIntent::OperatorSearch
        ));

        assert!(matches!(
            processor.detect_intent("||"),
            QueryIntent::OperatorSearch
        ));

        // Test arrow functions
        assert!(matches!(
            processor.detect_intent("=>"),
            QueryIntent::OperatorSearch
        ));

        // Test optional chaining
        assert!(matches!(
            processor.detect_intent("?."),
            QueryIntent::OperatorSearch
        ));

        // Test comparison operators
        assert!(matches!(
            processor.detect_intent("==="),
            QueryIntent::OperatorSearch
        ));
    }

    #[test]
    fn test_intent_detection_file_path() {
        // Contract: Should detect file path queries
        let processor = QueryProcessor::new().unwrap();

        // Test file paths
        assert!(matches!(
            processor.detect_intent("src/components/Button.tsx"),
            QueryIntent::FilePath
        ));

        // Test directory paths
        assert!(matches!(
            processor.detect_intent("src/utils/"),
            QueryIntent::FilePath
        ));

        // Test relative paths
        assert!(matches!(
            processor.detect_intent("lib/helpers.ts"),
            QueryIntent::FilePath
        ));

        // Test component directories
        assert!(matches!(
            processor.detect_intent("components/"),
            QueryIntent::FilePath
        ));
    }

    #[test]
    fn test_query_transformation_exact() {
        // Contract: Should wrap exact queries in quotes
        let processor = QueryProcessor::new().unwrap();

        let transformed = processor.transform_query("getUserById", &QueryIntent::ExactSymbol);
        assert_eq!(transformed, "\"getUserById\"");

        let transformed2 = processor.transform_query("UserService", &QueryIntent::ExactSymbol);
        assert_eq!(transformed2, "\"UserService\"");
    }

    #[test]
    fn test_language_specific_boosting() {
        // Contract: Should apply correct boosts for different languages
        let boosting = LanguageBoosting::new();

        // Test language-level boosts
        assert_eq!(boosting.get_language_boost("typescript"), 1.2);
        assert_eq!(boosting.get_language_boost("javascript"), 1.2);
        assert_eq!(boosting.get_language_boost("rust"), 1.1);
        assert_eq!(boosting.get_language_boost("python"), 1.1);
        assert_eq!(boosting.get_language_boost("java"), 1.0);
        assert_eq!(boosting.get_language_boost("unknown"), 1.0);

        // Test field-specific boosts for TypeScript
        assert_eq!(boosting.get_field_boost("typescript", "signature"), 1.3);
        assert_eq!(boosting.get_field_boost("typescript", "doc_comment"), 1.1);
        assert_eq!(boosting.get_field_boost("typescript", "symbol_name"), 1.0); // Default

        // Test field-specific boosts for Rust
        assert_eq!(boosting.get_field_boost("rust", "signature"), 1.2);
        assert_eq!(boosting.get_field_boost("rust", "doc_comment"), 1.2);
        assert_eq!(boosting.get_field_boost("rust", "symbol_name"), 1.0); // Default

        // Test field boosts for languages without specific configuration
        assert_eq!(boosting.get_field_boost("java", "signature"), 1.0);
        assert_eq!(boosting.get_field_boost("java", "doc_comment"), 1.0);
        assert_eq!(boosting.get_field_boost("unknown", "signature"), 1.0);

        // Test non-existent fields return default boost
        assert_eq!(
            boosting.get_field_boost("typescript", "nonexistent_field"),
            1.0
        );
        assert_eq!(boosting.get_field_boost("rust", "nonexistent_field"), 1.0);
    }

    #[test]
    fn test_mixed_intent_detection() {
        // Contract: Should handle complex queries with multiple intents
        let processor = QueryProcessor::new().unwrap();

        // Test exact symbol + file path
        match processor.detect_intent("getUserById in src/user.ts") {
            QueryIntent::Mixed(intents) => {
                assert!(intents.contains(&QueryIntent::ExactSymbol));
                assert!(intents.contains(&QueryIntent::FilePath));
            }
            _ => panic!("Expected Mixed intent"),
        }

        // Test generic type + file path
        match processor.detect_intent("List<User> src/types.ts") {
            QueryIntent::Mixed(intents) => {
                assert!(intents.contains(&QueryIntent::GenericType));
                assert!(intents.contains(&QueryIntent::FilePath));
            }
            _ => panic!("Expected Mixed intent with generic and file path"),
        }

        // Single word should not be Mixed
        assert!(!matches!(
            processor.detect_intent("getUserById"),
            QueryIntent::Mixed(_)
        ));
    }
}
