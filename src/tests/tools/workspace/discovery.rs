// TDD Tests for Vendor Pattern Detection (.julieignore auto-generation)
//
// Tests the automatic vendor code detection logic that generates .julieignore
// on first workspace scan. Ensures patterns are detected correctly and formatted
// properly for the ignore matcher.

use crate::tools::workspace::ManageWorkspaceTool;
use std::path::PathBuf;
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════════════════
// Test Helper: Create ManageWorkspaceTool
// ═══════════════════════════════════════════════════════════════════════

fn create_tool() -> ManageWorkspaceTool {
    ManageWorkspaceTool {
        operation: "test".to_string(), // Dummy operation for testing
        path: None,
        force: None,
        name: None,
        workspace_id: None,
        detailed: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Test Helper: Create Temp Workspace with Files
// ═══════════════════════════════════════════════════════════════════════

fn create_workspace_with_files(files: Vec<&str>) -> (TempDir, Vec<PathBuf>) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_root = temp_dir.path();

    let mut file_paths = Vec::new();

    for file_path in files {
        let full_path = workspace_root.join(file_path);

        // Create parent directories
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }

        // Create empty file
        std::fs::write(&full_path, "").expect("Failed to create file");

        file_paths.push(full_path);
    }

    (temp_dir, file_paths)
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: analyze_vendor_patterns() - High Confidence Directory Names
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_analyze_vendor_patterns_detects_libs_directory() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/libs/file1.js",
        "Scripts/libs/file2.js",
        "Scripts/libs/file3.js",
        "Scripts/libs/file4.js",
        "Scripts/libs/file5.js",
        "Scripts/libs/file6.js", // >5 files triggers detection
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1, "Should detect 1 vendor directory");
    assert_eq!(patterns[0], "Scripts/libs", "Should detect Scripts/libs");
}

#[test]
fn test_analyze_vendor_patterns_detects_plugin_directory() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/plugin/plugin1.js",
        "Scripts/plugin/plugin2.js",
        "Scripts/plugin/plugin3.js",
        "Scripts/plugin/plugin4.js",
        "Scripts/plugin/plugin5.js",
        "Scripts/plugin/plugin6.js", // >5 files
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0], "Scripts/plugin");
}

#[test]
fn test_analyze_vendor_patterns_detects_vendor_directory() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "vendor/lib1.js",
        "vendor/lib2.js",
        "vendor/lib3.js",
        "vendor/lib4.js",
        "vendor/lib5.js",
        "vendor/lib6.js", // >5 files
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0], "vendor");
}

#[test]
fn test_analyze_vendor_patterns_ignores_small_libs_directory() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/libs/file1.js",
        "Scripts/libs/file2.js",
        "Scripts/libs/file3.js", // Only 3 files, needs >5
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect libs/ with <5 files");
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: analyze_vendor_patterns() - Medium Confidence jQuery/Bootstrap
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_analyze_vendor_patterns_detects_jquery_files() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/jquery-1.12.4.js",
        "Scripts/jquery-ui.js",
        "Scripts/jquery.validate.js",
        "Scripts/jquery.unobtrusive-ajax.js", // >3 jquery files triggers detection
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1, "Should detect directory with >3 jquery files");
    assert_eq!(patterns[0], "Scripts");
}

#[test]
fn test_analyze_vendor_patterns_detects_bootstrap_files() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Styles/bootstrap.css",
        "Styles/bootstrap-theme.css",
        "Styles/bootstrap.min.css", // >2 bootstrap files triggers detection
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1, "Should detect directory with >2 bootstrap files");
    assert_eq!(patterns[0], "Styles");
}

#[test]
fn test_analyze_vendor_patterns_ignores_few_jquery_files() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "Scripts/jquery.js",
        "Scripts/jquery-ui.js", // Only 2 jquery files, needs >3
        "Scripts/custom.js",
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect with only 2 jquery files");
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: analyze_vendor_patterns() - Medium Confidence Minified Concentration
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_analyze_vendor_patterns_detects_minified_concentration() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "dist/app.min.js",
        "dist/vendor.min.js",
        "dist/styles.min.css",
        "dist/bootstrap.min.css",
        "dist/jquery.min.js",
        "dist/angular.min.js",
        "dist/lodash.min.js",
        "dist/moment.min.js",
        "dist/react.min.js",
        "dist/vue.min.js",
        "dist/axios.min.js", // 11 minified files (>10)
        "dist/config.js",     // 12 total files, 11/12 = 91% (>50%)
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1, "Should detect high minified concentration");
    assert_eq!(patterns[0], "dist");
}

#[test]
fn test_analyze_vendor_patterns_ignores_low_minified_concentration() {
    let tool = create_tool();
    // Use "compiled" instead of "build" since "build" is now a recognized vendor directory
    let (temp_dir, files) = create_workspace_with_files(vec![
        "compiled/app.min.js",
        "compiled/vendor.min.js",
        "compiled/styles.min.css", // 3 minified files (needs >10)
        "compiled/source1.js",
        "compiled/source2.js",
        "compiled/source3.js",
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect with <10 minified files");
}

#[test]
fn test_analyze_vendor_patterns_ignores_minified_below_50_percent() {
    let tool = create_tool();
    // Use "compiled" instead of "build" since "build" is now a recognized vendor directory
    let (temp_dir, files) = create_workspace_with_files(vec![
        "compiled/app.min.js",
        "compiled/vendor.min.js",
        "compiled/styles.min.css",
        "compiled/bootstrap.min.css",
        "compiled/jquery.min.js",
        "compiled/angular.min.js",
        "compiled/lodash.min.js",
        "compiled/moment.min.js",
        "compiled/react.min.js",
        "compiled/vue.min.js",
        "compiled/axios.min.js", // 11 minified files (>10) ✓
        // But add 11+ non-minified files to drop below 50%
        "compiled/source1.js",
        "compiled/source2.js",
        "compiled/source3.js",
        "compiled/source4.js",
        "compiled/source5.js",
        "compiled/source6.js",
        "compiled/source7.js",
        "compiled/source8.js",
        "compiled/source9.js",
        "compiled/source10.js",
        "compiled/source11.js",
        "compiled/source12.js", // 23 total, 11/23 = 47% (<50%)
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect when minified <50%");
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: analyze_vendor_patterns() - Multiple Patterns
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_analyze_vendor_patterns_detects_multiple_directories() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        // First vendor directory (libs/)
        "Scripts/libs/lib1.js",
        "Scripts/libs/lib2.js",
        "Scripts/libs/lib3.js",
        "Scripts/libs/lib4.js",
        "Scripts/libs/lib5.js",
        "Scripts/libs/lib6.js",
        // Second vendor directory (plugin/)
        "Scripts/plugin/p1.js",
        "Scripts/plugin/p2.js",
        "Scripts/plugin/p3.js",
        "Scripts/plugin/p4.js",
        "Scripts/plugin/p5.js",
        "Scripts/plugin/p6.js",
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 2, "Should detect 2 vendor directories");
    assert!(patterns.contains(&"Scripts/libs".to_string()));
    assert!(patterns.contains(&"Scripts/plugin".to_string()));
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: analyze_vendor_patterns() - No False Positives
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_analyze_vendor_patterns_no_false_positives_for_normal_code() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "src/components/UserService.ts",
        "src/components/AuthService.ts",
        "src/components/PaymentService.ts",
        "src/utils/helpers.ts",
        "src/utils/validators.ts",
    ]);

    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect normal source code");
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: generate_julieignore_file() - File Format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_generate_julieignore_file_creates_file_with_correct_format() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    let patterns = vec![
        "Scripts/libs".to_string(),
        "Scripts/plugin".to_string(),
    ];

    tool.generate_julieignore_file(workspace_path, &patterns)
        .expect("Failed to generate .julieignore");

    let julieignore_path = workspace_path.join(".julieignore");
    assert!(julieignore_path.exists(), ".julieignore should be created");

    let content = std::fs::read_to_string(&julieignore_path)
        .expect("Failed to read .julieignore");

    // Verify patterns end with "/" not "/**"
    assert!(content.contains("Scripts/libs/"), "Pattern should end with /");
    assert!(content.contains("Scripts/plugin/"), "Pattern should end with /");
    assert!(!content.contains("Scripts/libs/**"), "Pattern should NOT contain /**");
    assert!(!content.contains("Scripts/plugin/**"), "Pattern should NOT contain /**");

    // Verify header documentation exists
    assert!(content.contains("# .julieignore - Julie Code Intelligence Exclusion Patterns"));
    assert!(content.contains("Auto-generated by Julie on"));

    // Verify usage instructions exist
    assert!(content.contains("What Julie Did Automatically"));
    assert!(content.contains("Why Exclude Vendor Code?"));
    assert!(content.contains("How to Modify This File"));
}

#[test]
fn test_generate_julieignore_file_includes_date() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    let patterns = vec!["vendor".to_string()];

    tool.generate_julieignore_file(workspace_path, &patterns)
        .expect("Failed to generate .julieignore");

    let content = std::fs::read_to_string(workspace_path.join(".julieignore"))
        .expect("Failed to read .julieignore");

    // Should contain current date in YYYY-MM-DD format
    let date_regex = regex::Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();
    assert!(date_regex.is_match(&content), "Should include current date");
}

#[test]
fn test_generate_julieignore_file_empty_patterns() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    let patterns: Vec<String> = vec![];

    tool.generate_julieignore_file(workspace_path, &patterns)
        .expect("Failed to generate .julieignore");

    let content = std::fs::read_to_string(workspace_path.join(".julieignore"))
        .expect("Failed to read .julieignore");

    // Should still create file with documentation, just no patterns
    assert!(content.contains("# .julieignore"));
    assert!(content.contains("Auto-Detected Vendor Directories"));
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: dir_to_pattern() - Path Normalization
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_dir_to_pattern_strips_workspace_prefix() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_root = temp_dir.path();

    let dir = workspace_root.join("Scripts/libs");

    let pattern = tool.dir_to_pattern(&dir, workspace_root);

    assert_eq!(pattern, "Scripts/libs", "Should strip workspace prefix");
}

#[test]
fn test_dir_to_pattern_normalizes_backslashes() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_root = temp_dir.path();

    // Simulate Windows path with backslashes
    let dir = workspace_root.join("Scripts").join("libs");

    let pattern = tool.dir_to_pattern(&dir, workspace_root);

    // Should use forward slashes regardless of platform
    assert!(pattern.contains("/"), "Should use forward slashes");
    assert!(!pattern.contains("\\"), "Should NOT contain backslashes");
}

#[test]
fn test_dir_to_pattern_handles_nested_directories() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_root = temp_dir.path();

    let dir = workspace_root.join("src/vendor/libs/external");

    let pattern = tool.dir_to_pattern(&dir, workspace_root);

    assert_eq!(pattern, "src/vendor/libs/external", "Should handle nested paths");
}

// ═══════════════════════════════════════════════════════════════════════
// Integration Test: Full Workflow
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_vendor_detection_full_workflow() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        // Vendor directory (libs/)
        "Scripts/libs/jquery.js",
        "Scripts/libs/bootstrap.js",
        "Scripts/libs/angular.js",
        "Scripts/libs/lodash.js",
        "Scripts/libs/moment.js",
        "Scripts/libs/axios.js",
        // Custom code (should NOT be detected)
        "Scripts/PatientCase.js",
        "Scripts/Scheduling.js",
    ]);

    // Step 1: Analyze for patterns
    let patterns = tool.analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1, "Should detect 1 vendor directory");
    assert_eq!(patterns[0], "Scripts/libs");

    // Step 2: Generate .julieignore
    tool.generate_julieignore_file(temp_dir.path(), &patterns)
        .expect("Failed to generate .julieignore");

    // Step 3: Verify file created with correct format
    let julieignore_path = temp_dir.path().join(".julieignore");
    assert!(julieignore_path.exists());

    let content = std::fs::read_to_string(&julieignore_path)
        .expect("Failed to read .julieignore");

    assert!(content.contains("Scripts/libs/"), "Should have correct pattern format");
    assert!(!content.contains("Scripts/libs/**"), "Should NOT use /** format");
}

// ═══════════════════════════════════════════════════════════════════════
// BUG REPRODUCTION: Blacklisted vendor directories not detected
// ═══════════════════════════════════════════════════════════════════════

/// Test: discover_indexable_files should create .julieignore for blacklisted vendor dirs
///
/// BUG: When a workspace has directories that are BOTH in BLACKLISTED_DIRECTORIES
/// (like target/, vendor/, node_modules/) AND contain many files (vendor pattern),
/// the .julieignore file should still be created with those patterns.
///
/// CURRENT BEHAVIOR: discover_indexable_files filters out blacklisted dirs BEFORE
/// analyzing for vendor patterns, so these directories are never detected and
/// .julieignore is never created.
///
/// EXPECTED BEHAVIOR: Even though these dirs are in the hardcoded blacklist,
/// we should still detect them as vendor patterns and create .julieignore so
/// users can see what's being excluded and modify it if needed.
#[test]
fn test_discover_indexable_files_creates_julieignore_for_blacklisted_vendor_dirs() {
    let tool = create_tool();

    // Create workspace with target/ directory (blacklisted AND vendor pattern)
    let (temp_dir, _files) = create_workspace_with_files(vec![
        "src/main.rs",
        "src/lib.rs",
        "target/debug/deps/file1.rlib",
        "target/debug/deps/file2.rlib",
        "target/debug/deps/file3.rlib",
        "target/debug/deps/file4.rlib",
        "target/debug/deps/file5.rlib",
        "target/debug/deps/file6.rlib", // >5 files should trigger vendor detection
        "target/debug/deps/file7.rlib",
        "target/debug/deps/file8.rlib",
    ]);

    // Call discover_indexable_files (simulates first workspace scan)
    let _indexable_files = tool.discover_indexable_files(temp_dir.path())
        .expect("Failed to discover files");

    // Verify .julieignore was created
    let julieignore_path = temp_dir.path().join(".julieignore");
    assert!(
        julieignore_path.exists(),
        "Expected .julieignore to be created for blacklisted vendor directory target/"
    );

    // Verify it contains the target/ pattern
    let content = std::fs::read_to_string(&julieignore_path)
        .expect("Failed to read .julieignore");

    assert!(
        content.contains("target/"),
        "Expected .julieignore to contain 'target/' pattern. Content:\n{}",
        content
    );
}
