// Path Relevance Scoring Tests
//
// Comprehensive tests for PathRelevanceScorer following TDD methodology
// Tests verified scoring patterns from codesearch PathRelevanceFactor.cs

use crate::utils::path_relevance::PathRelevanceScorer;
use std::path::Path;

#[cfg(test)]
mod path_relevance_tests {
    use super::*;

    #[test]
    fn test_directory_scoring_verified_weights() {
        let scorer = PathRelevanceScorer::new("getUserData");

        // VERIFIED weights from PathRelevanceFactor.cs:25-58
        assert_eq!(scorer.get_directory_score(Path::new("src/main.rs")), 1.0);
        assert_eq!(scorer.get_directory_score(Path::new("lib/utils.js")), 1.0);
        assert_eq!(scorer.get_directory_score(Path::new("tests/unit.rs")), 0.4);
        assert_eq!(scorer.get_directory_score(Path::new("docs/readme.md")), 0.2);
        assert_eq!(scorer.get_directory_score(Path::new("node_modules/react/index.js")), 0.1);
    }

    #[test]
    fn test_test_file_detection() {
        let scorer = PathRelevanceScorer::new("getUserData");

        // Should detect test files
        assert!(scorer.is_test_file(Path::new("src/user_test.rs")));
        assert!(scorer.is_test_file(Path::new("src/user.test.js")));
        assert!(scorer.is_test_file(Path::new("tests/user.rs")));
        assert!(scorer.is_test_file(Path::new("__tests__/user.js")));
        assert!(scorer.is_test_file(Path::new("test_user.py")));

        // Should not detect non-test files
        assert!(!scorer.is_test_file(Path::new("src/user.rs")));
        assert!(!scorer.is_test_file(Path::new("src/user_data.js")));
    }

    #[test]
    fn test_penalty_when_not_searching_test() {
        let scorer = PathRelevanceScorer::new("getUserData");

        // Compare test file in src/ vs production file in src/ (same directory)
        let test_score = scorer.calculate_score("src/user_test.rs");
        let production_score = scorer.calculate_score("src/user.rs");

        println!("Test score: {}, Production score: {}, Ratio: {}",
                 test_score, production_score, test_score / production_score);

        // Test file should have much lower score due to penalty
        assert!(test_score < production_score);
        // Should be ~42% of production score due to 0.5 penalty for test files in production dirs
        assert!((test_score / production_score - 0.42).abs() < 0.05);
    }

    #[test]
    fn test_no_penalty_when_searching_test() {
        let scorer = PathRelevanceScorer::new("test getUserData");

        // Compare test file vs production file when searching for "test"
        let test_score = scorer.calculate_score("src/user_test.rs");
        let production_score = scorer.calculate_score("src/user.rs");

        println!("Test search - Test score: {}, Production score: {}, Ratio: {}",
                 test_score, production_score, test_score / production_score);

        // Test files should not get penalty when searching for "test"
        // Test files won't get production boost, so ratio should be ~0.83 (1.0/1.2)
        assert!((test_score / production_score - 0.83).abs() < 0.1);
    }

    #[test]
    fn test_production_boost() {
        let scorer = PathRelevanceScorer::new("getUserData");

        let production_score = scorer.calculate_score("src/user.rs");
        let base_score = scorer.get_directory_score(Path::new("src/user.rs"));

        // Production code should get 1.2x boost
        assert!((production_score / base_score - 1.2).abs() < 0.1);
    }

    #[test]
    fn test_combined_scoring_scenarios() {
        let scorer = PathRelevanceScorer::new("getUserData");

        // High score: src directory + production boost
        let src_prod = scorer.calculate_score("src/user.rs");

        // Medium score: test file in src (gets penalty but still in src)
        let test_in_src = scorer.calculate_score("src/user_test.rs");

        // Lower score: test file in tests directory
        let test_in_tests = scorer.calculate_score("tests/user_test.rs");

        // Very low score: node_modules
        let deps = scorer.calculate_score("node_modules/react/index.js");

        println!("Scores - Src prod: {}, Test in src: {}, Test in tests: {}, Deps: {}",
                 src_prod, test_in_src, test_in_tests, deps);

        // Verify relative ordering
        assert!(src_prod > test_in_src);
        assert!(test_in_src > test_in_tests);
        assert!(test_in_tests > deps);
        assert!(src_prod > 1.0); // Should have boost
        assert!(deps < 0.2); // Should be very low
    }

    #[test]
    fn test_search_context_awareness() {
        let test_searcher = PathRelevanceScorer::new("test user login");
        let prod_searcher = PathRelevanceScorer::new("user login");

        let test_file_path = "src/user_test.rs";

        // When searching for "test", test files should not be penalized
        let test_score_with_test_query = test_searcher.calculate_score(test_file_path);
        let test_score_with_prod_query = prod_searcher.calculate_score(test_file_path);

        assert!(test_score_with_test_query > test_score_with_prod_query);
    }

    #[test]
    fn test_realistic_codesearch_scenarios() {
        // Test scenarios that would help put desired results in top 3
        let scorer = PathRelevanceScorer::new("getUserData");

        // Scenario 1: Looking for user data function
        let main_impl = scorer.calculate_score("src/user/service.rs");
        let test_impl = scorer.calculate_score("tests/user/service_test.rs");
        let docs = scorer.calculate_score("docs/api/user.md");
        let deps = scorer.calculate_score("node_modules/@types/user.d.ts");

        // Main implementation should score highest
        assert!(main_impl > test_impl);
        assert!(main_impl > docs);
        assert!(main_impl > deps);

        // Order should be: main_impl > test_impl > docs > deps
        assert!(test_impl > docs);
        assert!(docs > deps);
    }
}