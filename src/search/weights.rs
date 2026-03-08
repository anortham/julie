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
