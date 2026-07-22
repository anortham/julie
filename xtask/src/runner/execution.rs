use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};

use crate::manifest::TestManifest;
use crate::process::manifest_command;

use super::prebuild::transform_command_for_coverage;
use super::rendering::{format_duration, render_bucket_end, status_label};
use super::{
    BucketPlan, BucketResult, BucketStatus, CommandExecutor, CommandOutcome, CommandResult,
    resolve_bucket_plan,
};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

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

pub(super) fn execute_bucket<E, W>(
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BucketFailure {
    pub(super) result: BucketResult,
    pub(super) message: String,
}
