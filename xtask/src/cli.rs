use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

use crate::inventory::InventoryTarget;
use crate::manifest::TestManifest;

const PROGRAM_TIERS: &[&str] = &["reliability", "benchmark"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestCommand {
    Changed {
        timeout_multiplier: u64,
        coverage: bool,
        scale: bool,
    },
    List,
    Tier {
        name: String,
        timeout_multiplier: u64,
        coverage: bool,
    },
    Bucket {
        name: String,
        timeout_multiplier: u64,
        coverage: bool,
    },
    Inventory {
        target: InventoryTarget,
    },
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
    Test(TestCommand),
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
        "search-matrix" => bail!(
            "`search-matrix` moved to `xtask-eval`; use `cargo xtask-eval search-matrix ...`"
        ),
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

pub fn parse_test_command<I, S>(args: I) -> Result<TestCommand>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();

    if args.len() < 2 || args[1] != "test" {
        bail!("expected `cargo xtask test <changed|tier|list|bucket|inventory>`");
    }

    let mut tail = args.into_iter().skip(2);
    let Some(kind) = tail.next() else {
        bail!("missing xtask test subcommand");
    };

    match kind.as_str() {
        "changed" => {
            let mut scale = false;
            let mut rest = Vec::new();
            for arg in tail {
                if arg == "--scale" {
                    scale = true;
                } else {
                    rest.push(arg);
                }
            }
            let options = parse_options(rest)?;
            Ok(TestCommand::Changed {
                timeout_multiplier: options.timeout_multiplier,
                coverage: options.coverage,
                scale,
            })
        }
        "list" => {
            ensure_no_extra_args(tail.collect())?;
            Ok(TestCommand::List)
        }
        "bucket" => {
            let Some(name) = tail.next() else {
                bail!("missing bucket name for `cargo xtask test bucket <name>`");
            };
            let options = parse_options(tail.collect())?;
            Ok(TestCommand::Bucket {
                name,
                timeout_multiplier: options.timeout_multiplier,
                coverage: options.coverage,
            })
        }
        "inventory" => Ok(TestCommand::Inventory {
            target: parse_inventory_target(tail.collect())?,
        }),
        other => {
            let options = parse_options(tail.collect())?;
            Ok(TestCommand::Tier {
                name: other.to_string(),
                timeout_multiplier: options.timeout_multiplier,
                coverage: options.coverage,
            })
        }
    }
}

pub fn validate_cli_command(manifest: &TestManifest, command: CliCommand) -> Result<CliCommand> {
    match command {
        CliCommand::Test(test_command) => Ok(CliCommand::Test(validate_test_command(
            manifest,
            test_command,
        )?)),
        CliCommand::SyncPlugin(command) => Ok(CliCommand::SyncPlugin(command)),
        CliCommand::DevLink(command) => Ok(CliCommand::DevLink(command)),
        CliCommand::DevRestart(command) => Ok(CliCommand::DevRestart(command)),
    }
}

pub fn validate_test_command(manifest: &TestManifest, command: TestCommand) -> Result<TestCommand> {
    match command {
        TestCommand::Changed { .. } => Ok(command),
        TestCommand::Inventory { target } => match target {
            InventoryTarget::Tier(name) => {
                if manifest.tiers.contains_key(&name) {
                    Ok(TestCommand::Inventory {
                        target: InventoryTarget::Tier(name),
                    })
                } else {
                    bail!("unknown inventory tier `{name}`")
                }
            }
            InventoryTarget::Bucket(name) => {
                if manifest.buckets.contains_key(&name) {
                    Ok(TestCommand::Inventory {
                        target: InventoryTarget::Bucket(name),
                    })
                } else {
                    bail!("unknown inventory bucket `{name}`")
                }
            }
        },
        TestCommand::Tier {
            name,
            timeout_multiplier,
            coverage,
        } => {
            if manifest.tiers.contains_key(&name) || is_program_tier(&name) {
                Ok(TestCommand::Tier {
                    name,
                    timeout_multiplier,
                    coverage,
                })
            } else {
                bail!("unsupported xtask test command `{name}`")
            }
        }
        TestCommand::Bucket {
            name,
            timeout_multiplier,
            coverage,
        } => {
            if manifest.buckets.contains_key(&name) {
                Ok(TestCommand::Bucket {
                    name,
                    timeout_multiplier,
                    coverage,
                })
            } else {
                bail!(
                    "unknown test bucket `{name}`; run `cargo xtask test list` to see available buckets"
                )
            }
        }
        other => Ok(other),
    }
}

fn parse_inventory_target(args: Vec<String>) -> Result<InventoryTarget> {
    let mut iter = args.into_iter();
    let Some(flag) = iter.next() else {
        bail!("missing inventory target; use `--tier <name>` or `--bucket <name>`");
    };
    let Some(name) = iter.next() else {
        bail!("missing value for {flag}");
    };

    if iter.next().is_some() {
        bail!("unexpected extra arguments for inventory command");
    }

    match flag.as_str() {
        "--tier" => Ok(InventoryTarget::Tier(name)),
        "--bucket" => Ok(InventoryTarget::Bucket(name)),
        other => bail!("unexpected inventory target flag `{other}`"),
    }
}

struct ParsedOptions {
    timeout_multiplier: u64,
    coverage: bool,
}

fn parse_options(args: Vec<String>) -> Result<ParsedOptions> {
    let mut timeout_multiplier = 1u64;
    let mut coverage = false;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--coverage" => coverage = true,
            "--timeout-multiplier" => {
                let raw_value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("missing value for --timeout-multiplier"))?;
                timeout_multiplier = raw_value.parse::<u64>().map_err(|_| {
                    anyhow::anyhow!("invalid `--timeout-multiplier` value `{raw_value}`")
                })?;
                if timeout_multiplier == 0 {
                    bail!("timeout multiplier must be greater than zero");
                }
            }
            other => bail!("unexpected argument: {other}"),
        }
    }

    Ok(ParsedOptions {
        timeout_multiplier,
        coverage,
    })
}

fn ensure_no_extra_args(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        return Ok(());
    }

    bail!("unexpected extra arguments: {}", args.join(" "))
}

fn is_program_tier(name: &str) -> bool {
    PROGRAM_TIERS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> TestManifest {
        TestManifest::from_str(
            r#"
[tiers]
fast = ["cli"]
smoke = ["cli"]

[buckets.cli]
expected_seconds = 1
timeout_seconds = 2
commands = ["cargo test --lib tests::cli_tests"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn cli_tests_validate_reliability_program_tier() {
        let parsed = parse_test_command(["xtask", "test", "reliability"]).unwrap();
        let validated = validate_test_command(&sample_manifest(), parsed).unwrap();

        assert!(matches!(
            validated,
            TestCommand::Tier {
                name,
                timeout_multiplier: 1,
                coverage: false,
            } if name == "reliability"
        ));
    }

    #[test]
    fn cli_tests_validate_benchmark_program_tier() {
        let parsed = parse_test_command(["xtask", "test", "benchmark"]).unwrap();
        let validated = validate_test_command(&sample_manifest(), parsed).unwrap();

        assert!(matches!(
            validated,
            TestCommand::Tier {
                name,
                timeout_multiplier: 1,
                coverage: false,
            } if name == "benchmark"
        ));
    }

    #[test]
    fn cli_tests_parse_changed_command() {
        let parsed = parse_test_command(["xtask", "test", "changed"]).unwrap();

        assert!(matches!(
            parsed,
            TestCommand::Changed {
                timeout_multiplier: 1,
                coverage: false,
                scale: false,
            }
        ));
    }

    #[test]
    fn cli_tests_parse_changed_scale_flag() {
        let parsed = parse_test_command(["xtask", "test", "changed", "--scale"]).unwrap();

        assert!(matches!(
            parsed,
            TestCommand::Changed {
                timeout_multiplier: 1,
                coverage: false,
                scale: true,
            }
        ));
    }

    #[test]
    fn cli_tests_search_matrix_command_returns_migration_error() {
        let err = parse_cli_command(["xtask", "search-matrix", "baseline", "--profile", "smoke"])
            .unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("cargo xtask-eval search-matrix"),
            "expected migration hint for search-matrix, got: {message}"
        );
    }

    #[test]
    fn cli_tests_eval_command_returns_migration_error() {
        let err = parse_cli_command(["xtask", "eval", "ablation"]).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("cargo xtask-eval eval"),
            "expected migration hint for eval, got: {message}"
        );
    }

    #[test]
    fn cli_tests_dev_restart_takes_no_args() {
        let parsed = parse_cli_command(["xtask", "dev-restart"]).unwrap();
        assert_eq!(parsed, CliCommand::DevRestart(DevRestartCommand));
    }

    #[test]
    fn cli_tests_dev_restart_rejects_force_flag() {
        // `--force` was removed with the daemon (Phase 3d.2b); it is now an
        // unknown argument like any other.
        let err = parse_cli_command(["xtask", "dev-restart", "--force"]).unwrap_err();
        assert!(
            err.to_string().contains("--force"),
            "expected error to mention `--force`, got: {err}"
        );
    }

    #[test]
    fn cli_tests_dev_restart_rejects_unknown_args() {
        let err = parse_cli_command(["xtask", "dev-restart", "--bogus"]).unwrap_err();
        assert!(
            err.to_string().contains("--bogus"),
            "expected error to mention `--bogus`, got: {err}"
        );
    }
}
