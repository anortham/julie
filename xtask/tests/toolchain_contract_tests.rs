use std::{fs, path::PathBuf};

#[test]
fn toolchain_contract_pins_release_build_inputs() {
    let toolchain = read_repo_file("rust-toolchain.toml");
    let cargo_config = read_repo_file(".cargo/config.toml");
    let release_workflow = read_repo_file(".github/workflows/release.yml");
    let readme = read_repo_file("README.md");
    let development = read_repo_file("docs/DEVELOPMENT.md");

    assert!(toolchain.contains("channel = \"1.97.0\""));
    assert!(toolchain.contains("profile = \"minimal\""));
    assert!(toolchain.contains("components = [\"rustfmt\", \"clippy\"]"));
    assert!(cargo_config.contains("MACOSX_DEPLOYMENT_TARGET = \"11.0\""));
    assert!(release_workflow.contains("uses: dtolnay/rust-toolchain@1.97.0"));
    assert!(readme.contains("repository-pinned Rust 1.97.0 toolchain"));
    assert!(development.contains("/opt/homebrew/opt/rustup/bin"));
    assert!(development.contains("rustup show active-toolchain"));
}

fn read_repo_file(relative_path: &str) -> String {
    fs::read_to_string(repo_file(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"))
}

fn repo_file(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(relative_path)
}
