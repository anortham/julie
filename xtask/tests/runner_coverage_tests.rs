use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use xtask::cli::{TestCommand, parse_test_command};
use xtask::manifest::TestManifest;
use xtask::runner::{
    CommandExecutor, CommandOutcome, CommandResult, run_tier, transform_command_for_coverage,
};

#[test]
fn runner_tests_cli_parses_coverage_flag_for_tier() {
    assert!(matches!(
        parse_test_command(["xtask", "test", "dev", "--coverage"]),
        Ok(TestCommand::Tier {
            name,
            timeout_multiplier: 1,
            coverage: true,
        }) if name == "dev"
    ));
}

#[test]
fn runner_tests_cli_parses_coverage_flag_for_bucket() {
    assert!(matches!(
        parse_test_command(["xtask", "test", "bucket", "cli", "--coverage"]),
        Ok(TestCommand::Bucket {
            name,
            timeout_multiplier: 1,
            coverage: true,
        }) if name == "cli"
    ));
}

#[test]
fn runner_tests_cli_coverage_and_timeout_multiplier_together() {
    assert!(matches!(
        parse_test_command([
            "xtask",
            "test",
            "dev",
            "--coverage",
            "--timeout-multiplier",
            "3",
        ]),
        Ok(TestCommand::Tier {
            name,
            timeout_multiplier: 3,
            coverage: true,
        }) if name == "dev"
    ));
}

#[test]
fn runner_tests_cli_timeout_multiplier_before_coverage() {
    assert!(matches!(
        parse_test_command([
            "xtask",
            "test",
            "dev",
            "--timeout-multiplier",
            "2",
            "--coverage",
        ]),
        Ok(TestCommand::Tier {
            name,
            timeout_multiplier: 2,
            coverage: true,
        }) if name == "dev"
    ));
}

#[test]
fn runner_tests_transform_cargo_test_to_llvm_cov() {
    assert_eq!(
        transform_command_for_coverage("cargo test --lib tests::cli_tests"),
        "cargo llvm-cov --no-report test --lib tests::cli_tests"
    );
}

#[test]
fn runner_tests_transform_preserves_skip_args() {
    assert_eq!(
        transform_command_for_coverage(
            "cargo test --lib tests::core::database -- --skip search_quality"
        ),
        "cargo llvm-cov --no-report test --lib tests::core::database -- --skip search_quality"
    );
}

#[test]
fn runner_tests_transform_preserves_package_flag() {
    assert_eq!(
        transform_command_for_coverage("cargo test -p xtask"),
        "cargo llvm-cov --no-report test -p xtask"
    );
}

#[test]
fn runner_tests_coverage_mode_transforms_commands_sent_to_executor() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::successful();
    let mut output = Vec::new();

    run_tier(&manifest, "smoke", 1, true, &executor, &mut output).unwrap();

    let calls = executor.command_calls();
    assert!(
        calls
            .iter()
            .all(|c| c.starts_with("cargo llvm-cov --no-report")),
        "expected all commands to be transformed, got: {calls:?}"
    );
}

#[test]
fn runner_tests_non_coverage_mode_leaves_commands_unchanged() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::successful();
    let mut output = Vec::new();

    run_tier(&manifest, "smoke", 1, false, &executor, &mut output).unwrap();

    let calls = executor.command_calls();
    assert!(
        calls.iter().all(|c| !c.starts_with("cargo llvm-cov")),
        "expected no coverage transformation in non-coverage mode, got: {calls:?}"
    );
}

#[test]
fn runner_tests_transform_nextest_to_llvm_cov() {
    assert_eq!(
        transform_command_for_coverage("cargo nextest run --lib tests::cli_tests"),
        "cargo llvm-cov --no-report nextest --lib tests::cli_tests"
    );
}

#[test]
fn runner_tests_transform_nextest_preserves_skip_args() {
    assert_eq!(
        transform_command_for_coverage(
            "cargo nextest run --lib tests::core::database -- --skip search_quality"
        ),
        "cargo llvm-cov --no-report nextest --lib tests::core::database -- --skip search_quality"
    );
}

#[test]
fn runner_tests_transform_leaves_non_cargo_test_unchanged() {
    assert_eq!(transform_command_for_coverage("echo hello"), "echo hello");
}

#[test]
fn runner_tests_transform_leaves_cargo_test_prefix_collisions_unchanged() {
    assert_eq!(
        transform_command_for_coverage("cargo testfoo --lib tests::cli_tests"),
        "cargo testfoo --lib tests::cli_tests"
    );
}

#[test]
fn runner_tests_transform_leaves_nextest_run_prefix_collisions_unchanged() {
    assert_eq!(
        transform_command_for_coverage("cargo nextest runner --lib tests::cli_tests"),
        "cargo nextest runner --lib tests::cli_tests"
    );
}

#[test]
fn runner_tests_prebuild_runs_before_bucket_commands() {
    let manifest = target_aware_manifest();
    let executor = FakeExecutor::with_outcomes([
        (
            "cargo nextest run --no-run -p julie-core --lib",
            CommandOutcome::Passed {
                elapsed: Duration::from_secs(2),
            },
        ),
        (
            "cargo nextest run --no-run -p xtask",
            CommandOutcome::Passed {
                elapsed: Duration::from_secs(3),
            },
        ),
        (
            "cargo test --no-run -p julie-runtime --lib",
            CommandOutcome::Passed {
                elapsed: Duration::from_secs(5),
            },
        ),
    ]);
    let mut output = Vec::new();

    let summary = run_tier(&manifest, "smoke", 1, false, &executor, &mut output).unwrap();

    let calls = executor.command_calls();
    assert_eq!(
        &calls[..3],
        [
            "cargo nextest run --no-run -p julie-core --lib",
            "cargo nextest run --no-run -p xtask",
            "cargo test --no-run -p julie-runtime --lib",
        ],
        "selected test targets must be prebuilt once in first-seen order; got: {calls:?}"
    );
    assert_eq!(
        calls.iter().filter(|c| c.contains("--no-run")).count(),
        3,
        "each unique test target should be prebuilt exactly once, got: {calls:?}"
    );
    assert_eq!(
        &calls[3..],
        [
            "cargo nextest run -p julie-core --lib tests::database",
            "cargo nextest run -p julie-core --lib tests::paths",
            "cargo nextest run -p xtask changed_tests",
            "cargo build -p julie-core",
            "cargo test -p julie-runtime --lib tests::watcher",
        ],
        "bucket work must start only after every selected test target is built"
    );
    assert_eq!(summary.prebuild_elapsed, Duration::from_secs(10));
}

#[test]
fn runner_tests_prebuild_failure_aborts_before_any_bucket() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_outcomes([(
        "cargo test --no-run --lib",
        CommandOutcome::Failed {
            elapsed: Duration::from_secs(5),
            exit_code: Some(1),
        },
    )]);
    let mut output = Vec::new();

    let error = run_tier(&manifest, "smoke", 1, false, &executor, &mut output).unwrap_err();

    assert!(
        error.to_string().contains("prebuild"),
        "error should mention prebuild, got: {}",
        error
    );
    assert_eq!(
        error.summary.bucket_results.len(),
        0,
        "no bucket results should exist when prebuild fails"
    );
    let calls = executor.command_calls();
    assert_eq!(
        calls.len(),
        1,
        "only prebuild should have run, got: {calls:?}"
    );
    assert_eq!(calls[0], "cargo test --no-run --lib");
}

#[test]
fn runner_tests_prebuild_coverage_mode_transforms_command() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::successful();
    let mut output = Vec::new();

    run_tier(&manifest, "smoke", 1, true, &executor, &mut output).unwrap();

    let calls = executor.command_calls();
    assert_eq!(
        calls[0], "cargo llvm-cov --no-report test --no-run --lib",
        "coverage mode should transform the prebuild command, got: {calls:?}"
    );
}

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
scope_label = "smoke"
commands = ["cargo test --lib tests::cli_tests"]
"#,
    )
    .unwrap()
}

fn target_aware_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
fast = ["core"]
smoke = ["core", "harness"]

[buckets.core]
expected_seconds = 5
timeout_seconds = 30
scope_label = "core"
commands = [
    "cargo nextest run -p julie-core --lib tests::database",
    "cargo nextest run -p julie-core --lib tests::paths",
]

[buckets.harness]
expected_seconds = 5
timeout_seconds = 30
scope_label = "harness"
commands = [
    "cargo nextest run -p xtask changed_tests",
    "cargo build -p julie-core",
    "cargo test -p julie-runtime --lib tests::watcher",
]
"#,
    )
    .unwrap()
}

struct FakeExecutor {
    outcomes: Rc<RefCell<HashMap<String, VecDeque<CommandOutcome>>>>,
    commands: Rc<RefCell<Vec<String>>>,
}

impl FakeExecutor {
    fn successful() -> Self {
        Self::with_outcomes([])
    }

    fn with_outcomes<const N: usize>(entries: [(&str, CommandOutcome); N]) -> Self {
        let mut outcomes: HashMap<String, VecDeque<CommandOutcome>> = HashMap::new();
        for (command, outcome) in entries {
            outcomes
                .entry(command.to_string())
                .or_default()
                .push_back(outcome);
        }
        Self {
            outcomes: Rc::new(RefCell::new(outcomes)),
            commands: Rc::new(RefCell::new(Vec::new())),
        }
    }

    fn command_calls(&self) -> Vec<String> {
        self.commands.borrow().clone()
    }
}

impl CommandExecutor for FakeExecutor {
    fn run(&self, _bucket: &str, command: &str, _timeout: Duration) -> Result<CommandResult> {
        self.commands.borrow_mut().push(command.to_string());
        let outcome = self
            .outcomes
            .borrow_mut()
            .get_mut(command)
            .and_then(VecDeque::pop_front)
            .unwrap_or(CommandOutcome::Passed {
                elapsed: Duration::from_secs(1),
            });
        Ok(CommandResult {
            outcome,
            captured: String::new(),
        })
    }
}
