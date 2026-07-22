use std::fmt;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::manifest::TestManifest;
use crate::process::manifest_command;

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
    pub expected_seconds: u64,
    pub scope_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub bucket_names: Vec<String>,
    pub bucket_results: Vec<BucketResult>,
    pub passed_buckets: usize,
    /// Warm wall: sum of bucket execution times only (excludes prebuild).
    pub total_elapsed: Duration,
    /// Time spent prebuilding selected Rust test targets before bucket execution.
    pub prebuild_elapsed: Duration,
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
pub struct BucketPlan {
    pub expected_seconds: u64,
    pub timeout_seconds: u64,
    pub scope_label: String,
    pub commands: Vec<String>,
}

const PROGRAM_TIERS: &[(&str, &[&str])] = &[
    (
        "reliability",
        &["registry", "workspace-init", "integration"],
    ),
    ("benchmark", &["system-health"]),
];

pub struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
    fn run(&self, _bucket: &str, command: &str, timeout: Duration) -> Result<CommandResult> {
        let start = Instant::now();
        let deadline = start + timeout;
        let mut process = manifest_command(command)?;
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
    if let Some(rest) = strip_exact_command_prefix(command, "cargo test") {
        format!("cargo llvm-cov --no-report test{rest}")
    } else if let Some(rest) = strip_exact_command_prefix(command, "cargo nextest run") {
        format!("cargo llvm-cov --no-report nextest{rest}")
    } else {
        command.to_string()
    }
}

fn strip_exact_command_prefix<'a>(command: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = command.strip_prefix(prefix)?;
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        Some(rest)
    } else {
        None
    }
}

pub fn render_manifest_listing(manifest: &TestManifest) -> String {
    let mut output = String::from("TIERS\n");
    for (tier_name, buckets) in &manifest.tiers {
        output.push_str(&format!("- {tier_name}: {}\n", buckets.join(", ")));
    }

    output.push_str("\nBUCKETS\n");
    for (bucket_name, bucket) in &manifest.buckets {
        let expensive_marker = if bucket.expensive { " [expensive]" } else { "" };
        output.push_str(&format!("- {bucket_name}{expensive_marker}\n"));
        output.push_str(&format!(
            "  expected_seconds = {}\n",
            bucket.expected_seconds
        ));
        output.push_str(&format!("  timeout_seconds = {}\n", bucket.timeout_seconds));
        output.push_str(&format!("  scope_label = {}\n", bucket.scope_label));
        output.push_str(&format!("  owner = {}\n", bucket.owner));
        output.push_str(&format!("  expensive = {}\n", bucket.expensive));
        if let Some(notes) = &bucket.notes {
            output.push_str(&format!("  notes = {notes}\n"));
        }
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
            summary: empty_summary(Vec::new(), Duration::ZERO),
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

    let prebuild_elapsed = match prebuild_test_binary(manifest, &bucket_names, coverage, executor) {
        Ok(elapsed) => elapsed,
        Err(error) => {
            return Err(RunFailure {
                summary: empty_summary(bucket_names, error.elapsed),
                message: error.message,
            });
        }
    };

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
                        prebuild_elapsed,
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
        prebuild_elapsed,
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
    let bucket_names = vec![bucket_name.to_string()];
    let prebuild_elapsed = match prebuild_test_binary(manifest, &bucket_names, coverage, executor) {
        Ok(elapsed) => elapsed,
        Err(error) => {
            return Err(RunFailure {
                summary: empty_summary(bucket_names, error.elapsed),
                message: error.message,
            });
        }
    };

    match execute_bucket(
        manifest,
        bucket_name,
        timeout_multiplier,
        coverage,
        executor,
        writer,
    ) {
        Ok(bucket_result) => Ok(RunSummary {
            bucket_names,
            passed_buckets: usize::from(bucket_result.status == BucketStatus::Passed),
            total_elapsed: bucket_result.elapsed,
            prebuild_elapsed,
            bucket_results: vec![bucket_result],
        }),
        Err(error) => Err(RunFailure {
            summary: RunSummary {
                bucket_names,
                passed_buckets: 0,
                total_elapsed: error.result.elapsed,
                prebuild_elapsed,
                bucket_results: vec![error.result],
            },
            message: error.message,
        }),
    }
}

pub fn render_summary(summary: &RunSummary) -> String {
    let cold_wall = summary.prebuild_elapsed + summary.total_elapsed;
    let mut output = format!(
        "SUMMARY: {} buckets passed in {} (warm)\nPREBUILD: {}\nCOLD WALL: {}\n",
        summary.passed_buckets,
        format_duration(summary.total_elapsed),
        format_duration(summary.prebuild_elapsed),
        format_duration(cold_wall),
    );

    for result in &summary.bucket_results {
        let slow_marker = if bucket_is_slow(result) { " SLOW" } else { "" };
        output.push_str(&format!(
            "- {}: expected {}, actual {}, commands {}, scope {}{}\n",
            result.bucket_name,
            format_duration(Duration::from_secs(result.expected_seconds)),
            format_duration(result.elapsed),
            result.command_count,
            result.scope_label,
            slow_marker
        ));
    }

    output
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

fn prebuild_test_binary<E: CommandExecutor>(
    manifest: &TestManifest,
    bucket_names: &[String],
    coverage: bool,
    executor: &E,
) -> std::result::Result<Duration, PrebuildError> {
    const TIMEOUT_SECS: u64 = 600;
    let mut total_elapsed = Duration::ZERO;

    for raw_command in selected_prebuild_commands(manifest, bucket_names) {
        let command = if coverage {
            transform_command_for_coverage(&raw_command)
        } else {
            raw_command
        };

        let result = executor
            .run("prebuild", &command, Duration::from_secs(TIMEOUT_SECS))
            .map_err(|e| PrebuildError {
                elapsed: total_elapsed,
                message: format!("prebuild `{command}` failed to launch: {e}"),
            })?;

        total_elapsed += command_outcome_elapsed(&result.outcome);
        match result.outcome {
            CommandOutcome::Passed { .. } => {}
            CommandOutcome::Failed { exit_code, .. } => {
                return Err(PrebuildError {
                    elapsed: total_elapsed,
                    message: format!(
                        "prebuild `{command}` failed (exit code: {})",
                        exit_code.map_or_else(|| "unknown".to_string(), |c| c.to_string())
                    ),
                });
            }
            CommandOutcome::TimedOut { .. } => {
                return Err(PrebuildError {
                    elapsed: total_elapsed,
                    message: format!("prebuild `{command}` timed out after {TIMEOUT_SECS}s"),
                });
            }
        }
    }

    Ok(total_elapsed)
}

fn selected_prebuild_commands(manifest: &TestManifest, bucket_names: &[String]) -> Vec<String> {
    let mut commands = Vec::new();
    for bucket_name in bucket_names {
        let Some(bucket) = manifest.buckets.get(bucket_name) else {
            continue;
        };
        for command in &bucket.commands {
            let Some(prebuild) = derive_prebuild_command(command) else {
                continue;
            };
            if !commands.contains(&prebuild) {
                commands.push(prebuild);
            }
        }
    }
    commands
}

fn derive_prebuild_command(command: &str) -> Option<String> {
    let (prefix, rest) = if let Some(rest) = strip_exact_command_prefix(command, "cargo test") {
        ("cargo test", rest)
    } else if let Some(rest) = strip_exact_command_prefix(command, "cargo nextest run") {
        ("cargo nextest run", rest)
    } else {
        return None;
    };

    let mut selected_arguments = Vec::new();
    let mut arguments = rest.split_whitespace();
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            break;
        }
        if is_prebuild_flag(argument) || is_inline_prebuild_option(argument) {
            selected_arguments.push(argument);
        } else if is_prebuild_option(argument) {
            selected_arguments.push(argument);
            if let Some(value) = arguments.next() {
                selected_arguments.push(value);
            }
        }
    }

    let mut prebuild = format!("{prefix} --no-run");
    if !selected_arguments.is_empty() {
        prebuild.push(' ');
        prebuild.push_str(&selected_arguments.join(" "));
    }
    Some(prebuild)
}

fn is_prebuild_flag(argument: &str) -> bool {
    matches!(
        argument,
        "--workspace"
            | "--all"
            | "--lib"
            | "--bins"
            | "--examples"
            | "--tests"
            | "--benches"
            | "--all-targets"
            | "--doc"
            | "--all-features"
            | "--no-default-features"
            | "--release"
            | "--locked"
            | "--frozen"
            | "--offline"
    )
}

fn is_prebuild_option(argument: &str) -> bool {
    matches!(
        argument,
        "-p" | "--package"
            | "--exclude"
            | "--bin"
            | "--example"
            | "--test"
            | "--bench"
            | "-F"
            | "--features"
            | "--target"
            | "--target-dir"
            | "--manifest-path"
            | "--profile"
    )
}

fn is_inline_prebuild_option(argument: &str) -> bool {
    [
        "--package=",
        "--exclude=",
        "--bin=",
        "--example=",
        "--test=",
        "--bench=",
        "--features=",
        "--target=",
        "--target-dir=",
        "--manifest-path=",
        "--profile=",
    ]
    .iter()
    .any(|prefix| argument.starts_with(prefix))
        || (argument.starts_with("-p") && argument.len() > 2)
        || (argument.starts_with("-F") && argument.len() > 2)
}

fn command_outcome_elapsed(outcome: &CommandOutcome) -> Duration {
    match outcome {
        CommandOutcome::Passed { elapsed }
        | CommandOutcome::Failed { elapsed, .. }
        | CommandOutcome::TimedOut { elapsed } => *elapsed,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrebuildError {
    elapsed: Duration,
    message: String,
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
            expected_seconds: 0,
            scope_label: "bucket".to_string(),
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

    for (command_index, raw_command) in bucket.commands.iter().enumerate() {
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
                write_command_timing(
                    writer,
                    bucket_name,
                    command_index + 1,
                    bucket.commands.len(),
                    BucketStatus::Passed,
                    command_elapsed,
                    command,
                )
                .map_err(|error| BucketFailure {
                    result: build_bucket_result(
                        bucket_name,
                        BucketStatus::Failed,
                        elapsed,
                        &bucket,
                    ),
                    message: error.to_string(),
                })?;
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
                write_command_timing(
                    writer,
                    bucket_name,
                    command_index + 1,
                    bucket.commands.len(),
                    BucketStatus::Failed,
                    command_elapsed,
                    command,
                )
                .map_err(|error| BucketFailure {
                    result: build_bucket_result(
                        bucket_name,
                        BucketStatus::Failed,
                        elapsed,
                        &bucket,
                    ),
                    message: error.to_string(),
                })?;
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
                write_command_timing(
                    writer,
                    bucket_name,
                    command_index + 1,
                    bucket.commands.len(),
                    BucketStatus::TimedOut,
                    command_elapsed,
                    command,
                )
                .map_err(|error| BucketFailure {
                    result: build_bucket_result(
                        bucket_name,
                        BucketStatus::Failed,
                        elapsed,
                        &bucket,
                    ),
                    message: error.to_string(),
                })?;
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

fn write_command_timing<W: Write>(
    writer: &mut W,
    bucket_name: &str,
    command_index: usize,
    command_count: usize,
    status: BucketStatus,
    elapsed: Duration,
    command: &str,
) -> std::io::Result<()> {
    writer.write_all(
        format!(
            "COMMAND {bucket_name} {command_index}/{command_count} {} ({}) {command}\n",
            status_label(status),
            format_duration(elapsed)
        )
        .as_bytes(),
    )
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
        expected_seconds: bucket.expected_seconds,
        scope_label: bucket.scope_label.clone(),
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
                expected_seconds: 0,
                scope_label: "bucket".to_string(),
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
                expected_seconds: 0,
                scope_label: "bucket".to_string(),
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

fn empty_summary(bucket_names: Vec<String>, prebuild_elapsed: Duration) -> RunSummary {
    RunSummary {
        bucket_names,
        bucket_results: Vec::new(),
        passed_buckets: 0,
        total_elapsed: Duration::ZERO,
        prebuild_elapsed,
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

fn bucket_is_slow(result: &BucketResult) -> bool {
    if result.expected_seconds == 0 {
        return false;
    }

    result.elapsed.as_secs_f64() > (result.expected_seconds as f64 * 1.5)
}

/// Resolve a bucket's declared plan from the manifest.
///
/// Shared by the runner and changed-budget pricing so every named bucket —
/// including `system-health` — has a single source of truth for expected seconds.
pub fn resolve_bucket_plan(manifest: &TestManifest, bucket_name: &str) -> Option<BucketPlan> {
    manifest.buckets.get(bucket_name).map(|bucket| BucketPlan {
        expected_seconds: bucket.expected_seconds,
        timeout_seconds: bucket.timeout_seconds,
        scope_label: bucket.scope_label.clone(),
        commands: bucket.commands.clone(),
    })
}

/// Sum declared `expected_seconds` for a selection via [`resolve_bucket_plan`].
///
/// Unknown names contribute nothing; callers that need OverBudget gating (Task 4)
/// should use this same path so specials cannot be priced as free/zero.
pub fn declared_expected_seconds<'a>(
    manifest: &TestManifest,
    bucket_names: impl IntoIterator<Item = &'a str>,
) -> u64 {
    bucket_names
        .into_iter()
        .filter_map(|name| resolve_bucket_plan(manifest, name).map(|plan| plan.expected_seconds))
        .sum()
}

fn special_tier_bucket_names(tier_name: &str) -> Option<Vec<String>> {
    PROGRAM_TIERS
        .iter()
        .find(|(name, _)| *name == tier_name)
        .map(|(_, buckets)| buckets.iter().map(|bucket| (*bucket).to_string()).collect())
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
fast = ["registry"]
daemon = ["registry"]
workspace-init = ["workspace-init"]
integration = ["integration"]

[buckets.registry]
expected_seconds = 1
timeout_seconds = 2
commands = ["registry cmd"]

[buckets.workspace-init]
expected_seconds = 1
timeout_seconds = 2
commands = ["workspace init cmd"]

[buckets.integration]
expected_seconds = 1
timeout_seconds = 2
commands = ["integration cmd"]

[buckets.system-health]
expected_seconds = 30
timeout_seconds = 120
commands = ["cargo nextest run --lib tests::integration::system_health"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn runner_tests_declared_expected_seconds_prices_system_health_from_manifest() {
        let manifest = TestManifest::load(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_tiers.toml"),
        )
        .expect("load checked-in test_tiers.toml");

        assert!(
            manifest.buckets.contains_key("system-health"),
            "system-health must live in the manifest so pricing is uniform"
        );

        assert_eq!(
            declared_expected_seconds(&manifest, ["system-health"]),
            30,
            "system-health declared expected_seconds must price at 30s"
        );

        let plan = resolve_bucket_plan(&manifest, "system-health")
            .expect("system-health resolves via shared bucket-plan path");
        assert_eq!(plan.expected_seconds, 30);
        assert_eq!(plan.timeout_seconds, 120);
        assert_eq!(
            plan.commands,
            vec!["cargo nextest run --lib tests::integration::system_health".to_string()]
        );
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
                "registry cmd",
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
                "registry".to_string(),
                "workspace-init".to_string(),
                "integration".to_string(),
            ]
        );
        assert_eq!(
            executor.calls(),
            vec![
                "registry cmd".to_string(),
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

    #[test]
    fn runner_tests_summary_includes_prebuild_elapsed() {
        let manifest = manifest_with_program_buckets();
        let executor = FakeExecutor::with_outcomes(&[
            (
                "cargo nextest run --no-run --lib",
                CommandOutcome::Passed {
                    elapsed: Duration::from_millis(47_000),
                },
            ),
            (
                "cargo nextest run --lib tests::integration::system_health",
                CommandOutcome::Passed {
                    elapsed: Duration::from_secs(1),
                },
            ),
        ]);
        let mut output = Vec::new();

        let summary = run_tier(&manifest, "benchmark", 1, false, &executor, &mut output).unwrap();

        assert_eq!(
            summary.prebuild_elapsed,
            Duration::from_millis(47_000),
            "prebuild_elapsed should record the prebuild command duration"
        );
        assert_eq!(
            summary.total_elapsed,
            Duration::from_secs(1),
            "total_elapsed must remain warm bucket wall, not include prebuild"
        );

        let rendered = render_summary(&summary);
        assert!(
            rendered.contains("PREBUILD: 47.0s"),
            "rendered summary should show prebuild duration, got:\n{rendered}"
        );
        assert!(
            rendered.contains("warm"),
            "rendered summary should label warm bucket wall, got:\n{rendered}"
        );
        assert!(
            rendered.contains("COLD WALL: 48.0s"),
            "cold wall should be prebuild + warm buckets, got:\n{rendered}"
        );
    }
}
