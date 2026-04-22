use std::io::{self, Write};
use std::process::Command;

use anyhow::anyhow;
use xtask::changed::{
    ChangedSelectionMode, collect_changed_paths, render_changed_selection, select_changed_buckets,
};
use xtask::cli::{CliCommand, TestCommand, parse_cli_command, validate_cli_command};
use xtask::manifest::TestManifest;
use xtask::runner::{
    ProcessCommandExecutor, render_manifest_listing, render_summary, run_bucket, run_named_buckets,
    run_tier,
};
use xtask::search_matrix::run_search_matrix_command;
use xtask::workspace_root;

fn main() -> anyhow::Result<()> {
    let raw_command = parse_cli_command(std::env::args())?;
    let executor = ProcessCommandExecutor;
    let mut stdout = io::stdout().lock();

    let coverage = matches!(
        &raw_command,
        CliCommand::Test(TestCommand::Tier { coverage: true, .. })
            | CliCommand::Test(TestCommand::Bucket { coverage: true, .. })
    );

    if coverage {
        stdout.write_all(b"COVERAGE: cleaning previous profraw data\n")?;
        let status = Command::new("cargo")
            .args(["llvm-cov", "clean", "--workspace"])
            .status()?;
        if !status.success() {
            return Err(anyhow!("cargo llvm-cov clean failed"));
        }
    }

    match raw_command {
        CliCommand::Test(command) => {
            let manifest = TestManifest::load(workspace_root().join("xtask/test_tiers.toml"))?;
            let command = match validate_cli_command(&manifest, CliCommand::Test(command))? {
                CliCommand::Test(command) => command,
                CliCommand::SearchMatrix(_) => unreachable!("validated test command changed shape"),
            };

            match command {
                TestCommand::Changed {
                    timeout_multiplier,
                    coverage,
                } => {
                    let changed_paths = collect_changed_paths(&workspace_root())?;
                    let selection = select_changed_buckets(&manifest, &changed_paths);
                    stdout.write_all(render_changed_selection(&selection).as_bytes())?;

                    if selection.mode == ChangedSelectionMode::NoChanges {
                        return Ok(());
                    }

                    match run_named_buckets(
                        &manifest,
                        &selection.bucket_names,
                        timeout_multiplier,
                        coverage,
                        &executor,
                        &mut stdout,
                    ) {
                        Ok(summary) => stdout.write_all(render_summary(&summary).as_bytes())?,
                        Err(error) => {
                            stdout.write_all(render_summary(&error.summary).as_bytes())?;
                            return Err(anyhow!(error));
                        }
                    }
                }
                TestCommand::List => {
                    stdout.write_all(render_manifest_listing(&manifest).as_bytes())?;
                }
                TestCommand::Tier {
                    name,
                    timeout_multiplier,
                    coverage,
                } => match run_tier(
                    &manifest,
                    &name,
                    timeout_multiplier,
                    coverage,
                    &executor,
                    &mut stdout,
                ) {
                    Ok(summary) => stdout.write_all(render_summary(&summary).as_bytes())?,
                    Err(error) => {
                        stdout.write_all(render_summary(&error.summary).as_bytes())?;
                        return Err(anyhow!(error));
                    }
                },
                TestCommand::Bucket {
                    name,
                    timeout_multiplier,
                    coverage,
                } => match run_bucket(
                    &manifest,
                    &name,
                    timeout_multiplier,
                    coverage,
                    &executor,
                    &mut stdout,
                ) {
                    Ok(summary) => stdout.write_all(render_summary(&summary).as_bytes())?,
                    Err(error) => {
                        stdout.write_all(render_summary(&error.summary).as_bytes())?;
                        return Err(anyhow!(error));
                    }
                },
            }
        }
        CliCommand::SearchMatrix(command) => {
            run_search_matrix_command(&command, &mut stdout)?;
        }
    }

    if coverage {
        stdout.write_all(b"\nCOVERAGE: generating report\n")?;
        drop(stdout); // release lock so cargo llvm-cov can print to stdout
        let status = Command::new("cargo")
            .args(["llvm-cov", "report", "--html"])
            .status()?;
        if !status.success() {
            return Err(anyhow!("cargo llvm-cov report failed"));
        }
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"COVERAGE: HTML report at target/llvm-cov/html/index.html\n")?;
    }

    Ok(())
}
