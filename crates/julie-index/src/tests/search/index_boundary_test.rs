use std::{fs, path::PathBuf};

#[test]
fn search_index_implementation_files_stay_within_limit() {
    for relative_path in [
        "src/search/index.rs",
        "src/search/index/types.rs",
        "src/search/index/compatibility.rs",
        "src/search/index/lifecycle.rs",
        "src/search/index/mutation.rs",
        "src/search/index/query/mod.rs",
        "src/search/index/query/execution.rs",
        "src/search/index/query/terms.rs",
        "src/search/index/query/files.rs",
    ] {
        assert_line_limit(relative_path, 500);
    }
}

fn assert_line_limit(relative_path: &str, limit: usize) {
    let contents = fs::read_to_string(crate_file(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"));
    let line_count = contents.lines().count();

    assert!(
        line_count <= limit,
        "{relative_path} has {line_count} lines; limit is {limit}"
    );
}

fn crate_file(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative_path)
}
