# Shelve Codehealth Metrics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove unreliable security risk, change risk, and test coverage metrics from all user-facing and agent-facing surfaces while keeping the analysis source files dormant for potential future use.

**Architecture:** The codehealth system has three broken metrics (security_risk, change_risk, test_coverage) that produce misleading data due to incomplete call graphs and heuristic miscalibration. We remove them from the indexing pipeline, tool output, query_metrics categories, dashboard, and skills. We keep test_quality (rates test functions themselves, not coverage) and centrality (used internally for search ranking). The snapshot infrastructure stays but is simplified to track only symbol/file counts.

**Tech Stack:** Rust, Tera templates, SQLite

---

### Task 1: Remove analysis calls from indexing pipeline

**Files:**
- Modify: `src/tools/workspace/indexing/processor.rs:472-500`
- Modify: `src/analysis/mod.rs`

- [ ] **Step 1: Remove three analysis calls from processor**

In `src/tools/workspace/indexing/processor.rs`, delete the three blocks that call `compute_test_coverage`, `compute_change_risk_scores`, and `compute_security_risk` (lines ~472-500). Keep the `compute_test_quality_metrics` block (lines ~462-470) and the `compute_reference_scores` block above it. Keep the codehealth snapshot block below (~502-513).

Delete these three blocks:

```rust
                // Compute test-to-code coverage linkage
                let t = std::time::Instant::now();
                if let Err(e) = crate::analysis::compute_test_coverage(&db_lock) {
                    warn!("Failed to compute test coverage: {}", e);
                }
                info!(
                    "⏱️  compute_test_coverage: {:.2}s",
                    t.elapsed().as_secs_f64()
                );

                // Compute change risk scores
                let t = std::time::Instant::now();
                if let Err(e) = crate::analysis::compute_change_risk_scores(&db_lock) {
                    warn!("Failed to compute change risk scores: {}", e);
                }
                info!(
                    "⏱️  compute_change_risk_scores: {:.2}s",
                    t.elapsed().as_secs_f64()
                );

                // Compute structural security risk scores
                let t = std::time::Instant::now();
                if let Err(e) = crate::analysis::compute_security_risk(&db_lock) {
                    warn!("Failed to compute security risk: {}", e);
                }
                info!(
                    "⏱️  compute_security_risk: {:.2}s",
                    t.elapsed().as_secs_f64()
                );
```

- [ ] **Step 2: Remove public re-exports from analysis mod**

In `src/analysis/mod.rs`, remove the re-exports and module declarations for the three shelved modules. Keep `test_quality`.

Before:
```rust
pub mod change_risk;
pub mod security_risk;
pub mod test_coverage;
pub mod test_quality;

pub use change_risk::compute_change_risk_scores;
pub use security_risk::compute_security_risk;
pub use test_coverage::compute_test_coverage;
pub use test_quality::compute_test_quality_metrics;
```

After:
```rust
pub mod test_quality;

pub use test_quality::compute_test_quality_metrics;
```

Note: The `change_risk.rs`, `security_risk.rs`, and `test_coverage.rs` files stay on disk but become dead code. They're shelved, not deleted.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: Successful compilation (possibly with dead code warnings for the shelved modules, that's fine)

If there are compilation errors from other modules still importing the removed re-exports, fix them in subsequent tasks.

- [ ] **Step 4: Commit**

```bash
git add src/tools/workspace/indexing/processor.rs src/analysis/mod.rs
git commit -m "refactor: remove security_risk, change_risk, test_coverage from indexing pipeline

These metrics produced unreliable data (3% test coverage on a well-tested
project, inflated security risk scores). The analysis source files are
kept on disk but no longer called during indexing."
```

---

### Task 2: Strip risk labels from deep_dive output

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs`

- [ ] **Step 1: Remove format_change_risk_info and format_security_risk_info functions**

In `src/tools/deep_dive/formatting.rs`, delete the entire `format_change_risk_info` function (starts at line ~179, ~90 lines) and the entire `format_security_risk_info` function (starts at line ~272, ~100 lines).

- [ ] **Step 2: Remove all calls to the deleted functions**

There are 12 call sites in the same file, appearing in pairs at lines ~464-465, ~503-504, ~593-594, ~642-643, ~711-712, ~736-737. Delete each pair:

```rust
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
```

These appear in the formatting functions for each depth level (overview, context, full) and each symbol kind.

- [ ] **Step 3: Verify it compiles**

Run: `cargo build 2>&1 | tail -5`
Expected: Successful compilation

- [ ] **Step 4: Run targeted deep_dive tests**

Run: `cargo test --lib deep_dive 2>&1 | tail -20`
Expected: All tests pass. The test `test_change_risk_labels_say_dependents_not_callers` will either need to be removed or will fail since it asserts on "Change Risk:" output.

If `test_change_risk_labels_say_dependents_not_callers` fails, delete it from `src/tests/tools/deep_dive_tests.rs` (search for that function name).

- [ ] **Step 5: Commit**

```bash
git add src/tools/deep_dive/formatting.rs src/tests/tools/deep_dive_tests.rs
git commit -m "refactor: remove change_risk and security_risk from deep_dive output

Agents were seeing misleading risk labels like 'Security Risk: HIGH'
on harmless helper functions. The data quality didn't justify surfacing
these to agents who might act on them."
```

---

### Task 3: Strip risk labels from get_context output

**Files:**
- Modify: `src/tools/get_context/formatting.rs`
- Modify: `src/tools/get_context/pipeline.rs`
- Modify: `src/tests/tools/get_context_formatting_tests.rs`

- [ ] **Step 1: Remove risk_label and security_label from PivotEntry struct**

In `src/tools/get_context/formatting.rs`, remove these two fields from the `PivotEntry` struct (lines ~62-64):

```rust
    /// Change risk label (HIGH/MEDIUM/LOW) from metadata, if available.
    pub risk_label: Option<String>,
    /// Security risk label (HIGH/MEDIUM/LOW) from metadata, if available.
    pub security_label: Option<String>,
```

Keep `test_quality_label` (line ~66).

- [ ] **Step 2: Remove risk tag formatting from format_context_readable**

In `format_context_readable` (~line 148-157), remove:

```rust
        let risk_tag = pivot
            .risk_label
            .as_ref()
            .map(|l| format!("  [{} risk]", l))
            .unwrap_or_default();
        let security_tag = pivot
            .security_label
            .as_ref()
            .map(|l| format!("  [{} security]", l))
            .unwrap_or_default();
```

And update the format string that uses them (line ~164) to remove `{risk_tag}{security_tag}`:

Before:
```rust
            "{}:{} ({}){}{}{}\n",
            pivot.file_path, pivot.start_line, pivot.kind, risk_tag, security_tag, quality_tag
```

After:
```rust
            "{}:{} ({}){}\n",
            pivot.file_path, pivot.start_line, pivot.kind, quality_tag
```

- [ ] **Step 3: Remove risk tag formatting from format_context_compact**

In `format_context_compact` (~line 229-238), remove:

```rust
        let risk_tag = pivot
            .risk_label
            .as_ref()
            .map(|l| format!(" risk={}", l))
            .unwrap_or_default();
        let security_tag = pivot
            .security_label
            .as_ref()
            .map(|l| format!(" security={}", l))
            .unwrap_or_default();
```

And update the PIVOT format string (~line 244-249) to remove `{}{}` for risk/security tags:

Before:
```rust
            "PIVOT {} {}:{} kind={} centrality={}{}{}{}\n",
            pivot.name, pivot.file_path, pivot.start_line, pivot.kind,
            label, risk_tag, security_tag, quality_tag
```

After:
```rust
            "PIVOT {} {}:{} kind={} centrality={}{}\n",
            pivot.name, pivot.file_path, pivot.start_line, pivot.kind,
            label, quality_tag
```

- [ ] **Step 4: Remove risk label extraction from pipeline**

In `src/tools/get_context/pipeline.rs` (~lines 395-411), remove the `risk_label` and `security_label` extraction blocks:

```rust
        let risk_label = batch
            .full_symbols
            .get(&pivot.result.id)
            .and_then(|sym| sym.metadata.as_ref())
            .and_then(|m| m.get("change_risk"))
            .and_then(|r| r.get("label"))
            .and_then(|l| l.as_str())
            .map(String::from);

        let security_label = batch
            .full_symbols
            .get(&pivot.result.id)
            .and_then(|sym| sym.metadata.as_ref())
            .and_then(|m| m.get("security_risk"))
            .and_then(|r| r.get("label"))
            .and_then(|l| l.as_str())
            .map(String::from);
```

And remove `risk_label,` and `security_label,` from the `PivotEntry` construction (~lines 431-432).

- [ ] **Step 5: Fix test helpers**

In `src/tests/tools/get_context_formatting_tests.rs`, remove `risk_label: None,` and `security_label: None,` from any PivotEntry construction in test helpers (line ~35-36).

- [ ] **Step 6: Verify it compiles and tests pass**

Run: `cargo test --lib get_context 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add src/tools/get_context/formatting.rs src/tools/get_context/pipeline.rs src/tests/tools/get_context_formatting_tests.rs
git commit -m "refactor: remove risk and security labels from get_context output"
```

---

### Task 4: Remove code_health and trend categories from query_metrics

**Files:**
- Modify: `src/tools/metrics/mod.rs`
- Modify: `src/tools/metrics/query.rs`
- Modify: `src/tools/metrics/trend.rs`
- Modify: `src/tests/tools/metrics/query_metrics_tests.rs`
- Modify: `src/tests/tools/metrics/trend_tests.rs`

- [ ] **Step 1: Remove code_health and trend match arms from QueryMetricsTool::call_tool**

In `src/tools/metrics/mod.rs`, remove the `"trend"` match arm (lines ~106-156) and the `"code_health"` match arm (lines ~192-249).

Update the error message in the catch-all arm:

Before:
```rust
"Unknown category '{}'. Valid categories: code_health, session, history, trend."
```

After:
```rust
"Unknown category '{}'. Valid categories: session, history."
```

- [ ] **Step 2: Update defaults and struct docs**

In `src/tools/metrics/mod.rs`:

Change `default_category` to return `"session"`:
```rust
fn default_category() -> String {
    "session".to_string()
}
```

Remove `default_sort_by` function entirely (no longer needed without code_health).

Update the `QueryMetricsTool` struct:
- Change `category` doc to: `/// Metrics category: "session" (default) or "history"`
- Remove `sort_by`, `min_risk`, `has_tests`, `kind`, `file_pattern`, `language`, `exclude_tests` fields entirely (they were only used by code_health)
- Keep: `category`, `order`, `limit`, `workspace`

- [ ] **Step 3: Remove query.rs module (code_health queries)**

The entire `src/tools/metrics/query.rs` file is only used by the code_health category. Remove the module declaration from `src/tools/metrics/mod.rs`:

```rust
pub(crate) mod query;
```

The file itself stays on disk as dead code (shelved).

- [ ] **Step 4: Remove trend.rs module declaration**

Remove from `src/tools/metrics/mod.rs`:

```rust
pub(crate) mod trend;
```

The file stays on disk as dead code.

- [ ] **Step 5: Delete trend and query_metrics tests**

Delete the test files that test shelved functionality:
- `src/tests/tools/metrics/query_metrics_tests.rs` - entirely about code_health queries
- `src/tests/tools/metrics/trend_tests.rs` - entirely about trend formatting

Update `src/tests/tools/metrics/mod.rs` to remove those module declarations.

- [ ] **Step 6: Verify it compiles and tests pass**

Run: `cargo test --lib metrics 2>&1 | tail -20`
Expected: All tests pass (session and history tests should still work)

- [ ] **Step 7: Commit**

```bash
git add src/tools/metrics/mod.rs src/tests/tools/metrics/
git commit -m "refactor: remove code_health and trend categories from query_metrics

Only session and history categories remain. These provide tool call
statistics and session data, which are based on real operational data."
```

---

### Task 5: Simplify codehealth snapshot to symbol/file counts only

**Files:**
- Modify: `src/daemon/database.rs`
- Modify: `src/tests/daemon/database.rs`

- [ ] **Step 1: Simplify CodehealthSnapshot struct**

In `src/daemon/database.rs`, simplify the `CodehealthSnapshot` struct (~line 724) to only keep:

```rust
#[derive(Debug, Clone, Default)]
pub struct CodehealthSnapshot {
    pub total_symbols: i64,
    pub total_files: i64,
}
```

Remove: `security_high`, `security_medium`, `security_low`, `change_high`, `change_medium`, `change_low`, `symbols_tested`, `symbols_untested`, `avg_centrality`, `max_centrality`.

- [ ] **Step 2: Simplify CodehealthSnapshotRow struct**

Similarly simplify `CodehealthSnapshotRow` (~line 740):

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodehealthSnapshotRow {
    pub id: i64,
    pub workspace_id: String,
    pub timestamp: i64,
    pub total_symbols: i64,
    pub total_files: i64,
}
```

Update the `from_row` implementation to only read the columns that remain.

- [ ] **Step 3: Simplify insert_codehealth_snapshot**

In `insert_codehealth_snapshot` (~line 510), simplify the INSERT to only include `total_symbols` and `total_files`:

```rust
pub fn insert_codehealth_snapshot(&self, workspace_id: &str, snapshot: &CodehealthSnapshot) -> Result<()> {
    let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Lock: {e}"))?;
    conn.execute(
        "INSERT INTO codehealth_snapshots (workspace_id, total_symbols, total_files)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![workspace_id, snapshot.total_symbols, snapshot.total_files],
    )?;
    Ok(())
}
```

- [ ] **Step 4: Simplify snapshot_codehealth_from_db**

In `snapshot_codehealth_from_db` (~line 596), remove all the security/change/coverage/centrality queries. Keep only:

```rust
pub fn snapshot_codehealth_from_db(
    &self,
    workspace_id: &str,
    symbols_db: &crate::database::SymbolDatabase,
) -> Result<()> {
    let conn = &symbols_db.conn;

    let total_symbols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE kind NOT IN ('import', 'export') \
             AND (content_type IS NULL OR content_type != 'documentation')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_files: i64 = conn
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap_or(0);

    let snapshot = CodehealthSnapshot {
        total_symbols,
        total_files,
    };

    self.insert_codehealth_snapshot(workspace_id, &snapshot)
}
```

- [ ] **Step 5: Simplify get_latest_snapshot and get_snapshot_history**

Update these two functions to only SELECT the remaining columns. The SQL schema still has the old columns (we're not migrating the table), so the SELECT just needs to read fewer columns.

For `get_latest_snapshot`:
```rust
"SELECT id, workspace_id, timestamp, total_symbols, total_files
 FROM codehealth_snapshots WHERE workspace_id = ?1
 ORDER BY timestamp DESC LIMIT 1"
```

For `get_snapshot_history`:
```rust
"SELECT id, workspace_id, timestamp, total_symbols, total_files
 FROM codehealth_snapshots WHERE workspace_id = ?1
 ORDER BY timestamp DESC LIMIT ?2"
```

- [ ] **Step 6: Fix snapshot tests**

In `src/tests/daemon/database.rs`, update the snapshot tests to use the simplified structs. Tests that assert on security/change/coverage fields should be simplified to only check `total_symbols` and `total_files`.

- [ ] **Step 7: Verify it compiles and tests pass**

Run: `cargo test --lib snapshot 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 8: Commit**

```bash
git add src/daemon/database.rs src/tests/daemon/database.rs
git commit -m "refactor: simplify codehealth snapshots to symbol/file counts only

Risk counts, test coverage counts, and centrality stats removed from
snapshots. The DB schema retains the columns (no migration needed),
but new snapshots only populate total_symbols and total_files."
```

---

### Task 6: Remove centrality from dashboard detail template

**Files:**
- Modify: `dashboard/templates/partials/project_detail.html`

- [ ] **Step 1: Remove centrality rows from Symbol Stats column**

In `dashboard/templates/partials/project_detail.html`, remove these two conditional blocks (~lines 58-69):

```html
        {% if health.avg_centrality %}
        <tr>
          <td style="color: var(--julie-text-muted); border: none; padding: 0.25rem 0.5rem 0.25rem 0;">Avg centrality</td>
          <td style="border: none; padding: 0.25rem 0;">{{ health.avg_centrality | round(precision=1) }}</td>
        </tr>
        {% endif %}
        {% if health.max_centrality %}
        <tr>
          <td style="color: var(--julie-text-muted); border: none; padding: 0.25rem 0.5rem 0.25rem 0;">Max centrality</td>
          <td style="border: none; padding: 0.25rem 0;">{{ health.max_centrality | round(precision=0) }}</td>
        </tr>
        {% endif %}
```

The Symbol Stats column now shows only "Code symbols" and "Total files".

- [ ] **Step 2: Commit**

```bash
git add dashboard/templates/partials/project_detail.html
git commit -m "refactor: remove centrality from dashboard project detail

Centrality is an internal search ranking signal, not meaningful as a
user-facing metric."
```

---

### Task 7: Delete codehealth and security-audit skills

**Files:**
- Delete: `.claude/skills/codehealth/SKILL.md`
- Delete: `.claude/skills/security-audit/SKILL.md`

- [ ] **Step 1: Delete the skill files**

```bash
rm .claude/skills/codehealth/SKILL.md
rmdir .claude/skills/codehealth
rm .claude/skills/security-audit/SKILL.md
rmdir .claude/skills/security-audit
```

- [ ] **Step 2: Verify remaining skills are unaffected**

Check that `architecture` and `metrics` skills don't reference any removed categories:

The `architecture` skill uses `query_metrics(sort_by="centrality")` which relies on code_health category. Update `.claude/skills/architecture/SKILL.md` line 30:

Replace:
```
query_metrics(sort_by="centrality", order="desc", exclude_tests=true, limit=15)
```

With:
```
fast_search(query="<project area>", search_target="definitions", limit=15)
```

And update the allowed-tools to remove `mcp__julie__query_metrics` if it's only used for that one call. Check the full file first to confirm.

The `metrics` skill uses only `session` and `history` categories, so it's fine.

- [ ] **Step 3: Commit**

```bash
git add -A .claude/skills/
git commit -m "refactor: delete codehealth and security-audit skills

These skills depended on unreliable risk scoring and test coverage
metrics that have been shelved."
```

---

### Task 8: Run full test suite and fix any remaining breakage

**Files:**
- Any files with remaining compilation errors or test failures

- [ ] **Step 1: Run cargo xtask test dev**

Run: `cargo xtask test dev 2>&1 | tail -30`
Expected: All 8 buckets pass

- [ ] **Step 2: Fix any failures**

If there are compilation errors from imports of removed types or functions, fix them. Common places to check:
- Any file importing from `crate::analysis::change_risk`, `crate::analysis::security_risk`, or `crate::analysis::test_coverage`
- Any test file constructing `CodehealthSnapshot` or `CodehealthSnapshotRow` with the old fields
- The `get_context_scoring_tests.rs` file uses symbol names like `test_compute_security_risk` and `compute_security_risk` as test data (not actual imports), so those should be fine

- [ ] **Step 3: Run cargo clippy**

Run: `cargo clippy 2>&1 | grep -E '^(error|warning)' | head -20`
Expected: No new warnings beyond existing ones. Dead code warnings for the shelved analysis modules are acceptable.

- [ ] **Step 4: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "fix: resolve remaining compilation and test issues from codehealth shelving"
```

---

### Task 9: Update CLAUDE.md to reflect shelved status

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update the codehealth references in CLAUDE.md**

Search CLAUDE.md for any references to codehealth, security risk, test coverage metrics, or the removed skills. Update the "Project Overview" or any section that mentions these features to note they're shelved.

If there's a section mentioning codehealth as a feature, update it to note the metrics system is shelved due to data quality concerns but the snapshot infrastructure tracks symbol/file counts over time.

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md to reflect shelved codehealth metrics"
```
