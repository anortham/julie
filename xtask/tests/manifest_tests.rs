use xtask::manifest::TestManifest;

#[test]
fn manifest_tests_parse_tiers_and_buckets_from_toml() {
    let manifest = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap();

    assert_eq!(manifest.tiers["smoke"], vec!["cli"]);
    assert_eq!(manifest.buckets["cli"].expected_seconds, 1);
    assert_eq!(manifest.buckets["cli"].timeout_seconds, 60);
    assert_eq!(manifest.buckets["cli"].commands.len(), 1);
    assert_eq!(
        manifest.buckets["cli"].commands[0],
        "cargo test --lib tests::cli_tests"
    );
}

#[test]
fn manifest_tests_reject_unknown_top_level_fields() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]

[extra]
surprise = true
"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn manifest_tests_reject_unknown_bucket_fields() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
unexpected = "nope"
"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn manifest_tests_reject_tier_references_to_missing_buckets() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["missing"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("references missing bucket 'missing'"));
}

#[test]
fn manifest_tests_reject_empty_tiers() {
    let error = TestManifest::from_str(
        r#"
[tiers]
dev = []

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("tier 'dev' must define at least one bucket"));
}

#[test]
fn manifest_tests_reject_buckets_without_commands() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = []
"#,
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("must define at least one command"));
}

#[test]
fn manifest_tests_reject_zero_expected_seconds() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 0
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("expected_seconds must be > 0"));
}

#[test]
fn manifest_tests_reject_zero_timeout_seconds() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 0
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(error.to_string().contains("timeout_seconds must be > 0"));
}

#[test]
fn manifest_tests_reject_timeout_shorter_than_expected_runtime() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 61
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(error
        .to_string()
        .contains("timeout_seconds must be >= expected_seconds"));
}

#[test]
fn manifest_tests_parse_blocked_tier_notes() {
    let manifest = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]
system = ["workspace-init"]

[blocked_tiers]
system = "pre-existing workspace_init failure/outlier"

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]

[buckets.workspace-init]
expected_seconds = 10
timeout_seconds = 60
commands = ["cargo test --lib tests::core::workspace_init"]
"#,
    )
    .unwrap();

    assert_eq!(
        manifest.blocked_tiers["system"],
        "pre-existing workspace_init failure/outlier"
    );
}
