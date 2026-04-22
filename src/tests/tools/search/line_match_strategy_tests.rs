#[cfg(test)]
mod line_match_strategy_tests {
    use crate::tools::search::LineMatchStrategy;
    use crate::tools::search::query::{line_match_strategy, line_matches};

    #[test]
    fn test_single_identifier_produces_substring() {
        let strategy = line_match_strategy("files_by_language");
        match strategy {
            LineMatchStrategy::Substring(s) => {
                assert_eq!(s, "files_by_language");
            }
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_single_camel_case_produces_substring() {
        let strategy = line_match_strategy("LanguageParserPool");
        match strategy {
            LineMatchStrategy::Substring(s) => {
                assert_eq!(s, "languageparserpool");
            }
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_multi_word_produces_file_level() {
        let strategy = line_match_strategy("spawn_blocking statistics");
        match &strategy {
            LineMatchStrategy::FileLevel { terms } => {
                assert_eq!(terms.len(), 2);
                assert!(terms.contains(&"spawn_blocking".to_string()));
                assert!(terms.contains(&"statistics".to_string()));
            }
            other => panic!(
                "Expected FileLevel, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_multi_word_with_exclusion_keeps_tokens_strategy() {
        let strategy = line_match_strategy("spawn_blocking -test");
        match &strategy {
            LineMatchStrategy::Tokens { required, excluded } => {
                assert!(required.contains(&"spawn_blocking".to_string()));
                assert!(excluded.contains(&"test".to_string()));
            }
            other => panic!("Expected Tokens, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn test_file_level_line_matches_or_logic() {
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["spawn_blocking".to_string(), "statistics".to_string()],
        };
        // Matches line with first term
        assert!(line_matches(
            &strategy,
            "let handle = spawn_blocking(move || {"
        ));
        // Matches line with second term
        assert!(line_matches(
            &strategy,
            "// compute statistics for the batch"
        ));
        // Does NOT match line with neither term
        assert!(!line_matches(
            &strategy,
            "fn process_data(input: &[u8]) -> Result<()> {"
        ));
        // Case-insensitive
        assert!(line_matches(&strategy, "SPAWN_BLOCKING is loud"));
    }

    #[test]
    fn test_file_level_line_matches_tokenized_terms() {
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["tokens".to_string(), "estimation".to_string()],
        };

        assert!(
            line_matches(&strategy, "pub struct TokenEstimator;"),
            "file-level verifier should honor tokenizer splits/stems, not only raw substrings",
        );
    }

    #[test]
    fn test_tokens_strategy_excluded_terms_use_tokenized_forms() {
        let strategy = LineMatchStrategy::Tokens {
            required: vec!["format".to_string()],
            excluded: vec!["tests".to_string()],
        };

        assert!(
            !line_matches(&strategy, "fn test_format_output() {}"),
            "excluded query terms should catch tokenized/stemmed forms on the line",
        );
    }

    #[test]
    fn test_quoted_phrase_matches_tokenized_code_separator_sequence() {
        let strategy = line_match_strategy("\"router use\"");

        assert!(
            line_matches(&strategy, "router.use('/foo', middleware);"),
            "quoted phrase verifier should match adjacent code tokens split by punctuation",
        );
    }
}
