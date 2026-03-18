# Codebase Report Skills Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `query_metrics` MCP tool and three report skills (`/codehealth`, `/security-audit`, `/architecture`) that produce human-readable reports from Julie's existing analysis data.

**Architecture:** New `query_metrics` tool queries SQLite metadata (security_risk, change_risk, test_coverage, centrality) with sorting and filtering. Three SKILL.md files instruct Claude how to use `query_metrics` + existing tools to format reports. Tool follows existing rmcp patterns (struct + `#[tool]` macro in handler.rs).

**Tech Stack:** Rust (rmcp, serde, schemars, rusqlite, globset), Claude Code skills (SKILL.md with YAML frontmatter)

**Spec:** `docs/superpowers/specs/2026-03-18-codebase-report-skills-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `src/tools/metrics/mod.rs` | **Create** | `QueryMetricsTool` struct, parameter definitions, `call_tool` implementation |
| `src/tools/metrics/query.rs` | **Create** | SQL query builder, metadata extraction, result formatting |
| `src/tools/mod.rs` | **Modify** | Register `metrics` module, re-export `QueryMetricsTool` |
| `src/handler.rs` | **Modify** | Add `#[tool]` registration for `query_metrics` |
| `src/tests/tools/metrics/mod.rs` | **Create** | Test module for query_metrics |
| `src/tests/tools/metrics/query_metrics_tests.rs` | **Create** | Integration tests using fixture database |
| `src/tests/tools/mod.rs` | **Modify** | Register `metrics` test module |
| `.claude/skills/codehealth/SKILL.md` | **Create** | `/codehealth` skill |
| `.claude/skills/security-audit/SKILL.md` | **Create** | `/security-audit` skill |
| `.claude/skills/architecture/SKILL.md` | **Create** | `/architecture` skill |

---

### Task 1: `QueryMetricsTool` Struct + Module Skeleton

**Files:**
- Create: `src/tools/metrics/mod.rs`
- Create: `src/tools/metrics/query.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create the metrics module directory**

```bash
mkdir -p src/tools/metrics
```

- [ ] **Step 2: Create `src/tools/metrics/mod.rs` with the tool struct**

```rust
//! query_metrics — find symbols ranked by analysis metadata
//!
//! Complements fast_search (text-based) by finding code by quality scores:
//! security risk, change risk, test coverage, and centrality.

pub(crate) mod query;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

fn default_sort_by() -> String {
    "security_risk".to_string()
}

fn default_order() -> String {
    "desc".to_string()
}

fn default_limit() -> u32 {
    20
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

/// Query symbols ranked and filtered by analysis metadata.
/// Returns symbols sorted by security risk, change risk, test coverage, or centrality.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct QueryMetricsTool {
    /// Sort by: "security_risk", "change_risk", "test_coverage", "centrality"
    #[serde(default = "default_sort_by")]
    pub sort_by: String,

    /// Sort order: "desc" (worst/highest first) or "asc" (best/lowest first)
    #[serde(default = "default_order")]
    pub order: String,

    /// Minimum risk level filter (only applies when sort_by is a risk metric).
    /// Values: "low", "medium", "high"
    #[serde(default)]
    pub min_risk: Option<String>,

    /// Filter by test coverage presence. true = has tests, false = no tests.
    #[serde(default)]
    pub has_tests: Option<bool>,

    /// Filter by symbol kind: "function", "class", "struct", "trait", "method", etc.
    #[serde(default)]
    pub kind: Option<String>,

    /// File pattern filter (glob syntax, e.g. "src/**")
    #[serde(default)]
    pub file_pattern: Option<String>,

    /// Language filter
    #[serde(default)]
    pub language: Option<String>,

    /// Exclude test symbols from results
    #[serde(default)]
    pub exclude_tests: Option<bool>,

    /// Maximum results (default: 20, max: 100)
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Workspace: "primary" (default) or a reference workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl QueryMetricsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("📊 query_metrics: sort_by={}, order={}, limit={}", self.sort_by, self.order, self.limit);

        // Placeholder — Task 2 implements the actual query
        let message = format!("query_metrics: sort_by={} (not yet implemented)", self.sort_by);
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
```

- [ ] **Step 3: Create `src/tools/metrics/query.rs` skeleton**

```rust
//! SQL query building and result formatting for query_metrics tool

use crate::database::SymbolDatabase;
use crate::tools::search::matches_glob_pattern;
use anyhow::Result;

/// Result from a metrics query
#[derive(Debug)]
pub struct MetricsResult {
    pub name: String,
    pub file_path: String,
    pub start_line: u32,
    pub kind: String,
    pub reference_score: f64,
    pub security_risk_score: Option<f64>,
    pub security_risk_label: Option<String>,
    pub change_risk_score: Option<f64>,
    pub change_risk_label: Option<String>,
    pub test_coverage_tier: Option<String>,
    pub test_count: Option<u32>,
    pub raw_metadata: Option<String>,
}

/// Query symbols ranked by a metadata field
pub fn query_by_metrics(
    db: &SymbolDatabase,
    sort_by: &str,
    order: &str,
    min_risk: Option<&str>,
    has_tests: Option<bool>,
    kind: Option<&str>,
    file_pattern: Option<&str>,
    language: Option<&str>,
    exclude_tests: Option<bool>,
    limit: u32,
) -> Result<Vec<MetricsResult>> {
    // Placeholder — Task 2 implements
    Ok(vec![])
}

/// Format metrics results into human-readable output
pub fn format_metrics_output(
    results: &[MetricsResult],
    sort_by: &str,
    order: &str,
) -> String {
    // Placeholder — Task 3 implements
    String::new()
}
```

- [ ] **Step 4: Register the module in `src/tools/mod.rs`**

Add to `src/tools/mod.rs`:

```rust
pub mod metrics; // Metadata-based symbol queries (query_metrics)
```

And add the re-export:

```rust
pub use metrics::QueryMetricsTool;
```

- [ ] **Step 5: Verify it compiles**

```bash
cargo check 2>&1 | tail -5
```

Expected: Clean compile (tool struct exists but isn't registered in handler yet).

- [ ] **Step 6: Commit**

```bash
git add src/tools/metrics/ src/tools/mod.rs
git commit -m "feat(metrics): add QueryMetricsTool struct and module skeleton"
```

---

### Task 2: SQL Query Implementation

**Files:**
- Modify: `src/tools/metrics/query.rs`
- Create: `src/tests/tools/metrics/mod.rs`
- Create: `src/tests/tools/metrics/query_metrics_tests.rs`
- Modify: `src/tests/tools/mod.rs`

- [ ] **Step 1: Create test module structure**

```bash
mkdir -p src/tests/tools/metrics
```

Create `src/tests/tools/metrics/mod.rs`:

```rust
mod query_metrics_tests;
```

Add `pub mod metrics;` inside the `pub mod tools { ... }` block in `src/tests/mod.rs` (around line 73, alongside the existing `pub mod editing`, `pub mod deep_dive_tests`, `pub mod search` entries).

- [ ] **Step 2: Write failing test for query_by_metrics**

Create `src/tests/tools/metrics/query_metrics_tests.rs`:

```rust
use crate::database::SymbolDatabase;
use crate::tools::metrics::query::{query_by_metrics, MetricsResult};

/// Helper: create a temp database with test symbols and metadata
fn setup_test_db() -> (SymbolDatabase, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = SymbolDatabase::new(&db_path).unwrap();

    // Insert files first (foreign key constraint: symbols.file_path references files.path)
    db.conn.execute_batch("
        INSERT INTO files (path, language, size, last_modified)
        VALUES
        ('src/handlers/input.rs', 'rust', 1000, '2026-01-01'),
        ('src/auth/token.rs', 'rust', 500, '2026-01-01'),
        ('src/utils/format.rs', 'rust', 300, '2026-01-01'),
        ('src/tests/handlers.rs', 'rust', 200, '2026-01-01'),
        ('src/utils/old.rs', 'rust', 100, '2026-01-01'),
        ('src/services/user.rs', 'rust', 800, '2026-01-01');
    ").unwrap();

    // Insert test symbols with metadata (note: column is start_line, not line_number)
    db.conn.execute_batch("
        INSERT INTO symbols (id, name, kind, file_path, start_line, end_line, visibility, language, reference_score, metadata)
        VALUES
        -- High security risk, no tests, public function
        ('sym1', 'process_input', 'function', 'src/handlers/input.rs', 45, 80, 'public', 'rust', 0.85,
         '{\"security_risk\":{\"score\":0.82,\"label\":\"HIGH\",\"signals\":{\"sink_calls\":[\"execute\"],\"exposure\":0.75}},\"change_risk\":{\"score\":0.65,\"label\":\"MEDIUM\"}}'),
        -- Medium security risk, has tests
        ('sym2', 'validate_token', 'function', 'src/auth/token.rs', 12, 30, 'public', 'rust', 0.72,
         '{\"security_risk\":{\"score\":0.45,\"label\":\"MEDIUM\"},\"change_risk\":{\"score\":0.3,\"label\":\"LOW\"},\"test_coverage\":{\"best_tier\":\"thorough\",\"worst_tier\":\"adequate\",\"test_count\":3}}'),
        -- No security risk, has tests, high centrality
        ('sym3', 'format_output', 'function', 'src/utils/format.rs', 5, 15, 'public', 'rust', 0.95,
         '{\"change_risk\":{\"score\":0.2,\"label\":\"LOW\"},\"test_coverage\":{\"best_tier\":\"adequate\",\"worst_tier\":\"stub\",\"test_count\":1}}'),
        -- Test function (should be excludable)
        ('sym4', 'test_process_input', 'function', 'src/tests/handlers.rs', 10, 20, 'private', 'rust', 0.0,
         '{\"is_test\":true,\"test_quality\":{\"tier\":\"thorough\"}}'),
        -- Low centrality, no tests (dead code candidate)
        ('sym5', 'unused_helper', 'function', 'src/utils/old.rs', 1, 5, 'private', 'rust', 0.0,
         '{\"change_risk\":{\"score\":0.1,\"label\":\"LOW\"}}'),
        -- Class symbol
        ('sym6', 'UserService', 'class', 'src/services/user.rs', 1, 100, 'public', 'rust', 0.6,
         '{\"security_risk\":{\"score\":0.3,\"label\":\"LOW\"},\"change_risk\":{\"score\":0.5,\"label\":\"MEDIUM\"}}');
    ").unwrap();

    (db, tmp) // Return tmp to keep it alive for test duration
}

#[test]
fn test_query_by_security_risk_desc() {
    let (db, _tmp) = setup_test_db();
    let results = query_by_metrics(
        &db, "security_risk", "desc",
        None, None, None, None, None, None, 10,
    ).unwrap();

    assert!(!results.is_empty(), "Should return results");
    // First result should be highest security risk
    assert_eq!(results[0].name, "process_input");
    assert_eq!(results[0].security_risk_label.as_deref(), Some("HIGH"));
}

#[test]
fn test_query_by_centrality_asc_for_dead_code() {
    let (db, _tmp) = setup_test_db();
    let results = query_by_metrics(
        &db, "centrality", "asc",
        None, None, None, None, None, Some(true), 10,
    ).unwrap();

    // Should exclude test symbols, return lowest centrality first
    assert!(results.iter().all(|r| r.name != "test_process_input"), "Test symbols should be excluded");
    assert_eq!(results[0].name, "unused_helper", "Lowest centrality non-test symbol first");
}

#[test]
fn test_query_min_risk_filter() {
    let (db, _tmp) = setup_test_db();
    let results = query_by_metrics(
        &db, "security_risk", "desc",
        Some("medium"), None, None, None, None, None, 10,
    ).unwrap();

    // Should only return MEDIUM and HIGH
    for r in &results {
        let label = r.security_risk_label.as_deref().unwrap_or("none");
        assert!(label == "HIGH" || label == "MEDIUM", "Got unexpected label: {}", label);
    }
}

#[test]
fn test_query_has_tests_filter() {
    let (db, _tmp) = setup_test_db();
    // has_tests = false: symbols WITHOUT test coverage
    let results = query_by_metrics(
        &db, "security_risk", "desc",
        None, Some(false), None, None, None, Some(true), 10,
    ).unwrap();

    for r in &results {
        assert!(r.test_coverage_tier.is_none(), "{} should have no test coverage", r.name);
    }
}

#[test]
fn test_query_kind_filter() {
    let (db, _tmp) = setup_test_db();
    let results = query_by_metrics(
        &db, "centrality", "desc",
        None, None, Some("class"), None, None, None, 10,
    ).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "UserService");
}

#[test]
fn test_query_file_pattern_filter() {
    let (db, _tmp) = setup_test_db();
    let results = query_by_metrics(
        &db, "centrality", "desc",
        None, None, None, Some("src/utils/**"), None, None, 10,
    ).unwrap();

    for r in &results {
        assert!(r.file_path.starts_with("src/utils/"), "File {} doesn't match pattern", r.file_path);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib test_query_by_security_risk_desc 2>&1 | tail -10
```

Expected: FAIL — `query_by_metrics` returns empty vec.

- [ ] **Step 3: Implement `query_by_metrics` in `src/tools/metrics/query.rs`**

Replace the placeholder implementation:

```rust
//! SQL query building and result formatting for query_metrics tool

use crate::database::SymbolDatabase;
use crate::tools::search::matches_glob_pattern;
use anyhow::{Result, anyhow};
use tracing::debug;

/// Result from a metrics query
#[derive(Debug)]
pub struct MetricsResult {
    pub name: String,
    pub file_path: String,
    pub line_number: u32,
    pub kind: String,
    pub reference_score: f64,
    pub security_risk_score: Option<f64>,
    pub security_risk_label: Option<String>,
    pub change_risk_score: Option<f64>,
    pub change_risk_label: Option<String>,
    pub test_coverage_tier: Option<String>,
    pub test_count: Option<u32>,
    pub raw_metadata: Option<String>,
}

/// Query symbols ranked by a metadata field
pub fn query_by_metrics(
    db: &SymbolDatabase,
    sort_by: &str,
    order: &str,
    min_risk: Option<&str>,
    has_tests: Option<bool>,
    kind: Option<&str>,
    file_pattern: Option<&str>,
    language: Option<&str>,
    exclude_tests: Option<bool>,
    limit: u32,
) -> Result<Vec<MetricsResult>> {
    let limit = limit.min(100);

    // Build ORDER BY clause based on sort_by
    let order_dir = if order == "asc" { "ASC" } else { "DESC" };
    let order_clause = match sort_by {
        "security_risk" => format!(
            "COALESCE(json_extract(metadata, '$.security_risk.score'), 0.0) {}",
            order_dir
        ),
        "change_risk" => format!(
            "COALESCE(json_extract(metadata, '$.change_risk.score'), 0.0) {}",
            order_dir
        ),
        "test_coverage" => {
            // Sort by test coverage tier: map to numeric for ordering
            // thorough=4, adequate=3, thin=2, stub=1, NULL=0
            let tier_expr = "CASE json_extract(metadata, '$.test_coverage.best_tier') \
                WHEN 'thorough' THEN 4 WHEN 'adequate' THEN 3 \
                WHEN 'thin' THEN 2 WHEN 'stub' THEN 1 ELSE 0 END";
            format!("{} {}", tier_expr, order_dir)
        }
        "centrality" => format!("reference_score {}", order_dir),
        _ => return Err(anyhow!("Invalid sort_by: '{}'. Must be one of: security_risk, change_risk, test_coverage, centrality", sort_by)),
    };

    // Build WHERE clauses
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    // Kind filter
    if let Some(k) = kind {
        conditions.push("kind = ?".to_string());
        params.push(Box::new(k.to_string()));
    }

    // Language filter
    if let Some(lang) = language {
        conditions.push("language = ?".to_string());
        params.push(Box::new(lang.to_string()));
    }

    // Exclude test symbols
    if exclude_tests == Some(true) {
        conditions.push(
            "(json_extract(metadata, '$.is_test') IS NULL OR json_extract(metadata, '$.is_test') != 1)"
                .to_string(),
        );
    }

    // has_tests filter
    match has_tests {
        Some(true) => {
            conditions.push("json_extract(metadata, '$.test_coverage') IS NOT NULL".to_string());
        }
        Some(false) => {
            conditions.push("json_extract(metadata, '$.test_coverage') IS NULL".to_string());
        }
        None => {}
    }

    // min_risk filter (only applies to risk sort_by fields)
    if let Some(min) = min_risk {
        let risk_path = match sort_by {
            "security_risk" => Some("$.security_risk.label"),
            "change_risk" => Some("$.change_risk.label"),
            _ => None, // Ignored for non-risk metrics
        };
        if let Some(path) = risk_path {
            let labels = match min.to_lowercase().as_str() {
                "high" => vec!["HIGH"],
                "medium" => vec!["HIGH", "MEDIUM"],
                "low" => vec!["HIGH", "MEDIUM", "LOW"],
                _ => vec!["HIGH", "MEDIUM", "LOW"],
            };
            let placeholders: Vec<&str> = labels.iter().map(|_| "?").collect();
            conditions.push(format!(
                "json_extract(metadata, '{}') IN ({})",
                path,
                placeholders.join(", ")
            ));
            for label in labels {
                params.push(Box::new(label.to_string()));
            }
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Fetch more than limit to allow post-filtering by file_pattern
    let fetch_limit = if file_pattern.is_some() {
        limit * 5 // Over-fetch to compensate for file_pattern post-filtering
    } else {
        limit
    };

    let sql = format!(
        "SELECT name, file_path, start_line, kind, reference_score, metadata \
         FROM symbols {} ORDER BY {} LIMIT {}",
        where_clause, order_clause, fetch_limit
    );

    debug!("query_metrics SQL: {}", sql);

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = db.conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, u32>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, f64>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (name, file_path, line_number, kind, reference_score, metadata_str) = row?;

        // Post-filter by file_pattern using globset (same as fast_search)
        if let Some(pattern) = file_pattern {
            if !matches_glob_pattern(&file_path, pattern) {
                continue;
            }
        }

        // Parse metadata JSON
        let meta: serde_json::Value = metadata_str
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::json!({}));

        let result = MetricsResult {
            name,
            file_path,
            line_number,
            kind,
            reference_score,
            security_risk_score: meta.pointer("/security_risk/score").and_then(|v| v.as_f64()),
            security_risk_label: meta.pointer("/security_risk/label").and_then(|v| v.as_str()).map(String::from),
            change_risk_score: meta.pointer("/change_risk/score").and_then(|v| v.as_f64()),
            change_risk_label: meta.pointer("/change_risk/label").and_then(|v| v.as_str()).map(String::from),
            test_coverage_tier: meta.pointer("/test_coverage/best_tier").and_then(|v| v.as_str()).map(String::from),
            test_count: meta.pointer("/test_coverage/test_count").and_then(|v| v.as_u64()).map(|v| v as u32),
            raw_metadata: metadata_str,
        };

        results.push(result);

        if results.len() >= limit as usize {
            break;
        }
    }

    debug!("query_metrics returned {} results", results.len());
    Ok(results)
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --lib test_query_by_security_risk_desc 2>&1 | tail -10
cargo test --lib test_query_by_centrality_asc 2>&1 | tail -10
cargo test --lib test_query_min_risk_filter 2>&1 | tail -10
cargo test --lib test_query_has_tests_filter 2>&1 | tail -10
cargo test --lib test_query_kind_filter 2>&1 | tail -10
cargo test --lib test_query_file_pattern_filter 2>&1 | tail -10
```

Expected: All PASS.

- [ ] **Step 5: Commit**

```bash
git add src/tools/metrics/query.rs src/tests/tools/metrics/
git commit -m "feat(metrics): implement query_by_metrics SQL query engine"
```

---

### Task 3: Output Formatting + `call_tool` Integration

**Files:**
- Modify: `src/tools/metrics/query.rs`
- Modify: `src/tools/metrics/mod.rs`

- [ ] **Step 1: Write test for format_metrics_output**

Add to `src/tests/tools/metrics/query_metrics_tests.rs`:

```rust
use crate::tools::metrics::query::format_metrics_output;

#[test]
fn test_format_metrics_output_security_risk() {
    let results = vec![
        MetricsResult {
            name: "process_input".to_string(),
            file_path: "src/handlers/input.rs".to_string(),
            start_line: 45,
            kind: "function".to_string(),
            reference_score: 0.85,
            security_risk_score: Some(0.82),
            security_risk_label: Some("HIGH".to_string()),
            change_risk_score: Some(0.65),
            change_risk_label: Some("MEDIUM".to_string()),
            test_coverage_tier: None,
            test_count: None,
            raw_metadata: None,
        },
    ];

    let output = format_metrics_output(&results, "security_risk", "desc");
    assert!(output.contains("process_input"), "Should contain symbol name");
    assert!(output.contains("HIGH"), "Should contain risk label");
    assert!(output.contains("src/handlers/input.rs:45"), "Should contain file:line");
    assert!(output.contains("no tests"), "Should show no test coverage");
}

#[test]
fn test_format_metrics_output_empty() {
    let results = vec![];
    let output = format_metrics_output(&results, "security_risk", "desc");
    assert!(output.contains("No symbols found"), "Should show empty message");
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test --lib test_format_metrics_output_security_risk 2>&1 | tail -10
```

Expected: FAIL — returns empty string.

- [ ] **Step 3: Implement `format_metrics_output`**

Replace the placeholder in `src/tools/metrics/query.rs`:

```rust
/// Format metrics results into human-readable output
pub fn format_metrics_output(
    results: &[MetricsResult],
    sort_by: &str,
    order: &str,
) -> String {
    if results.is_empty() {
        return format!("No symbols found matching the query (sort_by: {}, order: {}).", sort_by, order);
    }

    let sort_label = match sort_by {
        "security_risk" => "security risk",
        "change_risk" => "change risk",
        "test_coverage" => "test coverage",
        "centrality" => "centrality",
        _ => sort_by,
    };
    let order_label = if order == "asc" { "ascending" } else { "descending" };

    let mut lines = vec![
        format!("Top {} symbols by {} ({}):\n", results.len(), sort_label, order_label),
    ];

    for (i, r) in results.iter().enumerate() {
        lines.push(format!(
            "{}. {} ({}:{})",
            i + 1, r.name, r.file_path, r.start_line
        ));

        let mut details = Vec::new();

        // Security risk
        if let Some(label) = &r.security_risk_label {
            details.push(format!("Security: {}", label));
        }

        // Change risk
        if let Some(label) = &r.change_risk_label {
            details.push(format!("Change Risk: {}", label));
        }

        // Test coverage
        match &r.test_coverage_tier {
            Some(tier) => {
                let count_str = r.test_count.map_or(String::new(), |c| format!(" ({} tests)", c));
                details.push(format!("Tests: {}{}", tier, count_str));
            }
            None => details.push("Tests: none".to_string()),
        }

        // Centrality
        details.push(format!("Centrality: {:.2}", r.reference_score));

        lines.push(format!("   {}", details.join(" | ")));
        lines.push(String::new()); // blank line between entries
    }

    lines.join("\n")
}
```

- [ ] **Step 4: Run formatting tests**

```bash
cargo test --lib test_format_metrics_output 2>&1 | tail -10
```

Expected: PASS.

- [ ] **Step 5: Wire up `call_tool` in `src/tools/metrics/mod.rs`**

Replace the placeholder `call_tool` implementation:

```rust
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};

impl QueryMetricsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!(
            "📊 query_metrics: sort_by={}, order={}, limit={}",
            self.sort_by, self.order, self.limit
        );

        // Resolve workspace (free function, same pattern as deep_dive/get_symbols)
        let workspace_target = resolve_workspace_filter(
            self.workspace.as_deref(), handler
        ).await?;

        // Build params for the blocking query closure
        let sort_by = self.sort_by.clone();
        let order = self.order.clone();
        let min_risk = self.min_risk.clone();
        let has_tests = self.has_tests;
        let kind = self.kind.clone();
        let file_pattern = self.file_pattern.clone();
        let language = self.language.clone();
        let exclude_tests = self.exclude_tests;
        let limit = self.limit;

        match workspace_target {
            WorkspaceTarget::Reference(ref_workspace_id) => {
                let db_arc = handler.get_database_for_workspace(&ref_workspace_id).await?;
                let result = tokio::task::spawn_blocking(move || -> Result<String> {
                    let db = db_arc.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
                    let results = query::query_by_metrics(
                        &db, &sort_by, &order, min_risk.as_deref(), has_tests,
                        kind.as_deref(), file_pattern.as_deref(), language.as_deref(),
                        exclude_tests, limit,
                    )?;
                    Ok(query::format_metrics_output(&results, &sort_by, &order))
                }).await.map_err(|e| anyhow::anyhow!("spawn_blocking: {}", e))??;
                Ok(CallToolResult::text_content(vec![Content::text(result)]))
            }
            WorkspaceTarget::Primary => {
                let workspace = handler.get_workspace().await?.ok_or_else(|| {
                    anyhow::anyhow!("No workspace initialized. Run manage_workspace(operation=\"index\") first.")
                })?;
                let db_arc = workspace.db.clone();
                let result = tokio::task::spawn_blocking(move || -> Result<String> {
                    let db = db_arc.lock().map_err(|e| anyhow::anyhow!("DB lock: {}", e))?;
                    let results = query::query_by_metrics(
                        &db, &sort_by, &order, min_risk.as_deref(), has_tests,
                        kind.as_deref(), file_pattern.as_deref(), language.as_deref(),
                        exclude_tests, limit,
                    )?;
                    Ok(query::format_metrics_output(&results, &sort_by, &order))
                }).await.map_err(|e| anyhow::anyhow!("spawn_blocking: {}", e))??;
                Ok(CallToolResult::text_content(vec![Content::text(result)]))
            }
        }
    }
}
```

- [ ] **Step 6: Verify it compiles**

```bash
cargo check 2>&1 | tail -10
```

Expected: Clean compile.

- [ ] **Step 7: Commit**

```bash
git add src/tools/metrics/
git commit -m "feat(metrics): implement output formatting and call_tool integration"
```

---

### Task 4: Register `query_metrics` in Handler

**Files:**
- Modify: `src/handler.rs`

- [ ] **Step 1: Add `#[tool]` registration in handler.rs**

In the `#[tool_router] impl JulieServerHandler` block (around line 432), add after the last tool registration:

```rust
    // ========== Metrics & Reporting Tools ==========

    #[tool(
        name = "query_metrics",
        description = "Query symbols ranked by analysis metadata (security risk, change risk, test coverage, centrality). Use to find the riskiest code, untested functions, dead code, or highest-centrality entry points.",
        annotations(
            title = "Query Code Metrics",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn query_metrics(
        &self,
        Parameters(params): Parameters<QueryMetricsTool>,
    ) -> Result<CallToolResult, McpError> {
        debug!("📊 Query metrics: {:?}", params);
        params
            .call_tool(self)
            .await
            .map_err(|e| McpError::internal_error(format!("query_metrics failed: {}", e), None))
    }
```

- [ ] **Step 2: Add import if needed**

Verify `QueryMetricsTool` is importable. The `use crate::tools::*` or explicit import should cover it via the re-export in `src/tools/mod.rs`.

- [ ] **Step 3: Verify it compiles**

```bash
cargo check 2>&1 | tail -5
```

Expected: Clean compile. The tool is now registered and will appear in MCP tool listing.

- [ ] **Step 4: Commit**

```bash
git add src/handler.rs
git commit -m "feat(metrics): register query_metrics tool in MCP handler"
```

---

### Task 5: `/codehealth` Skill

**Files:**
- Create: `.claude/skills/codehealth/SKILL.md`

- [ ] **Step 1: Create skill directory and SKILL.md**

```bash
mkdir -p .claude/skills/codehealth
```

Create `.claude/skills/codehealth/SKILL.md`:

```yaml
---
name: codehealth
description: Generate a codebase health report — risk hotspots, test gaps, dead code candidates, and prioritized recommendations. Use when the user asks about code quality, wants to find risky code, or asks "what should we fix first?"
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__query_metrics, mcp__julie__deep_dive, mcp__julie__get_context
---

# Codebase Health Report

Generate a comprehensive health report for the codebase (or a focused area if specified).

## Arguments

`$ARGUMENTS` is an optional area focus — a search query like "authentication", a file pattern like "src/tools/", or empty for the whole codebase.

## Query Pattern

### Step 1: Scope (if area specified)

If `$ARGUMENTS` is not empty, first orient on the area:

```
get_context(query="$ARGUMENTS")
```

Note the file paths of the pivots — use these to build a `file_pattern` for subsequent queries.

### Step 2: Gather Data

Run these queries (adjust `file_pattern` if scoped to an area):

1. **Risk Hotspots:**
```
query_metrics(sort_by="change_risk", order="desc", exclude_tests=true, limit=10)
```

2. **Test Gaps:**
```
query_metrics(sort_by="centrality", order="desc", has_tests=false, exclude_tests=true, limit=10)
```

3. **Dead Code Candidates:**
```
query_metrics(sort_by="centrality", order="asc", exclude_tests=true, limit=10)
```

### Step 3: Deep Dive on Worst Offenders

For the top 3-5 most concerning symbols from the risk hotspots and test gaps queries:

```
deep_dive(symbol="<name>", depth="overview")
```

This reveals callers, callees, and detailed metadata.

### Step 4: Security Quick Check

```
query_metrics(sort_by="security_risk", order="desc", min_risk="medium", exclude_tests=true, limit=5)
```

## Report Format

Present the report in this structure:

```markdown
# Codebase Health Report
**Scope:** [area or "Full codebase"] | **Date:** [today]

## Summary
- Total symbols analyzed: [from query results]
- Risk hotspots found: [count of HIGH/MEDIUM change_risk]
- Untested high-centrality code: [count from test gaps query]
- Security signals: [count from security check]

## Risk Hotspots
[Top 10 by change_risk, showing name, file:line, risk level, test coverage, centrality]
[For top 3-5, include a one-sentence explanation of why it's risky based on deep_dive]

## Test Gaps
[High-centrality symbols with no test coverage]
[Explain why each matters: "This function is called by N other functions but has no tests"]

## Dead Code Candidates
[Zero-centrality symbols, excluding obvious entry points like main/run/setup/new]
[Note: zero centrality may mean the symbol is an entry point not yet detected — flag uncertain cases]

## Security Signals
[Any medium+ security risk symbols, with brief explanation]

## Recommendations
[Prioritized list: what to fix first and why, based on risk × centrality × test coverage]
[Focus on actionable items, not exhaustive lists]
```

## Guidelines

- Keep the report concise — this is an executive summary, not a line-by-line audit
- Explain findings in plain language — assume the reader may not be deeply familiar with the code
- When centrality = 0 for a public function named like an entry point (main, run, setup, start, handler, index), note it's likely a legitimate entry point, not dead code
- If no concerning findings in a category, say so — "No high-risk items found" is useful information
```

- [ ] **Step 2: Commit**

```bash
git add .claude/skills/codehealth/
git commit -m "feat(skills): add /codehealth report skill"
```

---

### Task 6: `/security-audit` Skill

**Files:**
- Create: `.claude/skills/security-audit/SKILL.md`

- [ ] **Step 1: Create skill directory and SKILL.md**

```bash
mkdir -p .claude/skills/security-audit
```

Create `.claude/skills/security-audit/SKILL.md`:

```yaml
---
name: security-audit
description: Run a security audit of the codebase — finds injection sinks, untested security code, and high-exposure risks. Use when the user asks about security, wants a security review, or is concerned about AI-generated code safety.
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__query_metrics, mcp__julie__deep_dive, mcp__julie__get_context
---

# Security Audit

Analyze the codebase for security risks. Designed to be understandable by someone who isn't a security expert — explain WHY something is risky, not just that it was flagged.

## Arguments

`$ARGUMENTS` is an optional area focus. Empty = full codebase.

## Query Pattern

### Step 1: Find All Security Signals

```
query_metrics(sort_by="security_risk", order="desc", min_risk="low", exclude_tests=true, limit=30)
```

### Step 2: Find Untested Security-Sensitive Code (The Scariest Combination)

```
query_metrics(sort_by="security_risk", order="desc", has_tests=false, min_risk="medium", exclude_tests=true, limit=20)
```

### Step 3: Deep Dive on HIGH Risk Symbols

For every symbol with security_risk label "HIGH", run:

```
deep_dive(symbol="<name>", depth="context")
```

This reveals the actual code, callers (who uses this risky code?), and detailed security signals.

### Step 4: Check High-Exposure Risks

```
query_metrics(sort_by="security_risk", order="desc", min_risk="medium", exclude_tests=true, limit=15)
```

Cross-reference with centrality from the results — high centrality + high security risk = the most dangerous combination.

## Report Format

```markdown
# Security Audit Report
**Scope:** [area or "Full codebase"] | **Date:** [today]

## Executive Summary
- **Overall Risk Level:** [HIGH/MEDIUM/LOW based on findings]
- Security signals found: [total count]
  - HIGH: [count] | MEDIUM: [count] | LOW: [count]
- Untested security-sensitive code: [count] ⚠️

## Critical Findings

### Injection Risks
[Symbols with sink_calls signals — SQL, command execution, XSS]
For each:
- **Symbol:** name (file:line)
- **What's happening:** [Plain language: "This function builds SQL queries by concatenating user input"]
- **Why it's dangerous:** [Plain language: "An attacker could inject SQL commands through the input parameter"]
- **Who uses it:** [Callers from deep_dive — shows blast radius]
- **Test status:** [Has tests / No tests]
- **Recommendation:** [Use parameterized queries / input sanitization / etc.]

### Authentication & Authorization
[Symbols related to auth with security signals]

### Sensitive Data Handling
[Crypto usage, secrets patterns]

### Other Security Signals
[Remaining findings]

## Untested Security Code ⚠️
[The scariest section: known security-sensitive code with ZERO tests]
[For each: what it does, why it needs tests, what kind of test to write]

## High-Exposure Risks
[Security issues in high-centrality code — these are the most impactful to fix]
[Sorted by centrality × security_risk score]

## Recommendations (Priority Order)
1. [Most critical: untested HIGH-risk code]
2. [HIGH-risk code with tests but known injection patterns]
3. [MEDIUM-risk code in high-centrality positions]
4. [LOW-risk items worth monitoring]

For each recommendation:
- What to fix and where
- Why it matters (in plain language)
- Suggested approach (parameterized queries, input validation, auth checks, etc.)
```

## Guidelines

- **Explain everything in plain language.** The reader may be a non-technical founder or a junior dev who used AI to write the code.
- Security signals come from static analysis — they indicate PATTERNS that are risky, not confirmed vulnerabilities. Frame findings as "this pattern is risky because..." not "this is a vulnerability."
- The combination of untested + security-sensitive is the highest priority. A tested security-sensitive function is infinitely better than an untested one.
- High centrality amplifies risk — a vulnerable function called by 50 other functions is much worse than an isolated one.
- If NO security signals are found, say so clearly: "No security risk signals detected. This doesn't guarantee the code is secure, but no common risk patterns were found by static analysis."
```

- [ ] **Step 2: Commit**

```bash
git add .claude/skills/security-audit/
git commit -m "feat(skills): add /security-audit report skill"
```

---

### Task 7: `/architecture` Skill

**Files:**
- Create: `.claude/skills/architecture/SKILL.md`

- [ ] **Step 1: Create skill directory and SKILL.md**

```bash
mkdir -p .claude/skills/architecture
```

Create `.claude/skills/architecture/SKILL.md`:

```yaml
---
name: architecture
description: Generate an architecture overview — key entry points, module map, dependency flow, and suggested reading order. Use when the user is new to a codebase, asks "how does this work?", wants an architecture overview, or needs onboarding documentation.
user-invocable: true
disable-model-invocation: true
allowed-tools: mcp__julie__query_metrics, mcp__julie__deep_dive, mcp__julie__get_context, mcp__julie__get_symbols
---

# Architecture Overview

Generate a structural overview of the codebase or a specific area. Designed for onboarding, documentation, or understanding unfamiliar code.

## Arguments

`$ARGUMENTS` is the area to analyze. Can be a concept ("authentication"), a path ("src/tools/"), or empty for the whole codebase.

## Query Pattern

### Step 1: Oriented Discovery

```
get_context(query="$ARGUMENTS", format="readable")
```

This returns pivots (key symbols with code), neighbors (connected symbols), and a file map — all token-budgeted. The pivots are the starting point for understanding the area.

### Step 2: Find Key Entry Points

```
query_metrics(sort_by="centrality", order="desc", exclude_tests=true, limit=15)
```

High-centrality symbols are the most connected — they're the architectural backbone. Focus on public functions/methods.

### Step 3: Understand Entry Point Structure

For the top 5 highest-centrality symbols:

```
get_symbols(file_path="<file>", mode="structure", max_depth=1)
```

This shows the full outline of each key file without reading all the code.

### Step 4: Trace Key Connections

For the top 3 entry points:

```
deep_dive(symbol="<name>", depth="overview")
```

This reveals callers (who depends on this?) and callees (what does it use?) — the dependency flow.

## Report Format

```markdown
# Architecture Overview
**Area:** [area or "Full codebase"] | **Date:** [today]

## Overview
[2-3 paragraph description of what this area/codebase does, inferred from:
- Symbol names and kinds (what concepts exist?)
- File paths (how is it organized?)
- Doc comments on pivots (what do the key functions say they do?)
- Centrality patterns (what's important?)]

## Key Entry Points
[Top 10-15 highest-centrality public symbols, formatted as:]

| Symbol | Location | Centrality | Role |
|--------|----------|-----------|------|
| name | file:line | score | One-line description of what it does |

## Module Map
[Group files by directory/responsibility. For each group:]

### [Directory/Module Name]
**Purpose:** [What this module is responsible for]
**Key files:**
- `file.rs` — [what it contains, key exports]
- `file2.rs` — [what it contains]

## Dependency Flow
[How the key modules connect to each other]
[Use the caller/callee data from deep_dive to show:]

```
External input → [Entry Point A] → [Module B] → [Database Layer]
                → [Entry Point C] → [Module D] → [External API]
```

[Describe the main data flow paths in 3-5 sentences]

## Suggested Reading Order
[For someone new to this code, recommend which files to read first and why:]

1. **Start here:** `file.rs` — [why: it's the main entry point / defines the core types]
2. **Then read:** `file2.rs` — [why: it implements the key logic called by #1]
3. **Then read:** `file3.rs` — [why: it handles the output / storage / API layer]
4. **Reference as needed:** `file4.rs` — [why: utility code, read when you encounter calls to it]
```

## Guidelines

- Focus on STRUCTURE, not implementation details — this is a map, not a tutorial
- Use the centrality scores to identify what matters — high centrality = architecturally important
- Group files by what they DO, not by what they ARE (group by "authentication" not by "structs vs functions")
- The suggested reading order should form a narrative: start with the big picture, then drill into specifics
- If the codebase is small (<20 files), show everything. If large, focus on the highest-centrality modules
```

- [ ] **Step 2: Commit**

```bash
git add .claude/skills/architecture/
git commit -m "feat(skills): add /architecture overview skill"
```

---

### Task 8: Run `cargo xtask test dev` + Final Verification

**Files:** None (verification only)

- [ ] **Step 1: Run cargo xtask test dev**

```bash
cargo xtask test dev 2>&1 | tail -30
```

Expected: All buckets pass (except known pre-existing failures).

- [ ] **Step 2: Verify the tool appears in the MCP tool list**

Build a release binary and check that `query_metrics` appears:

```bash
cargo build --release 2>&1 | tail -5
```

- [ ] **Step 3: Verify skill files exist and have correct frontmatter**

```bash
head -5 .claude/skills/codehealth/SKILL.md
head -5 .claude/skills/security-audit/SKILL.md
head -5 .claude/skills/architecture/SKILL.md
```

Expected: Each shows YAML frontmatter with name, description, disable-model-invocation, allowed-tools.

- [ ] **Step 4: Update TODO.md**

Add a review note:

```markdown
- 2026-03-18 Added `query_metrics` MCP tool and 3 report skills (`/codehealth`, `/security-audit`, `/architecture`). Skills leverage existing analysis data (security_risk, change_risk, test_coverage, centrality) via the new metadata query tool. Phase 2: complexity metrics + `/hotspots`.
```

- [ ] **Step 5: Commit**

```bash
git add TODO.md
git commit -m "docs: add review note for codebase report skills"
```
