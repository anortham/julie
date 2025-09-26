// Path Relevance Factor for Search Quality Enhancement
//
// Port of codesearch PathRelevanceFactor.cs - verified scoring patterns
// Dramatically improves search result quality by boosting production code
//
// VERIFIED: From PathRelevanceFactor.cs:151,179 and directory weights lines 25-58

use std::path::Path;

/// Path relevance scorer for search result quality enhancement
/// Uses verified patterns from codesearch to prioritize production code
pub struct PathRelevanceScorer {
    /// Whether the search query contains "test" keyword
    search_contains_test: bool,
}

impl PathRelevanceScorer {
    /// Create new path relevance scorer
    pub fn new(search_query: &str) -> Self {
        Self {
            search_contains_test: search_query.to_lowercase().contains("test"),
        }
    }

    /// Calculate relevance score for a file path
    ///
    /// # Arguments
    /// * `file_path` - File path to score
    ///
    /// # Returns
    /// Relevance multiplier (higher = more relevant)
    pub fn calculate_score(&self, file_path: &str) -> f32 {
        let path = Path::new(file_path);
        let path_str = path.to_string_lossy().to_lowercase();

        // Base score from directory type
        let mut score = self.get_directory_score(path);

        // Check if file is in a dedicated test directory (not just a test file in a production directory)
        let in_test_directory = path_str.starts_with("test/") ||
                                path_str.starts_with("tests/") ||
                                path_str.starts_with("spec/") ||
                                path_str.contains("/test/") ||
                                path_str.contains("/tests/") ||
                                path_str.contains("/spec/") ||
                                path_str.contains("__tests__/");

        // Apply test file penalty only for test files in production directories
        if self.is_test_file(path) && !self.search_contains_test && !in_test_directory {
            // Use less severe penalty for test files in production directories
            let is_in_production_directory = path_str.contains("src") || path_str.contains("lib");
            let penalty = if is_in_production_directory {
                self.get_production_test_penalty()
            } else {
                self.get_test_penalty()
            };
            score *= penalty;
        }

        // Apply production boost for source code
        if self.is_production_code(path) {
            score *= self.get_production_boost();
        }

        score
    }

    /// Get directory-based score
    /// VERIFIED from PathRelevanceFactor.cs:25-58
    pub fn get_directory_score(&self, path: &Path) -> f32 {
        let path_str = path.to_string_lossy().to_lowercase();

        // Check directory patterns in order of specificity
        // IMPORTANT: Check specific directories BEFORE checking filename patterns
        if path_str.contains("node_modules") || path_str.contains("vendor") {
            return 0.1; // Lowest priority for dependencies
        }

        if path_str.contains("docs") || path_str.contains("documentation") {
            return 0.2; // Low priority for docs
        }

        // Check for production source directories FIRST (before test filename patterns)
        if path_str.contains("src") || path_str.contains("lib") {
            return 1.0; // High priority for source code (even test files in src get this)
        }

        // Only then check for dedicated test directories
        if path_str.contains("test") || path_str.contains("spec") || path_str.contains("__tests__") {
            return 0.4; // Medium-low priority for tests
        }

        // Default score for unrecognized directories
        0.7
    }

    /// Check if file is a test file
    pub fn is_test_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();
        let file_name = path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Check common test patterns
        path_str.contains("test") ||
        path_str.contains("spec") ||
        path_str.contains("__tests__") ||
        file_name.ends_with("_test.rs") ||
        file_name.ends_with(".test.js") ||
        file_name.ends_with(".test.ts") ||
        file_name.ends_with(".spec.js") ||
        file_name.ends_with(".spec.ts") ||
        file_name.starts_with("test_")
    }

    /// Check if file is production source code
    fn is_production_code(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        // Production indicators
        (path_str.contains("src") || path_str.contains("lib")) &&
        !self.is_test_file(path) &&
        !path_str.contains("node_modules") &&
        !path_str.contains("vendor")
    }

    /// Get test file penalty factor
    /// VERIFIED from PathRelevanceFactor.cs:151
    fn get_test_penalty(&self) -> f32 {
        0.15 // 85% penalty for test files when not searching "test"
    }

    /// Get test file penalty for production directories (less severe)
    fn get_production_test_penalty(&self) -> f32 {
        0.5 // 50% penalty for test files in production directories
    }

    /// Get production code boost factor
    /// VERIFIED from PathRelevanceFactor.cs:179
    fn get_production_boost(&self) -> f32 {
        1.2 // 20% boost for production code
    }
}

#[cfg(test)]
mod tests {
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
        println!("Test search - Test score: {}, Production score: {}, Ratio: {}",
                 test_score, production_score, test_score / production_score);

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
        println!("Combined - Src prod: {}, Test file: {}, Deps: {}",
                 src_prod, test_file, deps);

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
}