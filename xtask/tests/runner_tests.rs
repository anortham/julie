use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use xtask::cli::{TestCommand, parse_test_command, validate_test_command};
use xtask::manifest::TestManifest;
use xtask::runner::{
    BucketResult, BucketStatus, CommandExecutor, CommandOutcome, ProcessCommandExecutor,
    RunSummary, render_bucket_result, render_manifest_listing, render_summary, run_bucket,
    run_tier, transform_command_for_coverage,
};

#[test]
fn runner_tests_list_command_shows_tiers_buckets_and_timeouts() {
    let manifest = sample_manifest();

    let output = render_manifest_listing(&manifest);

    assert!(output.contains("smoke"));
    assert!(output.contains("workspace-init"));
    assert!(output.contains("timeout_seconds"));
}

#[test]
fn runner_tests_run_tier_executes_buckets_in_manifest_order() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::successful();
    let mut output = Vec::new();

    let result = run_tier(&manifest, "dev", 1, false, &executor, &mut output).unwrap();

    assert_eq!(
        result.bucket_names,
        vec!["cli", "core-database", "tools-search"]
    );
    assert_eq!(
        executor.bucket_calls(),
        vec!["cli", "core-database", "tools-search"]
    );
}

#[test]
fn runner_tests_timeout_error_names_the_bucket_and_budget() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_outcomes([(
        "cargo test --lib tests::core::workspace_init",
        CommandOutcome::TimedOut {
            elapsed: Duration::from_secs(61),
        },
    )]);
    let mut output = Vec::new();

    let error = run_bucket(&manifest, "workspace-init", 2, false, &executor, &mut output).unwrap_err();
    let message = error.to_string();

    assert!(message.contains("workspace-init"));
    assert!(message.contains("120s"));
    assert!(message.contains("expected 30s"));
    assert!(message.contains("cargo test --lib tests::core::workspace_init"));
}

#[test]
fn runner_tests_summary_output_reports_total_elapsed_time() {
    let summary = RunSummary {
        bucket_names: vec!["cli".to_string(), "tools-search".to_string()],
        bucket_results: vec![
            BucketResult {
                bucket_name: "cli".to_string(),
                status: BucketStatus::Passed,
                elapsed: Duration::from_millis(12_300),
                command_count: 1,
            },
            BucketResult {
                bucket_name: "tools-search".to_string(),
                status: BucketStatus::Passed,
                elapsed: Duration::from_millis(55_900),
                command_count: 1,
            },
        ],
        passed_buckets: 2,
        total_elapsed: Duration::from_millis(68_200),
    };

    let output = render_summary(&summary);

    assert!(output.contains("SUMMARY:"));
    assert!(output.contains("passed in"));
    assert!(output.contains("68.2s"));
}

#[test]
fn runner_tests_bucket_output_has_start_and_end_markers() {
    let output = render_bucket_result(&BucketResult {
        bucket_name: "tools-search".to_string(),
        status: BucketStatus::Passed,
        elapsed: Duration::from_millis(3_100),
        command_count: 1,
    });

    assert!(output.contains("START tools-search"));
    assert!(output.contains("END tools-search"));
    assert!(output.contains("PASS"));
}

#[test]
fn runner_tests_cli_contract_supports_tiers_list_and_bucket() {
    assert!(matches!(
        parse_test_command(["xtask", "test", "smoke"]),
        Ok(TestCommand::Tier {
            name,
            timeout_multiplier: 1,
            coverage: false,
        }) if name == "smoke"
    ));

    assert!(matches!(
        parse_test_command(["xtask", "test", "list"]),
        Ok(TestCommand::List)
    ));

    assert!(matches!(
        parse_test_command([
            "xtask",
            "test",
            "bucket",
            "tools-search",
            "--timeout-multiplier",
            "3",
        ]),
        Ok(TestCommand::Bucket {
            name,
            timeout_multiplier: 3,
            coverage: false,
        }) if name == "tools-search"
    ));
}

#[test]
fn runner_tests_cli_rejects_unsupported_subcommands() {
    let parsed = parse_test_command(["xtask", "test", "weird"]).unwrap();
    let error = validate_test_command(&sample_manifest(), parsed).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("unsupported xtask test command `weird`")
    );
}

#[test]
fn runner_tests_cli_validation_accepts_manifest_defined_tiers_without_hardcoded_list() {
    let validated = validate_test_command(
        &sample_manifest(),
        parse_test_command(["xtask", "test", "dogfood"]).unwrap(),
    )
    .unwrap();

    assert!(matches!(
        validated,
        TestCommand::Tier {
            name,
            timeout_multiplier: 1,
            coverage: false,
        } if name == "dogfood"
    ));
}

#[test]
fn runner_tests_cli_rejects_missing_bucket_name() {
    let error = parse_test_command(["xtask", "test", "bucket"]).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("missing bucket name for `cargo xtask test bucket <name>`")
    );
}

#[test]
fn runner_tests_cli_rejects_extra_args_after_tier() {
    let error = parse_test_command(["xtask", "test", "smoke", "oops"]).unwrap_err();

    assert!(
        error.to_string().contains("unexpected argument: oops"),
        "got: {error}"
    );
}

#[test]
fn runner_tests_cli_rejects_extra_args_after_bucket() {
    let error =
        parse_test_command(["xtask", "test", "bucket", "tools-search", "oops"]).unwrap_err();

    assert!(
        error.to_string().contains("unexpected argument: oops"),
        "got: {error}"
    );
}

#[test]
fn runner_tests_cli_rejects_invalid_timeout_multiplier() {
    let error =
        parse_test_command(["xtask", "test", "smoke", "--timeout-multiplier", "nope"]).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("invalid `--timeout-multiplier` value `nope`")
    );
}

#[test]
fn runner_tests_cli_rejects_zero_timeout_multiplier() {
    let error = parse_test_command([
        "xtask",
        "test",
        "bucket",
        "tools-search",
        "--timeout-multiplier",
        "0",
    ])
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("timeout multiplier must be greater than zero")
    );
}

#[test]
fn runner_tests_run_bucket_emits_fail_marker_through_execution_path() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_outcomes([(
        "cargo test --lib tests::tools::search",
        CommandOutcome::Failed {
            elapsed: Duration::from_millis(700),
            exit_code: Some(9),
        },
    )]);
    let mut output = Vec::new();

    let error = run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap_err();
    let rendered = String::from_utf8(output).unwrap();

    assert!(error.to_string().contains("exit code: 9"));
    assert_eq!(error.summary.bucket_results.len(), 1);
    assert_eq!(error.summary.bucket_results[0].status, BucketStatus::Failed);
    assert!(rendered.contains("START tools-search"));
    assert!(rendered.contains("END tools-search FAIL (0.7s)"));
}

#[test]
fn runner_tests_run_bucket_emits_timeout_marker_through_execution_path() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_outcomes([(
        "cargo test --lib tests::core::workspace_init",
        CommandOutcome::TimedOut {
            elapsed: Duration::from_secs(61),
        },
    )]);
    let mut output = Vec::new();

    let error = run_bucket(&manifest, "workspace-init", 2, false, &executor, &mut output).unwrap_err();
    let rendered = String::from_utf8(output).unwrap();

    assert!(error.to_string().contains("timed out after 120s"));
    assert_eq!(error.summary.bucket_results.len(), 1);
    assert_eq!(
        error.summary.bucket_results[0].status,
        BucketStatus::TimedOut
    );
    assert!(rendered.contains("START workspace-init"));
    assert!(rendered.contains("END workspace-init TIMEOUT (61.0s)"));
}

#[test]
fn runner_tests_bucket_timeout_is_consumed_across_commands() {
    let manifest = multi_command_manifest();
    let executor = FakeExecutor::with_outcomes([
        (
            "cargo test --lib tests::tools::search::one",
            CommandOutcome::Passed {
                elapsed: Duration::from_secs(40),
            },
        ),
        (
            "cargo test --lib tests::tools::search::two",
            CommandOutcome::Passed {
                elapsed: Duration::from_secs(15),
            },
        ),
    ]);
    let mut output = Vec::new();

    let error = run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap_err();

    assert!(error.to_string().contains("timed out after 50s"));
    assert_eq!(
        error.summary.bucket_results[0].status,
        BucketStatus::TimedOut
    );
    assert_eq!(
        executor.timeouts_for_bucket("tools-search"),
        vec![Duration::from_secs(50), Duration::from_secs(10)]
    );
}

#[test]
fn runner_tests_run_bucket_returns_structured_result() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::successful();
    let mut output = Vec::new();

    let summary = run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap();

    assert_eq!(summary.bucket_results.len(), 1);
    assert_eq!(summary.bucket_results[0].bucket_name, "tools-search");
    assert_eq!(summary.bucket_results[0].status, BucketStatus::Passed);
    assert_eq!(summary.bucket_results[0].command_count, 1);
}

#[test]
fn runner_tests_run_tier_failure_preserves_partial_structured_results() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_outcomes([(
        "cargo test --lib tests::core::database",
        CommandOutcome::Failed {
            elapsed: Duration::from_secs(2),
            exit_code: Some(7),
        },
    )]);
    let mut output = Vec::new();

    let error = run_tier(&manifest, "dev", 1, false, &executor, &mut output).unwrap_err();

    assert_eq!(error.summary.bucket_results.len(), 2);
    assert_eq!(error.summary.bucket_results[0].bucket_name, "cli");
    assert_eq!(error.summary.bucket_results[0].status, BucketStatus::Passed);
    assert_eq!(error.summary.bucket_results[1].bucket_name, "core-database");
    assert_eq!(error.summary.bucket_results[1].status, BucketStatus::Failed);
    assert_eq!(error.summary.passed_buckets, 1);
    assert!(error.to_string().contains("exit code: 7"));
}

#[cfg(unix)]
#[test]
fn runner_tests_process_executor_kills_timed_out_command_tree() {
    let pid_file = unique_temp_path("xtask-runner-timeout-child");
    let command = format!(
        "sh -c 'sleep 30 & child=$!; printf %s \"$child\" > \"{}\"; wait \"$child\"'",
        pid_file.display()
    );
    let executor = ProcessCommandExecutor;

    let outcome = executor
        .run("process-tree", &command, Duration::from_millis(100))
        .unwrap();

    assert!(matches!(outcome, CommandOutcome::TimedOut { .. }));

    let child_pid = read_child_pid(&pid_file);
    let child_still_alive = process_is_alive(child_pid);
    cleanup_process(child_pid);
    let _ = fs::remove_file(&pid_file);

    assert!(
        !child_still_alive,
        "timed out command left child process {child_pid} running"
    );
}

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
smoke = ["cli"]
dev = ["cli", "core-database", "tools-search"]
system = ["workspace-init"]
dogfood = ["search-quality"]
full = ["cli", "core-database", "tools-search", "workspace-init", "search-quality"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
commands = ["cargo test --lib tests::cli_tests"]

[buckets.core-database]
expected_seconds = 10
timeout_seconds = 40
commands = ["cargo test --lib tests::core::database"]

[buckets.tools-search]
expected_seconds = 15
timeout_seconds = 45
commands = ["cargo test --lib tests::tools::search"]

[buckets.workspace-init]
expected_seconds = 30
timeout_seconds = 60
commands = ["cargo test --lib tests::core::workspace_init"]

[buckets.search-quality]
expected_seconds = 90
timeout_seconds = 270
commands = ["cargo test --lib search_quality"]
"#,
    )
    .unwrap()
}

fn multi_command_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
dev = ["tools-search"]

[buckets.tools-search]
expected_seconds = 10
timeout_seconds = 50
commands = [
  "cargo test --lib tests::tools::search::one",
  "cargo test --lib tests::tools::search::two",
]
"#,
    )
    .unwrap()
}

struct FakeExecutor {
    outcomes: Rc<RefCell<HashMap<String, VecDeque<CommandOutcome>>>>,
    calls: Rc<RefCell<Vec<String>>>,
    commands: Rc<RefCell<Vec<String>>>,
    timeouts: Rc<RefCell<HashMap<String, Vec<Duration>>>>,
}

impl FakeExecutor {
    fn successful() -> Self {
        Self {
            outcomes: Rc::new(RefCell::new(HashMap::new())),
            calls: Rc::new(RefCell::new(Vec::new())),
            commands: Rc::new(RefCell::new(Vec::new())),
            timeouts: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn with_outcomes<const N: usize>(entries: [(&str, CommandOutcome); N]) -> Self {
        let mut outcomes = HashMap::new();
        for (command, outcome) in entries {
            outcomes.insert(command.to_string(), VecDeque::from([outcome]));
        }

        Self {
            outcomes: Rc::new(RefCell::new(outcomes)),
            calls: Rc::new(RefCell::new(Vec::new())),
            commands: Rc::new(RefCell::new(Vec::new())),
            timeouts: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn bucket_calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn command_calls(&self) -> Vec<String> {
        self.commands.borrow().clone()
    }

    fn timeouts_for_bucket(&self, bucket: &str) -> Vec<Duration> {
        self.timeouts
            .borrow()
            .get(bucket)
            .cloned()
            .unwrap_or_default()
    }
}

impl CommandExecutor for FakeExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> Result<CommandOutcome> {
        self.calls.borrow_mut().push(bucket.to_string());
        self.commands.borrow_mut().push(command.to_string());
        self.timeouts
            .borrow_mut()
            .entry(bucket.to_string())
            .or_default()
            .push(timeout);

        let outcome = self
            .outcomes
            .borrow_mut()
            .get_mut(command)
            .and_then(|queue| queue.pop_front())
            .unwrap_or(CommandOutcome::Passed {
                elapsed: Duration::from_secs(1),
            });

        Ok(outcome)
    }
}

#[cfg(unix)]
fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
    let unique = format!(
        "{}-{}-{}",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

#[cfg(unix)]
fn read_child_pid(pid_file: &std::path::Path) -> u32 {
    for _ in 0..100 {
        if let Ok(pid) = fs::read_to_string(pid_file) {
            return pid.trim().parse().unwrap();
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    panic!("child pid file was not written: {}", pid_file.display());
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn cleanup_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
}

// --- Coverage flag tests ---

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

// --- Command transformation tests ---

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
        calls.iter().all(|c| c.starts_with("cargo test")),
        "expected original commands, got: {calls:?}"
    );
}

#[test]
fn runner_tests_transform_leaves_non_cargo_test_unchanged() {
    assert_eq!(
        transform_command_for_coverage("echo hello"),
        "echo hello"
    );
}
