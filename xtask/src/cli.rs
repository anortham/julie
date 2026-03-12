use anyhow::{bail, Result};

use crate::manifest::TestManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestCommand {
    List,
    Tier {
        name: String,
        timeout_multiplier: u64,
    },
    Bucket {
        name: String,
        timeout_multiplier: u64,
    },
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
        bail!("expected `cargo xtask test <tier|list|bucket>`");
    }

    let mut tail = args.into_iter().skip(2);
    let Some(kind) = tail.next() else {
        bail!("missing xtask test subcommand");
    };

    match kind.as_str() {
        "list" => {
            ensure_no_extra_args(tail.collect())?;
            Ok(TestCommand::List)
        }
        "bucket" => {
            let Some(name) = tail.next() else {
                bail!("missing bucket name for `cargo xtask test bucket <name>`");
            };
            let timeout_multiplier = parse_timeout_multiplier(tail.collect())?;
            Ok(TestCommand::Bucket {
                name,
                timeout_multiplier,
            })
        }
        other => {
            let timeout_multiplier = parse_timeout_multiplier(tail.collect())?;
            Ok(TestCommand::Tier {
                name: other.to_string(),
                timeout_multiplier,
            })
        }
    }
}

pub fn validate_test_command(manifest: &TestManifest, command: TestCommand) -> Result<TestCommand> {
    match command {
        TestCommand::Tier {
            name,
            timeout_multiplier,
        } => {
            if manifest.tiers.contains_key(&name) {
                Ok(TestCommand::Tier {
                    name,
                    timeout_multiplier,
                })
            } else {
                bail!("unsupported xtask test command `{name}`")
            }
        }
        other => Ok(other),
    }
}

fn parse_timeout_multiplier(args: Vec<String>) -> Result<u64> {
    if args.is_empty() {
        return Ok(1);
    }

    if args.len() != 2 || args[0] != "--timeout-multiplier" {
        bail!("expected optional `--timeout-multiplier <n>`");
    }

    let raw_value = &args[1];
    let timeout_multiplier = raw_value
        .parse::<u64>()
        .map_err(|_| anyhow::anyhow!("invalid `--timeout-multiplier` value `{raw_value}`"))?;
    if timeout_multiplier == 0 {
        bail!("timeout multiplier must be greater than zero");
    }

    Ok(timeout_multiplier)
}

fn ensure_no_extra_args(args: Vec<String>) -> Result<()> {
    if args.is_empty() {
        return Ok(());
    }

    bail!("unexpected extra arguments: {}", args.join(" "))
}
