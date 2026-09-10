//! Intelligence page route handlers.

use std::collections::HashMap;
use std::f64::consts::PI;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Serialize;
use tera::Context;

use crate::dashboard::AppState;
use crate::dashboard::render_template;
use julie_index::checkout_store::CheckoutStore;

/// A high-centrality symbol returned by `top_symbols_by_centrality`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CentralitySymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
    pub reference_score: f64,
}

/// A high-activity file returned by `file_hotspots`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileHotspot {
    pub path: String,
    pub language: String,
    pub line_count: i32,
    pub size: i64,
    pub symbol_count: i64,
}

/// Workspace-wide aggregate counts.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AggregateStats {
    pub total_files: i64,
    pub total_symbols: i64,
    pub total_lines: i64,
    pub total_relationships: i64,
    pub language_count: i64,
}
use julie_index::graph::{Graph, SymbolId};
use julie_index::search::scoring::is_test_path;
use julie_index::snapshot::Snapshot;
use std::sync::Arc;

/// SVG donut chart circumference: 2 * pi * r where r = 0.7.
const CIRCUMFERENCE: f64 = 2.0 * PI * 0.7;

/// SVG donut chart segment with pre-computed stroke-dasharray values.
/// circumference = 2 * pi * 0.7 = ~4.398
#[derive(Debug, Clone, Serialize)]
pub struct DonutSegment {
    pub label: String,
    pub count: usize,
    pub percentage: f64,
    pub color_var: String,
    pub dash_length: f64,
    pub dash_offset: f64,
}

/// Map a symbol kind string to its CSS variable name.
///
/// Matches on lowercase kind. Unknown kinds fall back to `--kind-other`.
pub fn kind_css_var(kind: &str) -> &'static str {
    match kind.to_lowercase().as_str() {
        "function" => "--kind-function",
        "method" => "--kind-method",
        "struct" => "--kind-struct",
        "class" => "--kind-class",
        "trait" => "--kind-trait",
        "interface" => "--kind-interface",
        "enum" | "enum_member" => "--kind-enum",
        "type" => "--kind-type",
        "constant" => "--kind-constant",
        "variable" => "--kind-variable",
        "module" => "--kind-module",
        "namespace" => "--kind-namespace",
        "property" | "field" => "--kind-property",
        "import" | "export" => "--kind-import",
        _ => "--kind-other",
    }
}

/// Compute donut chart segments from a by-kind count map.
///
/// Segments are sorted by count descending. Each segment's `dash_length` is the
/// arc length for its fraction of the donut, and `dash_offset` is the negative
/// cumulative arc length so each segment starts where the previous ended.
pub fn compute_donut_segments(by_kind: &HashMap<String, usize>) -> Vec<DonutSegment> {
    if by_kind.is_empty() {
        return vec![];
    }

    let total: usize = by_kind.values().sum();
    if total == 0 {
        return vec![];
    }

    // Sort by count descending for visual prominence
    let mut entries: Vec<(&String, &usize)> = by_kind.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

    let mut segments = Vec::with_capacity(entries.len());
    let mut cumulative = 0.0_f64;

    for (kind, count) in entries {
        let fraction = *count as f64 / total as f64;
        let percentage = fraction * 100.0;
        let dash_length = fraction * CIRCUMFERENCE;
        let dash_offset = -cumulative;

        segments.push(DonutSegment {
            label: kind.clone(),
            count: *count,
            percentage,
            color_var: kind_css_var(kind).to_string(),
            dash_length,
            dash_offset,
        });

        cumulative += dash_length;
    }

    segments
}

/// Generate human-readable story cards describing the most notable workspace facts.
///
/// Returns between 3 and 5 observation strings depending on available data.
pub fn generate_story_cards(
    top_symbols: &[CentralitySymbol],
    hotspots: &[FileHotspot],
    by_kind: &HashMap<String, usize>,
    stats: &AggregateStats,
    lang_counts: &[(String, i64)],
) -> Vec<String> {
    let mut cards = Vec::new();

    // Card 1: most referenced symbol
    if let Some(sym) = top_symbols.first() {
        cards.push(format!(
            "Most referenced symbol: {} (score: {:.1})",
            sym.name, sym.reference_score
        ));
    }

    // Card 2: most complex file (by composite score, not raw size)
    if let Some(hotspot) = hotspots.first() {
        cards.push(format!(
            "Most complex file: {} ({} lines, {} symbols)",
            hotspot.path, hotspot.line_count, hotspot.symbol_count
        ));
    }

    // Card 3: dominant language
    let total_files = stats.total_files;
    if total_files > 0
        && let Some((lang, count)) = lang_counts.first()
    {
        let pct = (*count as f64 / total_files as f64) * 100.0;
        cards.push(format!(
            "Dominant language: {} ({:.0}% of files)",
            lang, pct
        ));
    }

    // Card 4: most common symbol kind
    let total_symbols: usize = by_kind.values().sum();
    if total_symbols > 0
        && let Some((kind, count)) = by_kind.iter().max_by_key(|(_, v)| *v)
    {
        let pct = (*count as f64 / total_symbols as f64) * 100.0;
        cards.push(format!("Most common symbol kind: {} ({:.0}%)", kind, pct));
    }

    // Card 5: total references tracked (only if meaningful)
    if stats.total_relationships > 100 {
        cards.push(format!(
            "Total references tracked: {}",
            format_number(stats.total_relationships)
        ));
    }

    cards
}

/// Format an integer with comma separators (e.g. 12847 -> "12,847").
pub fn format_number(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();

    for (i, ch) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(*ch);
    }

    if n < 0 {
        format!("-{}", result)
    } else {
        result
    }
}

/// Format a duration in milliseconds as a human-readable string.
///
/// - >= 60000ms: "Xm Y.Zs"
/// - >= 1000ms:  "X.Ys"
/// - else:       "Xms"
pub fn format_duration_ms(ms: i64) -> String {
    if ms >= 60_000 {
        format!("{}m {:.1}s", ms / 60_000, (ms % 60_000) as f64 / 1000.0)
    } else if ms >= 1_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

pub(crate) fn require_registered_workspace(
    state: &AppState,
    workspace_id: &str,
) -> Result<crate::registry::database::WorkspaceRow, StatusCode> {
    if workspace_id.contains('/')
        || workspace_id.contains('\\')
        || workspace_id.contains("..")
        || workspace_id.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    state
        .dashboard
        .daemon_db()
        .and_then(|db| db.get_workspace(workspace_id).ok().flatten())
        .ok_or(StatusCode::NOT_FOUND)
}

pub(crate) fn open_workspace_snapshot(
    state: &AppState,
    workspace_id: &str,
) -> Result<Arc<Snapshot>, StatusCode> {
    let workspace = require_registered_workspace(state, workspace_id)?;
    let paths =
        crate::paths::RegistryPaths::try_new().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let index_dir = paths.workspace_index_dir(workspace_id);
    if !index_dir.join("facts.sqlite").exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    let root = std::path::PathBuf::from(&workspace.path);
    let store =
        CheckoutStore::open(&index_dir, &root).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(store.current())
}

pub fn top_symbols_by_centrality(graph: &Graph, limit: usize) -> Vec<CentralitySymbol> {
    let mut symbols: Vec<CentralitySymbol> = (0..graph.len())
        .map(|index| SymbolId(index as u32))
        .filter(|id| !is_test_path(&graph.symbol(*id).path))
        .map(|id| {
            let row = graph.symbol(id);
            CentralitySymbol {
                name: row.name.clone(),
                kind: format!("{:?}", row.kind).to_lowercase(),
                language: row.language.clone(),
                file_path: row.path.clone(),
                signature: row.signature.clone(),
                reference_score: graph.reference_score(id),
            }
        })
        .filter(|symbol| symbol.reference_score > 0.0)
        .collect();
    symbols.sort_by(|left, right| {
        right
            .reference_score
            .partial_cmp(&left.reference_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    symbols.truncate(limit);
    symbols
}

pub fn file_hotspots(snapshot: &Snapshot, limit: usize) -> Vec<FileHotspot> {
    let graph = snapshot.graph();
    let languages: HashMap<String, String> = snapshot
        .facts()
        .ok()
        .and_then(|facts| facts.reader().paths().ok())
        .into_iter()
        .flatten()
        .map(|row| (row.path, row.language))
        .collect();
    let mut by_path: HashMap<String, FileHotspot> = HashMap::new();
    for index in 0..graph.len() {
        let row = graph.symbol(SymbolId(index as u32));
        let entry = by_path
            .entry(row.path.clone())
            .or_insert_with(|| FileHotspot {
                path: row.path.clone(),
                language: languages.get(&row.path).cloned().unwrap_or_default(),
                line_count: 0,
                size: 0,
                symbol_count: 0,
            });
        entry.symbol_count += 1;
        entry.line_count = entry.line_count.max(row.span.end_line as i32);
    }
    let mut hotspots: Vec<FileHotspot> = by_path.into_values().collect();
    hotspots.sort_by(|left, right| {
        let left_score = left.line_count as f64 + left.symbol_count as f64 * 10.0;
        let right_score = right.line_count as f64 + right.symbol_count as f64 * 10.0;
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hotspots.truncate(limit);
    hotspots
}

pub fn aggregate_stats(snapshot: &Snapshot) -> AggregateStats {
    let graph = snapshot.graph();
    let stats = graph.stats();
    let languages: std::collections::BTreeSet<String> = snapshot
        .facts()
        .ok()
        .and_then(|facts| facts.reader().paths().ok())
        .into_iter()
        .flatten()
        .map(|row| row.language)
        .filter(|language| !language.is_empty())
        .collect();
    let total_lines = file_hotspots(snapshot, usize::MAX)
        .into_iter()
        .map(|hotspot| hotspot.line_count as i64)
        .sum();
    AggregateStats {
        total_files: graph.paths().len() as i64,
        total_symbols: stats.symbols as i64,
        total_lines,
        total_relationships: stats.edges as i64,
        language_count: languages.len() as i64,
    }
}

pub fn symbol_kind_counts(graph: &Graph) -> HashMap<String, usize> {
    let mut by_kind = HashMap::new();
    for index in 0..graph.len() {
        let kind = format!("{:?}", graph.symbol(SymbolId(index as u32)).kind).to_lowercase();
        *by_kind.entry(kind).or_default() += 1;
    }
    by_kind
}

pub fn language_file_counts(snapshot: &Snapshot) -> Vec<(String, i64)> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    if let Ok(facts) = snapshot.facts()
        && let Ok(paths) = facts.reader().paths()
    {
        for row in paths {
            if !row.language.is_empty() {
                *counts.entry(row.language).or_default() += 1;
            }
        }
    }
    let mut lang_counts: Vec<(String, i64)> = counts.into_iter().collect();
    lang_counts.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    lang_counts
}

/// Main intelligence page for a workspace.
pub async fn index(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let snapshot = open_workspace_snapshot(&state, &workspace_id)?;
    let graph = snapshot.graph();
    let top_symbols = top_symbols_by_centrality(graph, 15);
    let hotspots = file_hotspots(&snapshot, 10);
    let stats = aggregate_stats(&snapshot);
    let by_kind = symbol_kind_counts(graph);
    let lang_counts = language_file_counts(&snapshot);

    let donut_segments = compute_donut_segments(&by_kind);

    // Get index duration from daemon DB
    let index_duration_str = state
        .dashboard
        .daemon_db()
        .and_then(|daemon_db| daemon_db.get_workspace(&workspace_id).ok().flatten())
        .and_then(|ws| ws.last_index_duration_ms)
        .map(format_duration_ms);

    let mut context = Context::new();
    context.insert("active_page", "intelligence");
    context.insert("workspace_id", &workspace_id);
    context.insert("no_data", &false);
    context.insert("top_symbols", &top_symbols);
    context.insert("hotspots", &hotspots);
    context.insert("stats", &stats);
    context.insert("by_kind", &by_kind);
    context.insert("lang_counts", &lang_counts);
    context.insert("donut_segments", &donut_segments);
    context.insert("index_duration", &index_duration_str);

    let max_hotspot_score = hotspots
        .first()
        .map(|h| h.line_count as f64 + h.symbol_count as f64 * 10.0)
        .unwrap_or(1.0);
    context.insert("max_hotspot_score", &max_hotspot_score);

    render_template(&state, "intelligence.html", context).await
}

/// Lazy-loaded story cards partial for a workspace.
pub async fn story_cards(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let snapshot = match open_workspace_snapshot(&state, &workspace_id) {
        Ok(snapshot) => snapshot,
        Err(_) => return Ok(Html(String::new())),
    };
    let graph = snapshot.graph();
    let top_symbols = top_symbols_by_centrality(graph, 1);
    let hotspots = file_hotspots(&snapshot, 1);
    let stats = aggregate_stats(&snapshot);
    let by_kind = symbol_kind_counts(graph);
    let lang_counts = language_file_counts(&snapshot);

    let cards = generate_story_cards(&top_symbols, &hotspots, &by_kind, &stats, &lang_counts);

    let mut context = Context::new();
    context.insert("cards", &cards);

    render_template(&state, "partials/intelligence_stories.html", context).await
}
