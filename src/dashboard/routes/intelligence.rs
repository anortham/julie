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
use crate::database::analytics::{AggregateStats, CentralitySymbol, FileHotspot};

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

    // Card 2: largest file
    if let Some(hotspot) = hotspots.first() {
        cards.push(format!(
            "Largest file: {} ({} lines, {} symbols)",
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

/// Main intelligence page for a workspace.
pub async fn index(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let pool = match state.dashboard.workspace_pool() {
        Some(p) => p,
        None => {
            let mut context = Context::new();
            context.insert("active_page", "intelligence");
            context.insert("workspace_id", &workspace_id);
            context.insert("no_data", &true);
            return render_template(&state, "intelligence.html", context).await;
        }
    };

    let workspace = match pool.get(&workspace_id).await {
        Some(ws) => ws,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let db = match &workspace.db {
        Some(db) => db,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let (top_symbols, hotspots, stats, by_kind, lang_counts) = {
        let db_guard = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let top_symbols = db_guard
            .get_top_symbols_by_centrality(15)
            .unwrap_or_default();
        let hotspots = db_guard.get_file_hotspots(10).unwrap_or_default();
        let stats = db_guard.get_aggregate_stats().unwrap_or_default();
        let (by_kind, _by_language) = db_guard.get_symbol_statistics().unwrap_or_default();
        let lang_counts = db_guard.count_files_by_language().unwrap_or_default();

        (top_symbols, hotspots, stats, by_kind, lang_counts)
    };

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
    context.insert("index_duration_str", &index_duration_str);

    render_template(&state, "intelligence.html", context).await
}

/// Lazy-loaded story cards partial for a workspace.
pub async fn story_cards(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let pool = match state.dashboard.workspace_pool() {
        Some(p) => p,
        None => return Ok(Html(String::new())),
    };

    let workspace = match pool.get(&workspace_id).await {
        Some(ws) => ws,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let db = match &workspace.db {
        Some(db) => db,
        None => return Err(StatusCode::NOT_FOUND),
    };

    let (top_symbols, hotspots, stats, by_kind, lang_counts) = {
        let db_guard = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let top_symbols = db_guard
            .get_top_symbols_by_centrality(1)
            .unwrap_or_default();
        let hotspots = db_guard.get_file_hotspots(1).unwrap_or_default();
        let stats = db_guard.get_aggregate_stats().unwrap_or_default();
        let (by_kind, _by_language) = db_guard.get_symbol_statistics().unwrap_or_default();
        let lang_counts = db_guard.count_files_by_language().unwrap_or_default();

        (top_symbols, hotspots, stats, by_kind, lang_counts)
    };

    let cards = generate_story_cards(&top_symbols, &hotspots, &by_kind, &stats, &lang_counts);

    let mut context = Context::new();
    context.insert("cards", &cards);

    render_template(&state, "partials/intelligence_stories.html", context).await
}
