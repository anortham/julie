use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::cli::Tier;
use crate::process::shell_command;

const DEV: &[&str] = &[
    "cargo build -p julie --bin julie-server",
    "cargo nextest run --workspace -E 'not (test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/))'",
    "cargo nextest run --lib -p julie --run-ignored only -E 'test(/tests::cli::/)'",
];

const DOGFOOD: &[&str] = &[
    "cargo test --lib -p julie ensure_julie_fixture -- --ignored --nocapture",
    "cargo nextest run --workspace -E 'test(/search_quality/) | test(/fixtures::julie_db/) | test(/dogfood/)'",
];

/// Runs one shell command and reports how it ended.
pub trait CommandExecutor {
    fn run(&self, command: &str) -> Result<CommandOutcome>;
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
}

impl CommandOutcome {
    fn elapsed(&self) -> Duration {
        match self {
            CommandOutcome::Passed { elapsed } | CommandOutcome::Failed { elapsed, .. } => *elapsed,
        }
    }
}

/// Runs each command as a child process that inherits stdout and stderr.
pub struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
    fn run(&self, command: &str) -> Result<CommandOutcome> {
        let start = Instant::now();
        let status = Command::status(&mut shell_command(command)?)?;
        let elapsed = start.elapsed();
        Ok(if status.success() {
            CommandOutcome::Passed { elapsed }
        } else {
            CommandOutcome::Failed {
                elapsed,
                exit_code: status.code(),
            }
        })
    }
}

/// The commands a tier runs, in order.
pub fn tier_commands(tier: Tier) -> Vec<&'static str> {
    match tier {
        Tier::Dev => DEV.to_vec(),
        Tier::Dogfood => DOGFOOD.to_vec(),
        Tier::Full => DEV.iter().chain(DOGFOOD).copied().collect(),
    }
}

/// Runs the tier's commands in order and stops at the first failure.
pub fn run_tier<E: CommandExecutor, W: Write>(
    tier: Tier,
    executor: &E,
    writer: &mut W,
) -> Result<()> {
    let commands = tier_commands(tier);
    let mut passed = 0;
    let mut total = Duration::ZERO;
    let mut failure = None;

    for command in &commands {
        writeln!(writer, "RUN {command}")?;
        let outcome = executor.run(command)?;
        total += outcome.elapsed();
        let seconds = outcome.elapsed().as_secs_f64();
        match outcome {
            CommandOutcome::Passed { .. } => {
                passed += 1;
                writeln!(writer, "PASS {command} ({seconds:.1}s)")?;
            }
            CommandOutcome::Failed { exit_code, .. } => {
                writeln!(writer, "FAIL {command} ({seconds:.1}s)")?;
                failure = Some(format!(
                    "FAIL {command} (exit code {})",
                    exit_code.map_or("signal".to_string(), |code| code.to_string())
                ));
                break;
            }
        }
    }

    writeln!(
        writer,
        "SUMMARY: {} {passed}/{} commands in {:.1}s",
        tier.name(),
        commands.len(),
        total.as_secs_f64()
    )?;

    match failure {
        Some(message) => bail!(message),
        None => Ok(()),
    }
}
