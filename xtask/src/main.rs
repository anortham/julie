use std::io::{self, Write};

use anyhow::anyhow;
use xtask::cli::{parse_test_command, validate_test_command, TestCommand};
use xtask::manifest::TestManifest;
use xtask::runner::{
    render_manifest_listing, render_summary, run_bucket, run_tier, ProcessCommandExecutor,
};
use xtask::workspace_root;

fn main() -> anyhow::Result<()> {
    let command = parse_test_command(std::env::args())?;
    let manifest = TestManifest::load(workspace_root().join("xtask/test_tiers.toml"))?;
    let command = validate_test_command(&manifest, command)?;
    let executor = ProcessCommandExecutor;
    let mut stdout = io::stdout().lock();

    match command {
        TestCommand::List => {
            stdout.write_all(render_manifest_listing(&manifest).as_bytes())?;
        }
        TestCommand::Tier {
            name,
            timeout_multiplier,
        } => match run_tier(&manifest, &name, timeout_multiplier, &executor, &mut stdout) {
            Ok(summary) => stdout.write_all(render_summary(&summary).as_bytes())?,
            Err(error) => {
                stdout.write_all(render_summary(&error.summary).as_bytes())?;
                return Err(anyhow!(error));
            }
        },
        TestCommand::Bucket {
            name,
            timeout_multiplier,
        } => match run_bucket(&manifest, &name, timeout_multiplier, &executor, &mut stdout) {
            Ok(summary) => stdout.write_all(render_summary(&summary).as_bytes())?,
            Err(error) => {
                stdout.write_all(render_summary(&error.summary).as_bytes())?;
                return Err(anyhow!(error));
            }
        },
    }

    Ok(())
}
