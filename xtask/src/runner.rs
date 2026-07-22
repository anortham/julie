mod execution;
mod prebuild;
mod rendering;

#[cfg(test)]
mod tests;

use std::fmt;
use std::io::Write;
use std::time::Duration;

use crate::manifest::TestManifest;

use execution::execute_bucket;
use prebuild::prebuild_test_binary;

pub use execution::ProcessCommandExecutor;
pub use prebuild::transform_command_for_coverage;
pub use rendering::{render_bucket_result, render_manifest_listing, render_summary};

pub trait CommandExecutor {
    fn run(&self, bucket: &str, command: &str, timeout: Duration) -> anyhow::Result<CommandResult>;
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

fn empty_summary(bucket_names: Vec<String>, prebuild_elapsed: Duration) -> RunSummary {
    RunSummary {
        bucket_names,
        bucket_results: Vec::new(),
        passed_buckets: 0,
        total_elapsed: Duration::ZERO,
        prebuild_elapsed,
    }
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
