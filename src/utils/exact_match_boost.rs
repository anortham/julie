// ExactMatchBoost - Logarithmic scoring for exact matches
//
// Implements logarithmic boost scoring to prioritize exact matches in search results.
// Based on battle-tested search engine patterns for relevance ranking.

use std::collections::HashSet;

/// Provides logarithmic boost scoring for exact and partial matches
///
/// # Examples
/// ```
/// use julie::utils::exact_match_boost::ExactMatchBoost;
///
/// let booster = ExactMatchBoost::new("getUserData");
///
/// assert!(booster.calculate_boost("getUserData") > 2.0); // Exact match
/// assert!(booster.calculate_boost("getUserDataAsync") > 1.0); // Prefix match
/// assert_eq!(booster.calculate_boost("completely_different"), 1.0); // No match
/// ```
pub struct ExactMatchBoost {
    /// Original search query
    query: String,
    /// Lowercase version for case-insensitive matching
    pub query_lower: String,
    /// Query words split and normalized
    pub query_words: Vec<String>,
}

impl ExactMatchBoost {
    /// Create new ExactMatchBoost for given search query
    ///
    /// # Arguments
    /// * `query` - Search query to boost matches for
    pub fn new(query: &str) -> Self {
        let query_lower = query.to_lowercase();
        let query_words = Self::tokenize_query(&query_lower);

        Self {
            query: query.to_string(),
            query_lower,
            query_words,
        }
    }

    /// Check if symbol name is an exact match for the query
    ///
    /// Performs case-insensitive exact matching.
    ///
    /// # Arguments
    /// * `symbol_name` - Symbol name to check
    ///
    /// # Returns
    /// True if exact match, false otherwise
    pub fn is_exact_match(&self, symbol_name: &str) -> bool {
        self.query_lower == symbol_name.to_lowercase()
    }

    /// Calculate logarithmic boost factor for symbol name
    ///
    /// Returns boost multiplier based on match quality:
    /// - Exact match: ~2.5-3.0x boost
    /// - Prefix match: ~1.5-2.0x boost
    /// - Substring match: ~1.1-1.3x boost
    /// - No match: 1.0x (no boost)
    ///
    /// # Arguments
    /// * `symbol_name` - Symbol name to calculate boost for
    ///
    /// # Returns
    /// Boost multiplier (1.0 = no boost, >1.0 = boost)
    pub fn calculate_boost(&self, symbol_name: &str) -> f32 {
        if symbol_name.is_empty() || self.query.is_empty() {
            return 1.0;
        }

        let symbol_lower = symbol_name.to_lowercase();

        // Check different match types and calculate logarithmic boost
        if self.is_exact_match(symbol_name) {
            self.exact_match_boost()
        } else if self.is_prefix_match(&symbol_lower) {
            self.prefix_match_boost(&symbol_lower)
        } else if self.is_substring_match(&symbol_lower) {
            self.substring_match_boost(&symbol_lower)
        } else if self.is_word_boundary_match(symbol_name) {
            self.word_boundary_match_boost(symbol_name)
        } else {
            1.0 // No boost
        }
    }

    /// Calculate exact match boost using logarithmic scaling
    fn exact_match_boost(&self) -> f32 {
        // Base boost of ~2.7 for exact matches (e^1 ≈ 2.718)
        // This provides significant but not overwhelming boost
        (1.0 + self.query.len() as f32).ln() + 2.0
    }

    /// Calculate prefix match boost
    fn prefix_match_boost(&self, symbol_lower: &str) -> f32 {
        // Boost based on how much of the symbol is matched by prefix
        let match_ratio = self.query_lower.len() as f32 / symbol_lower.len() as f32;
        let base_boost = 1.0 + (1.0 + match_ratio).ln();

        // Scale to reasonable range (1.3 - 1.8)
        1.3 + (base_boost - 1.0) * 0.5
    }

    /// Calculate substring match boost
    fn substring_match_boost(&self, symbol_lower: &str) -> f32 {
        // Small boost for substring matches
        let match_ratio = self.query_lower.len() as f32 / symbol_lower.len() as f32;
        let base_boost = 1.0 + (1.0 + match_ratio * 0.5).ln() * 0.2;

        // Keep in small range (1.05 - 1.2)
        1.05 + (base_boost - 1.0) * 0.15
    }

    /// Calculate word boundary match boost (for camelCase, snake_case, etc.)
    fn word_boundary_match_boost(&self, symbol_name: &str) -> f32 {
        let word_matches = self.count_word_matches(symbol_name);
        if word_matches == 0 {
            return 1.0;
        }

        // Logarithmic boost based on word matches
        let match_ratio = word_matches as f32 / self.query_words.len() as f32;

        // If all words match, give a very high boost (almost like exact match)
        if word_matches == self.query_words.len() {
            return 2.5 + (1.0 + word_matches as f32).ln() * 0.5;
        }

        // Partial word matches get moderate boost
        1.3 + (1.0 + match_ratio).ln() * 0.3
    }

    /// Check if query is a prefix of symbol
    fn is_prefix_match(&self, symbol_lower: &str) -> bool {
        symbol_lower.starts_with(&self.query_lower)
    }

    /// Check if query is a substring of symbol
    fn is_substring_match(&self, symbol_lower: &str) -> bool {
        symbol_lower.contains(&self.query_lower)
    }

    /// Check if query words match word boundaries in symbol (camelCase, snake_case)
    fn is_word_boundary_match(&self, symbol_name: &str) -> bool {
        self.count_word_matches(symbol_name) > 0
    }

    /// Count how many query words match word boundaries in symbol
    pub fn count_word_matches(&self, symbol_name: &str) -> usize {
        // Tokenize the original symbol name (with camelCase intact)
        let symbol_words = Self::tokenize_symbol(symbol_name);
        let symbol_word_set: HashSet<&String> = symbol_words.iter().collect();

        self.query_words
            .iter()
            .filter(|word| symbol_word_set.contains(word))
            .count()
    }

    /// Tokenize query into words (split on spaces and normalize)
    pub(crate) fn tokenize_query(query: &str) -> Vec<String> {
        query
            .split_whitespace()
            .map(|word| word.to_lowercase())
            .filter(|word| !word.is_empty())
            .collect()
    }

    /// Tokenize symbol name into words (camelCase, snake_case, kebab-case)
    pub fn tokenize_symbol(symbol: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current_word = String::new();
        let chars: Vec<char> = symbol.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if ch.is_ascii_alphanumeric() {
                let char_is_upper = ch.is_uppercase();
                let _char_is_lower = ch.is_lowercase();
                let prev_char = if i > 0 { Some(chars[i - 1]) } else { None };
                let next_char = if i + 1 < chars.len() {
                    Some(chars[i + 1])
                } else {
                    None
                };

                let should_split = if let Some(prev) = prev_char {
                    // Split camelCase: lowercase followed by uppercase
                    // Example: "getUserData" -> split at 'r'|'U'
                    (prev.is_lowercase() && char_is_upper) ||
                    // Split acronyms followed by words: uppercase followed by uppercase then lowercase
                    // Example: "XMLParser" -> split at 'L'|'P' because 'L' is upper, 'P' is upper, next is lower
                    (prev.is_uppercase() && char_is_upper &&
                     next_char.is_some_and(|n| n.is_lowercase()) &&
                     !current_word.is_empty())
                } else {
                    false
                };

                if should_split {
                    words.push(current_word.to_lowercase());
                    current_word.clear();
                }

                current_word.push(ch);
            } else if ch == '_' || ch == '-' {
                // Word separator for snake_case and kebab-case
                if !current_word.is_empty() {
                    words.push(current_word.to_lowercase());
                    current_word.clear();
                }
            }
            // Skip other characters
        }

        if !current_word.is_empty() {
            words.push(current_word.to_lowercase());
        }

        words.into_iter().filter(|word| !word.is_empty()).collect()
    }
}
