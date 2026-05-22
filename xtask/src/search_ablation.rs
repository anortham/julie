//! Search-consolidation ablation harness (Plan P2.2).
//!
//! Runs the labeled query corpus (`docs/eval/julie-search-corpus-v1.json`)
//! through the definition search pipeline four ways:
//! - `keyword-only`        — reranker off, no embedding provider
//! - `keyword+reranker`    — reranker on,  no embedding provider
//! - `hybrid-only`         — reranker off, embedding provider Some
//! - `hybrid+reranker`     — reranker on,  embedding provider Some
//!
//! Reranker is toggled per-mode via `JULIE_RERANKER_ENABLED` (the env var
//! the production pipeline reads inside `apply_reranker_to_symbol_results`).
//! Embedding-provider toggle is a per-call parameter on
//! `definition_search_with_index_for_ablation`.
//!
//! ## Scope (flagged, not silent)
//!
//! The fixture at `fixtures/databases/julie-snapshot/symbols.db` has NO
//! `embeddings` / `symbol_embeddings` table. So even if we pass a real
//! provider, KNN returns nothing and "hybrid" devolves to keyword. The two
//! hybrid modes are therefore reported with `status="skipped"` and
//! `skip_reason="fixture lacks symbol embeddings"` until either:
//!
//! 1. The fixture is regenerated with the sidecar online, OR
//! 2. `--source ~/.julie/indexes/<id>` is added to point at a daemon-
//!    indexed workspace (TODO, follow-up for P3.2).
//!
//! The two keyword modes ARE measured and produce the numbers the P3.1
//! reranker decision needs.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use julie::database::SymbolDatabase;
use julie::search::{LanguageConfigs, SearchDocument, SearchFilter, SearchIndex};
use serde::{Deserialize, Serialize};

use crate::cli::EvalCommand;
use crate::workspace_root;

const FIXTURE_DB_REL: &str = "fixtures/databases/julie-snapshot/symbols.db";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    KeywordOnly,
    KeywordReranker,
    HybridOnly,
    HybridReranker,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::KeywordOnly => "keyword-only",
            Mode::KeywordReranker => "keyword+reranker",
            Mode::HybridOnly => "hybrid-only",
            Mode::HybridReranker => "hybrid+reranker",
        }
    }

    fn reranker_env_value(self) -> &'static str {
        match self {
            Mode::KeywordOnly | Mode::HybridOnly => "0",
            Mode::KeywordReranker | Mode::HybridReranker => "1",
        }
    }

    fn wants_embeddings(self) -> bool {
        matches!(self, Mode::HybridOnly | Mode::HybridReranker)
    }
}

#[derive(Debug, Deserialize)]
struct CorpusFile {
    version: u32,
    #[serde(default)]
    created: String,
    queries: Vec<CorpusQuery>,
}

#[derive(Debug, Deserialize, Clone)]
struct CorpusQuery {
    id: String,
    query: String,
    category: String,
    expected_paths: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct AblationReport {
    corpus_version: u32,
    corpus_created: String,
    commit: String,
    timestamp: String,
    query_count: usize,
    /// Per-mode `--limit` — also the K in MRR@K below. Recorded so a reader
    /// who finds the JSON later doesn't have to guess what `mrr_at_k` means.
    k: usize,
    fixture_path: String,
    fixture_symbol_count: usize,
    fixture_indexed_symbols: usize,
    modes: Vec<ModeResult>,
}

#[derive(Debug, Serialize)]
struct ModeResult {
    mode: String,
    status: String,
    skip_reason: Option<String>,
    total_queries: usize,
    top1_relevant: usize,
    top1_relevant_pct: f64,
    /// MRR@K, where K is `AblationReport::k` (matches `--limit`). Renamed
    /// from `mrr_at_10` after the Codex review pointed out that the metric
    /// silently truncates to the actual K, not 10.
    mrr_at_k: f64,
    latency_ms_mean: f64,
    latency_ms_p50: u64,
    latency_ms_p95: u64,
    by_category: BTreeMap<String, CategoryMetrics>,
    per_query: Vec<PerQueryResult>,
}

#[derive(Debug, Serialize, Default)]
struct CategoryMetrics {
    queries: usize,
    top1_relevant: usize,
    top1_relevant_pct: f64,
    mrr_at_k: f64,
}

#[derive(Debug, Serialize)]
struct PerQueryResult {
    id: String,
    category: String,
    query: String,
    expected_paths: Vec<String>,
    top1_path: Option<String>,
    relevant_rank: Option<usize>,
    hit_count: usize,
    latency_ms: u64,
}

pub fn run_eval_ablation_command(command: &EvalCommand, stdout: &mut dyn Write) -> Result<()> {
    let EvalCommand::Ablation { corpus, out, limit } = command;
    let limit = *limit as usize;

    let workspace = workspace_root();
    let corpus_path = if corpus.is_absolute() {
        corpus.clone()
    } else {
        workspace.join(corpus)
    };

    let corpus_data = load_corpus(&corpus_path)
        .with_context(|| format!("loading corpus at {}", corpus_path.display()))?;
    writeln!(
        stdout,
        "Loaded {} queries from {} (corpus v{}, created {})",
        corpus_data.queries.len(),
        corpus_path.display(),
        corpus_data.version,
        corpus_data.created
    )?;

    let fixture_db_path = workspace.join(FIXTURE_DB_REL);
    if !fixture_db_path.exists() {
        bail!(
            "Fixture DB not found at {}.\n\
             Build with: cargo test --lib build_julie_fixture -- --ignored --nocapture",
            fixture_db_path.display()
        );
    }

    let temp = tempfile::tempdir().context("creating temp dir")?;
    let db_path = temp.path().join("symbols.db");
    fs::copy(&fixture_db_path, &db_path).context("copying fixture DB")?;
    writeln!(stdout, "Copied fixture DB → {}", db_path.display())?;

    let db = SymbolDatabase::new(&db_path).context("opening copied SymbolDatabase")?;
    // SymbolDatabase::conn is private from this crate, so query the file
    // directly via a side-channel connection for counts and schema probes.
    let fixture_symbol_count = count_symbols(&db_path).context("counting symbols")?;
    writeln!(stdout, "Fixture symbols: {fixture_symbol_count}")?;

    let tantivy_dir = temp.path().join("tantivy");
    fs::create_dir_all(&tantivy_dir).context("creating tantivy dir")?;
    let configs = LanguageConfigs::load_embedded();
    let index = SearchIndex::open_or_create_with_language_configs(&tantivy_dir, &configs)
        .context("opening fresh Tantivy index")?;

    writeln!(stdout, "Backfilling Tantivy from SQLite...")?;
    let symbols = db.get_all_symbols().context("loading symbols")?;
    let mut indexed_symbols = 0usize;
    for sym in &symbols {
        let doc = SearchDocument::for_symbol(sym, vec![], String::new(), String::new());
        if index.add_search_doc(&doc).is_ok() {
            indexed_symbols += 1;
        }
    }
    if let Ok(file_contents) = db.get_all_file_contents_with_language() {
        for (path, language, content) in &file_contents {
            let doc = SearchDocument::file_from_parts(path, content, language);
            let _ = index.add_search_doc(&doc);
        }
    }
    index.commit().context("committing Tantivy index")?;
    writeln!(
        stdout,
        "  Tantivy ready: {indexed_symbols} symbols (of {} loaded)",
        symbols.len()
    )?;

    let fixture_has_embeddings = db_has_embeddings(&db_path);
    if !fixture_has_embeddings {
        writeln!(
            stdout,
            "  Fixture has NO symbol_embeddings table → hybrid modes will be skipped"
        )?;
    }

    // Snapshot existing env in an RAII guard so an early-return from any
    // mode (e.g. a `?` from definition_search_with_index_for_ablation)
    // still restores prior state. Per Codex review 2026-05-17: the previous
    // post-loop restore leaked env vars on error.
    let _env_guard = EnvGuard::capture(&["JULIE_RERANKER_ENABLED", "JULIE_EMBEDDING_PROVIDER"]);

    let mut mode_results = Vec::new();
    for mode in [
        Mode::KeywordOnly,
        Mode::KeywordReranker,
        Mode::HybridOnly,
        Mode::HybridReranker,
    ] {
        writeln!(stdout, "\n[mode] {}", mode.label())?;
        let result = run_mode(
            mode,
            &corpus_data.queries,
            &index,
            &db,
            limit,
            fixture_has_embeddings,
            stdout,
        )?;
        writeln!(
            stdout,
            "  status={} top1={}/{}  MRR@{}={:.3}  mean_latency_ms={:.1}",
            result.status,
            result.top1_relevant,
            result.total_queries,
            limit,
            result.mrr_at_k,
            result.latency_ms_mean
        )?;
        mode_results.push(result);
    }
    // _env_guard drops here, restoring snapshot.

    let report = AblationReport {
        corpus_version: corpus_data.version,
        corpus_created: corpus_data.created.clone(),
        commit: git_short_sha().unwrap_or_else(|| "unknown".to_string()),
        timestamp: iso_timestamp_now(),
        query_count: corpus_data.queries.len(),
        k: limit,
        fixture_path: fixture_db_path.to_string_lossy().into_owned(),
        fixture_symbol_count: fixture_symbol_count as usize,
        fixture_indexed_symbols: indexed_symbols,
        modes: mode_results,
    };

    let out_path = out.clone().unwrap_or_else(|| {
        let date = today_yyyymmdd();
        let commit = git_short_sha().unwrap_or_else(|| "unknown".to_string());
        workspace.join(format!(
            "docs/eval/julie-search-ablation/{date}-{commit}-baseline.json"
        ))
    });

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating output dir {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&report).context("serializing report")?;
    fs::write(&out_path, json)
        .with_context(|| format!("writing report to {}", out_path.display()))?;
    writeln!(stdout, "\nReport → {}", out_path.display())?;

    print_delta_table(&report, stdout)?;

    Ok(())
}

fn run_mode(
    mode: Mode,
    queries: &[CorpusQuery],
    index: &SearchIndex,
    _db: &SymbolDatabase,
    limit: usize,
    fixture_has_embeddings: bool,
    stdout: &mut dyn Write,
) -> Result<ModeResult> {
    // Reranker toggle: writes env that production code reads per-call.
    unsafe {
        std::env::set_var("JULIE_RERANKER_ENABLED", mode.reranker_env_value());
    }

    if mode.wants_embeddings() && !fixture_has_embeddings {
        let _ = writeln!(
            stdout,
            "  → skipping ({}): fixture has no symbol_embeddings",
            mode.label()
        );
        return Ok(ModeResult {
            mode: mode.label().to_string(),
            status: "skipped".to_string(),
            skip_reason: Some(
                "fixture lacks symbol embeddings; rerun against a daemon-indexed workspace \
                 (TODO: --source ~/.julie/indexes/<id>) to measure hybrid"
                    .to_string(),
            ),
            total_queries: queries.len(),
            top1_relevant: 0,
            top1_relevant_pct: 0.0,
            mrr_at_k: 0.0,
            latency_ms_mean: 0.0,
            latency_ms_p50: 0,
            latency_ms_p95: 0,
            by_category: BTreeMap::new(),
            per_query: Vec::new(),
        });
    }

    let mut latencies_ms = Vec::with_capacity(queries.len());
    let mut per_query = Vec::with_capacity(queries.len());
    let mut top1_relevant = 0usize;
    let mut reciprocal_sum = 0.0f64;
    let mut by_category: BTreeMap<String, CategoryMetrics> = BTreeMap::new();

    for entry in queries {
        let filter = SearchFilter::default();
        let started = Instant::now();
        // Both file-path and definition queries now go through the unified
        // search path (T9 cutover).  File-path queries filter to kind=="file"
        // hits; definition queries filter to non-file hits.  The reranker env
        // var is still honoured by the unified path.
        let all_hits = index
            .search_unified(&entry.query, &filter, limit)
            .with_context(|| {
                format!(
                    "search_unified failed for `{}` (id {})",
                    entry.query, entry.id
                )
            })?;
        let hit_paths: Vec<String> = if entry.category == "file-path" {
            all_hits
                .into_iter()
                .filter(|h| h.kind == "file")
                .map(|h| h.file_path)
                .collect()
        } else {
            all_hits
                .into_iter()
                .filter(|h| h.kind != "file")
                .map(|h| h.file_path)
                .collect()
        };
        let latency_ms = started.elapsed().as_millis() as u64;
        latencies_ms.push(latency_ms);

        let top1_path = hit_paths.first().cloned();
        let relevant_rank = first_relevant_rank_paths(&hit_paths, &entry.expected_paths);
        if relevant_rank == Some(1) {
            top1_relevant += 1;
        }
        // MRR@K — K matches the per-mode limit, not a hardcoded 10. The
        // hit_paths vec is already truncated to `limit` by the search call,
        // so any `Some(rank)` is in range; this gate is defensive.
        if let Some(rank) = relevant_rank {
            if rank <= limit {
                reciprocal_sum += 1.0 / rank as f64;
            }
        }

        let cat = by_category.entry(entry.category.clone()).or_default();
        cat.queries += 1;
        if relevant_rank == Some(1) {
            cat.top1_relevant += 1;
        }
        if let Some(rank) = relevant_rank {
            if rank <= limit {
                cat.mrr_at_k += 1.0 / rank as f64;
            }
        }

        per_query.push(PerQueryResult {
            id: entry.id.clone(),
            category: entry.category.clone(),
            query: entry.query.clone(),
            expected_paths: entry.expected_paths.clone(),
            top1_path,
            relevant_rank,
            hit_count: hit_paths.len(),
            latency_ms,
        });
    }

    for cat in by_category.values_mut() {
        if cat.queries > 0 {
            cat.mrr_at_k /= cat.queries as f64;
            cat.top1_relevant_pct = cat.top1_relevant as f64 / cat.queries as f64 * 100.0;
        }
    }

    let total = queries.len();
    let mrr = if total == 0 {
        0.0
    } else {
        reciprocal_sum / total as f64
    };
    let mean_ms = if latencies_ms.is_empty() {
        0.0
    } else {
        latencies_ms.iter().copied().sum::<u64>() as f64 / latencies_ms.len() as f64
    };
    let (p50, p95) = percentiles(&mut latencies_ms);
    let top1_pct = if total == 0 {
        0.0
    } else {
        top1_relevant as f64 / total as f64 * 100.0
    };

    Ok(ModeResult {
        mode: mode.label().to_string(),
        status: "ran".to_string(),
        skip_reason: None,
        total_queries: total,
        top1_relevant,
        top1_relevant_pct: top1_pct,
        mrr_at_k: mrr,
        latency_ms_mean: mean_ms,
        latency_ms_p50: p50,
        latency_ms_p95: p95,
        by_category,
        per_query,
    })
}

fn first_relevant_rank_paths(hit_paths: &[String], expected_paths: &[String]) -> Option<usize> {
    for (idx, path) in hit_paths.iter().enumerate() {
        if expected_paths
            .iter()
            .any(|expected| path == expected || path.contains(expected))
        {
            return Some(idx + 1);
        }
    }
    None
}

fn percentiles(values: &mut [u64]) -> (u64, u64) {
    if values.is_empty() {
        return (0, 0);
    }
    values.sort();
    let p50_idx = (values.len() as f64 * 0.50) as usize;
    let p95_idx = (values.len() as f64 * 0.95) as usize;
    let p50 = values[p50_idx.min(values.len() - 1)];
    let p95 = values[p95_idx.min(values.len() - 1)];
    (p50, p95)
}

fn load_corpus(path: &Path) -> Result<CorpusFile> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("reading corpus at {}", path.display()))?;
    let corpus: CorpusFile = serde_json::from_str(&text)
        .with_context(|| format!("parsing corpus JSON at {}", path.display()))?;
    if corpus.queries.is_empty() {
        bail!("corpus has zero queries");
    }
    Ok(corpus)
}

/// Julie stores symbol embeddings in the `symbol_vectors` virtual table
/// (sqlite-vec / vec0, see `src/database/vectors.rs:3`). An earlier version
/// of this probe checked for `embeddings` / `symbol_embeddings` which never
/// exist — that meant hybrid modes were silently skipped even on a workspace
/// that *did* have embeddings populated. Fixed via the Codex review of
/// 2026-05-17.
fn db_has_embeddings(db_path: &Path) -> bool {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // sqlite_master.type for vec0 virtual tables is 'table'; the name match
    // is sufficient because the table only exists when migration 010 has run.
    let row: rusqlite::Result<String> = conn.query_row(
        "SELECT name FROM sqlite_master WHERE type='table' \
         AND name = 'symbol_vectors' LIMIT 1",
        [],
        |row| row.get(0),
    );
    if row.is_err() {
        return false;
    }
    // The table can exist with zero rows (e.g. sidecar never finished). Treat
    // that as "no embeddings" so we don't run a hybrid mode that would silently
    // degenerate to keyword.
    let count: rusqlite::Result<i64> =
        conn.query_row("SELECT COUNT(*) FROM symbol_vectors", [], |row| row.get(0));
    matches!(count, Ok(n) if n > 0)
}

fn count_symbols(db_path: &Path) -> Result<i64> {
    let conn = rusqlite::Connection::open(db_path)
        .with_context(|| format!("opening sqlite probe connection at {}", db_path.display()))?;
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .context("SELECT COUNT(*) FROM symbols")?;
    Ok(count)
}

/// RAII guard that snapshots the requested env vars on construction and
/// restores their prior values on drop. Used to keep ablation env tweaks
/// (`JULIE_RERANKER_ENABLED`, `JULIE_EMBEDDING_PROVIDER`) from bleeding
/// into the rest of the xtask process — including when a `?` propagates
/// out of a mid-run search call.
struct EnvGuard {
    saved: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn capture(keys: &[&str]) -> Self {
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
            // SAFETY: single-threaded xtask context; this restores the
            // pre-capture state and matches the unsafe `set_var` used in
            // `run_mode`.
            unsafe {
                match value {
                    Some(v) => std::env::set_var(&key, v),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}

fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace_root())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    Some(s.trim().to_string())
}

fn today_yyyymmdd() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = epoch_secs_to_ymd(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

fn iso_timestamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, mo, d) = epoch_secs_to_ymd(secs);
    let day_secs = secs % 86_400;
    let h = day_secs / 3600;
    let mi = (day_secs % 3600) / 60;
    let s = day_secs % 60;
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// Convert epoch seconds (UTC) to (year, month, day). Civil calendar
/// conversion, valid for years 1970..=9999. Adapted from Howard Hinnant's
/// "chrono" algorithm.
fn epoch_secs_to_ymd(secs: u64) -> (i32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}

fn print_delta_table(report: &AblationReport, stdout: &mut dyn Write) -> Result<()> {
    let mrr_label = format!("MRR@{}", report.k);
    writeln!(stdout, "\n=== Ablation Summary (K={}) ===", report.k)?;
    writeln!(
        stdout,
        "{:<22}  {:>9}  {:>10}  {:>11}  {:>9}  {:>9}",
        "mode", "top1", mrr_label, "mean_ms", "p50_ms", "p95_ms"
    )?;
    for m in &report.modes {
        if m.status == "skipped" {
            writeln!(
                stdout,
                "{:<22}  (skipped: {})",
                m.mode,
                m.skip_reason.as_deref().unwrap_or("")
            )?;
            continue;
        }
        writeln!(
            stdout,
            "{:<22}  {:>4}/{:<4}  {:>10.3}  {:>11.1}  {:>9}  {:>9}",
            m.mode,
            m.top1_relevant,
            m.total_queries,
            m.mrr_at_k,
            m.latency_ms_mean,
            m.latency_ms_p50,
            m.latency_ms_p95,
        )?;
    }

    // By-category breakdown for the modes that ran.
    let mut categories: BTreeMap<String, ()> = BTreeMap::new();
    for m in &report.modes {
        for cat in m.by_category.keys() {
            categories.insert(cat.clone(), ());
        }
    }
    if !categories.is_empty() {
        writeln!(stdout, "\nBy category (top1 / total, {mrr_label}):")?;
        let mut header = format!("{:<16}", "category");
        for m in &report.modes {
            if m.status == "ran" {
                header.push_str(&format!("  {:>22}", m.mode));
            }
        }
        writeln!(stdout, "{header}")?;
        for cat in categories.keys() {
            let mut line = format!("{:<16}", cat);
            for m in &report.modes {
                if m.status != "ran" {
                    continue;
                }
                let cell = m
                    .by_category
                    .get(cat)
                    .map(|c| format!("{:>4}/{:<4} {:.3}", c.top1_relevant, c.queries, c.mrr_at_k))
                    .unwrap_or_else(|| "—".to_string());
                line.push_str(&format!("  {:>22}", cell));
            }
            writeln!(stdout, "{line}")?;
        }
    }

    Ok(())
}
