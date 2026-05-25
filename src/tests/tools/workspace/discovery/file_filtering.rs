//! File filtering and blacklist coverage.

use super::*;

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
fn test_should_index_file_accepts_cargo_lock() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("Cargo.lock");

    std::fs::write(&path, "version = 4\n").unwrap();

    let result = tool
        .should_index_file(&path, &blacklisted_exts, max_file_size, false)
        .unwrap();

    assert!(
        result,
        "Cargo.lock should be indexed as TOML; it is the Rust dependency manifest lockfile"
    );
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
fn test_doc_config_files_in_blacklisted_filenames() {
    assert!(BLACKLISTED_FILENAMES.contains(&"mkdocs.yml"));
    assert!(BLACKLISTED_FILENAMES.contains(&"mkdocs.yaml"));
    assert!(BLACKLISTED_FILENAMES.contains(&".readthedocs.yml"));
    assert!(BLACKLISTED_FILENAMES.contains(&".readthedocs.yaml"));
    assert!(BLACKLISTED_FILENAMES.contains(&"book.toml"));
    assert!(BLACKLISTED_FILENAMES.contains(&"_config.yml"));
}

#[test]
fn test_should_index_file_rejects_blacklisted_filenames() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;
    let temp_dir = tempfile::TempDir::new().unwrap();

    for filename in &["mkdocs.yml", "book.toml", "_config.yml"] {
        let path = temp_dir.path().join(filename);
        std::fs::write(&path, "key: value\n").unwrap();
        let result = tool.should_index_file(&path, &blacklisted_exts, max_file_size, false);
        assert!(result.is_ok());
        assert!(!result.unwrap(), "{} should be rejected", filename);
    }
}

#[test]
fn test_should_index_file_accepts_normal_yaml_toml() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;
    let temp_dir = tempfile::TempDir::new().unwrap();

    for filename in &["config.yml", "Cargo.toml"] {
        let path = temp_dir.path().join(filename);
        std::fs::write(&path, "key: value\n").unwrap();
        let result = tool.should_index_file(&path, &blacklisted_exts, max_file_size, false);
        assert!(result.is_ok());
        assert!(result.unwrap(), "{} should be accepted", filename);
    }
}

#[test]
fn test_should_index_file_accepts_unknown_text_extension() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("flake.nix");
    std::fs::write(&path, "{ description = \"text source\"; }\n").unwrap();

    let result = tool.should_index_file(&path, &blacklisted_exts, max_file_size, false);

    assert!(result.is_ok());
    assert!(
        result.unwrap(),
        "unknown text extensions should remain indexable as text-only files"
    );
}

#[test]
fn test_should_index_file_rejects_unknown_binary_extension() {
    let tool = create_tool();
    let blacklisted_exts: HashSet<&str> = BLACKLISTED_EXTENSIONS.iter().copied().collect();
    let max_file_size = 1024 * 1024;
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("payload.custom");
    std::fs::write(&path, [0, 159, 146, 150, 0, 1, 2, 3]).unwrap();

    let result = tool.should_index_file(&path, &blacklisted_exts, max_file_size, false);

    assert!(result.is_ok());
    assert!(
        !result.unwrap(),
        "unknown binary extensions should not be indexed just because they have an extension"
    );
}
