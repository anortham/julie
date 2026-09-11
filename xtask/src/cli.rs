use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

/// A fixed command list that `cargo xtask test <tier>` runs in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Dev,
    Dogfood,
    Full,
}

impl Tier {
    pub const NAMES: &[&str] = &["dev", "dogfood", "full"];

    pub fn name(self) -> &'static str {
        match self {
            Tier::Dev => "dev",
            Tier::Dogfood => "dogfood",
            Tier::Full => "full",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "dev" => Some(Tier::Dev),
            "dogfood" => Some(Tier::Dogfood),
            "full" => Some(Tier::Full),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncPluginCommand {
    pub plugin_root: Option<PathBuf>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevLinkCommand {
    pub cache_root: Option<PathBuf>,
    pub dry_run: bool,
}

/// `cargo xtask dev-restart`.
///
/// Advisory command (post Phase 3c.3 in-process cutover). There is no longer a
/// shared daemon to soft-restart or SIGTERM: each MCP session runs its own
/// in-process `julie-server`, leader-locked per workspace. `dev-restart` just
/// prints how to load a freshly built binary (restart the MCP client / start a
/// new session). Takes no arguments — the legacy `--force` SIGTERM path was
/// removed with the daemon (Phase 3d.2b).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevRestartCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Test(Tier),
    SyncPlugin(SyncPluginCommand),
    DevLink(DevLinkCommand),
    DevRestart(DevRestartCommand),
}

pub fn parse_cli_command<I, S>(args: I) -> Result<CliCommand>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();

    let Some(command) = args.get(1) else {
        bail!("expected `cargo xtask <test|sync-plugin|dev-link|dev-restart> ...`");
    };

    match command.as_str() {
        "test" => Ok(CliCommand::Test(parse_test_command(args)?)),
        "search-matrix" => {
            bail!("`search-matrix` moved to `xtask-eval`; use `cargo xtask-eval search-matrix ...`")
        }
        "sync-plugin" => Ok(CliCommand::SyncPlugin(parse_sync_plugin_command(args)?)),
        "dev-link" => Ok(CliCommand::DevLink(parse_dev_link_command(args)?)),
        "dev-restart" => parse_dev_restart_command(args),
        "eval" => bail!("`eval` moved to `xtask-eval`; use `cargo xtask-eval eval ...`"),
        other => bail!("unsupported xtask command `{other}`"),
    }
}

fn parse_dev_link_command(args: Vec<String>) -> Result<DevLinkCommand> {
    let mut tail = args.into_iter().skip(2);
    let mut cache_root: Option<PathBuf> = None;
    let mut dry_run = false;

    while let Some(arg) = tail.next() {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "--cache-root" => {
                let raw = tail
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --cache-root"))?;
                cache_root = Some(PathBuf::from(raw));
            }
            other => bail!("unexpected argument: {other}"),
        }
    }

    Ok(DevLinkCommand {
        cache_root,
        dry_run,
    })
}

fn parse_dev_restart_command(args: Vec<String>) -> Result<CliCommand> {
    // `dev-restart` is advisory and takes no arguments. The legacy `--force`
    // SIGTERM path was removed with the daemon (Phase 3d.2b).
    if let Some(arg) = args.into_iter().nth(2) {
        bail!("unexpected argument for `dev-restart`: {arg}");
    }
    Ok(CliCommand::DevRestart(DevRestartCommand))
}

fn parse_sync_plugin_command(args: Vec<String>) -> Result<SyncPluginCommand> {
    let mut tail = args.into_iter().skip(2);
    let mut plugin_root: Option<PathBuf> = None;
    let mut dry_run = false;

    while let Some(arg) = tail.next() {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            "--plugin-root" => {
                let raw = tail
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --plugin-root"))?;
                plugin_root = Some(PathBuf::from(raw));
            }
            other => bail!("unexpected argument: {other}"),
        }
    }

    Ok(SyncPluginCommand {
        plugin_root,
        dry_run,
    })
}

fn parse_test_command(args: Vec<String>) -> Result<Tier> {
    let usage = || anyhow!("expected `cargo xtask test <{}>`", Tier::NAMES.join("|"));
    let mut tail = args.into_iter().skip(2);
    let tier = tail
        .next()
        .and_then(|raw| Tier::parse(&raw))
        .ok_or_else(usage)?;
    if tail.next().is_some() {
        return Err(usage());
    }
    Ok(tier)
}
