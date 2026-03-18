use std::fs;
use std::path::PathBuf;

use xtask::manifest::TestManifest;
use xtask::workspace_root;

#[test]
fn docs_contract_tests_claude_md_uses_xtask_runner_as_canonical_workflow() {
    let contents = read_repo_file("CLAUDE.md");
    let manifest = load_manifest();

    assert_contains_public_commands(&contents);
    assert!(contents.contains("Use raw cargo filters only to narrow failures"));
    assert_blocked_tier_caveat(&contents, &manifest);
}

#[test]
fn docs_contract_tests_agents_md_defaults_to_xtask_dev_tier() {
    let contents = read_repo_file("AGENTS.md");

    assert!(contents.contains("cargo xtask test dev"));
    assert!(contents.contains("Use raw cargo filters only to narrow failures"));
    assert!(!contents.contains("cargo test --lib -- --skip search_quality"));
}

#[test]
fn docs_contract_tests_cargo_alias_uses_quiet_xtask_runner() {
    let contents = read_repo_file(".cargo/config.toml");

    assert!(contents.contains("xtask = \"run -q -p xtask --\""));
}

#[test]
fn docs_contract_tests_readme_lists_public_xtask_commands() {
    let contents = read_repo_file("README.md");
    let manifest = load_manifest();

    assert_contains_public_commands(&contents);
    assert_blocked_tier_caveat(&contents, &manifest);
}

fn assert_contains_public_commands(contents: &str) {
    for command in [
        "cargo xtask test smoke",
        "cargo xtask test dev",
        "cargo xtask test system",
        "cargo xtask test dogfood",
        "cargo xtask test full",
        "cargo xtask test list",
    ] {
        assert!(
            contents.contains(command),
            "missing public command `{command}`"
        );
    }
}

fn read_repo_file(relative_path: &str) -> String {
    fs::read_to_string(repo_file(relative_path)).unwrap()
}

fn load_manifest() -> TestManifest {
    TestManifest::load(repo_file("xtask/test_tiers.toml")).unwrap()
}

fn assert_blocked_tier_caveat(contents: &str, manifest: &TestManifest) {
    if !manifest.blocked_tiers.is_empty() {
        assert!(contents.contains("green-by-default"));
        assert!(contents.contains("workspace_init"));

        for tier_name in manifest.blocked_tiers.keys() {
            assert!(
                contents.contains(&format!("`{tier_name}`")),
                "missing blocked tier name `{tier_name}` in docs"
            );
        }
    }
}

fn repo_file(relative_path: &str) -> PathBuf {
    workspace_root().join(relative_path)
}
