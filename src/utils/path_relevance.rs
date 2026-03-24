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
        let in_test_directory = path_str.starts_with("test/")
            || path_str.starts_with("tests/")
            || path_str.starts_with("spec/")
            || path_str.contains("/test/")
            || path_str.contains("/tests/")
            || path_str.contains("/spec/")
            || path_str.contains("__tests__/");

        // Apply test file penalty for test files outside dedicated test directories
        if self.is_test_file(path) && !self.search_contains_test && !in_test_directory {
            score *= self.get_test_penalty();
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

        // Check for dedicated test directories BEFORE the generic src/lib check so that
        // language layouts like Java's src/test/java/ score as tests, not production.
        if path_str.contains("/test/")
            || path_str.contains("/tests/")
            || path_str.contains("/spec/")
            || path_str.contains("/specs/")
            || path_str.contains("/__tests__/")
            || path_str.starts_with("test/")
            || path_str.starts_with("tests/")
            || path_str.starts_with("spec/")
            || path_str.starts_with("specs/")
        {
            return 0.4; // Medium-low priority for tests
        }

        // Generic source directory heuristic — applies after test dirs are excluded
        if path_str.contains("src") || path_str.contains("lib") {
            return 1.0; // High priority for source code
        }

        // Remaining test-filename patterns (e.g. __tests__ without surrounding slashes)
        if path_str.contains("__tests__") {
            return 0.4;
        }

        // Default score for unrecognized directories
        0.7
    }

    /// Check if file is a test file
    pub fn is_test_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Check common test patterns
        path_str.contains("test")
            || path_str.contains("spec")
            || path_str.contains("__tests__")
            || file_name.ends_with("_test.rs")
            || file_name.ends_with(".test.js")
            || file_name.ends_with(".test.ts")
            || file_name.ends_with(".spec.js")
            || file_name.ends_with(".spec.ts")
            || file_name.starts_with("test_")
    }

    /// Check if file is production source code
    fn is_production_code(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        !self.is_test_file(path)
            && !path_str.contains("node_modules")
            && !path_str.contains("vendor")
            && !path_str.contains("docs")
            && !path_str.contains("documentation")
    }

    /// Get test file penalty factor
    /// VERIFIED from PathRelevanceFactor.cs:151
    fn get_test_penalty(&self) -> f32 {
        0.15 // 85% penalty for test files when not searching "test"
    }

    /// Get production code boost factor
    /// VERIFIED from PathRelevanceFactor.cs:179
    fn get_production_boost(&self) -> f32 {
        1.2 // 20% boost for production code
    }
}
