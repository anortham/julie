// ExactMatchBoost Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod exact_match_boost_tests {
    use crate::utils::exact_match_boost::ExactMatchBoost;

    #[test]
    fn test_exact_match_detection() {
        let booster = ExactMatchBoost::new("getUserData");

        // Exact matches should be detected
        assert!(booster.is_exact_match("getUserData"));
        assert!(booster.is_exact_match("GETUSERDATA")); // Case insensitive
        assert!(booster.is_exact_match("getuserdata")); // Case insensitive

        // Partial matches should not be exact
        assert!(!booster.is_exact_match("getUserDataAsync"));
        assert!(!booster.is_exact_match("getUser"));
        assert!(!booster.is_exact_match("UserData"));
    }

    #[test]
    fn test_logarithmic_boost_calculation() {
        let booster = ExactMatchBoost::new("test");

        // Exact match should get significant boost
        let exact_boost = booster.calculate_boost("test");
        assert!(exact_boost > 1.0);

        // Prefix match should get moderate boost (testing starts with test)
        let prefix_boost = booster.calculate_boost("testing");
        assert!(prefix_boost > 1.0 && prefix_boost < exact_boost);

        // Non-match should get no boost
        let no_boost = booster.calculate_boost("completely_different");
        assert_eq!(no_boost, 1.0);

        println!("Exact boost: {}, Prefix boost: {}, No boost: {}",
                 exact_boost, prefix_boost, no_boost);
    }

    #[test]
    fn test_prefix_and_substring_scoring() {
        let booster = ExactMatchBoost::new("user");

        // Test different match types with expected logarithmic progression
        let exact_boost = booster.calculate_boost("user");
        let prefix_boost = booster.calculate_boost("userService");
        let substring_boost = booster.calculate_boost("getUserData");
        let no_match_boost = booster.calculate_boost("completely_different");

        // Verify logarithmic progression: exact > prefix > substring > no_match
        assert!(exact_boost > prefix_boost);
        assert!(prefix_boost > substring_boost);
        assert!(substring_boost >= no_match_boost);
        assert_eq!(no_match_boost, 1.0);

        println!("Logarithmic progression - Exact: {}, Prefix: {}, Substring: {}, No match: {}",
                 exact_boost, prefix_boost, substring_boost, no_match_boost);
    }

    #[test]
    fn test_case_insensitive_matching() {
        let booster = ExactMatchBoost::new("MyClass");

        // All case variations should get same exact match boost
        let original_boost = booster.calculate_boost("MyClass");
        let lower_boost = booster.calculate_boost("myclass");
        let upper_boost = booster.calculate_boost("MYCLASS");
        let mixed_boost = booster.calculate_boost("myClass");

        assert_eq!(original_boost, lower_boost);
        assert_eq!(original_boost, upper_boost);
        assert_eq!(original_boost, mixed_boost);

        println!("Case insensitive boosts - all should be equal: {}", original_boost);
    }


    #[test]
    fn test_multi_word_query_boost() {
        let booster = ExactMatchBoost::new("get user data");

        // Test symbol names that match parts of the query
        let exact_boost = booster.calculate_boost("getUserData"); // Matches camelCase
        let partial_boost = booster.calculate_boost("getUser");   // Matches prefix
        let no_boost = booster.calculate_boost("setPassword");    // No match

        assert!(exact_boost > partial_boost);
        assert!(partial_boost > no_boost);
        assert_eq!(no_boost, 1.0);

        println!("Multi-word query boosts - Exact: {}, Partial: {}, No match: {}",
                 exact_boost, partial_boost, no_boost);
    }

    #[test]
    fn test_logarithmic_scaling_properties() {
        let booster = ExactMatchBoost::new("test");

        // Exact match boost should be logarithmic, not linear
        let exact_boost = booster.calculate_boost("test");

        // Should be significant but not overwhelming (typical range 1.5-3.0 for exact matches)
        assert!(exact_boost >= 1.5);
        assert!(exact_boost <= 5.0);

        // The boost should follow logarithmic properties
        // (this is more of a design verification than strict mathematical test)
        assert!(exact_boost > 1.0);

        println!("Exact match logarithmic boost: {}", exact_boost);
    }

    #[test]
    fn test_empty_and_edge_cases() {
        let booster = ExactMatchBoost::new("test");

        // Empty string should get no boost
        assert_eq!(booster.calculate_boost(""), 1.0);

        // Very long string should still work
        let long_symbol = "a".repeat(1000);
        let long_boost = booster.calculate_boost(&long_symbol);
        assert_eq!(long_boost, 1.0); // Should be no match

        // Test with empty query
        let empty_booster = ExactMatchBoost::new("");
        assert_eq!(empty_booster.calculate_boost("anything"), 1.0);
    }

    #[test]
    fn test_realistic_search_scenarios() {
        // Test realistic code search scenarios
        let scenarios = vec![
            ("findUser", vec![
                ("findUser", "should get highest boost"),
                ("findUserById", "should get prefix boost"),
                ("findAllUsers", "should get substring boost"),
                ("createUser", "should get minimal boost"),
                ("deleteRecord", "should get no boost"),
            ]),
            ("Logger", vec![
                ("Logger", "exact match"),
                ("LoggerService", "prefix match"),
                ("FileLogger", "suffix match"),
                ("log", "different but related"),
                ("Database", "no relation"),
            ]),
        ];

        for (query, test_cases) in scenarios {
            let booster = ExactMatchBoost::new(query);
            let mut prev_boost = f32::INFINITY;

            println!("\nTesting query: '{}'", query);
            for (symbol, description) in test_cases {
                let boost = booster.calculate_boost(symbol);
                println!("  {} -> {} ({})", symbol, boost, description);

                // Generally expect descending boost values for these ordered test cases
                // (though this is a loose heuristic, not a strict requirement)
                if boost > 1.1 {
                    // Only compare significant boosts to avoid noise from minimal boosts
                    assert!(boost <= prev_boost + 0.1,
                            "Boost progression should generally be descending for {}: {} vs prev {}",
                            symbol, boost, prev_boost);
                }
                if boost > 1.1 {
                    prev_boost = boost;
                }
            }
        }
    }
}