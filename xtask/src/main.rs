use std::io;

use xtask::cli::{
    CliCommand, DevLinkCommand, DevRestartCommand, SyncPluginCommand, parse_cli_command,
};
use xtask::runner::{ProcessCommandExecutor, run_tier};
use xtask::workspace_root;

fn main() -> anyhow::Result<()> {
    let command = match parse_cli_command(std::env::args()) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let mut stdout = io::stdout().lock();

    match command {
        CliCommand::Test(tier) => run_tier(tier, &ProcessCommandExecutor, &mut stdout)?,
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
        CliCommand::DevRestart(DevRestartCommand) => {
            xtask::dev_workflow::run_dev_restart(&mut stdout)?;
        }
    }

    Ok(())
}
