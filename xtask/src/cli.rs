use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

use crate::manifest::TestManifest;

const PROGRAM_TIERS: &[&str] = &["reliability", "benchmark"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestCommand {
    Changed {
        timeout_multiplier: u64,
        coverage: bool,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchMatrixCommand {
    Mine { days: u32, out: PathBuf },
    Baseline { profile: String, out: Option<PathBuf> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Test(TestCommand),
    SearchMatrix(SearchMatrixCommand),
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
        bail!("expected `cargo xtask <test|search-matrix> ...`");
    };

    match command.as_str() {
        "test" => Ok(CliCommand::Test(parse_test_command(args)?)),
        "search-matrix" => Ok(CliCommand::SearchMatrix(parse_search_matrix_command(args)?)),
        other => bail!("unsupported xtask command `{other}`"),
    }
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
        bail!("expected `cargo xtask test <changed|tier|list|bucket>`");
    }

    let mut tail = args.into_iter().skip(2);
    let Some(kind) = tail.next() else {
        bail!("missing xtask test subcommand");
    };

    match kind.as_str() {
        "changed" => {
            let options = parse_options(tail.collect())?;
            Ok(TestCommand::Changed {
                timeout_multiplier: options.timeout_multiplier,
                coverage: options.coverage,
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
        CliCommand::Test(test_command) => {
            Ok(CliCommand::Test(validate_test_command(manifest, test_command)?))
        }
        CliCommand::SearchMatrix(command) => Ok(CliCommand::SearchMatrix(command)),
    }
}

fn parse_search_matrix_command(args: Vec<String>) -> Result<SearchMatrixCommand> {
    let mut tail = args.into_iter().skip(2);
    let Some(kind) = tail.next() else {
        bail!("missing `cargo xtask search-matrix <mine|baseline>` subcommand");
    };

    match kind.as_str() {
        "mine" => {
            let options = parse_search_matrix_options(tail.collect())?;
            if options.profile.is_some() {
                bail!("`--profile` is not valid for `cargo xtask search-matrix mine`");
            }
            let days = options
                .days
                .ok_or_else(|| anyhow!("missing required `--days <n>`"))?;
            let out = options
                .out
                .ok_or_else(|| anyhow!("missing required `--out <path>`"))?;
            Ok(SearchMatrixCommand::Mine { days, out })
        }
        "baseline" => {
            let options = parse_search_matrix_options(tail.collect())?;
            if options.days.is_some() {
                bail!("`--days` is not valid for `cargo xtask search-matrix baseline`");
            }
            let profile = options
                .profile
                .ok_or_else(|| anyhow!("missing required `--profile <name>`"))?;
            Ok(SearchMatrixCommand::Baseline {
                profile,
                out: options.out,
            })
        }
        other => bail!("unsupported `cargo xtask search-matrix` subcommand `{other}`"),
    }
}

pub fn validate_test_command(manifest: &TestManifest, command: TestCommand) -> Result<TestCommand> {
    match command {
        TestCommand::Changed { .. } => Ok(command),
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
        other => Ok(other),
    }
}

struct ParsedOptions {
    timeout_multiplier: u64,
    coverage: bool,
}

struct ParsedSearchMatrixOptions {
    days: Option<u32>,
    profile: Option<String>,
    out: Option<PathBuf>,
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

fn parse_search_matrix_options(args: Vec<String>) -> Result<ParsedSearchMatrixOptions> {
    let mut days = None;
    let mut profile = None;
    let mut out = None;
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--days" => {
                let raw_value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --days"))?;
                let parsed = raw_value
                    .parse::<u32>()
                    .map_err(|_| anyhow!("invalid `--days` value `{raw_value}`"))?;
                if parsed == 0 {
                    bail!("`--days` must be greater than zero");
                }
                days = Some(parsed);
            }
            "--profile" => {
                let raw_value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --profile"))?;
                profile = Some(raw_value);
            }
            "--out" => {
                let raw_value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --out"))?;
                out = Some(PathBuf::from(raw_value));
            }
            other => bail!("unexpected argument: {other}"),
        }
    }

    Ok(ParsedSearchMatrixOptions { days, profile, out })
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
            }
        ));
    }

    #[test]
    fn cli_tests_parse_top_level_search_matrix_command() {
        let parsed = parse_cli_command([
            "xtask",
            "search-matrix",
            "baseline",
            "--profile",
            "smoke",
        ])
        .unwrap();

        assert!(matches!(
            parsed,
            CliCommand::SearchMatrix(SearchMatrixCommand::Baseline { profile, out: None })
                if profile == "smoke"
        ));
    }
}
