use super::*;

/// Test 6: scan_workspace_files respects .julieignore patterns
/// Given: Workspace with .julieignore file containing ignore patterns
/// When: scan_workspace_files() is called
/// Expected: Files matching .julieignore patterns are excluded
///
/// Bug: Discovery respects .julieignore, but scan_workspace_files does not
/// This causes false "needs indexing" warnings for ignored files
#[test]
fn test_scan_workspace_files_respects_julieignore() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create some test files
    let normal_file = workspace_path.join("normal.rs");
    let ignored_file = workspace_path.join("ignored.rs");
    let generated_dir = workspace_path.join("generated");
    fs::create_dir(&generated_dir)?;
    let generated_file = generated_dir.join("schema.rs");

    fs::write(&normal_file, "fn normal() {}")?;
    fs::write(&ignored_file, "fn ignored() {}")?;
    fs::write(&generated_file, "fn generated() {}")?;

    // Create .julieignore file
    let julieignore_path = workspace_path.join(".julieignore");
    fs::write(
        &julieignore_path,
        "# Ignore specific files and directories\nignored.rs\ngenerated/\n",
    )?;

    // Call scan_workspace_files
    let files = julie_core::workspace_scan::scan_workspace_files(workspace_path)?;

    // Verify: normal.rs is included
    assert!(files.contains("normal.rs"), "Should find normal.rs");

    // Verify: ignored.rs is excluded (respects .julieignore)
    assert!(
        !files.contains("ignored.rs"),
        "Should NOT find ignored.rs (in .julieignore)"
    );

    // Verify: generated/schema.rs is excluded (respects .julieignore directory pattern)
    assert!(
        !files.contains("generated/schema.rs"),
        "Should NOT find generated/schema.rs (directory in .julieignore)"
    );

    Ok(())
}

/// Test: scan_workspace_files respects .gitignore
/// Given: Workspace with .git dir and .gitignore
/// When: scan_workspace_files() is called
/// Expected: Files matching .gitignore are excluded
#[test]
fn test_scan_workspace_files_respects_gitignore() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();
    fs::create_dir_all(root.join(".git"))?;
    fs::write(root.join(".gitignore"), "generated/\n")?;
    fs::create_dir_all(root.join("generated"))?;
    fs::write(root.join("generated/api.rs"), "// auto-generated")?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/main.rs"), "fn main() {}")?;

    let files = julie_core::workspace_scan::scan_workspace_files(root)?;
    assert!(files.contains("src/main.rs"), "should include src/main.rs");
    assert!(
        !files.iter().any(|f| f.contains("generated")),
        "should exclude gitignored dir"
    );

    Ok(())
}

#[test]
fn test_scan_workspace_files_includes_unknown_text_extensions() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let root = temp_dir.path();
    fs::write(
        root.join("flake.nix"),
        "{ description = \"text source\"; }\n",
    )?;
    fs::write(
        root.join("diagram.svg"),
        "<svg><text>ignored</text></svg>\n",
    )?;

    let files = julie_core::workspace_scan::scan_workspace_files(root)?;

    assert!(
        files.contains("flake.nix"),
        "unknown text extensions should participate in startup and repair scans"
    );
    assert!(
        !files.contains("diagram.svg"),
        "blacklisted text extensions should stay excluded from scans"
    );

    Ok(())
}

/// Test 7: scan_workspace_files returns Unix-style paths (Windows bug fix)
/// Given: Files in nested directories
/// When: scan_workspace_files() is called
/// Expected: All paths use forward slashes (/), not backslashes (\)
///
/// Bug: On Windows, strip_prefix() returns paths with backslashes (src\file.rs)
/// But database stores paths with forward slashes (src/file.rs)
/// This causes staleness detection to fail because paths don't match
#[test]
fn test_scan_workspace_files_returns_unix_style_paths() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    // Create nested directory structure with files
    let src_dir = workspace_path.join("src");
    let tools_dir = src_dir.join("tools");
    fs::create_dir_all(&tools_dir)?;

    // Create files at different nesting levels
    let root_file = workspace_path.join("main.rs");
    let src_file = src_dir.join("lib.rs");
    let nested_file = tools_dir.join("search.rs");

    fs::write(&root_file, "fn main() {}")?;
    fs::write(&src_file, "pub mod tools;")?;
    fs::write(&nested_file, "pub fn search() {}")?;

    // Call scan_workspace_files
    let files = julie_core::workspace_scan::scan_workspace_files(workspace_path)?;

    // Verify: ALL paths use Unix-style forward slashes
    for file_path in &files {
        // Path should NOT contain backslashes (Windows separators)
        assert!(
            !file_path.contains('\\'),
            "Path '{}' contains backslash separator (should be Unix-style with /)",
            file_path
        );

        // Nested paths should use forward slashes
        if file_path.contains('/') {
            // Verify it's a properly formed Unix-style path
            let parts: Vec<&str> = file_path.split('/').collect();
            assert!(
                parts.len() >= 2,
                "Nested path '{}' should have multiple components separated by /",
                file_path
            );
        }
    }

    // Verify expected files are present (with Unix-style paths)
    assert!(files.contains("main.rs"), "Should find root file");
    assert!(
        files.contains("src/lib.rs"),
        "Should find src file with / separator"
    );
    assert!(
        files.contains("src/tools/search.rs"),
        "Should find nested file with / separators"
    );

    // Verify we found exactly 3 files
    assert_eq!(files.len(), 3, "Should find exactly 3 files");

    Ok(())
}
