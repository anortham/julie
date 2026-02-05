//! Tests for important_patterns post-search scoring boost
//!
//! Verifies that search results whose signatures match language-specific
//! important_patterns (e.g., "pub fn", "public class") get a score boost,
//! pushing primary definitions above private/internal ones.

#[cfg(test)]
mod tests {
    use crate::search::index::SymbolSearchResult;
    use crate::search::language_config::LanguageConfigs;
    use crate::search::scoring::apply_important_patterns_boost;

    fn make_result(name: &str, signature: &str, language: &str, score: f32) -> SymbolSearchResult {
        SymbolSearchResult {
            id: format!("test_{}", name),
            name: name.to_string(),
            signature: signature.to_string(),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: language.to_string(),
            start_line: 1,
            score,
        }
    }

    #[test]
    fn test_important_patterns_boost_pub_fn() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("process", "fn process()", "rust", 1.0),
            make_result("process", "pub fn process()", "rust", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // "pub fn" matches an important pattern in rust.toml — should be boosted
        assert!(
            results[0].signature.contains("pub fn"),
            "pub fn should rank first after boost: {:?}",
            results.iter().map(|r| &r.signature).collect::<Vec<_>>()
        );
        assert!(
            results[0].score > results[1].score,
            "Boosted result should have higher score: {} vs {}",
            results[0].score,
            results[1].score
        );
    }

    #[test]
    fn test_important_patterns_boost_preserves_order_when_no_match() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("foo", "fn foo()", "rust", 2.0),
            make_result("bar", "fn bar()", "rust", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // Neither matches an important pattern — relative order preserved
        assert_eq!(results[0].name, "foo");
        assert_eq!(results[1].name, "bar");
    }

    #[test]
    fn test_important_patterns_boost_multiple_languages() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("User", "class User", "csharp", 1.0),
            make_result("User", "public class User", "csharp", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // C# "public class" is an important pattern
        assert!(
            results[0].signature.contains("public class"),
            "public class should rank first for C#: {:?}",
            results.iter().map(|r| &r.signature).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_important_patterns_boost_only_once() {
        let configs = LanguageConfigs::load_embedded();

        // A signature that matches multiple patterns should only be boosted once
        let mut results = vec![
            make_result("process", "pub async fn process()", "rust", 1.0),
        ];

        apply_important_patterns_boost(&mut results, &configs);

        // "pub fn" matches, "async fn" matches — but boost only applies once (1.5x)
        assert!(
            (results[0].score - 1.5).abs() < 0.01,
            "Should boost exactly once (1.5x), got: {}",
            results[0].score
        );
    }

    #[test]
    fn test_important_patterns_boost_unknown_language() {
        let configs = LanguageConfigs::load_embedded();

        let mut results = vec![
            make_result("foo", "some signature", "unknown_lang", 1.0),
        ];

        let score_before = results[0].score;
        apply_important_patterns_boost(&mut results, &configs);

        // Unknown language — no config, no boost
        assert_eq!(
            results[0].score, score_before,
            "Unknown language should not be boosted"
        );
    }
}
