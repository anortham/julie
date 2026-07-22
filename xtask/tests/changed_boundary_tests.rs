use std::{fs, path::PathBuf};

#[test]
fn changed_implementation_files_stay_within_limit() {
    for relative_path in [
        "xtask/src/changed.rs",
        "xtask/src/changed/diff.rs",
        "xtask/src/changed/policy.rs",
        "xtask/src/changed/rendering.rs",
        "xtask/src/changed/mapping.rs",
        "xtask/src/changed/mapping/front.rs",
        "xtask/src/changed/mapping/crates.rs",
        "xtask/src/changed/mapping/product.rs",
    ] {
        assert_line_limit(relative_path, 500);
    }

    assert_line_limit("xtask/src/changed/tests.rs", 1000);
}

fn assert_line_limit(relative_path: &str, limit: usize) {
    let contents = fs::read_to_string(repo_file(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"));
    let line_count = contents.lines().count();

    assert!(
        line_count <= limit,
        "{relative_path} has {line_count} lines; limit is {limit}"
    );
}

fn repo_file(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(relative_path)
}
