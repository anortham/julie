#[cfg(test)]
mod tests {
    use crate::search::weights::{QueryIntent, classify_query};

    #[test]
    fn test_classify_snake_case_as_symbol() {
        assert_eq!(classify_query("hybrid_search"), QueryIntent::SymbolLookup);
        assert_eq!(
            classify_query("prepare_batch_for_embedding"),
            QueryIntent::SymbolLookup
        );
    }

    #[test]
    fn test_classify_camel_case_as_symbol() {
        assert_eq!(
            classify_query("SearchWeightProfile"),
            QueryIntent::SymbolLookup
        );
        assert_eq!(classify_query("SymbolDatabase"), QueryIntent::SymbolLookup);
    }

    #[test]
    fn test_classify_qualified_name_as_symbol() {
        assert_eq!(
            classify_query("std::collections::HashMap"),
            QueryIntent::SymbolLookup
        );
        assert_eq!(classify_query("Phoenix.Router"), QueryIntent::SymbolLookup);
    }

    #[test]
    fn test_classify_natural_language_as_conceptual() {
        assert_eq!(
            classify_query("error handling and retry logic"),
            QueryIntent::Conceptual
        );
        assert_eq!(
            classify_query("how does authentication work"),
            QueryIntent::Conceptual
        );
        assert_eq!(
            classify_query("search scoring and ranking"),
            QueryIntent::Conceptual
        );
    }

    #[test]
    fn test_classify_short_natural_language_as_mixed() {
        assert_eq!(classify_query("error handling"), QueryIntent::Mixed);
        assert_eq!(classify_query("payment validation"), QueryIntent::Mixed);
    }

    #[test]
    fn test_classify_single_lowercase_word_as_mixed() {
        assert_eq!(classify_query("search"), QueryIntent::Mixed);
        assert_eq!(classify_query("database"), QueryIntent::Mixed);
    }

    #[test]
    fn test_classify_mixed_code_and_nl() {
        assert_eq!(
            classify_query("SymbolDatabase query methods"),
            QueryIntent::Mixed
        );
    }

    #[test]
    fn test_classify_empty_as_mixed() {
        assert_eq!(classify_query(""), QueryIntent::Mixed);
        assert_eq!(classify_query("   "), QueryIntent::Mixed);
    }
}

/// Characterization tests for `is_nl_like_query` — verifies the NL detection
/// heuristic that gates hybrid search activation in `fast_search`.
#[cfg(test)]
mod nl_query_detection_tests {
    use crate::search::scoring::is_nl_like_query;

    #[test]
    fn test_is_nl_like_query_examples() {
        // NL queries that SHOULD trigger hybrid search
        assert!(is_nl_like_query("how does the server start up"));
        assert!(is_nl_like_query("find symbols similar to each other"));
        assert!(is_nl_like_query("what happens when a file is modified"));

        // Code queries that should NOT trigger hybrid search
        assert!(!is_nl_like_query("UserService"));
        assert!(!is_nl_like_query("extract_identifiers"));
        assert!(!is_nl_like_query("rrf_merge"));
    }

    /// Mixed queries (identifier name + prose context) are the most common
    /// dogfood pattern — e.g. "how does fast_refs find callers". Before the
    /// fix, any term containing `_` or mixedCase vetoed NL detection for the
    /// whole query, silently disabling hybrid search and the reranker for
    /// these queries. They must be treated as NL so the reranker engages.
    #[test]
    fn test_is_nl_like_query_mixed_identifier_and_prose() {
        // Dogfound failures from 2026-05-17 reranker session:
        assert!(
            is_nl_like_query("how does fast_refs find callers"),
            "NL question that names a symbol should engage hybrid search"
        );
        assert!(
            is_nl_like_query("parse_query reranker intent classification"),
            "symbol + topical context should engage hybrid search"
        );

        // Minimal mixed cases:
        assert!(
            is_nl_like_query("parse_query bug"),
            "single identifier + prose word should engage hybrid search"
        );
        assert!(
            is_nl_like_query("UserService refactor"),
            "mixedCase identifier + prose word should engage hybrid search"
        );
        assert!(
            is_nl_like_query("the rrf_merge function"),
            "prose + identifier + prose should engage hybrid search"
        );
    }

    /// Regression guard: when ALL terms look like identifiers, the query is
    /// still a code search and must NOT trigger hybrid. This keeps exact
    /// multi-symbol lookups ("parse_query rrf_merge", "UserService AuthHandler")
    /// out of the NL path.
    #[test]
    fn test_is_nl_like_query_all_identifiers_stays_code() {
        assert!(!is_nl_like_query("extract_identifiers rrf_merge"));
        assert!(!is_nl_like_query("UserService AuthHandler"));
        assert!(!is_nl_like_query("foo_bar baz_qux"));
        assert!(!is_nl_like_query("parse_query score_candidate"));
    }
}
