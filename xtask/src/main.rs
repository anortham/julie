use std::io::{self, Write};

use anyhow::anyhow;
use xtask::changed::{
    ChangedSelectionMode, collect_changed_paths, render_changed_selection, select_changed_buckets,
};
use xtask::cli::{
    CliCommand, DevLinkCommand, DevRestartCommand, SyncPluginCommand, TestCommand,
    parse_cli_command, validate_cli_command,
};
use xtask::inventory::{ProcessInventoryExecutor, render_inventory_report, run_inventory};
use xtask::manifest::TestManifest;
use xtask::process::cargo_status;
use xtask::runner::{
    ProcessCommandExecutor, render_manifest_listing, render_summary, run_bucket, run_named_buckets,
    run_tier,
};
use xtask::search_ablation::run_eval_ablation_command;
use xtask::search_matrix::run_search_matrix_command;
use xtask::workspace_root;

fn clean_coverage_data(stdout: &mut dyn Write) -> anyhow::Result<()> {
    stdout.write_all(b"COVERAGE: cleaning previous profraw data\n")?;
    let status = cargo_status(&["llvm-cov", "clean", "--workspace"])?;
    if !status.success() {
        return Err(anyhow!("cargo llvm-cov clean failed"));
    }
    Ok(())
}

fn begin_coverage_run(
    coverage: bool,
    stdout: &mut dyn Write,
    should_report_coverage: &mut bool,
) -> anyhow::Result<()> {
    if coverage {
        clean_coverage_data(stdout)?;
        *should_report_coverage = true;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let raw_command = parse_cli_command(std::env::args())?;
    let executor = ProcessCommandExecutor;
    let mut stdout = io::stdout().lock();
    let mut should_report_coverage = false;

    match raw_command {
        CliCommand::Test(command) => {
            let manifest = TestManifest::load(workspace_root().join("xtask/test_tiers.toml"))?;
            let command = match validate_cli_command(&manifest, CliCommand::Test(command))? {
                CliCommand::Test(command) => command,
                CliCommand::SearchMatrix(_)
                | CliCommand::SyncPlugin(_)
                | CliCommand::DevLink(_)
                | CliCommand::DevRestart(_)
                | CliCommand::Eval(_) => unreachable!("validated test command changed shape"),
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

                    begin_coverage_run(coverage, &mut stdout, &mut should_report_coverage)?;

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
                TestCommand::Inventory { target } => {
                    let report = run_inventory(&manifest, &target, &ProcessInventoryExecutor)?;
                    stdout.write_all(render_inventory_report(&report).as_bytes())?;
                }
                TestCommand::Tier {
                    name,
                    timeout_multiplier,
                    coverage,
                } => {
                    begin_coverage_run(coverage, &mut stdout, &mut should_report_coverage)?;
                    match run_tier(
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
                    }
                }
                TestCommand::Bucket {
                    name,
                    timeout_multiplier,
                    coverage,
                } => {
                    begin_coverage_run(coverage, &mut stdout, &mut should_report_coverage)?;
                    match run_bucket(
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
                    }
                }
            }
        }
        CliCommand::SearchMatrix(command) => {
            run_search_matrix_command(&command, &mut stdout)?;
        }
        CliCommand::Eval(command) => {
            run_eval_ablation_command(&command, &mut stdout)?;
        }
        CliCommand::SyncPlugin(SyncPluginCommand {
            plugin_root,
            dry_run,
        }) => {
            let workspace = workspace_root();
            let plugin =
                plugin_root.unwrap_or_else(|| xtask::sync_plugin::default_plugin_root(&workspace));
            xtask::sync_plugin::run_sync_plugin(&workspace, &plugin, dry_run, &mut stdout)?;
        }
        CliCommand::DevLink(DevLinkCommand {
            cache_root,
            dry_run,
        }) => {
            let workspace = workspace_root();
            let cache = cache_root.unwrap_or_else(xtask::dev_workflow::default_cache_root);
            xtask::dev_workflow::run_dev_link(&workspace, dry_run, &cache, &mut stdout)?;
        }
        CliCommand::DevRestart(DevRestartCommand { force }) => {
            xtask::dev_workflow::run_dev_restart(&mut stdout, force)?;
        }
    }

    if should_report_coverage {
        stdout.write_all(b"\nCOVERAGE: generating report\n")?;
        drop(stdout); // release lock so cargo llvm-cov can print to stdout
        let status = cargo_status(&["llvm-cov", "report", "--html"])?;
        if !status.success() {
            return Err(anyhow!("cargo llvm-cov report failed"));
        }
        let mut stdout = io::stdout().lock();
        stdout.write_all(b"COVERAGE: HTML report at target/llvm-cov/html/index.html\n")?;
    }

    Ok(())
}
