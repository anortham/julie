use std::fmt;
use std::io::Write;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::manifest::{BucketConfig, TestManifest};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

pub trait CommandExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> Result<CommandOutcome>;
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

pub struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
    fn run(&self, _bucket: &str, command: &str, timeout: Duration) -> Result<CommandOutcome> {
        let start = Instant::now();
        let deadline = start + timeout;
        let mut process = shell_command(command);
        configure_command_for_termination(&mut process);
        let mut child = process.spawn()?;

        loop {
            if let Some(status) = child.try_wait()? {
                let elapsed = start.elapsed();
                return Ok(if status.success() {
                    CommandOutcome::Passed { elapsed }
                } else {
                    CommandOutcome::Failed {
                        elapsed,
                        exit_code: status.code(),
                    }
                });
            }

            let now = Instant::now();
            if now >= deadline {
                let _ = terminate_child_on_timeout(&mut child);
                return Ok(CommandOutcome::TimedOut {
                    elapsed: start.elapsed(),
                });
            }

            let remaining = deadline.saturating_duration_since(now);
            thread::sleep(remaining.min(Duration::from_millis(10)));
        }
    }
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
        .ok_or_else(|| RunFailure {
            summary: empty_summary(Vec::new()),
            message: format!("unknown test tier `{tier_name}`"),
        })?;

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
    let bucket = manifest
        .buckets
        .get(bucket_name)
        .ok_or_else(|| BucketFailure {
            result: BucketResult {
                bucket_name: bucket_name.to_string(),
                status: BucketStatus::Failed,
                elapsed: Duration::ZERO,
                command_count: 0,
            },
            message: format!("unknown test bucket `{bucket_name}`"),
        })?;
    let timeout = bucket_timeout(bucket, timeout_multiplier).map_err(|error| BucketFailure {
        result: build_bucket_result(bucket_name, BucketStatus::Failed, Duration::ZERO, bucket),
        message: error.to_string(),
    })?;
    let bucket_started = Instant::now();
    let mut elapsed = Duration::ZERO;

    writer
        .write_all(format!("START {bucket_name}\n").as_bytes())
        .map_err(|error| BucketFailure {
            result: build_bucket_result(bucket_name, BucketStatus::Failed, elapsed, bucket),
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
            let result = build_bucket_result(bucket_name, BucketStatus::TimedOut, elapsed, bucket);
            write_bucket_end(writer, &result)?;
            return Err(BucketFailure {
                result,
                message: timeout_error(bucket_name, bucket, timeout, command),
            });
        }

        let outcome = executor
            .run(bucket_name, command, remaining_timeout)
            .map_err(|error| BucketFailure {
                result: build_bucket_result(bucket_name, BucketStatus::Failed, elapsed, bucket),
                message: error.to_string(),
            })?;

        match outcome {
            CommandOutcome::Passed {
                elapsed: command_elapsed,
            } => {
                elapsed = bucket_started.elapsed().max(elapsed + command_elapsed);
                if elapsed >= timeout {
                    let result =
                        build_bucket_result(bucket_name, BucketStatus::TimedOut, elapsed, bucket);
                    write_bucket_end(writer, &result)?;
                    return Err(BucketFailure {
                        result,
                        message: timeout_error(bucket_name, bucket, timeout, command),
                    });
                }
            }
            CommandOutcome::Failed {
                elapsed: command_elapsed,
                exit_code,
            } => {
                elapsed = bucket_started.elapsed().max(elapsed + command_elapsed);
                let result =
                    build_bucket_result(bucket_name, BucketStatus::Failed, elapsed, bucket);
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
                let result =
                    build_bucket_result(bucket_name, BucketStatus::TimedOut, elapsed, bucket);
                write_bucket_end(writer, &result)?;
                return Err(BucketFailure {
                    result,
                    message: timeout_error(bucket_name, bucket, timeout, command),
                });
            }
        }
    }

    let result = build_bucket_result(bucket_name, BucketStatus::Passed, elapsed, bucket);
    write_bucket_end(writer, &result)?;
    Ok(result)
}

fn build_bucket_result(
    bucket_name: &str,
    status: BucketStatus,
    elapsed: Duration,
    bucket: &BucketConfig,
) -> BucketResult {
    BucketResult {
        bucket_name: bucket_name.to_string(),
        status,
        elapsed,
        command_count: bucket.commands.len(),
    }
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

fn bucket_timeout(bucket: &BucketConfig, timeout_multiplier: u64) -> Result<Duration> {
    let seconds = bucket
        .timeout_seconds
        .checked_mul(timeout_multiplier)
        .ok_or_else(|| anyhow!("timeout multiplier overflowed bucket timeout"))?;
    Ok(Duration::from_secs(seconds))
}

fn timeout_error(
    bucket_name: &str,
    bucket: &BucketConfig,
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
