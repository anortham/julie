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
