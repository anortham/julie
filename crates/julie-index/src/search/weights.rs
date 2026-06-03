//! Search weight profiles for per-tool RRF tuning.
//!
//! Each tool uses a different weight profile to bias the merge between
//! keyword (Tantivy) and semantic (embedding KNN) results:
//! - `fast_search`: Code-heavy — keyword matters most for symbol lookup
//! - `recall`: Memory-heavy — semantic similarity matters for concept recall
//! - `get_context`: Balanced — both code and memory are equally useful

/// Weight profile controlling how keyword and semantic results are merged.
#[derive(Debug, Clone)]
pub struct SearchWeightProfile {
    /// Weight for keyword/BM25 results (Tantivy).
    pub keyword_weight: f32,
    /// Weight for semantic/embedding results (KNN).
    pub semantic_weight: f32,
}

impl SearchWeightProfile {
    /// Profile for `fast_search` — code-heavy, keywords dominant.
    pub fn fast_search() -> Self {
        Self {
            keyword_weight: 1.0,
            semantic_weight: 0.7,
        }
    }

    /// Profile for `recall` — memory-heavy, semantic dominant.
    pub fn recall() -> Self {
        Self {
            keyword_weight: 0.7,
            semantic_weight: 1.0,
        }
    }

    /// Profile for `get_context` — balanced between keyword and semantic.
    pub fn get_context() -> Self {
        Self {
            keyword_weight: 1.0,
            semantic_weight: 0.8,
        }
    }
}

/// Query intent classification for dynamic weight profile selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Exact symbol lookup (snake_case, CamelCase, qualified names)
    SymbolLookup,
    /// Natural language / conceptual query (4+ words, no code-like tokens)
    Conceptual,
    /// Ambiguous mix of code and natural language
    Mixed,
}

/// Classify a search query to determine optimal keyword/semantic weighting.
///
/// Uses lightweight heuristics (no ML):
/// - snake_case, CamelCase, `::`, `.` separators -> SymbolLookup
/// - 4+ space-separated words with no code-like tokens -> Conceptual
/// - Everything else -> Mixed
pub fn classify_query(query: &str) -> QueryIntent {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return QueryIntent::Mixed;
    }

    // Check for code-like patterns
    let has_snake_case = trimmed.contains('_') && trimmed.chars().any(|c| c.is_lowercase());
    let has_qualified =
        trimmed.contains("::") || (trimmed.contains('.') && !trimmed.ends_with('.'));
    let has_camel_case = trimmed.chars().any(|c| c.is_uppercase())
        && trimmed.chars().any(|c| c.is_lowercase())
        && !trimmed.contains(' ');

    if has_snake_case || has_qualified || has_camel_case {
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() >= 3 {
            return QueryIntent::Mixed;
        }
        return QueryIntent::SymbolLookup;
    }

    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 4 {
        return QueryIntent::Conceptual;
    }

    QueryIntent::Mixed
}

impl QueryIntent {
    /// Map query intent to a search weight profile.
    pub fn to_weight_profile(&self) -> SearchWeightProfile {
        match self {
            QueryIntent::SymbolLookup => SearchWeightProfile {
                keyword_weight: 1.0,
                semantic_weight: 0.3,
            },
            QueryIntent::Conceptual => SearchWeightProfile {
                keyword_weight: 0.5,
                semantic_weight: 1.0,
            },
            QueryIntent::Mixed => SearchWeightProfile {
                keyword_weight: 0.8,
                semantic_weight: 0.8,
            },
        }
    }
}
