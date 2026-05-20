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
                assert_eq!(s, "LanguageParserPool");
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
    fn test_clean_or_disjunction_produces_file_level() {
        let strategy = line_match_strategy("logging.basicConfig OR datefmt");
        match &strategy {
            LineMatchStrategy::FileLevel { terms } => {
                assert_eq!(
                    terms,
                    &vec!["logging.basicConfig".to_string(), "datefmt".to_string()]
                );
            }
            other => panic!(
                "Expected FileLevel, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_qualified_or_disjunction_produces_file_level() {
        let strategy = line_match_strategy("Command::Search OR Command::Refs OR Command::Tool");
        match &strategy {
            LineMatchStrategy::FileLevel { terms } => {
                assert_eq!(
                    terms,
                    &vec![
                        "Command::Search".to_string(),
                        "Command::Refs".to_string(),
                        "Command::Tool".to_string(),
                    ],
                );
            }
            other => panic!(
                "Expected FileLevel, got {:?}",
                std::mem::discriminant(other)
            ),
        }
    }

    #[test]
    fn test_multi_word_or_stays_substring() {
        let strategy = line_match_strategy("INSERT OR REPLACE symbols");
        match strategy {
            LineMatchStrategy::Substring(s) => assert_eq!(s, "INSERT OR REPLACE symbols"),
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_sql_not_null_stays_substring() {
        let strategy = line_match_strategy("IS NOT NULL");
        match strategy {
            LineMatchStrategy::Substring(s) => assert_eq!(s, "IS NOT NULL"),
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_do_not_edit_stays_substring() {
        let strategy = line_match_strategy("DO NOT EDIT");
        match strategy {
            LineMatchStrategy::Substring(s) => assert_eq!(s, "DO NOT EDIT"),
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_quoted_or_phrase_stays_substring() {
        let strategy = line_match_strategy("\"INSERT OR REPLACE\"");
        match strategy {
            LineMatchStrategy::Substring(s) => assert_eq!(s, "\"INSERT OR REPLACE\""),
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_uppercase_sql_or_stays_substring() {
        let strategy = line_match_strategy("INSERT OR REPLACE");
        match strategy {
            LineMatchStrategy::Substring(s) => assert_eq!(s, "INSERT OR REPLACE"),
            other => panic!(
                "Expected Substring, got {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    #[test]
    fn test_kebab_or_produces_file_level() {
        let strategy = line_match_strategy("security-signals OR audit-events");
        match &strategy {
            LineMatchStrategy::FileLevel { terms } => {
                assert_eq!(
                    terms,
                    &vec!["security-signals".to_string(), "audit-events".to_string(),],
                );
            }
            other => panic!(
                "Expected FileLevel, got {:?}",
                std::mem::discriminant(other)
            ),
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
    fn test_file_level_compound_identifier_matches_contiguous_identifier() {
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["workspace_is_primary".to_string()],
        };

        assert!(line_matches(
            &strategy,
            "fn workspace_is_primary(workspace: &WorkspaceTarget) -> bool {",
        ));
        assert!(line_matches(
            &strategy,
            "let is_primary = workspace.is_primary();",
        ));
    }

    #[test]
    fn test_file_level_compound_identifier_does_not_match_individual_subtokens() {
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["workspace_is_primary".to_string(), "edit_file".to_string()],
        };

        assert!(!line_matches(
            &strategy,
            "workspace_label: Some(\"primary\".to_string()),",
        ));
        assert!(!line_matches(
            &strategy,
            "file_path: \"src/lib.rs\".to_string(),",
        ));
    }

    #[test]
    fn test_tokens_compound_required_identifier_does_not_match_individual_subtokens() {
        let strategy = LineMatchStrategy::Tokens {
            required: vec!["workspace_is_primary".to_string()],
            excluded: Vec::new(),
        };

        assert!(line_matches(
            &strategy,
            "let ok = workspace_is_primary(target);",
        ));
        assert!(!line_matches(
            &strategy,
            "workspace_label: Some(\"primary\".to_string()),",
        ));
    }

    #[test]
    fn test_tokens_compound_excluded_identifier_does_not_exclude_individual_subtokens() {
        let strategy = LineMatchStrategy::Tokens {
            required: vec!["call_tool".to_string()],
            excluded: vec!["edit_file".to_string()],
        };

        assert!(line_matches(
            &strategy,
            "call_tool handles file_path without editing",
        ));
        assert!(!line_matches(
            &strategy,
            "call_tool invokes edit_file for text changes",
        ));
    }

    #[test]
    fn test_file_level_simple_terms_keep_tokenized_matching() {
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["tokens".to_string()],
        };

        assert!(line_matches(&strategy, "pub struct TokenEstimator;"));
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

    #[test]
    fn test_hyphen_query_matches_underscored_line() {
        let strategy = line_match_strategy("security-signals");

        assert!(
            line_matches(&strategy, "let security_signals = Signals::new();"),
            "hyphenated query should match an underscored code identifier",
        );
    }

    #[test]
    fn test_underscore_query_matches_hyphenated_line() {
        let strategy = line_match_strategy("security_signals");

        assert!(
            line_matches(&strategy, "name = \"security-signals\""),
            "underscored query should match a hyphenated literal",
        );
    }

    #[test]
    fn test_backslash_stripping_matches_literal_punctuation() {
        let strategy = line_match_strategy("\\.julie/logs");

        assert!(
            line_matches(&strategy, "tail -f .julie/logs/julie.log"),
            "escaped punctuation query should match the literal path",
        );
    }

    #[test]
    fn test_punctuation_heavy_queries_match_literal_substrings() {
        let os_strategy = line_match_strategy("OS.has_feature");
        let arg_action_strategy = line_match_strategy("ArgAction::SetTrue");
        let unwrap_strategy = line_match_strategy("target_symbol_id.unwrap_or");
        let tool_name_strategy = line_match_strategy("tool_name(&self)");

        assert!(matches!(os_strategy, LineMatchStrategy::Substring(_)));
        assert!(matches!(
            arg_action_strategy,
            LineMatchStrategy::Substring(_)
        ));
        assert!(matches!(unwrap_strategy, LineMatchStrategy::Substring(_)));
        assert!(matches!(
            tool_name_strategy,
            LineMatchStrategy::Substring(_)
        ));

        assert!(
            line_matches(&os_strategy, "if os.has_feature("),
            "dot-heavy query should still match the exact literal shape",
        );
        assert!(
            line_matches(&arg_action_strategy, "ArgAction::SetTrue,"),
            "Rust-style enum path should match as a literal substring",
        );
        assert!(
            line_matches(
                &unwrap_strategy,
                "let target_symbol_id = target_symbol_id.unwrap_or(\"unknown\");"
            ),
            "call-shaped query should match the exact literal substring",
        );
        assert!(
            line_matches(&tool_name_strategy, "fn tool_name(&self) -> &'static str {"),
            "call-shaped query with self should match the exact literal substring",
        );
    }

    #[test]
    fn test_punctuation_heavy_queries_match_separator_variants_via_tokens() {
        let os_strategy = line_match_strategy("OS.has_feature");
        let arg_action_strategy = line_match_strategy("ArgAction::SetTrue");
        let unwrap_strategy = line_match_strategy("target_symbol_id.unwrap_or");
        let tool_name_strategy = line_match_strategy("tool_name(&self)");

        assert!(matches!(os_strategy, LineMatchStrategy::Substring(_)));
        assert!(matches!(
            arg_action_strategy,
            LineMatchStrategy::Substring(_)
        ));
        assert!(matches!(unwrap_strategy, LineMatchStrategy::Substring(_)));
        assert!(matches!(
            tool_name_strategy,
            LineMatchStrategy::Substring(_)
        ));

        assert!(
            line_matches(&os_strategy, "if OS::has_feature() {"),
            "dot-heavy query should survive separator normalization",
        );
        assert!(
            line_matches(&arg_action_strategy, "ArgAction.SetTrue"),
            "double-colon query should survive separator normalization",
        );
        assert!(
            line_matches(
                &unwrap_strategy,
                "let next = target_symbol_id.unwrap_or_else(|| next_symbol_id);"
            ),
            "call-shaped query should survive tokenized matching when the suffix changes",
        );
        assert!(
            line_matches(
                &tool_name_strategy,
                "fn tool_name(self: &Self) -> &'static str {"
            ),
            "self receiver syntax should survive tokenized matching",
        );
    }

    #[test]
    fn test_punctuation_heavy_queries_do_not_match_unrelated_separated_tokens() {
        let os_strategy = line_match_strategy("OS.has_feature");

        assert!(
            !line_matches(
                &os_strategy,
                "let os = platform_os(); let has_feature = feature_flags.enabled;"
            ),
            "separator fallback should not match unrelated tokens that are split across expressions",
        );
    }

    // ── Finding A: camelCase / snake_case cross-matching ──

    #[test]
    fn test_camel_case_query_matches_snake_case_line() {
        let strategy = line_match_strategy("workspaceIsPrimary");
        assert!(
            line_matches(&strategy, "let workspace_is_primary = true;"),
            "camelCase query should match snake_case code line"
        );
    }

    #[test]
    fn test_snake_case_query_matches_camel_case_line() {
        let strategy = line_match_strategy("workspace_is_primary");
        assert!(
            line_matches(&strategy, "let workspaceIsPrimary = true;"),
            "snake_case query should match camelCase code line via _↔- swap + case-boundary variants"
        );
    }

    #[test]
    fn test_kebab_case_query_matches_snake_case_line() {
        let strategy = line_match_strategy("workspace-is-primary");
        assert!(
            line_matches(&strategy, "workspace_is_primary"),
            "kebab-case query should match snake_case code line via _↔- swap"
        );
    }

    #[test]
    fn test_filelevel_camel_case_compound_term_matches_snake_case_line() {
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["workspaceIsPrimary".to_string()],
        };
        assert!(
            line_matches(&strategy, "workspace_is_primary"),
            "FileLevel compound camelCase term should match snake_case line"
        );
    }

    // ── Finding B: Same-line AND density boosting ──

    #[test]
    fn test_filelevel_density_sort_promotes_multi_term_line() {
        use crate::tools::search::line_mode::collect_line_matches;
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["alpha".to_string(), "beta".to_string()],
        };
        let content =
            "line with alpha only\nline with beta only\nline with alpha and beta together";
        let mut matches = Vec::new();
        collect_line_matches(&mut matches, content, "test.rs", &strategy, 10);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].line_number, 3, "dense line should be first");
    }

    #[test]
    fn test_filelevel_ties_preserve_source_order() {
        use crate::tools::search::line_mode::collect_line_matches;
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["alpha".to_string(), "beta".to_string()],
        };
        let content = "line with alpha\nline with beta\nanother alpha line\nanother beta line";
        let mut matches = Vec::new();
        collect_line_matches(&mut matches, content, "test.rs", &strategy, 10);
        assert_eq!(matches.len(), 4);
        // All density=1, so line number order preserved
        let line_numbers: Vec<usize> = matches.iter().map(|m| m.line_number).collect();
        assert_eq!(line_numbers, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_filelevel_density_dedupes_repeated_query_terms() {
        use crate::tools::search::line_mode::collect_line_matches;
        let strategy = LineMatchStrategy::FileLevel {
            terms: vec!["alpha".to_string(), "alpha".to_string(), "beta".to_string()],
        };
        // Line 1: "alpha alpha" (2 occurrences of same term, but only 1 distinct)
        // Line 2: "alpha beta" (2 distinct terms)
        let content = "alpha alpha\nalpha beta";
        let mut matches = Vec::new();
        collect_line_matches(&mut matches, content, "test.rs", &strategy, 10);
        assert_eq!(matches.len(), 2);
        // Line 2 (alpha beta, density 2 distinct) outranks Line 1 (alpha alpha, density 1 distinct)
        assert_eq!(
            matches[0].line_number, 2,
            "line with distinct terms should outrank repeated single term"
        );
    }

    #[test]
    fn test_substring_strategy_preserves_source_order() {
        use crate::tools::search::line_mode::collect_line_matches;
        let strategy = LineMatchStrategy::Substring("alpha".to_string());
        let content = "beta line\nalpha line\ngamma alpha line";
        let mut matches = Vec::new();
        collect_line_matches(&mut matches, content, "test.rs", &strategy, 10);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 2);
        assert_eq!(matches[1].line_number, 3);
    }

    #[test]
    fn test_tokens_strategy_preserves_source_order() {
        use crate::tools::search::line_mode::collect_line_matches;
        let strategy = LineMatchStrategy::Tokens {
            required: vec!["alpha".to_string()],
            excluded: Vec::new(),
        };
        let content = "beta\nline with alpha3\nalpha line\nmore alpha";
        let mut matches = Vec::new();
        collect_line_matches(&mut matches, content, "test.rs", &strategy, 10);
        // Tokens strategy: "alpha" token should match tokenized forms
        assert!(matches.len() >= 2);
        let line_numbers: Vec<usize> = matches.iter().map(|m| m.line_number).collect();
        // Verify source order preserved (ascending)
        for i in 1..line_numbers.len() {
            assert!(
                line_numbers[i] > line_numbers[i - 1],
                "source order should be preserved for Tokens"
            );
        }
    }
}
