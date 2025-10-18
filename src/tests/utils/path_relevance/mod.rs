// Path Relevance Scoring Tests
//
// Extracted from: src/utils/path_relevance.rs
// This module contains all inline tests from the path relevance module,
// ensuring code relevance scoring works correctly for search quality enhancement.

use crate::utils::path_relevance::PathRelevanceScorer;
use std::path::Path;

#[test]
fn test_directory_scoring_verified_weights() {
    let scorer = PathRelevanceScorer::new("getUserData");

    // VERIFIED weights from PathRelevanceFactor.cs:25-58
    assert_eq!(scorer.get_directory_score(Path::new("src/main.rs")), 1.0);
    assert_eq!(scorer.get_directory_score(Path::new("lib/utils.js")), 1.0);
    assert_eq!(scorer.get_directory_score(Path::new("tests/unit.rs")), 0.4);
    assert_eq!(scorer.get_directory_score(Path::new("docs/readme.md")), 0.2);
    assert_eq!(
        scorer.get_directory_score(Path::new("node_modules/react/index.js")),
        0.1
    );
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
    let scorer = PathRelevanceScorer::new("getUserData"); // Not searching "test"

    // Test files should get penalty
    let test_score = scorer.calculate_score("src/user_test.rs");
    let production_score = scorer.calculate_score("src/user.rs");

    // Test file should have much lower score due to penalty
    assert!(test_score < production_score);
    assert!((test_score / production_score - 0.42).abs() < 0.1); // ~42% of production score (0.5 penalty for test files in production dirs)
}

#[test]
fn test_no_penalty_when_searching_test() {
    let scorer = PathRelevanceScorer::new("test getUserData"); // Searching "test"

    // Test files should NOT get penalty when searching for tests
    let test_score = scorer.calculate_score("src/user_test.rs");
    let production_score = scorer.calculate_score("src/user.rs");

    // Debug: Print actual scores
    println!(
        "Test search - Test score: {}, Production score: {}, Ratio: {}",
        test_score,
        production_score,
        test_score / production_score
    );

    // Both should have similar base scores (no test penalty applied)
    assert!((test_score / production_score - 1.0).abs() < 0.3); // Should be close
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

    // Low score: test file + penalty
    let test_file = scorer.calculate_score("tests/user_test.rs");

    // Very low score: node_modules
    let deps = scorer.calculate_score("node_modules/react/index.js");

    // Debug: Print actual scores
    println!(
        "Combined - Src prod: {}, Test file: {}, Deps: {}",
        src_prod, test_file, deps
    );

    // Verify relative ordering
    assert!(src_prod > test_file);
    assert!(test_file > deps);
    assert!(src_prod > 1.0); // Should have boost
    assert!(deps < 0.2); // Should be very low
}

#[test]
fn test_search_context_awareness() {
    let test_searcher = PathRelevanceScorer::new("test user login");
    let prod_searcher = PathRelevanceScorer::new("user login");

    // Use a test file in production directory where context matters
    let test_file_path = "src/user_test.rs";

    // When searching for "test", test files in production dirs should not be penalized
    let test_score_with_test_query = test_searcher.calculate_score(test_file_path);
    let test_score_with_prod_query = prod_searcher.calculate_score(test_file_path);

    assert!(test_score_with_test_query > test_score_with_prod_query);
}
