# Dashboard Intelligence Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `/intelligence/{workspace_id}` page that surfaces Julie's code understanding (top symbols, kind distribution, complexity hotspots, story cards) and enrich existing dashboard pages with centrality/reliability data.

**Architecture:** New `intelligence.rs` route module queries per-workspace SymbolDatabase via WorkspacePool (same pattern as `projects.rs`). Three new database queries (`get_top_symbols_by_centrality`, `get_file_hotspots`, `get_aggregate_stats`) plus one DaemonDatabase query (`get_tool_success_rate`). SVG donut chart rendered server-side in Tera templates. All data already exists in SQLite; zero migrations.

**Tech Stack:** Rust (axum handlers), Tera templates, server-side SVG, Bulma CSS, htmx for lazy-loading story cards.

**Spec:** `docs/superpowers/specs/2026-03-29-dashboard-intelligence-design.md`

---

## File Map

### New Files
| File | Responsibility |
|------|----------------|
| `src/database/analytics.rs` | New query functions: `get_top_symbols_by_centrality`, `get_file_hotspots`, `get_aggregate_stats` |
| `src/tests/dashboard/intelligence.rs` | Tests for intelligence route helpers (donut segments, story cards, analytics queries) |
| `src/dashboard/routes/intelligence.rs` | Route handlers for `/intelligence/{workspace_id}` and `/intelligence/{workspace_id}/stories` |
| `dashboard/templates/intelligence.html` | Main intelligence page template |
| `dashboard/templates/partials/intelligence_stories.html` | Lazy-loaded story cards partial |

### Modified Files
| File | Change |
|------|--------|
| `src/database/mod.rs` | Add `pub mod analytics;` and re-exports |
| `src/dashboard/routes/mod.rs` | Add `pub mod intelligence;` |
| `src/dashboard/mod.rs` | Register intelligence routes |
| `src/tests/dashboard/mod.rs` | Add `mod intelligence;` |
| `dashboard/templates/partials/project_row.html` | Add Top Symbol column |
| `dashboard/templates/partials/project_table.html` | Add column header for Top Symbol |
| `dashboard/templates/partials/project_detail.html` | Add kind breakdown bar + intelligence link |
| `dashboard/templates/metrics.html` | Add success rate summary card |
| `dashboard/templates/partials/search_results.html` | Add centrality badge |
| `dashboard/static/app.css` | Add symbol kind color variables + intelligence page styles |
| `src/dashboard/routes/projects.rs` | Pass top symbol and kind data to templates |
| `src/dashboard/routes/metrics.rs` | Query and pass success rate |
| `src/dashboard/routes/search.rs` | Query and pass top-20 centrality symbols for badges |

---

## Task 1: Database Analytics Queries

**Files:**
- Create: `src/database/analytics.rs`
- Modify: `src/database/mod.rs`
- Test: `src/tests/dashboard/intelligence.rs` (analytics query tests)

- [ ] **Step 1: Write failing tests for analytics queries**

Create `src/tests/dashboard/intelligence.rs`:

```rust
use crate::database::SymbolDatabase;
use tempfile::NamedTempFile;

fn setup_test_db() -> (SymbolDatabase, NamedTempFile) {
    let tmp = NamedTempFile::new().unwrap();
    let db = SymbolDatabase::new(tmp.path()).unwrap();

    // Insert test symbols with varying reference_score
    db.conn.execute_batch("
        INSERT INTO files (path, language, hash, size, last_modified, last_indexed, symbol_count, line_count)
        VALUES ('src/main.rs', 'rust', 'abc', 1200, 0, 0, 5, 120),
               ('src/lib.rs', 'rust', 'def', 3400, 0, 0, 12, 340),
               ('src/small.rs', 'rust', 'ghi', 200, 0, 0, 2, 20);

        INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_column, end_line, end_column, start_byte, end_byte, reference_score, signature)
        VALUES ('s1', 'process', 'function', 'rust', 'src/main.rs', 1, 0, 10, 0, 0, 100, 47.5, 'pub fn process(input: &str) -> Result<Output>'),
               ('s2', 'Config', 'struct', 'rust', 'src/lib.rs', 1, 0, 20, 0, 0, 200, 32.1, 'pub struct Config'),
               ('s3', 'Handler', 'trait', 'rust', 'src/lib.rs', 25, 0, 40, 0, 0, 300, 28.0, 'pub trait Handler'),
               ('s4', 'helper', 'function', 'rust', 'src/small.rs', 1, 0, 5, 0, 0, 50, 0.0, NULL),
               ('s5', 'main', 'function', 'rust', 'src/main.rs', 12, 0, 30, 0, 0, 400, 15.0, 'fn main()'),
               ('s6', 'Widget', 'class', 'typescript', 'src/widget.ts', 1, 0, 50, 0, 0, 500, 22.3, 'export class Widget');
    ").unwrap();

    (db, tmp)
}

#[test]
fn test_get_top_symbols_by_centrality() {
    let (db, _tmp) = setup_test_db();
    let top = db.get_top_symbols_by_centrality(3).unwrap();

    assert_eq!(top.len(), 3);
    assert_eq!(top[0].name, "process");
    assert!((top[0].reference_score - 47.5).abs() < 0.01);
    assert_eq!(top[0].kind, "function");
    assert_eq!(top[0].file_path, "src/main.rs");
    assert_eq!(top[0].signature.as_deref(), Some("pub fn process(input: &str) -> Result<Output>"));

    assert_eq!(top[1].name, "Config");
    assert_eq!(top[2].name, "Handler");
}

#[test]
fn test_get_top_symbols_excludes_zero_score() {
    let (db, _tmp) = setup_test_db();
    let top = db.get_top_symbols_by_centrality(100).unwrap();

    // "helper" has reference_score = 0.0 and should be excluded
    assert!(top.iter().all(|s| s.name != "helper"));
    assert_eq!(top.len(), 5);
}

#[test]
fn test_get_file_hotspots() {
    let (db, _tmp) = setup_test_db();
    let hotspots = db.get_file_hotspots(2).unwrap();

    assert_eq!(hotspots.len(), 2);
    // src/lib.rs has 340 lines + 12 symbols * 10 = 460 composite score (highest)
    assert_eq!(hotspots[0].path, "src/lib.rs");
    assert_eq!(hotspots[0].line_count, 340);
    assert_eq!(hotspots[0].symbol_count, 2); // only s2, s3 have file_path = src/lib.rs
    // src/main.rs has 120 lines + 5 symbols * 10 = 170
    assert_eq!(hotspots[1].path, "src/main.rs");
}

#[test]
fn test_get_aggregate_stats() {
    let (db, _tmp) = setup_test_db();
    let stats = db.get_aggregate_stats().unwrap();

    assert_eq!(stats.total_files, 3);
    assert_eq!(stats.total_symbols, 6);
    assert_eq!(stats.total_lines, 480); // 120 + 340 + 20
    assert_eq!(stats.language_count, 2); // rust + typescript (from symbols)
}
```

- [ ] **Step 2: Register the test module**

In `src/tests/dashboard/mod.rs`, add:

```rust
mod intelligence;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib test_get_top_symbols_by_centrality 2>&1 | tail -10`
Expected: FAIL (function not found)

- [ ] **Step 4: Create `src/database/analytics.rs` with structs and queries**

```rust
use anyhow::Result;
use serde::Serialize;

/// A symbol ranked by its centrality (reference_score).
#[derive(Debug, Clone, Serialize)]
pub struct CentralitySymbol {
    pub name: String,
    pub kind: String,
    pub language: String,
    pub file_path: String,
    pub signature: Option<String>,
    pub reference_score: f64,
}

/// A file ranked by complexity (composite of line count + symbol density).
#[derive(Debug, Clone, Serialize)]
pub struct FileHotspot {
    pub path: String,
    pub language: String,
    pub line_count: i32,
    pub size: i64,
    pub symbol_count: i64,
}

/// Aggregate workspace statistics for the fingerprint hero.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AggregateStats {
    pub total_files: i64,
    pub total_symbols: i64,
    pub total_lines: i64,
    pub total_relationships: i64,
    pub language_count: i64,
}

impl super::SymbolDatabase {
    /// Get the top N symbols by centrality score (reference_score > 0).
    pub fn get_top_symbols_by_centrality(&self, limit: usize) -> Result<Vec<CentralitySymbol>> {
        let query = "SELECT name, kind, language, file_path, signature, reference_score \
                     FROM symbols \
                     WHERE reference_score > 0 \
                     ORDER BY reference_score DESC \
                     LIMIT ?";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(CentralitySymbol {
                name: row.get(0)?,
                kind: row.get(1)?,
                language: row.get(2)?,
                file_path: row.get(3)?,
                signature: row.get(4)?,
                reference_score: row.get(5)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get the top N files by complexity (line_count + symbol_count * 10).
    pub fn get_file_hotspots(&self, limit: usize) -> Result<Vec<FileHotspot>> {
        let query = "SELECT f.path, f.language, f.line_count, f.size, \
                            COUNT(s.id) as symbol_count \
                     FROM files f \
                     LEFT JOIN symbols s ON s.file_path = f.path \
                     GROUP BY f.path \
                     ORDER BY (f.line_count + COUNT(s.id) * 10) DESC \
                     LIMIT ?";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([limit as i64], |row| {
            Ok(FileHotspot {
                path: row.get(0)?,
                language: row.get(1)?,
                line_count: row.get::<_, Option<i32>>(2)?.unwrap_or(0),
                size: row.get(3)?,
                symbol_count: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get aggregate stats for the codebase fingerprint.
    pub fn get_aggregate_stats(&self) -> Result<AggregateStats> {
        let total_files: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files", [], |row| row.get(0),
        )?;

        let total_symbols: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols", [], |row| row.get(0),
        )?;

        let total_lines: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(line_count), 0) FROM files", [], |row| row.get(0),
        )?;

        let total_relationships: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM relationships", [], |row| row.get(0),
        )?;

        let language_count: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT language) FROM files WHERE language != ''",
            [], |row| row.get(0),
        )?;

        Ok(AggregateStats {
            total_files,
            total_symbols,
            total_lines,
            total_relationships,
            language_count,
        })
    }
}
```

- [ ] **Step 5: Register the module in `src/database/mod.rs`**

Add after the existing module declarations:

```rust
pub mod analytics;
pub use analytics::*;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib test_get_top_symbols_by_centrality 2>&1 | tail -10`
Run: `cargo test --lib test_get_file_hotspots 2>&1 | tail -10`
Run: `cargo test --lib test_get_aggregate_stats 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 7: Commit**

```bash
git add src/database/analytics.rs src/database/mod.rs src/tests/dashboard/intelligence.rs src/tests/dashboard/mod.rs
git commit -m "feat(dashboard): add analytics queries for intelligence page

Add get_top_symbols_by_centrality, get_file_hotspots, and
get_aggregate_stats queries on SymbolDatabase. All read existing
data with zero schema changes."
```

---

## Task 2: Tool Success Rate Query (DaemonDatabase)

**Files:**
- Modify: `src/daemon/database.rs`
- Test: `src/tests/daemon/database.rs`

- [ ] **Step 1: Write failing test for success rate query**

Add to `src/tests/daemon/database.rs`:

```rust
#[test]
fn test_get_tool_success_rate() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = DaemonDatabase::new(tmp.path()).unwrap();

    // Register a workspace
    db.register_workspace("ws1", "/tmp/project").unwrap();

    // Insert tool calls: 8 success, 2 failure
    for i in 0..10 {
        db.insert_tool_call(
            "ws1",
            &format!("session_{}", i % 2),
            "fast_search",
            50.0,
            5,
            1000,
            500,
            i < 8, // first 8 succeed, last 2 fail
            None,
        ).unwrap();
    }

    let (total, succeeded) = db.get_tool_success_rate("ws1", 7).unwrap();
    assert_eq!(total, 10);
    assert_eq!(succeeded, 8);
}

#[test]
fn test_get_tool_success_rate_empty() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = DaemonDatabase::new(tmp.path()).unwrap();
    db.register_workspace("ws1", "/tmp/project").unwrap();

    let (total, succeeded) = db.get_tool_success_rate("ws1", 7).unwrap();
    assert_eq!(total, 0);
    assert_eq!(succeeded, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_get_tool_success_rate 2>&1 | tail -10`
Expected: FAIL (method not found)

- [ ] **Step 3: Add `get_tool_success_rate` to DaemonDatabase**

Add to `src/daemon/database.rs`, near the existing `query_tool_call_history` method:

```rust
/// Get tool call success rate for a workspace over the last N days.
/// Returns (total_calls, succeeded_calls).
pub fn get_tool_success_rate(&self, workspace_id: &str, days: u32) -> Result<(i64, i64)> {
    let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
    let cutoff = now_unix() - (days as i64 * 86400);

    let (total, succeeded): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END), 0) \
         FROM tool_calls \
         WHERE workspace_id = ?1 AND timestamp >= ?2",
        params![workspace_id, cutoff],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    Ok((total, succeeded))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_get_tool_success_rate 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "feat(dashboard): add tool success rate query to DaemonDatabase"
```

---

## Task 3: Symbol Kind CSS Variables and Intelligence Page Styles

**Files:**
- Modify: `dashboard/static/app.css`

- [ ] **Step 1: Add symbol kind color variables and intelligence page styles**

Add to `dashboard/static/app.css`, inside the `:root` block after the language colors:

```css
  /* Symbol kind colors */
  --kind-function:    #61afef;
  --kind-method:      #56b6c2;
  --kind-struct:      #98c379;
  --kind-class:       #98c379;
  --kind-trait:       #c678dd;
  --kind-interface:   #c678dd;
  --kind-enum:        #e5c07b;
  --kind-type:        #d19a66;
  --kind-constant:    #e06c75;
  --kind-variable:    #abb2bf;
  --kind-module:      #e8a040;
  --kind-namespace:   #e8a040;
  --kind-property:    #61afef;
  --kind-field:       #61afef;
  --kind-import:      #7a756c;
  --kind-other:       #8b8b8b;
```

Add at the end of the file, the intelligence page styles:

```css
/* ---------- Intelligence page ---------- */

.fingerprint-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 1rem;
  margin-bottom: 2rem;
}

.fingerprint-card {
  background: var(--julie-bg-card);
  border: 1px solid var(--julie-border);
  border-radius: var(--julie-radius);
  padding: 1.25rem 1rem;
  text-align: center;
}

.fingerprint-card .stat-value {
  font-family: var(--font-display);
  font-size: 2rem;
  font-weight: 700;
  color: var(--julie-primary);
  line-height: 1;
  margin-bottom: 0.25rem;
}

.fingerprint-card .stat-label {
  font-size: 0.75rem;
  color: var(--julie-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.donut-container {
  display: flex;
  align-items: flex-start;
  gap: 2rem;
  flex-wrap: wrap;
}

.donut-svg {
  flex-shrink: 0;
}

.donut-legend {
  flex: 1;
  min-width: 200px;
}

.donut-legend-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.2rem 0;
  font-size: 0.85rem;
}

.donut-legend-swatch {
  width: 12px;
  height: 12px;
  border-radius: 2px;
  flex-shrink: 0;
}

.donut-legend-label {
  color: var(--julie-text);
  flex: 1;
}

.donut-legend-count {
  color: var(--julie-text-muted);
  font-family: var(--font-mono);
  font-size: 0.8rem;
}

.story-card {
  background: var(--julie-bg-card);
  border: 1px solid var(--julie-border);
  border-radius: var(--julie-radius);
  padding: 0.75rem 1rem;
  font-size: 0.85rem;
  color: var(--julie-text);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.story-card .story-icon {
  color: var(--julie-primary);
  flex-shrink: 0;
}

.story-card .story-highlight {
  font-family: var(--font-mono);
  color: var(--julie-primary);
}

.hotspot-bar {
  height: 6px;
  background: var(--julie-primary-dim);
  border-radius: 3px;
  overflow: hidden;
  min-width: 60px;
}

.hotspot-bar-fill {
  height: 100%;
  background: var(--julie-primary);
  border-radius: 3px;
  transition: width var(--julie-transition);
}

.centrality-badge {
  display: inline-block;
  font-size: 0.65rem;
  font-weight: 600;
  padding: 0.1rem 0.4rem;
  border-radius: 3px;
  background: var(--julie-primary-dim);
  color: var(--julie-primary);
  margin-left: 0.4rem;
  vertical-align: middle;
}

.kind-bar-track {
  display: flex;
  height: 6px;
  border-radius: 3px;
  overflow: hidden;
  background: var(--julie-bg-inset);
  margin-top: 0.3rem;
}

.kind-bar-segment {
  height: 100%;
  min-width: 2px;
}
```

Also add the light theme overrides inside the `[data-theme="light"]` block:

```css
  --kind-function:    #2563eb;
  --kind-method:      #0891b2;
  --kind-struct:      #16a34a;
  --kind-class:       #16a34a;
  --kind-trait:       #9333ea;
  --kind-interface:   #9333ea;
  --kind-enum:        #ca8a04;
  --kind-type:        #ea580c;
  --kind-constant:    #dc2626;
  --kind-variable:    #6b7280;
  --kind-module:      #d97706;
  --kind-namespace:   #d97706;
  --kind-property:    #2563eb;
  --kind-field:       #2563eb;
  --kind-import:      #9ca3af;
  --kind-other:       #6b7280;
```

- [ ] **Step 2: Verify CSS loads (visual check)**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles (templates are embedded at build time via rust-embed)

- [ ] **Step 3: Commit**

```bash
git add dashboard/static/app.css
git commit -m "style(dashboard): add symbol kind colors and intelligence page styles"
```

---

## Task 4: Intelligence Route Handlers and Helper Functions

**Files:**
- Create: `src/dashboard/routes/intelligence.rs`
- Modify: `src/dashboard/routes/mod.rs`
- Modify: `src/dashboard/mod.rs`
- Test: `src/tests/dashboard/intelligence.rs` (add helper function tests)

- [ ] **Step 1: Write failing tests for donut segment computation and story card generation**

Add to `src/tests/dashboard/intelligence.rs`:

```rust
use crate::dashboard::routes::intelligence::{compute_donut_segments, generate_story_cards, kind_css_var};
use crate::database::analytics::{AggregateStats, CentralitySymbol, FileHotspot};
use std::collections::HashMap;

#[test]
fn test_kind_css_var_known_kinds() {
    assert_eq!(kind_css_var("function"), "--kind-function");
    assert_eq!(kind_css_var("struct"), "--kind-struct");
    assert_eq!(kind_css_var("trait"), "--kind-trait");
    assert_eq!(kind_css_var("class"), "--kind-class");
    assert_eq!(kind_css_var("method"), "--kind-method");
    assert_eq!(kind_css_var("enum"), "--kind-enum");
}

#[test]
fn test_kind_css_var_unknown_falls_back() {
    assert_eq!(kind_css_var("weird_kind"), "--kind-other");
}

#[test]
fn test_compute_donut_segments_basic() {
    let mut by_kind = HashMap::new();
    by_kind.insert("function".to_string(), 60usize);
    by_kind.insert("struct".to_string(), 30usize);
    by_kind.insert("constant".to_string(), 10usize);

    let segments = compute_donut_segments(&by_kind);

    assert_eq!(segments.len(), 3);
    // Should be sorted by count descending
    assert_eq!(segments[0].label, "function");
    assert_eq!(segments[0].count, 60);
    assert!((segments[0].percentage - 60.0).abs() < 0.1);

    assert_eq!(segments[1].label, "struct");
    assert_eq!(segments[2].label, "constant");

    // First segment offset should be 0
    assert!((segments[0].dash_offset - 0.0).abs() < 0.01);
    // Dash lengths should sum to full circumference (~4.398)
    let total_dash: f64 = segments.iter().map(|s| s.dash_length).sum();
    assert!((total_dash - 4.398).abs() < 0.01);
}

#[test]
fn test_compute_donut_segments_empty() {
    let by_kind = HashMap::new();
    let segments = compute_donut_segments(&by_kind);
    assert!(segments.is_empty());
}

#[test]
fn test_generate_story_cards() {
    let top_symbols = vec![
        CentralitySymbol {
            name: "process".into(),
            kind: "function".into(),
            language: "rust".into(),
            file_path: "src/main.rs".into(),
            signature: Some("pub fn process()".into()),
            reference_score: 47.5,
        },
    ];

    let hotspots = vec![
        FileHotspot {
            path: "src/big.rs".into(),
            language: "rust".into(),
            line_count: 847,
            size: 25000,
            symbol_count: 23,
        },
    ];

    let mut by_kind = HashMap::new();
    by_kind.insert("function".to_string(), 42usize);
    by_kind.insert("struct".to_string(), 18usize);

    let stats = AggregateStats {
        total_files: 100,
        total_symbols: 1500,
        total_lines: 45000,
        total_relationships: 12847,
        language_count: 3,
    };

    let lang_counts = vec![
        ("rust".to_string(), 78i64),
        ("typescript".to_string(), 15),
        ("python".to_string(), 7),
    ];

    let cards = generate_story_cards(&top_symbols, &hotspots, &by_kind, &stats, &lang_counts);

    assert!(cards.len() >= 3);
    assert!(cards.len() <= 5);

    // Should mention the top symbol
    assert!(cards.iter().any(|c| c.contains("process")));
    // Should mention the largest file
    assert!(cards.iter().any(|c| c.contains("src/big.rs")));
    // Should mention the dominant language
    assert!(cards.iter().any(|c| c.contains("rust") || c.contains("Rust")));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_kind_css_var_known_kinds 2>&1 | tail -10`
Expected: FAIL (module not found)

- [ ] **Step 3: Create `src/dashboard/routes/intelligence.rs`**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Html;
use serde::Serialize;
use tera::Context;

use crate::dashboard::render_template;
use crate::dashboard::AppState;
use crate::database::analytics::{AggregateStats, CentralitySymbol, FileHotspot};

/// SVG donut chart segment with pre-computed stroke-dasharray values.
/// Uses the circle + stroke-dasharray technique: each segment is a circle
/// with a dash that covers its arc and a gap for the rest.
/// circumference = 2 * pi * r = 2 * pi * 0.7 = ~4.398
#[derive(Debug, Clone, Serialize)]
pub struct DonutSegment {
    pub label: String,
    pub count: usize,
    pub percentage: f64,
    pub color_var: String,
    /// Length of the visible stroke dash (segment arc length).
    pub dash_length: f64,
    /// Offset to rotate this segment to its correct position.
    pub dash_offset: f64,
}

/// Map symbol kind to CSS variable name.
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

/// Compute donut chart segments from kind distribution.
/// Uses stroke-dasharray technique: circumference = 2 * pi * 0.7 = ~4.398
const CIRCUMFERENCE: f64 = 2.0 * std::f64::consts::PI * 0.7;

pub fn compute_donut_segments(
    by_kind: &std::collections::HashMap<String, usize>,
) -> Vec<DonutSegment> {
    let total: usize = by_kind.values().sum();
    if total == 0 {
        return vec![];
    }

    // Sort by count descending
    let mut entries: Vec<_> = by_kind.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));

    let mut segments = Vec::new();
    let mut cumulative_offset = 0.0;

    for (kind, count) in entries {
        let fraction = *count as f64 / total as f64;
        let percentage = fraction * 100.0;
        let dash_length = fraction * CIRCUMFERENCE;
        // Negative offset rotates clockwise from 12 o'clock
        let dash_offset = -cumulative_offset;

        segments.push(DonutSegment {
            label: kind.clone(),
            count: *count,
            percentage,
            color_var: kind_css_var(kind).to_string(),
            dash_length,
            dash_offset,
        });

        cumulative_offset += dash_length;
    }

    segments
}

/// Generate 3-5 story card strings from analytics data.
pub fn generate_story_cards(
    top_symbols: &[CentralitySymbol],
    hotspots: &[FileHotspot],
    by_kind: &std::collections::HashMap<String, usize>,
    stats: &AggregateStats,
    lang_counts: &[(String, i64)],
) -> Vec<String> {
    let mut cards = Vec::new();

    // 1. Most referenced symbol
    if let Some(top) = top_symbols.first() {
        cards.push(format!(
            "Most referenced symbol: {} (score: {:.1})",
            top.name, top.reference_score
        ));
    }

    // 2. Largest file
    if let Some(hot) = hotspots.first() {
        cards.push(format!(
            "Largest file: {} ({} lines, {} symbols)",
            hot.path, hot.line_count, hot.symbol_count
        ));
    }

    // 3. Dominant language
    if let Some((lang, count)) = lang_counts.first() {
        let total: i64 = lang_counts.iter().map(|(_, c)| c).sum();
        if total > 0 {
            let pct = (*count as f64 / total as f64) * 100.0;
            cards.push(format!(
                "Dominant language: {} ({:.0}% of files)",
                lang, pct
            ));
        }
    }

    // 4. Most common symbol kind
    if let Some((kind, count)) = by_kind.iter().max_by_key(|(_, c)| *c) {
        let total: usize = by_kind.values().sum();
        if total > 0 {
            let pct = (*count as f64 / total as f64) * 100.0;
            cards.push(format!(
                "Most common symbol kind: {} ({:.0}%)",
                kind, pct
            ));
        }
    }

    // 5. Total references (only if non-trivial)
    if stats.total_relationships > 100 {
        cards.push(format!(
            "Total references tracked: {}",
            format_number(stats.total_relationships)
        ));
    }

    cards
}

/// Format a number with comma separators (e.g. 12847 -> "12,847").
fn format_number(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Format index duration from milliseconds to human-readable string.
fn format_duration_ms(ms: i64) -> String {
    if ms >= 60_000 {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) as f64 / 1000.0;
        format!("{}m {:.1}s", mins, secs)
    } else if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

pub async fn index(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Html<String>, StatusCode> {
    let pool = match state.dashboard.workspace_pool() {
        Some(p) => p,
        None => {
            let mut context = Context::new();
            context.insert("active_page", "intelligence");
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

    let (top_symbols, hotspots, stats, kind_stats, lang_counts) = {
        let db_guard = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let top_symbols = db_guard.get_top_symbols_by_centrality(15).unwrap_or_default();
        let hotspots = db_guard.get_file_hotspots(10).unwrap_or_default();
        let stats = db_guard.get_aggregate_stats().unwrap_or_default();
        let (by_kind, _by_lang) = db_guard.get_symbol_statistics().unwrap_or_default();
        let lang_counts = db_guard.count_files_by_language().unwrap_or_default();

        (top_symbols, hotspots, stats, by_kind, lang_counts)
    };

    let donut_segments = compute_donut_segments(&kind_stats);

    // Get index duration from daemon DB
    let index_duration = state.dashboard.daemon_db().and_then(|ddb| {
        ddb.list_workspaces().ok().and_then(|wss| {
            wss.iter()
                .find(|w| w.workspace_id == workspace_id)
                .and_then(|w| w.last_index_duration_ms)
        })
    });

    let max_hotspot_score = hotspots
        .first()
        .map(|h| h.line_count as f64 + h.symbol_count as f64 * 10.0)
        .unwrap_or(1.0);

    let mut context = Context::new();
    context.insert("active_page", "intelligence");
    context.insert("no_data", &false);
    context.insert("workspace_id", &workspace_id);
    context.insert("top_symbols", &top_symbols);
    context.insert("hotspots", &hotspots);
    context.insert("stats", &stats);
    context.insert("donut_segments", &donut_segments);
    context.insert("max_hotspot_score", &max_hotspot_score);

    if let Some(ms) = index_duration {
        context.insert("index_duration", &format_duration_ms(ms));
    }

    render_template(&state, "intelligence.html", context).await
}

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

    let cards = {
        let db_guard = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        let top_symbols = db_guard.get_top_symbols_by_centrality(1).unwrap_or_default();
        let hotspots = db_guard.get_file_hotspots(1).unwrap_or_default();
        let stats = db_guard.get_aggregate_stats().unwrap_or_default();
        let (by_kind, _) = db_guard.get_symbol_statistics().unwrap_or_default();
        let lang_counts = db_guard.count_files_by_language().unwrap_or_default();

        generate_story_cards(&top_symbols, &hotspots, &by_kind, &stats, &lang_counts)
    };

    let mut context = Context::new();
    context.insert("cards", &cards);

    render_template(&state, "partials/intelligence_stories.html", context).await
}
```

- [ ] **Step 4: Register the module in `src/dashboard/routes/mod.rs`**

Add:

```rust
pub mod intelligence;
```

- [ ] **Step 5: Register routes in `src/dashboard/mod.rs`**

Add after the existing search routes (around line 141):

```rust
.route("/intelligence/{workspace_id}", get(routes::intelligence::index))
.route("/intelligence/{workspace_id}/stories", get(routes::intelligence::story_cards))
```

- [ ] **Step 6: Run tests to verify helpers pass**

Run: `cargo test --lib test_kind_css_var 2>&1 | tail -10`
Run: `cargo test --lib test_compute_donut 2>&1 | tail -10`
Run: `cargo test --lib test_generate_story 2>&1 | tail -10`
Expected: All PASS

- [ ] **Step 7: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles (templates not yet created, but routes should compile)

- [ ] **Step 8: Commit**

```bash
git add src/dashboard/routes/intelligence.rs src/dashboard/routes/mod.rs src/dashboard/mod.rs src/tests/dashboard/intelligence.rs
git commit -m "feat(dashboard): add intelligence route handlers and helpers

Donut segment computation, story card generation, kind CSS mapping,
and route handlers for /intelligence/{workspace_id}."
```

---

## Task 5: Intelligence Page Templates

**Files:**
- Create: `dashboard/templates/intelligence.html`
- Create: `dashboard/templates/partials/intelligence_stories.html`

- [ ] **Step 1: Create the main intelligence template**

Create `dashboard/templates/intelligence.html`:

```html
{% extends "base.html" %}
{% block title %}Intelligence - Julie Dashboard{% endblock %}
{% block content %}

{% if no_data %}
  <div style="text-align: center; padding: 4rem 0;">
    <h2 class="title" style="color: var(--julie-text-muted);">No Data Available</h2>
    <p style="color: var(--julie-text-muted);">
      Intelligence requires daemon mode with an indexed workspace.
    </p>
  </div>
{% else %}

  <div style="display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1.5rem;">
    <h1 class="title" style="margin-bottom: 0;">Intelligence</h1>
    <span class="mono" style="color: var(--julie-text-muted); font-size: 0.85rem;">{{ workspace_id }}</span>
  </div>

  <!-- Section 1: Codebase Fingerprint -->
  <div class="fingerprint-grid">
    <div class="fingerprint-card">
      <p class="stat-value">{{ stats.total_files }}</p>
      <p class="stat-label">Files</p>
    </div>
    <div class="fingerprint-card">
      <p class="stat-value">{{ stats.total_symbols }}</p>
      <p class="stat-label">Symbols</p>
    </div>
    <div class="fingerprint-card">
      <p class="stat-value">
        {% if stats.total_lines >= 1000 %}{{ (stats.total_lines / 1000) | round(precision=1) }}k
        {% else %}{{ stats.total_lines }}{% endif %}
      </p>
      <p class="stat-label">Lines of Code</p>
    </div>
    <div class="fingerprint-card">
      <p class="stat-value">{{ stats.language_count }}</p>
      <p class="stat-label">Languages</p>
    </div>
    <div class="fingerprint-card">
      <p class="stat-value">
        {% if stats.total_relationships >= 1000 %}{{ (stats.total_relationships / 1000) | round(precision=1) }}k
        {% else %}{{ stats.total_relationships }}{% endif %}
      </p>
      <p class="stat-label">References</p>
    </div>
    {% if index_duration %}
    <div class="fingerprint-card">
      <p class="stat-value" style="font-size: 1.4rem;">{{ index_duration }}</p>
      <p class="stat-label">Index Time</p>
    </div>
    {% endif %}
  </div>

  <!-- Section 2: Top Symbols ("Main Characters") -->
  <div class="julie-card" style="margin-bottom: 2rem;">
    <h2 class="subtitle" style="margin-bottom: 0.75rem; font-size: 1rem; color: var(--julie-text);">
      Top Symbols by Centrality
    </h2>
    {% if top_symbols | length > 0 %}
    <div style="overflow-x: auto;">
      <table class="table is-fullwidth is-narrow" style="background: transparent; font-size: 0.82rem;">
        <thead>
          <tr>
            <th style="width: 2.5rem; color: var(--julie-text-muted);">#</th>
            <th style="color: var(--julie-text-muted);">Symbol</th>
            <th style="color: var(--julie-text-muted);">Kind</th>
            <th style="color: var(--julie-text-muted);">Language</th>
            <th style="color: var(--julie-text-muted);">File</th>
            <th style="color: var(--julie-text-muted); text-align: right;">Score</th>
          </tr>
        </thead>
        <tbody>
          {% for sym in top_symbols %}
          <tr>
            <td style="color: var(--julie-text-muted);">{{ loop.index }}</td>
            <td>
              <span class="mono" style="color: var(--julie-text); font-weight: 600;">{{ sym.name }}</span>
              {% if sym.signature %}
                <br><span class="mono" style="color: var(--julie-text-muted); font-size: 0.75rem;">
                  {{ sym.signature | truncate(length=60) }}
                </span>
              {% endif %}
            </td>
            <td>
              <span style="display: inline-block; padding: 0.1rem 0.4rem; border-radius: 3px;
                           background: color-mix(in srgb, var({{ sym.color_var | default(value="--kind-other") }}) 15%, transparent);
                           color: var({{ sym.color_var | default(value="--kind-other") }}); font-size: 0.75rem;">
                {{ sym.kind }}
              </span>
            </td>
            <td style="color: var(--julie-text-muted);">{{ sym.language }}</td>
            <td><span class="mono" style="color: var(--julie-text-muted); font-size: 0.78rem;">{{ sym.file_path }}</span></td>
            <td style="text-align: right;">
              <span class="mono" style="color: var(--julie-primary);">{{ sym.reference_score | round(precision=1) }}</span>
            </td>
          </tr>
          {% endfor %}
        </tbody>
      </table>
    </div>
    {% else %}
      <p style="color: var(--julie-text-muted); font-size: 0.85rem;">
        No centrality data yet. Centrality scores are computed during indexing.
      </p>
    {% endif %}
  </div>

  <!-- Section 3: Symbol Kind Distribution -->
  <div class="julie-card" style="margin-bottom: 2rem;">
    <h2 class="subtitle" style="margin-bottom: 0.75rem; font-size: 1rem; color: var(--julie-text);">
      Symbol Kind Distribution
    </h2>
    {% if donut_segments | length > 0 %}
    <div class="donut-container">
      <!-- SVG Donut (stroke-dasharray technique) -->
      <svg class="donut-svg" width="180" height="180" viewBox="-1 -1 2 2" style="transform: rotate(-90deg);">
        {% for seg in donut_segments %}
          <circle cx="0" cy="0" r="0.7" fill="none"
                  stroke="var({{ seg.color_var }})" stroke-width="0.25"
                  stroke-dasharray="{{ seg.dash_length }} {{ 4.398 }}"
                  stroke-dashoffset="{{ seg.dash_offset }}"
                  opacity="0.85"/>
        {% endfor %}
      </svg>

      <!-- Legend -->
      <div class="donut-legend">
        {% for seg in donut_segments %}
        <div class="donut-legend-item">
          <span class="donut-legend-swatch" style="background: var({{ seg.color_var }});"></span>
          <span class="donut-legend-label">{{ seg.label }}</span>
          <span class="donut-legend-count">{{ seg.count }} ({{ seg.percentage | round(precision=1) }}%)</span>
        </div>
        {% endfor %}
      </div>
    </div>
    {% else %}
      <p style="color: var(--julie-text-muted); font-size: 0.85rem;">No symbol data available.</p>
    {% endif %}
  </div>

  <!-- Section 4: Complexity Hotspots -->
  <div class="julie-card" style="margin-bottom: 2rem;">
    <h2 class="subtitle" style="margin-bottom: 0.75rem; font-size: 1rem; color: var(--julie-text);">
      Complexity Hotspots
    </h2>
    {% if hotspots | length > 0 %}
    <table class="table is-fullwidth is-narrow" style="background: transparent; font-size: 0.82rem;">
      <thead>
        <tr>
          <th style="color: var(--julie-text-muted);">File</th>
          <th style="color: var(--julie-text-muted);">Language</th>
          <th style="color: var(--julie-text-muted); text-align: right;">Lines</th>
          <th style="color: var(--julie-text-muted); text-align: right;">Symbols</th>
          <th style="color: var(--julie-text-muted); min-width: 80px;">Complexity</th>
        </tr>
      </thead>
      <tbody>
        {% for file in hotspots %}
        <tr>
          <td><span class="mono" style="color: var(--julie-text); font-size: 0.78rem;">{{ file.path }}</span></td>
          <td style="color: var(--julie-text-muted);">{{ file.language }}</td>
          <td style="text-align: right;">{{ file.line_count }}</td>
          <td style="text-align: right;">{{ file.symbol_count }}</td>
          <td>
            {% set score = file.line_count + file.symbol_count * 10 %}
            {% set pct = score / max_hotspot_score * 100.0 %}
            <div class="hotspot-bar">
              <div class="hotspot-bar-fill" style="width: {{ pct | round(precision=1) }}%;"></div>
            </div>
          </td>
        </tr>
        {% endfor %}
      </tbody>
    </table>
    {% else %}
      <p style="color: var(--julie-text-muted); font-size: 0.85rem;">No file data available.</p>
    {% endif %}
  </div>

  <!-- Section 5: Story Cards (lazy loaded) -->
  <div class="julie-card">
    <h2 class="subtitle" style="margin-bottom: 0.75rem; font-size: 1rem; color: var(--julie-text);">
      Observations
    </h2>
    <div hx-get="/intelligence/{{ workspace_id }}/stories"
         hx-trigger="load"
         hx-swap="innerHTML">
      <p style="color: var(--julie-text-muted); font-size: 0.85rem;">Loading observations...</p>
    </div>
  </div>

{% endif %}
{% endblock %}
```

- [ ] **Step 2: Create the story cards partial**

Create `dashboard/templates/partials/intelligence_stories.html`:

```html
{% if cards | length > 0 %}
  <div style="display: flex; flex-direction: column; gap: 0.5rem;">
    {% for card in cards %}
    <div class="story-card">
      <span class="story-icon">&#9670;</span>
      <span>{{ card }}</span>
    </div>
    {% endfor %}
  </div>
{% else %}
  <p style="color: var(--julie-text-muted); font-size: 0.85rem;">
    Not enough data to generate observations yet.
  </p>
{% endif %}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles. Templates are loaded from disk in dev mode or embedded via rust-embed in release.

- [ ] **Step 4: Commit**

```bash
git add dashboard/templates/intelligence.html dashboard/templates/partials/intelligence_stories.html
git commit -m "feat(dashboard): add intelligence page templates

Main page with fingerprint, top symbols, SVG donut, hotspots.
Story cards lazy-loaded via htmx."
```

---

## Task 6: Enrich Projects Page (Top Symbol + Kind Bar + Intelligence Link)

**Files:**
- Modify: `src/dashboard/routes/projects.rs`
- Modify: `dashboard/templates/partials/project_row.html`
- Modify: `dashboard/templates/partials/project_table.html`
- Modify: `dashboard/templates/partials/project_detail.html`

- [ ] **Step 1: Add top symbol data to the projects route statuses handler**

In `src/dashboard/routes/projects.rs`, in the `statuses` handler, after fetching language data per workspace, also fetch the top-1 symbol by centrality. The statuses handler builds JSON for each workspace row. Add a `top_symbol` field.

Read `statuses()` first. Then modify it to include `top_symbol_name` for each workspace:

```rust
// Inside the per-workspace loop in statuses(), after language bar:
let top_symbol_name = if let Some(ws_arc) = pool_ref.get(&ws.workspace_id).await {
    if let Some(db) = &ws_arc.db {
        if let Ok(guard) = db.lock() {
            guard
                .get_top_symbols_by_centrality(1)
                .ok()
                .and_then(|v| v.into_iter().next())
                .map(|s| s.name)
        } else {
            None
        }
    } else {
        None
    }
} else {
    None
};
```

Add `top_symbol_name` to the JSON response for each workspace.

- [ ] **Step 2: Add kind bar data and intelligence link to detail handler**

In `src/dashboard/routes/projects.rs`, in the `detail` handler, add symbol kind statistics and a kind bar:

```rust
// After fetching language data, also fetch kind stats
let (kind_stats, kind_bar_html) = {
    let db_guard = db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let (by_kind, _) = db_guard.get_symbol_statistics().unwrap_or_default();

    let total: usize = by_kind.values().sum();
    let mut entries: Vec<_> = by_kind.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));

    let bar_html = if total > 0 {
        let segments: Vec<String> = entries.iter().take(8).map(|(kind, count)| {
            let pct = (**count as f64 / total as f64) * 100.0;
            let css_var = crate::dashboard::routes::intelligence::kind_css_var(kind);
            format!(
                r#"<div class="kind-bar-segment" style="width: {:.1}%; background: var({});" title="{}: {} ({:.1}%)"></div>"#,
                pct, css_var, kind, count, pct
            )
        }).collect();
        format!(r#"<div class="kind-bar-track">{}</div>"#, segments.join(""))
    } else {
        String::new()
    };

    (by_kind, bar_html)
};
```

Insert `kind_bar_html` and `workspace_id` into the template context.

- [ ] **Step 3: Update project_row.html to show top symbol**

Add a new column after the status column in `dashboard/templates/partials/project_row.html`:

```html
    <td id="topsym-{{ ws.workspace_id }}" style="max-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
      <span class="mono" style="color: var(--julie-text-muted); font-size: 0.78rem;"
            id="topsym-val-{{ ws.workspace_id }}"></span>
    </td>
```

- [ ] **Step 4: Update project_table.html to add column header**

In `dashboard/templates/partials/project_table.html`, add a `<th>` for "Top Symbol" after the Status header.

- [ ] **Step 5: Update project_detail.html to add kind bar and intelligence link**

In `dashboard/templates/partials/project_detail.html`, after the language_detail include, add:

```html
<!-- Symbol kind breakdown -->
{% if kind_bar_html %}
<div style="margin-top: 0.75rem;">
  <p class="label-text" style="margin-bottom: 0.3rem; font-size: 0.8rem;">Symbol Kinds</p>
  {{ kind_bar_html | safe }}
</div>
{% endif %}

<!-- Action buttons -->
<div style="margin-top: 0.75rem; display: flex; gap: 0.5rem;">
  <a href="/metrics?workspace={{ workspace_id }}"
     class="button is-dark is-small"
     style="font-size: 0.78rem;">
    View Metrics &rarr;
  </a>
  <a href="/intelligence/{{ workspace_id }}"
     class="button is-dark is-small"
     style="font-size: 0.78rem;">
    Intelligence &rarr;
  </a>
</div>
```

Replace the existing standalone metrics link at the bottom of the template.

- [ ] **Step 6: Verify compilation and visual check**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles

- [ ] **Step 7: Commit**

```bash
git add src/dashboard/routes/projects.rs dashboard/templates/partials/project_row.html dashboard/templates/partials/project_table.html dashboard/templates/partials/project_detail.html
git commit -m "feat(dashboard): add top symbol, kind bar, and intelligence link to projects"
```

---

## Task 7: Enrich Metrics Page (Success Rate Card)

**Files:**
- Modify: `src/dashboard/routes/metrics.rs`
- Modify: `dashboard/templates/metrics.html`

- [ ] **Step 1: Add success rate to metrics route handler**

In `src/dashboard/routes/metrics.rs`, in the `index` handler, after computing `saved_bytes`, add:

```rust
// Tool success rate
let (success_total, success_ok) = if workspace_id.is_empty() {
    // Aggregate across all workspaces
    let mut total = 0i64;
    let mut ok = 0i64;
    for ws in &workspaces {
        if let Ok((t, o)) = db.get_tool_success_rate(&ws.workspace_id, params.days) {
            total += t;
            ok += o;
        }
    }
    (total, ok)
} else {
    db.get_tool_success_rate(workspace_id, params.days).unwrap_or((0, 0))
};

let success_rate = if success_total > 0 {
    (success_ok as f64 / success_total as f64) * 100.0
} else {
    100.0
};
```

Add to context:

```rust
context.insert("success_rate", &success_rate);
context.insert("success_total", &success_total);
```

Do the same in the `table` handler.

- [ ] **Step 2: Add success rate card to metrics template**

In `dashboard/templates/metrics.html`, after the Context Saved card (around line 85), add a new column:

```html
    <div class="column">
      <div class="julie-card" style="text-align: center;"
           title="Percentage of tool calls that completed successfully">
        <p class="label-text">Success Rate</p>
        {% if success_total > 0 %}
          <p class="value-text" style="color:
            {% if success_rate >= 99.0 %}var(--julie-success)
            {% elif success_rate >= 95.0 %}var(--julie-warning)
            {% else %}var(--julie-danger){% endif %};">
            {{ success_rate | round(precision=1) }}%
          </p>
        {% else %}
          <p class="value-text" style="color: var(--julie-text-muted);">&mdash;</p>
        {% endif %}
      </div>
    </div>
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/dashboard/routes/metrics.rs dashboard/templates/metrics.html
git commit -m "feat(dashboard): add tool success rate card to metrics page"
```

---

## Task 8: Enrich Search Results (Centrality Badges)

**Files:**
- Modify: `src/dashboard/routes/search.rs`
- Modify: `dashboard/templates/partials/search_results.html`

- [ ] **Step 1: Fetch top-20 centrality symbols in search handler**

In `src/dashboard/routes/search.rs`, in the `search` handler, after running the search, fetch the top 20 centrality symbols for the workspace and attach badges to matching results:

```rust
// After running search and getting results, fetch top centrality symbols
let top_centrality: Vec<(String, usize)> = if let Some(pool) = state.dashboard.workspace_pool() {
    if let Some(ws) = pool.get(&workspace_id).await {
        if let Some(db) = &ws.db {
            if let Ok(guard) = db.lock() {
                guard.get_top_symbols_by_centrality(20)
                    .ok()
                    .map(|syms| syms.into_iter().enumerate().map(|(i, s)| (s.name, i + 1)).collect())
                    .unwrap_or_default()
            } else { vec![] }
        } else { vec![] }
    } else { vec![] }
} else { vec![] };
```

Convert to a HashMap for fast lookup and pass to template context:

```rust
let centrality_ranks: std::collections::HashMap<String, usize> = top_centrality.into_iter().collect();
context.insert("centrality_ranks", &centrality_ranks);
```

- [ ] **Step 2: Add centrality badge to search results template**

In `dashboard/templates/partials/search_results.html`, after the symbol name in each result, add:

```html
{% if centrality_ranks and centrality_ranks[result.name] %}
  {% set rank = centrality_ranks[result.name] %}
  <span class="centrality-badge">
    &#9733; Top {{ rank }}
  </span>
{% endif %}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -5`
Expected: Compiles

- [ ] **Step 4: Commit**

```bash
git add src/dashboard/routes/search.rs dashboard/templates/partials/search_results.html
git commit -m "feat(dashboard): add centrality badges to search results"
```

---

## Task 9: Run Full Test Suite and Fix Issues

**Files:**
- Various (depending on test results)

- [ ] **Step 1: Run dev test tier**

Run: `cargo xtask test dev`
Expected: All pass. Fix any failures.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1 | tail -20`
Expected: No warnings in new code

- [ ] **Step 3: Run fmt check**

Run: `cargo fmt -- --check 2>&1 | tail -10`
Expected: No formatting issues

- [ ] **Step 4: Fix any issues and commit**

If issues found:
```bash
git add -u
git commit -m "fix(dashboard): address test/lint issues in intelligence layer"
```

---

## Task 10: Visual Verification and Polish

**Files:**
- Various templates/CSS (if tweaks needed)

- [ ] **Step 1: Build release binary**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles

- [ ] **Step 2: Test intelligence page manually**

Start the daemon, navigate to `/intelligence/{workspace_id}` in browser. Verify:
- Fingerprint cards show correct stats
- Top symbols table is populated and sorted by score
- Donut chart renders with correct colors and proportions
- Hotspots table shows files sorted by complexity
- Story cards load via htmx

- [ ] **Step 3: Test enrichments manually**

Verify:
- Projects table shows Top Symbol column
- Project detail modal shows kind bar and Intelligence link
- Metrics page shows Success Rate card with color coding
- Search results show centrality badges on top symbols

- [ ] **Step 4: Fix any visual issues**

If the SVG donut segments don't align correctly, check that `dash_offset` values are computed correctly in `compute_donut_segments`. The offset should be the negative cumulative sum of previous segment dash lengths. If gap lines appear between segments, add `stroke-linecap="butt"` to each circle element.

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat(dashboard): intelligence layer visual polish"
```

---

## Summary

| Task | Description | Est. Effort |
|------|-------------|-------------|
| 1 | Database analytics queries | Low |
| 2 | Tool success rate query | Low |
| 3 | CSS variables and styles | Low |
| 4 | Intelligence route handlers | Medium |
| 5 | Intelligence page templates | Medium |
| 6 | Enrich projects page | Low-Medium |
| 7 | Enrich metrics page | Low |
| 8 | Enrich search results | Low |
| 9 | Full test suite | Low |
| 10 | Visual verification | Low |

**Dependencies:** Task 1 before Tasks 4-8. Task 2 before Task 7. Task 3 before Task 5. All others can run in parallel.
