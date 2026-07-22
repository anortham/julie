use std::time::Duration;

use crate::manifest::TestManifest;

use super::{CommandExecutor, CommandOutcome};

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

pub(super) fn prebuild_test_binary<E: CommandExecutor>(
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
pub(super) struct PrebuildError {
    pub(super) elapsed: Duration,
    pub(super) message: String,
}
