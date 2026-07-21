use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

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
            // SAFETY: single-threaded xtask-eval context; restores pre-capture state.
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

/// `cargo xtask-eval eval ablation [options]`.
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
    SearchMatrix(SearchMatrixCommand),
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
        bail!("expected `cargo xtask-eval <search-matrix|eval> ...`");
    };

    match command.as_str() {
        "search-matrix" => Ok(CliCommand::SearchMatrix(parse_search_matrix_command(args)?)),
        "eval" => Ok(CliCommand::Eval(parse_eval_command(args)?)),
        other => bail!("unsupported xtask-eval command `{other}`"),
    }
}

fn parse_eval_command(args: Vec<String>) -> Result<EvalCommand> {
    let mut tail = args.into_iter().skip(2);
    let Some(kind) = tail.next() else {
        bail!("missing `cargo xtask-eval eval <ablation>` subcommand");
    };

    match kind.as_str() {
        "ablation" => parse_eval_ablation_command(tail.collect()),
        other => bail!("unsupported `cargo xtask-eval eval` subcommand `{other}`"),
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
        bail!("missing `cargo xtask-eval search-matrix <mine|baseline>` subcommand");
    };

    match kind.as_str() {
        "mine" => {
            let options = parse_search_matrix_options(tail.collect())?;
            if options.profile.is_some() {
                bail!("`--profile` is not valid for `cargo xtask-eval search-matrix mine`");
            }
            if options.ablation.is_some() {
                bail!("`--ablation` is not valid for `cargo xtask-eval search-matrix mine`");
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
                bail!("`--days` is not valid for `cargo xtask-eval search-matrix baseline`");
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
        other => bail!("unsupported `cargo xtask-eval search-matrix` subcommand `{other}`"),
    }
}

struct ParsedSearchMatrixOptions {
    days: Option<u32>,
    profile: Option<String>,
    out: Option<PathBuf>,
    ablation: Option<Ablation>,
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
