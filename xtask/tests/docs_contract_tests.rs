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
fn docs_contract_tests_agents_md_promotes_changed_scope_first() {
    let contents = read_repo_file("AGENTS.md");

    assert!(contents.contains("cargo xtask test changed"));
    assert!(contents.contains("cargo xtask test dev"));
    assert!(contents.contains("Use raw cargo filters only to narrow failures"));
    assert!(!contents.contains("cargo test --lib -- --skip search_quality"));
    assert!(!contents.contains("This is the ONLY default. No exceptions."));
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

#[test]
fn docs_contract_tests_verification_ledger_template_is_operational() {
    let contents = read_repo_file("docs/plans/verification-ledger-template.md");

    assert!(contents.contains("## Verification Ledger"));
    assert!(contents.contains("| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |"));
    assert!(contents.contains("empty until a command has actually run"));
    assert!(contents.contains("## Example Rows"));
    assert!(contents.contains("Do not copy them into plan evidence."));
    assert!(contents.contains("cargo nextest run --lib docs_contract_tests_verification_ledger_template_is_operational 2>&1 \\| tail -10"));
    assert!(contents.contains("cargo xtask test changed"));
    assert!(contents.contains("cargo xtask test dogfood"));
    assert!(contents.contains("example-sha"));
    assert!(!contents.contains("TODO"));
    assert!(!contents.contains("TBD"));
    assert!(!contents.contains("fill in later"));
}

#[test]
fn docs_contract_tests_testing_guide_documents_ledger_reuse() {
    let contents = read_repo_file("docs/TESTING_GUIDE.md");

    assert!(contents.contains("docs/plans/verification-ledger-template.md"));
    assert!(contents.contains("same HEAD"));
    assert!(contents.contains("reuse"));
    assert!(contents.contains("expensive gate"));
}

#[test]
fn docs_contract_tests_testing_guide_documents_bucket_command() {
    let contents = read_repo_file("docs/TESTING_GUIDE.md");

    assert!(contents.contains("cargo xtask test bucket <name>"));
    assert!(contents.contains("cargo xtask test inventory --bucket <name>"));
    assert!(contents.contains("cargo xtask test inventory --tier dev"));
    assert!(contents.contains("Inventory is diagnostic evidence"));
    assert!(contents.contains("not a passing test gate"));
}

#[test]
fn docs_contract_tests_agent_docs_stay_in_sync() {
    let agents = read_repo_file("AGENTS.md");
    let claude = read_repo_file("CLAUDE.md");

    for required in [
        "cargo xtask test bucket <name>",
        "cargo xtask test inventory --bucket <name>",
        "Workers run exact tests only",
        "The orchestrating session handles regression checks",
    ] {
        assert!(agents.contains(required), "AGENTS.md missing `{required}`");
        assert!(claude.contains(required), "CLAUDE.md missing `{required}`");
    }
}

#[test]
fn docs_contract_tests_agents_points_to_ledger_template() {
    let agents = read_repo_file("AGENTS.md");
    let claude = read_repo_file("CLAUDE.md");

    for contents in [agents, claude] {
        assert!(contents.contains("docs/plans/verification-ledger-template.md"));
        assert!(contents.contains("Verification Ledger"));
        assert!(contents.contains("### TDD Cycle for All Development"));
        assert!(contents.contains("1. **RED**: Write a failing test first"));
        assert!(contents.contains("2. **GREEN**: Write minimal code to make test pass"));
    }
}

fn assert_contains_public_commands(contents: &str) {
    for command in [
        "cargo xtask test changed",
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
