use std::{fs, path::PathBuf};

#[test]
fn runner_implementation_files_stay_within_limit() {
    for relative_path in [
        "xtask/src/runner.rs",
        "xtask/src/runner/prebuild.rs",
        "xtask/src/runner/execution.rs",
        "xtask/src/runner/rendering.rs",
    ] {
        assert_line_limit(relative_path, 500);
    }

    assert_line_limit("xtask/src/runner/tests.rs", 1000);
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
