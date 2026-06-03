//! Focused body-analysis tests for `analyze_test_body`.

#[cfg(test)]
mod tests {
    use crate::analysis::test_quality::analyze_test_body;

    #[test]
    fn test_comment_assertions_not_counted() {
        let body = r#"
            let result = do_something();
            // assert_eq!(result, expected)  <-- commented out
            // should_err is a note
            println!("done");
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "commented-out assertions should not count"
        );
    }

    #[test]
    fn test_string_literal_mocks_not_counted() {
        let body = r#"
            let name = "mock_function_name";
            let desc = "when(something).thenReturn(value)";
            do_real_work();
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.mock_count, 0,
            "mock patterns inside strings should not count"
        );
    }

    #[test]
    fn test_block_comment_assertions_not_counted() {
        let body = r#"
            let x = 1;
            /* assert_eq!(x, 1);
               expect(x).toBe(1); */
            println!("test");
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "block-commented assertions should not count"
        );
    }

    #[test]
    fn test_real_assertions_still_counted_after_stripping() {
        let body = r#"
            // This test checks authentication
            let result = authenticate();
            assert_eq!(result, true);
            assert!(result.is_ok());
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "real assertions should still count after stripping comments"
        );
    }

    #[test]
    fn test_assert_in_variable_name_not_counted() {
        // "assertion_helper" contains "assert" but \bassert\b should only match whole word.
        let body = r#"
            let assertion_helper = setup();
            let assertive = true;
        "#;
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 0,
            "assert in variable names should not match"
        );
    }

    #[test]
    fn test_multiple_assertions_on_same_line() {
        let body = "assert_eq!(a, b); assert_ne!(c, d);";
        let assessment = analyze_test_body(body);
        assert_eq!(
            assessment.evidence.assertion_count, 2,
            "Two assertions on same line should both count"
        );
    }
}
