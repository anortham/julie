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

/// Tokenizer ablation variant for an A/B bakeoff run.
///
/// Matches the two env-var gates introduced in T3:
/// - `JULIE_ABLATE_STEMMING=1`   — disables the English stemmer step.
/// - `JULIE_ABLATE_CAMEL_EMIT=1` — disables CamelCase split emission.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Ablation {
    /// Baseline — no ablation (default). The env-var gates have been removed in T3;
    /// this variant is kept so `search-matrix baseline --ablation none` still parses
    /// and `apply_env` / `is_baseline` continue to compile.
    #[default]
    None,
}

impl Ablation {
    /// Parse from a CLI string. Only `"none"` is accepted; the `no-stemming`, `no-camel`,
    /// and `both` variants were removed in T3 along with their env-var gates.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "none" => Ok(Self::None),
            other => bail!(
                "invalid ablation variant `{other}`; \
                 expected: none  (no-stemming / no-camel / both were removed in T3)"
            ),
        }
    }

    /// Short label used in report filenames and JSON fields. Empty for baseline.
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "",
        }
    }

    /// Returns true when no ablation is active (always true now that only `None` exists).
    pub fn is_baseline(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Return a no-op env guard.  The env-var gates (`JULIE_ABLATE_STEMMING`,
    /// `JULIE_ABLATE_CAMEL_EMIT`) were removed in T3; this method exists only so
    /// existing call sites in `search_matrix.rs` continue to compile unchanged.
    pub fn apply_env(&self) -> EnvGuard {
        // Nothing to set — capture an empty set of keys so the guard is a true no-op.
        EnvGuard::capture(&[])
    }
}

/// RAII guard that restores env vars to their saved state on drop.
///
/// Used to scope ablation env-var mutations so they don't leak into the
/// calling shell environment or subsequent baseline runs in the same process.
pub struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    /// Capture the current values of `keys`. Call before mutating.
    pub fn capture(keys: &[&str]) -> Self {
        let saved = keys
            .iter()
            .map(|k| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        Self { saved }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.drain(..) {
            // SAFETY: single-threaded xtask context; restores pre-capture state.
            unsafe {
                match value {
                    Some(v) => std::env::set_var(&key, v),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchMatrixCommand {
    Mine {
        days: u32,
        out: PathBuf,
    },
    Baseline {
        profile: String,
        out: Option<PathBuf>,
        ablation: Ablation,
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

/// `cargo xtask eval ablation [options]`.
///
/// Runs the search-consolidation ablation harness against the labeled query
/// corpus. Each mode toggles the reranker (env var) and the embedding
/// provider (per-call) so we can attribute MRR/latency to specific pipeline
/// components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalCommand {
    Ablation {
        corpus: PathBuf,
        out: Option<PathBuf>,
        limit: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Test(TestCommand),
    SearchMatrix(SearchMatrixCommand),
    SyncPlugin(SyncPluginCommand),
    DevLink(DevLinkCommand),
    DevRestart(DevRestartCommand),
    Eval(EvalCommand),
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
        bail!("expected `cargo xtask <test|search-matrix|sync-plugin|dev-link|dev-restart|eval> ...`");
    };

    match command.as_str() {
        "test" => Ok(CliCommand::Test(parse_test_command(args)?)),
        "search-matrix" => Ok(CliCommand::SearchMatrix(parse_search_matrix_command(args)?)),
        "sync-plugin" => Ok(CliCommand::SyncPlugin(parse_sync_plugin_command(args)?)),
        "dev-link" => Ok(CliCommand::DevLink(parse_dev_link_command(args)?)),
        "dev-restart" => parse_dev_restart_command(args),
        "eval" => Ok(CliCommand::Eval(parse_eval_command(args)?)),
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
        CliCommand::SearchMatrix(command) => Ok(CliCommand::SearchMatrix(command)),
        CliCommand::SyncPlugin(command) => Ok(CliCommand::SyncPlugin(command)),
        CliCommand::DevLink(command) => Ok(CliCommand::DevLink(command)),
        CliCommand::DevRestart(command) => Ok(CliCommand::DevRestart(command)),
        CliCommand::Eval(command) => Ok(CliCommand::Eval(command)),
    }
}

fn parse_eval_command(args: Vec<String>) -> Result<EvalCommand> {
    let mut tail = args.into_iter().skip(2);
    let Some(kind) = tail.next() else {
        bail!("missing `cargo xtask eval <ablation>` subcommand");
    };

    match kind.as_str() {
        "ablation" => parse_eval_ablation_command(tail.collect()),
        other => bail!("unsupported `cargo xtask eval` subcommand `{other}`"),
    }
}

fn parse_eval_ablation_command(args: Vec<String>) -> Result<EvalCommand> {
    let mut corpus: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut limit: u32 = 10;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--corpus" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --corpus"))?;
                corpus = Some(PathBuf::from(raw));
            }
            "--out" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --out"))?;
                out = Some(PathBuf::from(raw));
            }
            "--limit" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --limit"))?;
                limit = raw
                    .parse::<u32>()
                    .map_err(|_| anyhow!("--limit must be a positive integer (got `{raw}`)"))?;
                if limit == 0 {
                    bail!("--limit must be >= 1");
                }
            }
            other => bail!("unexpected argument for `eval ablation`: {other}"),
        }
    }

    let corpus = corpus.unwrap_or_else(|| PathBuf::from("docs/eval/julie-search-corpus-v1.json"));

    Ok(EvalCommand::Ablation { corpus, out, limit })
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
            if options.ablation.is_some() {
                bail!("`--ablation` is not valid for `cargo xtask search-matrix mine`");
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
                ablation: options.ablation.unwrap_or_default(),
            })
        }
        other => bail!("unsupported `cargo xtask search-matrix` subcommand `{other}`"),
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

struct ParsedSearchMatrixOptions {
    days: Option<u32>,
    profile: Option<String>,
    out: Option<PathBuf>,
    ablation: Option<Ablation>,
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
    let mut ablation = None;
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
            "--ablation" => {
                let raw_value = iter
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --ablation"))?;
                ablation = Some(Ablation::parse(&raw_value)?);
            }
            other => bail!("unexpected argument: {other}"),
        }
    }

    Ok(ParsedSearchMatrixOptions {
        days,
        profile,
        out,
        ablation,
    })
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
        let parsed =
            parse_cli_command(["xtask", "search-matrix", "baseline", "--profile", "smoke"])
                .unwrap();

        assert!(matches!(
            parsed,
            CliCommand::SearchMatrix(SearchMatrixCommand::Baseline {
                profile,
                out: None,
                ablation: Ablation::None,
            }) if profile == "smoke"
        ));
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
