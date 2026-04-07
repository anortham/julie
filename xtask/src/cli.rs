use anyhow::{Result, bail};

use crate::manifest::TestManifest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestCommand {
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
            let options = parse_options(tail.collect())?;
            Ok(TestCommand::Bucket {
                name,
                timeout_multiplier: options.timeout_multiplier,
                coverage: options.coverage,
            })
        }
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

pub fn validate_test_command(manifest: &TestManifest, command: TestCommand) -> Result<TestCommand> {
    match command {
        TestCommand::Tier {
            name,
            timeout_multiplier,
            coverage,
        } => {
            if manifest.tiers.contains_key(&name) {
                Ok(TestCommand::Tier {
                    name,
                    timeout_multiplier,
                    coverage,
                })
            } else {
                bail!("unsupported xtask test command `{name}`")
            }
        }
        other => Ok(other),
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
