use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use xtask::cli::{TestCommand, parse_test_command, validate_test_command};
use xtask::inventory::InventoryTarget;
use xtask::manifest::TestManifest;
use xtask::runner::{
    BucketResult, BucketStatus, CommandExecutor, CommandOutcome, CommandResult,
    ProcessCommandExecutor, RunSummary, render_bucket_result, render_manifest_listing,
    render_summary, run_bucket, run_tier,
};

#[test]
fn runner_tests_list_command_shows_bucket_metadata() {
    let manifest = sample_manifest();

    let output = render_manifest_listing(&manifest);

    assert!(output.contains("smoke"));
    assert!(output.contains("workspace-init"));
    assert!(output.contains("timeout_seconds"));
    assert!(output.contains("scope_label"));
    assert!(output.contains("expensive"));
    assert!(output.contains("[expensive]"));
    assert!(output.contains("changed"));
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
        vec!["prebuild", "cli", "core-database", "tools-search"]
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

    let error = run_bucket(
        &manifest,
        "workspace-init",
        2,
        false,
        &executor,
        &mut output,
    )
    .unwrap_err();
    let message = error.to_string();

    assert!(message.contains("workspace-init"));
    assert!(message.contains("120s"));
    assert!(message.contains("expected 30s"));
    assert!(message.contains("cargo test --lib tests::core::workspace_init"));
}

#[test]
fn runner_tests_summary_reports_expected_actual_scope_and_slow_buckets() {
    let summary = RunSummary {
        bucket_names: vec!["cli".to_string(), "tools-search".to_string()],
        bucket_results: vec![
            BucketResult {
                bucket_name: "cli".to_string(),
                status: BucketStatus::Passed,
                elapsed: Duration::from_millis(12_300),
                command_count: 1,
                expected_seconds: 10,
                scope_label: "smoke".to_string(),
            },
            BucketResult {
                bucket_name: "tools-search".to_string(),
                status: BucketStatus::Passed,
                elapsed: Duration::from_millis(55_900),
                command_count: 1,
                expected_seconds: 30,
                scope_label: "tooling".to_string(),
            },
        ],
        passed_buckets: 2,
        total_elapsed: Duration::from_millis(68_200),
        prebuild_elapsed: Duration::ZERO,
    };

    let output = render_summary(&summary);

    assert!(output.contains("SUMMARY:"));
    assert!(output.contains("passed in 68.2s"));
    assert!(output.contains("cli"));
    assert!(output.contains("expected 10.0s"));
    assert!(output.contains("actual 12.3s"));
    assert!(output.contains("commands 1"));
    assert!(output.contains("scope smoke"));
    assert!(output.contains("tools-search"));
    assert!(output.contains("expected 30.0s"));
    assert!(output.contains("actual 55.9s"));
    assert!(output.contains("scope tooling"));
    assert!(output.contains("SLOW"));
}

#[test]
fn runner_tests_bucket_output_has_start_and_end_markers() {
    let output = render_bucket_result(&BucketResult {
        bucket_name: "tools-search".to_string(),
        status: BucketStatus::Passed,
        elapsed: Duration::from_millis(3_100),
        command_count: 1,
        expected_seconds: 15,
        scope_label: "tooling".to_string(),
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
fn runner_tests_cli_parses_inventory_command() {
    assert!(matches!(
        parse_test_command(["xtask", "test", "inventory", "--tier", "dev"]),
        Ok(TestCommand::Inventory {
            target: InventoryTarget::Tier(name),
        }) if name == "dev"
    ));

    assert!(matches!(
        parse_test_command(["xtask", "test", "inventory", "--bucket", "tools-search"]),
        Ok(TestCommand::Inventory {
            target: InventoryTarget::Bucket(name),
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
fn runner_tests_cli_rejects_unknown_bucket_name() {
    let parsed = parse_test_command(["xtask", "test", "bucket", "missing-bucket"]).unwrap();
    let error = validate_test_command(&sample_manifest(), parsed).unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("unknown test bucket `missing-bucket`"),
        "got: {message}"
    );
    assert!(message.contains("cargo xtask test list"), "got: {message}");
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
fn runner_tests_failed_bucket_writes_captured_output_between_markers() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_results([(
        "cargo test --lib tests::tools::search",
        CommandResult {
            outcome: CommandOutcome::Failed {
                elapsed: Duration::from_millis(700),
                exit_code: Some(1),
            },
            captured:
                "test tests::tools::search::test_foo ... FAILED\nassertion failed: left == right\n"
                    .to_string(),
        },
    )]);
    let mut output = Vec::new();

    run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap_err();
    let rendered = String::from_utf8(output).unwrap();

    let start_idx = rendered.find("START tools-search").expect("missing START");
    let captured_idx = rendered
        .find("test_foo ... FAILED")
        .expect("missing captured output");
    let end_idx = rendered.find("END tools-search FAIL").expect("missing END");
    assert!(
        start_idx < captured_idx,
        "captured output should come after START"
    );
    assert!(
        captured_idx < end_idx,
        "captured output should come before END"
    );
    assert!(rendered.contains("assertion failed: left == right"));
}

#[test]
fn runner_tests_timed_out_bucket_writes_captured_output_between_markers() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_results([(
        "cargo test --lib tests::core::workspace_init",
        CommandResult {
            outcome: CommandOutcome::TimedOut {
                elapsed: Duration::from_secs(61),
            },
            captured: "running 3 tests\ntest tests::core::workspace_init::slow_one has been running for over 60 seconds\n".to_string(),
        },
    )]);
    let mut output = Vec::new();

    run_bucket(
        &manifest,
        "workspace-init",
        2,
        false,
        &executor,
        &mut output,
    )
    .unwrap_err();
    let rendered = String::from_utf8(output).unwrap();

    assert!(rendered.contains("START workspace-init"));
    assert!(rendered.contains("has been running for over 60 seconds"));
    assert!(rendered.contains("END workspace-init TIMEOUT"));
}

#[test]
fn runner_tests_passed_bucket_discards_captured_output() {
    let manifest = sample_manifest();
    let executor = FakeExecutor::with_results([(
        "cargo test --lib tests::cli_tests",
        CommandResult {
            outcome: CommandOutcome::Passed {
                elapsed: Duration::from_millis(500),
            },
            captured: "Compiling julie v6.8.0\nrunning 50 tests\ntest result: ok. 50 passed\n"
                .to_string(),
        },
    )]);
    let mut output = Vec::new();

    run_bucket(&manifest, "cli", 1, false, &executor, &mut output).unwrap();
    let rendered = String::from_utf8(output).unwrap();

    assert!(rendered.contains("START cli"));
    assert!(rendered.contains("END cli PASS"));
    assert!(
        !rendered.contains("Compiling julie"),
        "compile noise should not leak on pass: {rendered}"
    );
    assert!(
        !rendered.contains("running 50 tests"),
        "cargo banner should not leak on pass: {rendered}"
    );
}

#[test]
fn runner_tests_bucket_output_includes_command_timings() {
    let manifest = multi_command_manifest();
    let executor = FakeExecutor::with_outcomes([
        (
            "cargo test --lib tests::tools::search::one",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(1_200),
            },
        ),
        (
            "cargo test --lib tests::tools::search::two",
            CommandOutcome::Passed {
                elapsed: Duration::from_millis(2_300),
            },
        ),
    ]);
    let mut output = Vec::new();

    run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap();
    let rendered = String::from_utf8(output).unwrap();

    assert!(rendered.contains(
        "COMMAND tools-search 1/2 PASS (1.2s) cargo test --lib tests::tools::search::one"
    ));
    assert!(rendered.contains(
        "COMMAND tools-search 2/2 PASS (2.3s) cargo test --lib tests::tools::search::two"
    ));
    assert!(rendered.contains("END tools-search PASS"));
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

    let error =
        run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap_err();
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

    let error = run_bucket(
        &manifest,
        "workspace-init",
        2,
        false,
        &executor,
        &mut output,
    )
    .unwrap_err();
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

    let error =
        run_bucket(&manifest, "tools-search", 1, false, &executor, &mut output).unwrap_err();

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
    let command = timeout_process_tree_command(&pid_file);
    let executor = ProcessCommandExecutor;

    let result = executor
        .run("process-tree", &command, Duration::from_millis(100))
        .unwrap();

    assert!(matches!(result.outcome, CommandOutcome::TimedOut { .. }));

    let child_pid = read_child_pid(&pid_file);
    let child_still_alive = process_is_alive(child_pid);
    cleanup_process(child_pid);
    let _ = fs::remove_file(&pid_file);

    assert!(
        !child_still_alive,
        "timed out command left child process {child_pid} running"
    );
}

#[cfg(unix)]
#[test]
fn runner_tests_process_executor_timeout_command_handles_shell_special_pid_path() {
    let pid_file = unique_temp_path("xtask-runner-timeout-child-'quoted'");
    let command = timeout_process_tree_command(&pid_file);
    let executor = ProcessCommandExecutor;

    let result = executor
        .run("process-tree", &command, Duration::from_millis(100))
        .unwrap();

    assert!(matches!(result.outcome, CommandOutcome::TimedOut { .. }));

    let child_pid = read_child_pid(&pid_file);
    let child_still_alive = process_is_alive(child_pid);
    cleanup_process(child_pid);
    let _ = fs::remove_file(&pid_file);

    assert!(
        !child_still_alive,
        "timed out command left child process {child_pid} running"
    );
}

#[cfg(unix)]
#[test]
fn runner_tests_process_is_alive_treats_zombie_as_dead() {
    let mut child = std::process::Command::new("sh")
        .args(["-c", "exit 0"])
        .spawn()
        .unwrap();
    let pid = child.id();

    for _ in 0..100 {
        if process_is_zombie(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        process_is_zombie(pid),
        "child {pid} did not become a zombie"
    );
    assert!(
        !process_is_alive(pid),
        "zombie child {pid} should not count as a live process"
    );

    let _ = child.wait();
}

fn sample_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
fast = ["cli"]
smoke = ["cli"]
dev = ["cli", "core-database", "tools-search"]
system = ["workspace-init"]
dogfood = ["search-quality"]
full = ["cli", "core-database", "tools-search", "workspace-init", "search-quality"]

[buckets.cli]
expected_seconds = 5
timeout_seconds = 30
scope_label = "smoke"
commands = ["cargo test --lib tests::cli_tests"]

[buckets.core-database]
expected_seconds = 10
timeout_seconds = 40
scope_label = "core"
commands = ["cargo test --lib tests::core::database"]

[buckets.tools-search]
expected_seconds = 15
timeout_seconds = 45
scope_label = "tooling"
commands = ["cargo test --lib tests::tools::search"]

[buckets.workspace-init]
expected_seconds = 30
timeout_seconds = 60
scope_label = "system"
commands = ["cargo test --lib tests::core::workspace_init"]

[buckets.search-quality]
expected_seconds = 90
timeout_seconds = 270
scope_label = "dogfood"
expensive = true
commands = ["cargo test --lib search_quality"]
"#,
    )
    .unwrap()
}

fn multi_command_manifest() -> TestManifest {
    TestManifest::from_str(
        r#"
[tiers]
fast = ["tools-search"]
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
    captured: Rc<RefCell<HashMap<String, VecDeque<String>>>>,
    calls: Rc<RefCell<Vec<String>>>,
    commands: Rc<RefCell<Vec<String>>>,
    timeouts: Rc<RefCell<HashMap<String, Vec<Duration>>>>,
}

impl FakeExecutor {
    fn successful() -> Self {
        Self {
            outcomes: Rc::new(RefCell::new(HashMap::new())),
            captured: Rc::new(RefCell::new(HashMap::new())),
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
            captured: Rc::new(RefCell::new(HashMap::new())),
            calls: Rc::new(RefCell::new(Vec::new())),
            commands: Rc::new(RefCell::new(Vec::new())),
            timeouts: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    fn bucket_calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn timeouts_for_bucket(&self, bucket: &str) -> Vec<Duration> {
        self.timeouts
            .borrow()
            .get(bucket)
            .cloned()
            .unwrap_or_default()
    }
}

impl FakeExecutor {
    fn with_results<const N: usize>(entries: [(&str, CommandResult); N]) -> Self {
        let mut outcomes: HashMap<String, VecDeque<CommandOutcome>> = HashMap::new();
        let mut captured: HashMap<String, VecDeque<String>> = HashMap::new();
        for (command, result) in entries {
            outcomes
                .entry(command.to_string())
                .or_default()
                .push_back(result.outcome);
            captured
                .entry(command.to_string())
                .or_default()
                .push_back(result.captured);
        }

        Self {
            outcomes: Rc::new(RefCell::new(outcomes)),
            captured: Rc::new(RefCell::new(captured)),
            calls: Rc::new(RefCell::new(Vec::new())),
            commands: Rc::new(RefCell::new(Vec::new())),
            timeouts: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl CommandExecutor for FakeExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> Result<CommandResult> {
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

        let captured = self
            .captured
            .borrow_mut()
            .get_mut(command)
            .and_then(|queue| queue.pop_front())
            .unwrap_or_default();

        Ok(CommandResult { outcome, captured })
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
fn timeout_process_tree_command(pid_file: &std::path::Path) -> String {
    format!(
        "PID_FILE={} sh -c 'sleep 30 & child=$!; printf %s \"$child\" > \"$PID_FILE\"; wait \"$child\"'",
        shell_quote(&pid_file.to_string_lossy())
    )
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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
    let kill_probe_succeeds = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    kill_probe_succeeds && !process_is_zombie(pid)
}

#[cfg(unix)]
fn process_is_zombie(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .trim_start()
                    .starts_with('Z')
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn cleanup_process(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
}
