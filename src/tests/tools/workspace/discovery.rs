// TDD Tests for Vendor Pattern Detection (.julieignore auto-generation)
//
// Tests the automatic vendor code detection logic that generates .julieignore
// on first workspace scan. Ensures patterns are detected correctly and formatted
// properly for the ignore matcher.

use crate::tools::shared::{BLACKLISTED_DIRECTORIES, BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES};
use crate::tools::workspace::ManageWorkspaceTool;
use std::collections::HashSet;
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
fn test_analyze_vendor_patterns_does_not_flag_libs_directory() {
    // libs/ is the standard source directory in Nx/Angular monorepos (apps/ + libs/).
    // It must NOT be treated as vendor — same reasoning as packages/ for npm/pnpm.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "libs/shared/src/index.ts",
        "libs/shared/src/lib.ts",
        "libs/ui/src/button.ts",
        "libs/ui/src/input.ts",
        "libs/ui/src/modal.ts",
        "libs/data-access/src/store.ts",
        "libs/data-access/src/effects.ts",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns.iter().any(|p| p == "libs" || p.starts_with("libs/")),
        "libs/ must NOT be flagged as vendor — it's the standard source dir in Nx/Angular monorepos. Got: {:?}",
        patterns,
    );
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0], "vendor");
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        1,
        "Should detect directory with >3 jquery files"
    );
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        1,
        "Should detect directory with >2 bootstrap files"
    );
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        0,
        "Should NOT detect with only 2 jquery files"
    );
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
        "dist/config.js",    // 12 total files, 11/12 = 91% (>50%)
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        1,
        "Should detect high minified concentration"
    );
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(
        patterns.len(),
        0,
        "Should NOT detect with <10 minified files"
    );
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
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
        // First vendor directory (vendor/)
        "Scripts/vendor/lib1.js",
        "Scripts/vendor/lib2.js",
        "Scripts/vendor/lib3.js",
        "Scripts/vendor/lib4.js",
        "Scripts/vendor/lib5.js",
        "Scripts/vendor/lib6.js",
        // Second vendor directory (plugin/)
        "Scripts/plugin/p1.js",
        "Scripts/plugin/p2.js",
        "Scripts/plugin/p3.js",
        "Scripts/plugin/p4.js",
        "Scripts/plugin/p5.js",
        "Scripts/plugin/p6.js",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 2, "Should detect 2 vendor directories");
    assert!(patterns.contains(&"Scripts/vendor".to_string()));
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

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 0, "Should NOT detect normal source code");
}

#[test]
fn test_analyze_vendor_patterns_does_not_flag_lib_directory() {
    // lib/ is a primary source directory in Elixir, Ruby, Dart, and Haskell.
    // It must NOT be flagged as vendor code.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "lib/my_app/router.ex",
        "lib/my_app/endpoint.ex",
        "lib/my_app/channel.ex",
        "lib/my_app/controller.ex",
        "lib/my_app/views/page.ex",
        "lib/my_app/views/layout.ex",
        "lib/my_app/views/error.ex",
        "lib/my_app/application.ex",
        "lib/my_app.ex",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns.iter().any(|p| p == "lib" || p.starts_with("lib/")),
        "lib/ must NOT be flagged as vendor — it's a source directory in Elixir/Ruby/Dart. Got: {:?}",
        patterns,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: generate_julieignore_file() - File Format
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_generate_julieignore_file_creates_file_with_correct_format() {
    let tool = create_tool();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let workspace_path = temp_dir.path();

    let patterns = vec!["Scripts/libs".to_string(), "Scripts/plugin".to_string()];

    tool.generate_julieignore_file(workspace_path, &patterns)
        .expect("Failed to generate .julieignore");

    let julieignore_path = workspace_path.join(".julieignore");
    assert!(julieignore_path.exists(), ".julieignore should be created");

    let content = std::fs::read_to_string(&julieignore_path).expect("Failed to read .julieignore");

    // Verify patterns end with "/" not "/**"
    assert!(
        content.contains("Scripts/libs/"),
        "Pattern should end with /"
    );
    assert!(
        content.contains("Scripts/plugin/"),
        "Pattern should end with /"
    );
    assert!(
        !content.contains("Scripts/libs/**"),
        "Pattern should NOT contain /**"
    );
    assert!(
        !content.contains("Scripts/plugin/**"),
        "Pattern should NOT contain /**"
    );

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

    assert_eq!(
        pattern, "src/vendor/libs/external",
        "Should handle nested paths"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Integration Test: Full Workflow
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_vendor_detection_full_workflow() {
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        // Vendor directory (vendor/)
        "Scripts/vendor/jquery.js",
        "Scripts/vendor/bootstrap.js",
        "Scripts/vendor/angular.js",
        "Scripts/vendor/lodash.js",
        "Scripts/vendor/moment.js",
        "Scripts/vendor/axios.js",
        // Custom code (should NOT be detected)
        "Scripts/PatientCase.js",
        "Scripts/Scheduling.js",
    ]);

    // Step 1: Analyze for patterns
    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert_eq!(patterns.len(), 1, "Should detect 1 vendor directory");
    assert_eq!(patterns[0], "Scripts/vendor");

    // Step 2: Generate .julieignore
    tool.generate_julieignore_file(temp_dir.path(), &patterns)
        .expect("Failed to generate .julieignore");

    // Step 3: Verify file created with correct format
    let julieignore_path = temp_dir.path().join(".julieignore");
    assert!(julieignore_path.exists());

    let content = std::fs::read_to_string(&julieignore_path).expect("Failed to read .julieignore");

    assert!(
        content.contains("Scripts/vendor/"),
        "Should have correct pattern format"
    );
    assert!(
        !content.contains("Scripts/vendor/**"),
        "Should NOT use /** format"
    );
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
    let _indexable_files = tool
        .discover_indexable_files(temp_dir.path())
        .expect("Failed to discover files");

    // Verify .julieignore was created
    let julieignore_path = temp_dir.path().join(".julieignore");
    assert!(
        julieignore_path.exists(),
        "Expected .julieignore to be created for blacklisted vendor directory target/"
    );

    // Verify it contains the target/ pattern
    let content = std::fs::read_to_string(&julieignore_path).expect("Failed to read .julieignore");

    assert!(
        content.contains("target/"),
        "Expected .julieignore to contain 'target/' pattern. Content:\n{}",
        content
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: Dotfile Discovery — .memories is excluded (non-code artifacts)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_memories_dir_excluded_from_discovery() {
    let tool = create_tool();
    let (temp_dir, _) = create_workspace_with_files(vec![
        "src/main.rs",
        "src/lib.rs",
        ".memories/checkpoint_abc123.md",
        ".memories/checkpoint_def456.md",
    ]);

    let indexable = tool
        .discover_indexable_files(temp_dir.path())
        .expect("Failed to discover files");

    // .memories files should not be discovered — they are non-code memory artifacts
    let memory_files: Vec<_> = indexable
        .iter()
        .filter(|p| p.to_string_lossy().contains(".memories"))
        .collect();

    assert!(
        memory_files.is_empty(),
        "Expected .memories files to be excluded from discovery"
    );

    // Verify actual source files ARE also discovered
    let rs_files: Vec<_> = indexable
        .iter()
        .filter(|p| p.extension().map_or(false, |e| e == "rs"))
        .collect();
    assert_eq!(rs_files.len(), 2, "Should discover both .rs source files");
}

#[test]
fn test_discover_indexable_files_respects_gitignore() {
    let tool = create_tool();
    let temp_dir = tempfile::TempDir::new().unwrap();
    let root = temp_dir.path();
    // Need .git dir for gitignore to be recognized
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join(".gitignore"), "generated/\n").unwrap();
    std::fs::create_dir_all(root.join("generated")).unwrap();
    std::fs::write(root.join("generated/api.rs"), "// generated").unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

    let files = tool.discover_indexable_files(root).unwrap();
    let paths: Vec<String> = files
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    assert!(
        paths.iter().any(|p| p.contains("main.rs")),
        "should include src/main.rs"
    );
    assert!(
        !paths.iter().any(|p| p.contains("generated")),
        "should exclude gitignored generated/"
    );
}

#[test]
fn test_claude_dir_in_blacklist() {
    assert!(
        BLACKLISTED_DIRECTORIES.contains(&".claude"),
        ".claude should be blacklisted"
    );
}

#[test]
fn test_packages_dir_not_in_blacklist() {
    // packages/ is the standard monorepo layout for npm/pnpm/Lerna/Nx/Turborepo.
    // It contains actual source code, not vendor/third-party dependencies.
    // Blacklisting it silently excludes ALL source files in JS/TS monorepos.
    assert!(
        !BLACKLISTED_DIRECTORIES.contains(&"packages"),
        "packages/ must NOT be blacklisted — it's the standard JS/TS monorepo source layout"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Tests: should_index_file — Unreadable file handling
// ═══════════════════════════════════════════════════════════════════════

/// Test: should_index_file returns Ok(false) for unreadable files instead of Err
///
/// Reproduces the bug where a Dropbox-locked file (os error 32 on Windows) caused
/// the entire indexing operation to fail.
#[test]
fn test_should_index_file_skips_unreadable_files() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;

    // A file path that doesn't exist — simulates an inaccessible/locked file
    // Use no extension so it hits both metadata check AND is_likely_text_file
    let nonexistent = PathBuf::from("/tmp/julie_test_nonexistent_file_no_extension");
    let result = tool.should_index_file(&nonexistent, &blacklisted_exts, max_file_size, false);

    // Should return Ok(false), NOT Err
    assert!(
        result.is_ok(),
        "should_index_file should not error on unreadable files: {:?}",
        result.err()
    );
    assert!(
        !result.unwrap(),
        "should_index_file should return false for unreadable files"
    );
}

#[test]
fn test_should_index_file_skips_lockfiles_by_name() {
    // Lockfiles like pnpm-lock.yaml and package-lock.json have non-blacklisted
    // extensions (.yaml, .json) but should be excluded by filename.
    // pnpm-lock.yaml alone produced 12,984 symbols in zod — 43% of total.
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;

    let temp_dir = TempDir::new().unwrap();

    for filename in &[
        "pnpm-lock.yaml",
        "package-lock.json",
        "composer.lock",
        "Pipfile.lock",
        "poetry.lock",
    ] {
        let path = temp_dir.path().join(filename);
        std::fs::write(&path, "content").unwrap();
        let result = tool
            .should_index_file(&path, &blacklisted_exts, max_file_size, false)
            .unwrap();
        assert!(
            !result,
            "{} should be excluded by filename blacklist",
            filename
        );
    }

    // Regular files with same extensions should still be indexed
    for filename in &["config.yaml", "package.json", "schema.json"] {
        let path = temp_dir.path().join(filename);
        std::fs::write(&path, "content").unwrap();
        let result = tool
            .should_index_file(&path, &blacklisted_exts, max_file_size, false)
            .unwrap();
        assert!(result, "{} should NOT be excluded", filename);
    }
}

#[test]
fn test_lockfiles_in_blacklisted_filenames() {
    assert!(
        BLACKLISTED_FILENAMES.contains(&"pnpm-lock.yaml"),
        "pnpm-lock.yaml should be in BLACKLISTED_FILENAMES"
    );
    assert!(
        BLACKLISTED_FILENAMES.contains(&"package-lock.json"),
        "package-lock.json should be in BLACKLISTED_FILENAMES"
    );
}

/// Test: Windows system directories are in BLACKLISTED_DIRECTORIES
#[test]
fn test_windows_system_dirs_in_blacklist() {
    assert!(
        BLACKLISTED_DIRECTORIES.contains(&"AppData"),
        "AppData should be in BLACKLISTED_DIRECTORIES"
    );
    assert!(
        BLACKLISTED_DIRECTORIES.contains(&"Application Data"),
        "Application Data should be in BLACKLISTED_DIRECTORIES"
    );
}

#[test]
fn test_analyze_vendor_patterns_does_not_flag_packages_directory() {
    // packages/ is the standard monorepo layout for npm/pnpm workspaces,
    // Lerna, Nx, and Turborepo projects. It contains actual source code,
    // not vendor/third-party code. Must NOT be flagged as vendor.
    let tool = create_tool();
    let (temp_dir, files) = create_workspace_with_files(vec![
        "packages/zod/src/v4/core/api.ts",
        "packages/zod/src/v4/core/checks.ts",
        "packages/zod/src/v4/core/parse.ts",
        "packages/zod/src/v4/core/schemas.ts",
        "packages/zod/src/v4/core/errors.ts",
        "packages/zod/src/v4/core/util.ts",
        "packages/zod/src/v4/core/core.ts",
        "packages/zod/src/v4/mini/parse.ts",
        "packages/docs/src/pages/index.tsx",
        "packages/docs/src/pages/docs.tsx",
    ]);

    let patterns = tool
        .analyze_vendor_patterns(&files, temp_dir.path())
        .expect("Failed to analyze patterns");

    assert!(
        !patterns
            .iter()
            .any(|p| p == "packages" || p.starts_with("packages/")),
        "packages/ must NOT be flagged as vendor — it's the standard JS/TS monorepo source layout. Got: {:?}",
        patterns,
    );
}
