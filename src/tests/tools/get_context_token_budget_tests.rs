//! Tests for get_context token-budget truncation helpers.

#[cfg(test)]
mod token_budget_tests {
    use crate::tools::get_context::pipeline::truncate_to_token_budget;

    #[test]
    fn test_truncate_small_content_unchanged() {
        let small_code = "fn hello() {\n    println!(\"hi\");\n}";
        let result = truncate_to_token_budget(small_code, 500);
        assert_eq!(result, small_code);
    }

    #[test]
    fn test_truncate_large_content_reduced() {
        let mut lines = vec!["fn big_function() {".to_string()];
        for i in 0..100 {
            lines.push(format!("    let x{} = {};", i, i));
        }
        lines.push("}".to_string());
        let large_code = lines.join("\n");

        let result = truncate_to_token_budget(&large_code, 50);
        assert!(
            result.len() < large_code.len(),
            "Result should be shorter than input"
        );
        assert!(
            result.contains("lines omitted to fit token budget"),
            "Should have omission marker"
        );

        let estimator = crate::utils::token_estimation::TokenEstimator::new();
        let result_tokens = estimator.estimate_string(&result);
        assert!(
            result_tokens <= 60,
            "Truncated content should be near the budget of 50 tokens. Got: {} tokens",
            result_tokens
        );
    }

    #[test]
    fn test_truncate_preserves_head_bias() {
        let mut lines =
            vec!["fn important_function(arg1: Type1, arg2: Type2) -> Result {".to_string()];
        for i in 0..50 {
            lines.push(format!("    let step{} = process{};", i, i));
        }
        lines.push("    Ok(final_result)".to_string());
        lines.push("}".to_string());
        let code = lines.join("\n");

        let result = truncate_to_token_budget(&code, 50);
        assert!(
            result.starts_with("fn important_function"),
            "Should preserve function signature"
        );
        assert!(result.ends_with("}"), "Should preserve closing brace");
    }

    #[test]
    fn test_truncate_very_short_content_unchanged() {
        let short = "a\nb\nc\nd\ne";
        let result = truncate_to_token_budget(short, 1);
        assert_eq!(result, short);
    }
}
