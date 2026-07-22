use xtask::manifest::TestManifest;

#[test]
fn manifest_tests_parse_tiers_and_buckets_from_toml() {
    let manifest = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
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
fn manifest_tests_parse_bucket_metadata_with_defaults() {
    let manifest = TestManifest::from_str(
        r#"
[tiers]
fast = ["legacy"]
smoke = ["legacy", "rich"]

[buckets.legacy]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]

[buckets.rich]
expected_seconds = 2
timeout_seconds = 90
scope_label = "integration"
owner = "tools"
expensive = true
notes = "uses large fixture"
commands = ["cargo test --lib tests::tools::search"]
"#,
    )
    .unwrap();

    let legacy = &manifest.buckets["legacy"];
    assert_eq!(legacy.scope_label, "bucket");
    assert_eq!(legacy.owner, "lead");
    assert!(!legacy.expensive);
    assert_eq!(legacy.notes, None);

    let rich = &manifest.buckets["rich"];
    assert_eq!(rich.scope_label, "integration");
    assert_eq!(rich.owner, "tools");
    assert!(rich.expensive);
    assert_eq!(rich.notes.as_deref(), Some("uses large fixture"));
}

#[test]
fn manifest_tests_reject_unknown_top_level_fields() {
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
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
fast = ["cli"]
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
fast = ["cli"]
smoke = ["missing"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("references missing bucket 'missing'")
    );
}

#[test]
fn manifest_tests_reject_empty_tiers() {
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
dev = []

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("tier 'dev' must define at least one bucket")
    );
}

#[test]
fn manifest_tests_reject_buckets_without_commands() {
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = []
"#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("must define at least one command")
    );
}

#[test]
fn manifest_tests_reject_zero_expected_seconds() {
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
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
fast = ["cli"]
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
fast = ["cli"]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 61
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("timeout_seconds must be >= expected_seconds")
    );
}

#[test]
fn manifest_tests_parse_blocked_tier_notes() {
    let manifest = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
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

#[test]
fn manifest_tests_reject_duplicate_bucket_commands() {
    let duplicate_command = "cargo test --lib tests::cli_tests";
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
smoke = ["alpha", "beta"]

[buckets.alpha]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]

[buckets.beta]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("duplicate command"));
    assert!(message.contains("'alpha'"));
    assert!(message.contains("'beta'"));
    assert!(message.contains(duplicate_command));
}

#[test]
fn manifest_tests_reject_duplicate_commands_in_same_bucket() {
    let duplicate_command = "cargo test --lib tests::cli_tests";
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
smoke = ["alpha"]

[buckets.alpha]
expected_seconds = 1
timeout_seconds = 60
commands = [
  "cargo test --lib tests::cli_tests",
  "cargo test --lib tests::cli_tests",
]
"#,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(message.contains("duplicate command"));
    assert!(message.contains("'alpha'"));
    assert!(message.contains(duplicate_command));
}

#[test]
fn manifest_tests_reject_missing_fast_tier() {
    let error = TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("manifest must define a 'fast' tier")
    );
}

#[test]
fn manifest_tests_reject_over_budget_fast_tier() {
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]

[buckets.cli]
expected_seconds = 61
timeout_seconds = 120
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("fast tier expected runtime must stay under 60s"),
        "unexpected error: {message}"
    );
    assert!(message.contains("got 61s"), "unexpected error: {message}");
}

#[test]
fn manifest_tests_accept_valid_fast_tier() {
    let manifest = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]

[buckets.cli]
expected_seconds = 60
timeout_seconds = 120
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap();

    assert_eq!(manifest.tiers["fast"], vec!["cli"]);
}

#[test]
fn manifest_tests_reject_over_budget_dev_tier() {
    let error = TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
dev = ["heavy"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 60
commands = ["cargo test --lib tests::cli_tests"]

[buckets.heavy]
expected_seconds = 601
timeout_seconds = 700
commands = ["cargo test --lib tests::heavy"]
"#,
    )
    .unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("dev tier expected runtime must stay under 600s"),
        "unexpected error: {message}"
    );
    assert!(message.contains("got 601s"), "unexpected error: {message}");
}
