// Cross-Language Semantic Grouping Module
//
// This module groups similar concepts across different programming languages
// using embedding vectors and similarity analysis.

use super::{CodeContext, EmbeddingEngine, SimilarityResult, cosine_similarity};
use crate::extractors::base::Symbol;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Groups similar concepts across different languages
pub struct SemanticGrouper {
    similarity_threshold: f32,
}

/// A group of semantically similar symbols across languages
#[derive(Debug, Clone)]
pub struct SemanticGroup {
    pub id: String,
    pub symbols: Vec<Symbol>,
    pub confidence: f32,
    pub similarity_score: f32,
    pub languages: Vec<String>,
    pub common_properties: Vec<String>,
    pub detected_pattern: ArchitecturalPattern,
}

/// Architectural patterns detected in semantic groups
#[derive(Debug, Clone)]
pub enum ArchitecturalPattern {
    FullStackEntity,  // UI -> API -> DB
    ApiContract,      // Frontend/Backend contract
    DataLayer,        // Service -> Database
    ServiceInterface, // Interface -> Implementation
    Unknown,
}

impl SemanticGrouper {
    pub fn new(similarity_threshold: f32) -> Self {
        Self {
            similarity_threshold,
        }
    }

    /// Find all symbols semantically related to the given symbol
    pub async fn find_semantic_group(
        &self,
        symbol: &Symbol,
        all_symbols: &[Symbol],
        embedding_engine: &mut EmbeddingEngine,
    ) -> Result<Vec<SemanticGroup>> {
        // Step 1: Generate embedding for the target symbol
        let context = CodeContext::from_symbol(symbol);
        let target_embedding = embedding_engine.embed_symbol(symbol, &context)?;

        // Step 2: Find candidate symbols from different languages
        let mut candidates_to_embed = Vec::new();

        for candidate_symbol in all_symbols {
            // Skip symbols from the same language (we want cross-language grouping)
            if candidate_symbol.language == symbol.language {
                continue;
            }
            candidates_to_embed.push(candidate_symbol.clone());
        }

        if candidates_to_embed.is_empty() {
            return Ok(vec![]);
        }

        // Generate embeddings for all candidates in batch
        let candidate_embeddings = embedding_engine.embed_symbols_batch(&candidates_to_embed)?;

        // Step 3: Calculate similarities and filter
        let mut candidates = Vec::new();

        // Create a map for quick lookup of embeddings by ID
        let embedding_map: HashMap<String, Vec<f32>> = candidate_embeddings.into_iter().collect();

        for candidate_symbol in candidates_to_embed {
            if let Some(candidate_embedding) = embedding_map.get(&candidate_symbol.id) {
                // Calculate similarity
                let similarity = cosine_similarity(&target_embedding, candidate_embedding);

                if similarity >= self.similarity_threshold {
                    candidates.push(SimilarityResult {
                        symbol_id: candidate_symbol.id.clone(),
                        similarity_score: similarity,
                        embedding: candidate_embedding.clone(),
                    });
                }
            }
        }

        // Step 3: Group candidates and validate semantic connections
        if candidates.is_empty() {
            return Ok(vec![]);
        }

        // Convert candidates back to symbols for validation
        let candidate_symbols: Vec<&Symbol> = candidates
            .iter()
            .filter_map(|result| all_symbols.iter().find(|s| s.id == result.symbol_id))
            .collect();

        // Step 4: Validate the semantic group
        if let Some(group) = self.validate_semantic_group(symbol, &candidate_symbols, &candidates) {
            Ok(vec![group])
        } else {
            Ok(vec![])
        }
    }

    /// Validate that the symbols form a legitimate semantic group
    fn validate_semantic_group(
        &self,
        target: &Symbol,
        candidates: &[&Symbol],
        similarity_results: &[SimilarityResult],
    ) -> Option<SemanticGroup> {
        // Must have symbols from at least 2 different languages (including target)
        let mut all_symbols = candidates.to_vec();
        all_symbols.push(target);

        let languages: HashSet<String> = all_symbols.iter().map(|s| s.language.clone()).collect();

        if languages.len() < 2 {
            return None;
        }

        // Check name similarity
        if !self.has_name_similarity(&all_symbols) {
            return None;
        }

        // Check structural similarity (if we can extract structure info)
        let structure_score = self.calculate_structure_similarity(&all_symbols);
        if structure_score < 0.3 {
            // Lower threshold since structure extraction is hard
            return None;
        }

        // Calculate overall group confidence
        let avg_similarity = similarity_results
            .iter()
            .map(|r| r.similarity_score)
            .sum::<f32>()
            / similarity_results.len() as f32;

        let confidence = (avg_similarity + structure_score) / 2.0;

        // Extract common properties before moving all_symbols
        let common_properties = self.extract_common_properties(&all_symbols);
        let owned_symbols: Vec<Symbol> = all_symbols.into_iter().cloned().collect();

        Some(SemanticGroup {
            id: uuid::Uuid::new_v4().to_string(),
            symbols: owned_symbols.clone(),
            confidence,
            similarity_score: avg_similarity,
            languages: languages.into_iter().collect(),
            common_properties,
            detected_pattern: self.detect_architectural_pattern(&owned_symbols),
        })
    }

    /// Check if symbols have similar names (fuzzy matching)
    pub(crate) fn has_name_similarity(&self, symbols: &[&Symbol]) -> bool {
        if symbols.len() < 2 {
            return false;
        }

        // Normalize names for comparison
        let normalized_names: Vec<String> = symbols
            .iter()
            .map(|s| self.normalize_name(&s.name))
            .collect();

        // Check if any pair has similar names
        for i in 0..normalized_names.len() {
            for j in (i + 1)..normalized_names.len() {
                if self.names_are_similar(&normalized_names[i], &normalized_names[j]) {
                    return true;
                }
            }
        }

        false
    }

    /// Normalize a symbol name for comparison
    pub(crate) fn normalize_name(&self, name: &str) -> String {
        name.to_lowercase()
            .trim_start_matches("i") // Remove interface prefix
            .trim_end_matches("dto")
            .trim_end_matches("entity")
            .trim_end_matches("model")
            .trim_end_matches("s") // Remove plural
            .to_string()
    }

    /// Check if two names are similar after normalization
    pub(crate) fn names_are_similar(&self, name1: &str, name2: &str) -> bool {
        // Normalize both names first
        let norm1 = self.normalize_name(name1);
        let norm2 = self.normalize_name(name2);

        // Exact match after normalization
        if norm1 == norm2 {
            return true;
        }

        // Check if one is contained in the other
        if norm1.contains(&norm2) || norm2.contains(&norm1) {
            return true;
        }

        // Simple Levenshtein distance check
        let distance = self.levenshtein_distance(&norm1, &norm2);
        let max_len = norm1.len().max(norm2.len());

        if max_len == 0 {
            return true;
        }

        // Allow up to 30% character differences
        (distance as f32 / max_len as f32) < 0.3
    }

    /// Calculate Levenshtein distance between two strings
    pub(crate) fn levenshtein_distance(&self, s1: &str, s2: &str) -> usize {
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();
        let len1 = chars1.len();
        let len2 = chars2.len();

        if len1 == 0 {
            return len2;
        }
        if len2 == 0 {
            return len1;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        #[allow(clippy::needless_range_loop)] // Index required for matrix initialization
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }

        matrix[len1][len2]
    }

    /// Calculate structural similarity between symbols
    pub(crate) fn calculate_structure_similarity(&self, symbols: &[&Symbol]) -> f32 {
        // For now, return a base score since extracting structural info is complex
        // In a full implementation, we'd parse signatures/types to extract fields

        // If all symbols have signatures, that's a good sign
        let signature_count = symbols.iter().filter(|s| s.signature.is_some()).count();

        if signature_count == symbols.len() {
            0.7 // Good structural similarity
        } else if signature_count > 0 {
            0.5 // Some structural similarity
        } else {
            0.3 // Minimal structural similarity
        }
    }

    /// Extract common properties/fields from symbols
    pub(crate) fn extract_common_properties(&self, symbols: &[&Symbol]) -> Vec<String> {
        // Simplified implementation - extract common words from names and signatures
        let mut word_counts: HashMap<String, usize> = HashMap::new();

        for symbol in symbols {
            // Extract words from symbol name
            let name_words = self.extract_words(&symbol.name);
            for word in name_words {
                *word_counts.entry(word).or_insert(0) += 1;
            }

            // Extract words from signature if available
            if let Some(signature) = &symbol.signature {
                let sig_words = self.extract_words(signature);
                for word in sig_words {
                    *word_counts.entry(word).or_insert(0) += 1;
                }
            }
        }

        // Return words that appear in multiple symbols
        word_counts
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(word, _)| word)
            .collect()
    }

    /// Extract meaningful words from text, including camelCase splitting
    fn extract_words(&self, text: &str) -> Vec<String> {
        let mut words = Vec::new();

        // First split on whitespace and punctuation
        let initial_words: Vec<&str> = text
            .split_whitespace()
            .flat_map(|word| {
                word.split(&['(', ')', '{', '}', '[', ']', '<', '>', ':', ';', ',', '.'])
            })
            .collect();

        for word in initial_words {
            let cleaned = word.trim();
            if cleaned.is_empty() {
                continue;
            }

            // Split camelCase/PascalCase words
            let camel_words = self.split_camel_case(cleaned);
            for camel_word in camel_words {
                if camel_word.len() > 2 && !self.is_stop_word(&camel_word.to_lowercase()) {
                    words.push(camel_word.to_lowercase());
                }
            }
        }

        words
    }

    /// Split camelCase or PascalCase strings into separate words
    fn split_camel_case(&self, input: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut current_word = String::new();
        let chars: Vec<char> = input.chars().collect();

        for (i, ch) in chars.iter().enumerate() {
            if ch.is_uppercase() && !current_word.is_empty() {
                // Check if this is the start of a new word
                // Don't split if it's an all-caps word like "API" or "XML"
                let next_is_lowercase = i + 1 < chars.len() && chars[i + 1].is_lowercase();
                if next_is_lowercase || (i > 0 && chars[i - 1].is_lowercase()) {
                    result.push(current_word.clone());
                    current_word.clear();
                }
            }
            current_word.push(*ch);
        }

        if !current_word.is_empty() {
            result.push(current_word);
        }

        // If no splitting occurred, return the original word
        if result.is_empty() {
            vec![input.to_string()]
        } else {
            result
        }
    }

    /// Check if a word should be ignored
    fn is_stop_word(&self, word: &str) -> bool {
        matches!(
            word,
            "the"
                | "and"
                | "or"
                | "but"
                | "in"
                | "on"
                | "at"
                | "to"
                | "for"
                | "of"
                | "with"
                | "by"
                | "public"
                | "private"
                | "static"
                | "class"
                | "interface"
                | "function"
                | "var"
                | "let"
                | "const"
                | "string"
                | "number"
                | "boolean"
        )
    }

    /// The magic: detect if this represents the same concept across layers
    pub fn detect_architectural_pattern(&self, symbols: &[Symbol]) -> ArchitecturalPattern {
        let has_frontend = symbols
            .iter()
            .any(|s| matches!(s.language.as_str(), "typescript" | "javascript"));
        let has_backend = symbols
            .iter()
            .any(|s| matches!(s.language.as_str(), "csharp" | "java" | "python"));
        let has_database = symbols.iter().any(|s| s.language == "sql");

        match (has_frontend, has_backend, has_database) {
            (true, true, true) => ArchitecturalPattern::FullStackEntity,
            (true, true, false) => ArchitecturalPattern::ApiContract,
            (false, true, true) => ArchitecturalPattern::DataLayer,
            _ => ArchitecturalPattern::Unknown,
        }
    }
}
