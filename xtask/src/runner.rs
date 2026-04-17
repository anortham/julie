use std::fmt;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::manifest::TestManifest;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub trait CommandExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> Result<CommandResult>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    Passed {
        elapsed: Duration,
    },
    Failed {
        elapsed: Duration,
        exit_code: Option<i32>,
    },
    TimedOut {
        elapsed: Duration,
    },
}

/// Outcome of a single command plus its merged stdout/stderr output.
///
/// Passing commands: callers should discard `captured` to keep the context clean.
/// Failing/timed-out commands: callers should emit `captured` so the failing
/// test output reaches the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub outcome: CommandOutcome,
    pub captured: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BucketStatus {
    Passed,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BucketResult {
    pub bucket_name: String,
    pub status: BucketStatus,
    pub elapsed: Duration,
    pub command_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub bucket_names: Vec<String>,
    pub bucket_results: Vec<BucketResult>,
    pub passed_buckets: usize,
    pub total_elapsed: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailure {
    pub summary: RunSummary,
    pub message: String,
}

impl fmt::Display for RunFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RunFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BucketFailure {
    result: BucketResult,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BucketPlan {
    expected_seconds: u64,
    timeout_seconds: u64,
    commands: Vec<String>,
}

const PROGRAM_TIERS: &[(&str, &[&str])] = &[
    ("reliability", &["daemon", "workspace-init", "integration"]),
    ("benchmark", &["system-health"]),
];

const SPECIAL_BUCKETS: &[(&str, u64, u64, &[&str])] = &[(
    "system-health",
    30,
    120,
    &["cargo nextest run --lib tests::integration::system_health"],
)];

pub struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
    fn run(&self, _bucket: &str, command: &str, timeout: Duration) -> Result<CommandResult> {
        let start = Instant::now();
        let deadline = start + timeout;
        let mut process = shell_command(command);
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
        configure_command_for_termination(&mut process);
        let mut child = process.spawn()?;

        // Drain both pipes on background threads so the child never blocks on
        // a full pipe buffer (~64KB on most systems). Without this, a noisy
        // test run would deadlock the try_wait loop.
        let stdout_pipe = child.stdout.take().expect("stdout should be piped");
        let stderr_pipe = child.stderr.take().expect("stderr should be piped");
        let stdout_reader = thread::spawn(move || read_all(stdout_pipe));
        let stderr_reader = thread::spawn(move || read_all(stderr_pipe));

        let outcome = loop {
            if let Some(status) = child.try_wait()? {
                let elapsed = start.elapsed();
                break if status.success() {
                    CommandOutcome::Passed { elapsed }
                } else {
                    CommandOutcome::Failed {
                        elapsed,
                        exit_code: status.code(),
                    }
                };
            }

            let now = Instant::now();
            if now >= deadline {
                let _ = terminate_child_on_timeout(&mut child);
                break CommandOutcome::TimedOut {
                    elapsed: start.elapsed(),
                };
            }

            let remaining = deadline.saturating_duration_since(now);
            thread::sleep(remaining.min(Duration::from_millis(10)));
        };

        let stdout_bytes = stdout_reader.join().unwrap_or_default();
        let stderr_bytes = stderr_reader.join().unwrap_or_default();
        let mut captured = String::from_utf8_lossy(&stdout_bytes).into_owned();
        if !stderr_bytes.is_empty() {
            captured.push_str(&String::from_utf8_lossy(&stderr_bytes));
        }

        Ok(CommandResult { outcome, captured })
    }
}

fn read_all<R: Read>(mut reader: R) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = reader.read_to_end(&mut buf);
    buf
}

fn configure_command_for_termination(_command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        _command.pre_exec(|| {
            if setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(unix)]
fn terminate_child_on_timeout(child: &mut Child) -> std::io::Result<()> {
    let pid = child.id() as i32;
    let result = unsafe { killpg(pid, SIGKILL) };
    if result == -1 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ESRCH) {
            child.kill()?;
        }
    }
    let _ = child.wait();
    Ok(())
}

#[cfg(windows)]
fn terminate_child_on_timeout(child: &mut Child) -> std::io::Result<()> {
    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .status()?;
    if !status.success() {
        child.kill()?;
    }
    let _ = child.wait();
    Ok(())
}

#[cfg(unix)]
const SIGKILL: i32 = 9;
#[cfg(unix)]
const ESRCH: i32 = 3;

#[cfg(unix)]
unsafe extern "C" {
    fn killpg(pgrp: i32, sig: i32) -> i32;
    fn setsid() -> i32;
}

/// Transforms a `cargo test` command into a `cargo llvm-cov --no-report` command
/// for coverage accumulation. Non-`cargo test` commands are returned unchanged.
pub fn transform_command_for_coverage(command: &str) -> String {
    if let Some(rest) = command.strip_prefix("cargo test") {
        format!("cargo llvm-cov --no-report test{rest}")
    } else if let Some(rest) = command.strip_prefix("cargo nextest run") {
        format!("cargo llvm-cov --no-report nextest{rest}")
    } else {
        command.to_string()
    }
}

pub fn render_manifest_listing(manifest: &TestManifest) -> String {
    let mut output = String::from("TIERS\n");
    for (tier_name, buckets) in &manifest.tiers {
        output.push_str(&format!("- {tier_name}: {}\n", buckets.join(", ")));
    }

    output.push_str("\nBUCKETS\n");
    for (bucket_name, bucket) in &manifest.buckets {
        output.push_str(&format!("- {bucket_name}\n"));
        output.push_str(&format!(
            "  expected_seconds = {}\n",
            bucket.expected_seconds
        ));
        output.push_str(&format!("  timeout_seconds = {}\n", bucket.timeout_seconds));
        for command in &bucket.commands {
            output.push_str(&format!("  command = {command}\n"));
        }
    }

    output.push_str("\nPROGRAM TIERS\n");
    for (tier_name, buckets) in PROGRAM_TIERS {
        output.push_str(&format!("- {tier_name}: {}\n", buckets.join(", ")));
    }

    output.push_str("\nWORKFLOWS\n");
    output.push_str("- changed: infer buckets from the current git diff\n");

    output
}

pub fn run_tier<E, W>(
    manifest: &TestManifest,
    tier_name: &str,
    timeout_multiplier: u64,
    coverage: bool,
    executor: &E,
    writer: &mut W,
) -> std::result::Result<RunSummary, RunFailure>
where
    E: CommandExecutor,
    W: Write,
{
    let bucket_names = manifest
        .tiers
        .get(tier_name)
        .cloned()
        .or_else(|| special_tier_bucket_names(tier_name))
        .ok_or_else(|| RunFailure {
            summary: empty_summary(Vec::new()),
            message: format!("unknown test tier `{tier_name}`"),
        })?;

    run_named_buckets(
        manifest,
        &bucket_names,
        timeout_multiplier,
        coverage,
        executor,
        writer,
    )
}

pub fn run_named_buckets<E, W>(
    manifest: &TestManifest,
    bucket_names: &[String],
    timeout_multiplier: u64,
    coverage: bool,
    executor: &E,
    writer: &mut W,
) -> std::result::Result<RunSummary, RunFailure>
where
    E: CommandExecutor,
    W: Write,
{
    let bucket_names = bucket_names.to_vec();

    if let Err(message) = prebuild_test_binary(coverage, executor) {
        return Err(RunFailure {
            summary: empty_summary(bucket_names),
            message,
        });
    }

    let mut total_elapsed = Duration::ZERO;
    let mut bucket_results = Vec::with_capacity(bucket_names.len());

    for bucket_name in &bucket_names {
        match execute_bucket(
            manifest,
            bucket_name,
            timeout_multiplier,
            coverage,
            executor,
            writer,
        ) {
            Ok(bucket_result) => {
                total_elapsed += bucket_result.elapsed;
                bucket_results.push(bucket_result);
            }
            Err(error) => {
                total_elapsed += error.result.elapsed;
                bucket_results.push(error.result);
                return Err(RunFailure {
                    summary: RunSummary {
                        bucket_names,
                        passed_buckets: bucket_results
                            .iter()
                            .filter(|result| result.status == BucketStatus::Passed)
                            .count(),
                        bucket_results,
                        total_elapsed,
                    },
                    message: error.message,
                });
            }
        }
    }

    Ok(RunSummary {
        bucket_names,
        passed_buckets: bucket_results
            .iter()
            .filter(|result| result.status == BucketStatus::Passed)
            .count(),
        total_elapsed,
        bucket_results,
    })
}

pub fn run_bucket<E, W>(
    manifest: &TestManifest,
    bucket_name: &str,
    timeout_multiplier: u64,
    coverage: bool,
    executor: &E,
    writer: &mut W,
) -> std::result::Result<RunSummary, RunFailure>
where
    E: CommandExecutor,
    W: Write,
{
    if let Err(message) = prebuild_test_binary(coverage, executor) {
        return Err(RunFailure {
            summary: empty_summary(vec![bucket_name.to_string()]),
            message,
        });
    }

    match execute_bucket(
        manifest,
        bucket_name,
        timeout_multiplier,
        coverage,
        executor,
        writer,
    ) {
        Ok(bucket_result) => Ok(RunSummary {
            bucket_names: vec![bucket_name.to_string()],
            passed_buckets: usize::from(bucket_result.status == BucketStatus::Passed),
            total_elapsed: bucket_result.elapsed,
            bucket_results: vec![bucket_result],
        }),
        Err(error) => Err(RunFailure {
            summary: RunSummary {
                bucket_names: vec![bucket_name.to_string()],
                passed_buckets: 0,
                total_elapsed: error.result.elapsed,
                bucket_results: vec![error.result],
            },
            message: error.message,
        }),
    }
}

pub fn render_summary(summary: &RunSummary) -> String {
    format!(
        "SUMMARY: {} buckets passed in {}\n",
        summary.passed_buckets,
        format_duration(summary.total_elapsed)
    )
}

pub fn render_bucket_result(result: &BucketResult) -> String {
    format!(
        "START {}\nEND {} {} ({})\n",
        result.bucket_name,
        result.bucket_name,
        status_label(result.status),
        format_duration(result.elapsed)
    )
}

fn prebuild_test_binary<E: CommandExecutor>(coverage: bool, executor: &E) -> Result<(), String> {
    const COMMAND: &str = "cargo nextest run --no-run --lib";
    const TIMEOUT_SECS: u64 = 600;

    let command = if coverage {
        transform_command_for_coverage(COMMAND)
    } else {
        COMMAND.to_string()
    };

    let result = executor
        .run("prebuild", &command, Duration::from_secs(TIMEOUT_SECS))
        .map_err(|e| format!("prebuild failed to launch: {e}"))?;

    match result.outcome {
        CommandOutcome::Passed { .. } => Ok(()),
        CommandOutcome::Failed { exit_code, .. } => Err(format!(
            "prebuild `{command}` failed (exit code: {})",
            exit_code.map_or_else(|| "unknown".to_string(), |c| c.to_string())
        )),
        CommandOutcome::TimedOut { .. } => Err(format!(
            "prebuild `{command}` timed out after {TIMEOUT_SECS}s"
        )),
    }
}

fn execute_bucket<E, W>(
    manifest: &TestManifest,
    bucket_name: &str,
    timeout_multiplier: u64,
    coverage: bool,
    executor: &E,
    writer: &mut W,
) -> std::result::Result<BucketResult, BucketFailure>
where
    E: CommandExecutor,
    W: Write,
{
    let bucket = resolve_bucket_plan(manifest, bucket_name).ok_or_else(|| BucketFailure {
        result: BucketResult {
            bucket_name: bucket_name.to_string(),
            status: BucketStatus::Failed,
            elapsed: Duration::ZERO,
            command_count: 0,
        },
        message: format!("unknown test bucket `{bucket_name}`"),
    })?;
    let timeout = bucket_timeout(&bucket, timeout_multiplier).map_err(|error| BucketFailure {
        result: build_bucket_result(bucket_name, BucketStatus::Failed, Duration::ZERO, &bucket),
        message: error.to_string(),
    })?;
    let bucket_started = Instant::now();
    let mut elapsed = Duration::ZERO;

    writer
        .write_all(format!("START {bucket_name}\n").as_bytes())
        .map_err(|error| BucketFailure {
            result: build_bucket_result(bucket_name, BucketStatus::Failed, elapsed, &bucket),
            message: error.to_string(),
        })?;

    for raw_command in &bucket.commands {
        let command = if coverage {
            &transform_command_for_coverage(raw_command)
        } else {
            raw_command
        };
        let remaining_timeout = timeout.checked_sub(elapsed).unwrap_or(Duration::ZERO);
        if remaining_timeout.is_zero() {
            let result = build_bucket_result(bucket_name, BucketStatus::TimedOut, elapsed, &bucket);
            write_bucket_end(writer, &result)?;
            return Err(BucketFailure {
                result,
                message: timeout_error(bucket_name, &bucket, timeout, command),
            });
        }

        let CommandResult { outcome, captured } = executor
            .run(bucket_name, command, remaining_timeout)
            .map_err(|error| BucketFailure {
                result: build_bucket_result(bucket_name, BucketStatus::Failed, elapsed, &bucket),
                message: error.to_string(),
            })?;

        match outcome {
            CommandOutcome::Passed {
                elapsed: command_elapsed,
            } => {
                elapsed = bucket_started.elapsed().max(elapsed + command_elapsed);
                if elapsed >= timeout {
                    write_captured_on_failure(writer, &captured)?;
                    let result =
                        build_bucket_result(bucket_name, BucketStatus::TimedOut, elapsed, &bucket);
                    write_bucket_end(writer, &result)?;
                    return Err(BucketFailure {
                        result,
                        message: timeout_error(bucket_name, &bucket, timeout, command),
                    });
                }
            }
            CommandOutcome::Failed {
                elapsed: command_elapsed,
                exit_code,
            } => {
                elapsed = bucket_started.elapsed().max(elapsed + command_elapsed);
                write_captured_on_failure(writer, &captured)?;
                let result =
                    build_bucket_result(bucket_name, BucketStatus::Failed, elapsed, &bucket);
                write_bucket_end(writer, &result)?;
                return Err(BucketFailure {
                    result,
                    message: format!(
                        "bucket `{bucket_name}` failed after {} running `{command}` (exit code: {})",
                        format_duration(elapsed),
                        exit_code.map_or_else(|| "unknown".to_string(), |code| code.to_string())
                    ),
                });
            }
            CommandOutcome::TimedOut {
                elapsed: command_elapsed,
            } => {
                elapsed = bucket_started.elapsed().max(elapsed + command_elapsed);
                write_captured_on_failure(writer, &captured)?;
                let result =
                    build_bucket_result(bucket_name, BucketStatus::TimedOut, elapsed, &bucket);
                write_bucket_end(writer, &result)?;
                return Err(BucketFailure {
                    result,
                    message: timeout_error(bucket_name, &bucket, timeout, command),
                });
            }
        }
    }

    let result = build_bucket_result(bucket_name, BucketStatus::Passed, elapsed, &bucket);
    write_bucket_end(writer, &result)?;
    Ok(result)
}

fn build_bucket_result(
    bucket_name: &str,
    status: BucketStatus,
    elapsed: Duration,
    bucket: &BucketPlan,
) -> BucketResult {
    BucketResult {
        bucket_name: bucket_name.to_string(),
        status,
        elapsed,
        command_count: bucket.commands.len(),
    }
}

fn write_captured_on_failure<W: Write>(
    writer: &mut W,
    captured: &str,
) -> std::result::Result<(), BucketFailure> {
    if captured.is_empty() {
        return Ok(());
    }
    writer
        .write_all(captured.as_bytes())
        .map_err(|error| BucketFailure {
            result: BucketResult {
                bucket_name: String::new(),
                status: BucketStatus::Failed,
                elapsed: Duration::ZERO,
                command_count: 0,
            },
            message: error.to_string(),
        })?;
    if !captured.ends_with('\n') {
        writer.write_all(b"\n").map_err(|error| BucketFailure {
            result: BucketResult {
                bucket_name: String::new(),
                status: BucketStatus::Failed,
                elapsed: Duration::ZERO,
                command_count: 0,
            },
            message: error.to_string(),
        })?;
    }
    Ok(())
}

fn write_bucket_end<W: Write>(
    writer: &mut W,
    result: &BucketResult,
) -> std::result::Result<(), BucketFailure> {
    writer
        .write_all(render_bucket_end(result).as_bytes())
        .map_err(|error| BucketFailure {
            result: result.clone(),
            message: error.to_string(),
        })
}

fn empty_summary(bucket_names: Vec<String>) -> RunSummary {
    RunSummary {
        bucket_names,
        bucket_results: Vec::new(),
        passed_buckets: 0,
        total_elapsed: Duration::ZERO,
    }
}

fn bucket_timeout(bucket: &BucketPlan, timeout_multiplier: u64) -> Result<Duration> {
    let seconds = bucket
        .timeout_seconds
        .checked_mul(timeout_multiplier)
        .ok_or_else(|| anyhow!("timeout multiplier overflowed bucket timeout"))?;
    Ok(Duration::from_secs(seconds))
}

fn timeout_error(
    bucket_name: &str,
    bucket: &BucketPlan,
    timeout: Duration,
    command: &str,
) -> String {
    format!(
        "bucket `{bucket_name}` timed out after {}s (expected {}s) while running `{command}`",
        timeout.as_secs(),
        bucket.expected_seconds
    )
}

fn render_bucket_end(result: &BucketResult) -> String {
    format!(
        "END {} {} ({})\n",
        result.bucket_name,
        status_label(result.status),
        format_duration(result.elapsed)
    )
}

fn status_label(status: BucketStatus) -> &'static str {
    match status {
        BucketStatus::Passed => "PASS",
        BucketStatus::Failed => "FAIL",
        BucketStatus::TimedOut => "TIMEOUT",
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
}

fn resolve_bucket_plan(manifest: &TestManifest, bucket_name: &str) -> Option<BucketPlan> {
    manifest
        .buckets
        .get(bucket_name)
        .map(|bucket| BucketPlan {
            expected_seconds: bucket.expected_seconds,
            timeout_seconds: bucket.timeout_seconds,
            commands: bucket.commands.clone(),
        })
        .or_else(|| special_bucket_plan(bucket_name))
}

fn special_tier_bucket_names(tier_name: &str) -> Option<Vec<String>> {
    PROGRAM_TIERS
        .iter()
        .find(|(name, _)| *name == tier_name)
        .map(|(_, buckets)| buckets.iter().map(|bucket| (*bucket).to_string()).collect())
}

fn special_bucket_plan(bucket_name: &str) -> Option<BucketPlan> {
    SPECIAL_BUCKETS
        .iter()
        .find(|(name, _, _, _)| *name == bucket_name)
        .map(
            |(_, expected_seconds, timeout_seconds, commands)| BucketPlan {
                expected_seconds: *expected_seconds,
                timeout_seconds: *timeout_seconds,
                commands: commands
                    .iter()
                    .map(|command| (*command).to_string())
                    .collect(),
            },
        )
}

#[cfg(unix)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    shell
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut shell = Command::new("cmd");
    shell.arg("/C").arg(command);
    shell
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::rc::Rc;

    struct FakeExecutor {
        expectations: Rc<RefCell<HashMap<String, VecDeque<CommandOutcome>>>>,
        calls: Rc<RefCell<Vec<(String, String, Duration)>>>,
    }

    impl FakeExecutor {
        fn with_outcomes(entries: &[(&str, CommandOutcome)]) -> Self {
            let mut expectations = HashMap::new();
            for (command, outcome) in entries {
                expectations
                    .entry((*command).to_string())
                    .or_insert_with(VecDeque::new)
                    .push_back(outcome.clone());
            }

            Self {
                expectations: Rc::new(RefCell::new(expectations)),
                calls: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|(_, command, _)| command.clone())
                .collect()
        }
    }

    impl CommandExecutor for FakeExecutor {
        fn run(&self, bucket: &str, command: &str, timeout: Duration) -> Result<CommandResult> {
            self.calls
                .borrow_mut()
                .push((bucket.to_string(), command.to_string(), timeout));

            let outcome = self
                .expectations
                .borrow_mut()
                .get_mut(command)
                .and_then(|queue| queue.pop_front())
                .unwrap_or_else(|| panic!("unexpected command: {command}"));

            Ok(CommandResult {
                outcome,
                captured: String::new(),
            })
        }
    }

    fn manifest_with_program_buckets() -> TestManifest {
        TestManifest::from_str(
            r#"
[tiers]
daemon = ["daemon"]
workspace-init = ["workspace-init"]
integration = ["integration"]

[buckets.daemon]
expected_seconds = 1
timeout_seconds = 2
commands = ["daemon cmd"]

[buckets.workspace-init]
expected_seconds = 1
timeout_seconds = 2
commands = ["workspace init cmd"]

[buckets.integration]
expected_seconds = 1
timeout_seconds = 2
commands = ["integration cmd"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn runner_tests_render_manifest_listing_includes_program_tiers() {
        let manifest = manifest_with_program_buckets();

        let listing = render_manifest_listing(&manifest);

        assert!(listing.contains("PROGRAM TIERS"), "{listing}");
        assert!(listing.contains("reliability"), "{listing}");
        assert!(listing.contains("benchmark"), "{listing}");
        assert!(listing.contains("system-health"), "{listing}");
    }

    #[test]
    fn runner_tests_reliability_tier_routes_program_bucket_sequence() {
        let manifest = manifest_with_program_buckets();
        let executor = FakeExecutor::with_outcomes(&[
            (
                "cargo nextest run --no-run --lib",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(5),
                },
            ),
            (
                "daemon cmd",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(10),
                },
            ),
            (
                "workspace init cmd",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(15),
                },
            ),
            (
                "integration cmd",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(20),
                },
            ),
        ]);
        let mut output = Vec::new();

        let summary = run_tier(&manifest, "reliability", 1, false, &executor, &mut output).unwrap();

        assert_eq!(
            summary.bucket_names,
            vec![
                "daemon".to_string(),
                "workspace-init".to_string(),
                "integration".to_string(),
            ]
        );
        assert_eq!(
            executor.calls(),
            vec![
                "cargo nextest run --no-run --lib".to_string(),
                "daemon cmd".to_string(),
                "workspace init cmd".to_string(),
                "integration cmd".to_string(),
            ]
        );
    }

    #[test]
    fn runner_tests_benchmark_bucket_runs_system_health_command() {
        let manifest = manifest_with_program_buckets();
        let executor = FakeExecutor::with_outcomes(&[
            (
                "cargo nextest run --no-run --lib",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(5),
                },
            ),
            (
                "cargo nextest run --lib tests::integration::system_health",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(25),
                },
            ),
        ]);
        let mut output = Vec::new();

        let summary =
            run_bucket(&manifest, "system-health", 1, false, &executor, &mut output).unwrap();

        assert_eq!(summary.bucket_names, vec!["system-health".to_string()]);
        assert_eq!(
            executor.calls(),
            vec![
                "cargo nextest run --no-run --lib".to_string(),
                "cargo nextest run --lib tests::integration::system_health".to_string(),
            ]
        );
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("END system-health PASS")
        );
    }
}
