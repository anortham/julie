# Security Risk Signals Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Per-symbol structural security risk scores (exposure, input handling, sink calls, blast radius, untested) surfaced in `deep_dive` and `get_context` output.

**Architecture:** New analysis module `security_risk.rs` runs post-indexing after `compute_change_risk_scores()`. Pre-loads identifier and relationship callee data in batch, matches sink patterns using final-segment case-insensitive matching, computes weighted score, stores in `metadata["security_risk"]`. Tool formatting reads metadata at display time.

**Tech Stack:** Rust, SQLite (json_extract), regex, serde_json, existing analysis pipeline pattern

**Spec:** `docs/superpowers/specs/2026-03-14-security-risk-signals-design.md`

---

## File Structure

| File | Responsibility | Change |
|------|---------------|--------|
| `src/analysis/mod.rs` | Analysis module root | Add `pub mod security_risk;` + re-export |
| `src/analysis/security_risk.rs` | **NEW** — Signal detection, scoring, `compute_security_risk()` | ~300 lines |
| `src/database/identifiers.rs` | Identifier queries | Add `get_call_identifiers_grouped()` method |
| `src/tools/workspace/indexing/processor.rs` | Indexing pipeline | Hook after `compute_change_risk_scores()` at ~line 530 |
| `src/tools/deep_dive/formatting.rs` | Deep dive output | Add `format_security_risk_info()` |
| `src/tools/get_context/formatting.rs` | Get context output | Add `security_label` to `PivotEntry`, append to both formats |
| `src/tools/get_context/pipeline.rs` | Get context pipeline | Extract `security_label` from metadata |
| `src/tests/analysis/mod.rs` | Test module declarations | Add `pub mod security_risk_tests;` |
| `src/tests/analysis/security_risk_tests.rs` | **NEW** — Signal detection + scoring tests | ~350 lines |
| `src/tests/tools/get_context_formatting_tests.rs` | Existing test helper | Add `security_label: None` to `make_pivot` |

---

## Chunk 1: Analysis Module

### Task 1: Signal helpers and types

**Files:**
- Create: `src/analysis/security_risk.rs`
- Modify: `src/analysis/mod.rs:1-14`
- Create: `src/tests/analysis/security_risk_tests.rs`
- Modify: `src/tests/analysis/mod.rs:1-3`

- [ ] **Step 1: Declare the module**

In `src/analysis/mod.rs`, add after `pub mod change_risk;`:

```rust
pub mod security_risk;
```

Add re-export after `pub use change_risk::compute_change_risk_scores;`:

```rust
pub use security_risk::compute_security_risk;
```

In `src/tests/analysis/mod.rs`, add after `pub mod change_risk_tests;`:

```rust
pub mod security_risk_tests;
```

- [ ] **Step 2: Create security_risk.rs with types and signal helpers**

Create `src/analysis/security_risk.rs`:

```rust
//! Structural security risk analysis: per-symbol scoring based on
//! exposure, input handling, sink calls, blast radius, and test coverage.
//!
//! Runs post-indexing after change_risk. Pre-loads callee data in batch,
//! then scores each symbol that triggers at least one security signal.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::{debug, info};

use crate::database::SymbolDatabase;
use crate::extractors::SymbolKind;

// =============================================================================
// Weights
// =============================================================================

const W_EXPOSURE: f64 = 0.25;
const W_INPUT_HANDLING: f64 = 0.25;
const W_SINK_CALLS: f64 = 0.30;
const W_BLAST_RADIUS: f64 = 0.10;
const W_UNTESTED: f64 = 0.10;

// =============================================================================
// Types
// =============================================================================

/// Summary stats from running security risk analysis.
#[derive(Debug, Clone, Default)]
pub struct SecurityRiskStats {
    pub total_scored: usize,
    pub high_risk: usize,
    pub medium_risk: usize,
    pub low_risk: usize,
    pub skipped_no_signals: usize,
}

// =============================================================================
// Sink patterns
// =============================================================================

/// Category A: Command/code execution sinks.
const EXECUTION_SINKS: &[&str] = &[
    "exec", "eval", "system", "popen", "spawn", "fork",
    "shell_exec", "subprocess", "shellexecute", "createprocess",
];

/// Category B: Database/query operation sinks.
const DATABASE_SINKS: &[&str] = &[
    "execute", "raw_sql", "exec_query", "executequery",
    "executeupdate", "rawquery", "runsql",
];

/// All sink patterns combined (lowercase for case-insensitive matching).
fn all_sink_patterns() -> Vec<&'static str> {
    let mut patterns = Vec::with_capacity(EXECUTION_SINKS.len() + DATABASE_SINKS.len());
    patterns.extend_from_slice(EXECUTION_SINKS);
    patterns.extend_from_slice(DATABASE_SINKS);
    patterns
}

// =============================================================================
// Input handling patterns (matched against signature parameter portion)
// =============================================================================

const INPUT_PATTERNS: &[&str] = &[
    // Web request types
    "Request", "HttpRequest", "HttpServletRequest", "ActionContext",
    "req:", "request:", "ctx:",
    // Query/form/body parameter types
    "Query", "Form", "Body", "Params", "FormData", "MultipartFile",
    "QueryString", "RouteParams",
    // Raw string/byte types in parameter position
    "&str", "String", "string", "str,", "str)", "bytes",
    "[]byte", "InputStream", "ByteArray", "Vec<u8>", "&[u8]",
];

// =============================================================================
// Signal computation helpers
// =============================================================================

/// Security-specific kind weight. Lower for containers/data than change_risk
/// because security risk is primarily about callable code.
/// Returns None for Import/Export (excluded from scoring).
pub fn security_kind_weight(kind: &SymbolKind) -> Option<f64> {
    match kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
        | SymbolKind::Destructor | SymbolKind::Operator => Some(1.0),
        SymbolKind::Class | SymbolKind::Struct | SymbolKind::Interface
        | SymbolKind::Trait | SymbolKind::Enum | SymbolKind::Union
        | SymbolKind::Module | SymbolKind::Namespace | SymbolKind::Type
        | SymbolKind::Delegate => Some(0.3),
        SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Property
        | SymbolKind::Field | SymbolKind::EnumMember | SymbolKind::Event => Some(0.1),
        SymbolKind::Import | SymbolKind::Export => None,
    }
}

/// Compute exposure signal: visibility * security_kind_weight.
pub fn exposure_score(visibility: Option<&str>, kind: &SymbolKind) -> f64 {
    let vis = match visibility {
        Some("public") => 1.0,
        Some("protected") => 0.5,
        Some("private") => 0.2,
        _ => 0.5,
    };
    let kw = security_kind_weight(kind).unwrap_or(0.0);
    vis * kw
}

/// Check if a signature's parameter portion contains input-handling patterns.
/// Splits at return type delimiter to avoid matching return types.
pub fn has_input_handling(signature: Option<&str>) -> bool {
    let sig = match signature {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };

    // Extract parameter portion only (before return type delimiter)
    let param_portion = extract_parameter_portion(sig);

    INPUT_PATTERNS.iter().any(|pattern| param_portion.contains(pattern))
}

/// Extract the parameter portion of a signature, excluding return type.
/// Handles: `-> Type` (Rust), `: Type` after `)` (TS/Python), `returns` keyword.
fn extract_parameter_portion(signature: &str) -> &str {
    // Try Rust/Swift style: find last " -> "
    if let Some(pos) = signature.rfind(" -> ") {
        return &signature[..pos];
    }
    // Try finding closing paren — everything before it is params
    if let Some(pos) = signature.rfind(')') {
        return &signature[..=pos];
    }
    // Fallback: use full signature
    signature
}

/// Match a callee name against sink patterns using final-segment case-insensitive matching.
/// Split by `::` and `.`, exact-match the final segment.
pub fn matches_sink_pattern(callee_name: &str, patterns: &[&str]) -> Option<String> {
    let final_segment = callee_name
        .rsplit(|c| c == ':' || c == '.')
        .next()
        .unwrap_or(callee_name)
        .to_lowercase();

    for pattern in patterns {
        if final_segment == *pattern {
            return Some(final_segment.clone());
        }
    }
    None
}

/// Compute sink calls signal from pre-loaded callee data.
/// Returns (score, detected_sink_names).
pub fn compute_sink_signal(
    callees_from_identifiers: &[String],
    callees_from_relationships: &[String],
    patterns: &[&str],
) -> (f64, Vec<String>) {
    let mut matched_sinks: HashSet<String> = HashSet::new();

    for callee in callees_from_identifiers.iter().chain(callees_from_relationships.iter()) {
        if let Some(sink_name) = matches_sink_pattern(callee, patterns) {
            matched_sinks.insert(sink_name);
        }
    }

    let mut sink_names: Vec<String> = matched_sinks.into_iter().collect();
    sink_names.sort();
    sink_names.truncate(5);

    let score = match sink_names.len() {
        0 => 0.0,
        1 => 0.7,
        _ => 1.0,
    };

    (score, sink_names)
}

/// Normalize reference_score to 0.0-1.0 using log sigmoid (same as change_risk).
pub fn normalize_blast_radius(reference_score: f64, p95: f64) -> f64 {
    if p95 <= 0.0 {
        return 0.0;
    }
    let normalized = (1.0 + reference_score).ln() / (1.0 + p95).ln();
    normalized.min(1.0)
}

/// Compute final security risk score.
pub fn compute_score(exposure: f64, input_handling: f64, sink_calls: f64, blast_radius: f64, untested: f64) -> f64 {
    W_EXPOSURE * exposure + W_INPUT_HANDLING * input_handling + W_SINK_CALLS * sink_calls + W_BLAST_RADIUS * blast_radius + W_UNTESTED * untested
}

/// Map score to tier label.
pub fn risk_label(score: f64) -> &'static str {
    if score >= 0.7 { "HIGH" }
    else if score >= 0.4 { "MEDIUM" }
    else { "LOW" }
}
```

- [ ] **Step 3: Write tests for signal helpers**

Create `src/tests/analysis/security_risk_tests.rs`:

```rust
//! Tests for structural security risk analysis.

#[cfg(test)]
mod tests {
    use crate::analysis::security_risk::*;
    use crate::extractors::SymbolKind;

    // =========================================================================
    // Exposure signal
    // =========================================================================

    #[test]
    fn test_exposure_public_function() {
        let score = exposure_score(Some("public"), &SymbolKind::Function);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_exposure_private_function() {
        let score = exposure_score(Some("private"), &SymbolKind::Function);
        assert!((score - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_exposure_public_struct() {
        // Container kind_weight = 0.3 for security
        let score = exposure_score(Some("public"), &SymbolKind::Struct);
        assert!((score - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_exposure_null_visibility() {
        let score = exposure_score(None, &SymbolKind::Function);
        assert_eq!(score, 0.5); // NULL = moderate
    }

    // =========================================================================
    // Input handling signal
    // =========================================================================

    #[test]
    fn test_input_handling_rust_str_param() {
        assert!(has_input_handling(Some("pub fn process(input: &str) -> Result<()>")));
    }

    #[test]
    fn test_input_handling_java_request() {
        assert!(has_input_handling(Some("public void handle(HttpServletRequest req, HttpServletResponse resp)")));
    }

    #[test]
    fn test_input_handling_python_string() {
        assert!(has_input_handling(Some("def process(data: str) -> bool")));
    }

    #[test]
    fn test_input_handling_no_match() {
        assert!(!has_input_handling(Some("pub fn compute(count: u32) -> u64")));
    }

    #[test]
    fn test_input_handling_return_type_not_matched() {
        // String is in return type, not params — should NOT match
        assert!(!has_input_handling(Some("pub fn get_name(id: u32) -> String")));
    }

    #[test]
    fn test_input_handling_none_signature() {
        assert!(!has_input_handling(None));
    }

    #[test]
    fn test_input_handling_empty_signature() {
        assert!(!has_input_handling(Some("")));
    }

    // =========================================================================
    // Sink matching
    // =========================================================================

    #[test]
    fn test_sink_match_exact() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("exec", patterns), Some("exec".to_string()));
    }

    #[test]
    fn test_sink_match_qualified_name() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("db.execute", patterns), Some("execute".to_string()));
    }

    #[test]
    fn test_sink_match_rust_qualified() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("conn::execute", patterns), Some("execute".to_string()));
    }

    #[test]
    fn test_sink_match_case_insensitive() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("db.Exec", patterns), Some("exec".to_string()));
    }

    #[test]
    fn test_sink_no_match_substring() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("execution_context", patterns), None);
    }

    #[test]
    fn test_sink_no_match_prefix() {
        let patterns = &["exec", "eval", "execute"];
        assert_eq!(matches_sink_pattern("executor", patterns), None);
    }

    // =========================================================================
    // Sink signal computation
    // =========================================================================

    #[test]
    fn test_sink_signal_no_matches() {
        let (score, names) = compute_sink_signal(&["foo".into()], &[], &["exec", "execute"]);
        assert_eq!(score, 0.0);
        assert!(names.is_empty());
    }

    #[test]
    fn test_sink_signal_one_match() {
        let (score, names) = compute_sink_signal(
            &["db.execute".into()], &[], &["exec", "execute"],
        );
        assert!((score - 0.7).abs() < 0.01);
        assert_eq!(names, vec!["execute"]);
    }

    #[test]
    fn test_sink_signal_multiple_matches() {
        let (score, names) = compute_sink_signal(
            &["db.execute".into(), "os.exec".into()], &[], &["exec", "execute"],
        );
        assert_eq!(score, 1.0);
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn test_sink_signal_deduplicates() {
        let (score, names) = compute_sink_signal(
            &["db.execute".into()],
            &["execute".into()], // same sink from relationship
            &["exec", "execute"],
        );
        assert!((score - 0.7).abs() < 0.01); // Still just one unique sink
        assert_eq!(names.len(), 1);
    }

    // =========================================================================
    // Score computation
    // =========================================================================

    #[test]
    fn test_high_security_risk() {
        // Public function, accepts string input, calls execute, high centrality, untested
        let score = compute_score(1.0, 1.0, 0.7, 0.8, 1.0);
        assert!(score >= 0.7, "Should be HIGH, got {:.2}", score);
        assert_eq!(risk_label(score), "HIGH");
    }

    #[test]
    fn test_low_security_risk() {
        // Private, no input handling, no sinks, low centrality, tested
        let score = compute_score(0.2, 0.0, 0.0, 0.1, 0.0);
        assert!(score < 0.4, "Should be LOW, got {:.2}", score);
        assert_eq!(risk_label(score), "LOW");
    }

    #[test]
    fn test_risk_label_boundaries() {
        assert_eq!(risk_label(0.7), "HIGH");
        assert_eq!(risk_label(0.69), "MEDIUM");
        assert_eq!(risk_label(0.4), "MEDIUM");
        assert_eq!(risk_label(0.39), "LOW");
    }

    // =========================================================================
    // Parameter extraction
    // =========================================================================

    #[test]
    fn test_extract_params_rust() {
        let params = extract_parameter_portion("pub fn process(input: &str) -> Result<()>");
        assert!(params.contains("&str"));
        assert!(!params.contains("Result"));
    }

    #[test]
    fn test_extract_params_no_return_type() {
        let params = extract_parameter_portion("def process(data)");
        assert_eq!(params, "def process(data)");
    }

    #[test]
    fn test_extract_params_closing_paren() {
        let params = extract_parameter_portion("void handle(Request req)");
        assert!(params.contains("Request"));
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::analysis::security_risk_tests`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/analysis/security_risk.rs src/analysis/mod.rs \
        src/tests/analysis/security_risk_tests.rs src/tests/analysis/mod.rs
git commit -m "feat(analysis): add security_risk module with signal helpers and tests"
```

---

### Task 2: Database query for grouped call identifiers

**Files:**
- Modify: `src/database/identifiers.rs:85-160`

- [ ] **Step 1: Add get_call_identifiers_grouped method**

In `src/database/identifiers.rs`, add after the existing `get_identifiers_by_names_and_kind` method (~line 160):

```rust
    /// Get all call identifiers grouped by containing_symbol_id.
    ///
    /// Returns a HashMap mapping symbol_id → Vec<callee_name>.
    /// Used by security risk analysis for batch sink detection.
    pub fn get_call_identifiers_grouped(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT containing_symbol_id, name FROM identifiers WHERE kind = 'call' AND containing_symbol_id IS NOT NULL"
        )?;

        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        })?;

        for row in rows {
            let (symbol_id, callee_name) = row?;
            grouped.entry(symbol_id).or_default().push(callee_name);
        }

        debug!("Loaded {} call identifiers across {} symbols",
            grouped.values().map(|v| v.len()).sum::<usize>(),
            grouped.len()
        );

        Ok(grouped)
    }
```

Add `use std::collections::HashMap;` and `use tracing::debug;` to the imports at the top of the file if not already present.

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 3: Commit**

```bash
git add src/database/identifiers.rs
git commit -m "feat(database): add get_call_identifiers_grouped for batch sink detection"
```

---

### Task 3: compute_security_risk implementation

**Files:**
- Modify: `src/analysis/security_risk.rs`
- Modify: `src/tests/analysis/security_risk_tests.rs`

- [ ] **Step 1: Write failing integration test**

Add to `src/tests/analysis/security_risk_tests.rs`:

```rust
    use crate::database::SymbolDatabase;
    use tempfile::TempDir;

    fn insert_file(db: &SymbolDatabase, path: &str) {
        db.conn.execute(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified) VALUES (?1, 'rust', 'h', 100, 0)",
            rusqlite::params![path],
        ).unwrap();
    }

    #[test]
    fn test_compute_security_risk_high_risk_symbol() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/handler.rs");
        insert_file(&db, "src/utils.rs");

        // High-risk: public function with string params that calls execute
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('s1', 'process_request', 'function', 'rust', 'src/handler.rs', 1, 0, 20, 0, 0, 0,
                    15.0, 'public', 'pub fn process_request(input: &str) -> Result<()>', NULL);
        "#).unwrap();

        // The sink it calls
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, metadata)
            VALUES ('s_sink', 'execute', 'function', 'rust', 'src/utils.rs', 1, 0, 5, 0, 0, 0, 0.0, 'public', NULL);
        "#).unwrap();

        // Relationship: s1 calls execute
        db.conn.execute_batch(r#"
            INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind, file_path, line_number)
            VALUES ('r1', 's1', 's_sink', 'calls', 'src/handler.rs', 10);
        "#).unwrap();

        // Also an identifier call
        db.conn.execute_batch(r#"
            INSERT INTO identifiers (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, containing_symbol_id)
            VALUES ('i1', 'execute', 'call', 'rust', 'src/handler.rs', 10, 0, 10, 15, 's1');
        "#).unwrap();

        let stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();
        assert!(stats.total_scored >= 1, "Should score at least s1");
        assert!(stats.high_risk >= 1, "s1 should be HIGH risk");

        // Verify metadata
        let s1 = db.get_symbol_by_id("s1").unwrap().unwrap();
        let meta = s1.metadata.unwrap();
        let security = meta.get("security_risk").unwrap();
        let label = security.get("label").unwrap().as_str().unwrap();
        assert_eq!(label, "HIGH");
        let signals = security.get("signals").unwrap();
        let sinks = signals.get("sink_calls").unwrap().as_array().unwrap();
        assert!(!sinks.is_empty(), "Should detect execute as a sink");
    }

    #[test]
    fn test_compute_security_risk_no_signals_no_key() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/lib.rs");

        // Private function with integer params, no sink calls
        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('safe', 'add_numbers', 'function', 'rust', 'src/lib.rs', 1, 0, 5, 0, 0, 0,
                    0.0, 'private', 'fn add_numbers(a: i32, b: i32) -> i32', NULL);
        "#).unwrap();

        let _stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();

        let sym = db.get_symbol_by_id("safe").unwrap().unwrap();
        if let Some(meta) = &sym.metadata {
            assert!(meta.get("security_risk").is_none(),
                "Symbol with no security signals should not have security_risk key");
        }
    }

    #[test]
    fn test_compute_security_risk_excludes_test_symbols() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "tests/test.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, signature, metadata)
            VALUES ('t1', 'test_exec', 'function', 'rust', 'tests/test.rs', 1, 0, 5, 0, 0, 0,
                    0.0, 'private', 'fn test_exec()', '{"is_test": true}');
        "#).unwrap();

        let stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();
        assert_eq!(stats.total_scored, 0, "Test symbols should be excluded");
    }

    #[test]
    fn test_compute_security_risk_excludes_imports() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        insert_file(&db, "src/lib.rs");

        db.conn.execute_batch(r#"
            INSERT INTO symbols (id, name, kind, language, file_path, start_line, start_col, end_line, end_col, start_byte, end_byte,
                                 reference_score, visibility, metadata)
            VALUES ('imp', 'use_exec', 'import', 'rust', 'src/lib.rs', 1, 0, 1, 0, 0, 0, 0.0, 'public', NULL);
        "#).unwrap();

        let stats = crate::analysis::security_risk::compute_security_risk(&db).unwrap();
        assert_eq!(stats.total_scored, 0, "Import symbols should be excluded");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tests::analysis::security_risk_tests::tests::test_compute_security_risk_high_risk`
Expected: FAIL — `compute_security_risk` not implemented.

- [ ] **Step 3: Implement compute_security_risk**

Add to `src/analysis/security_risk.rs`:

```rust
/// Compute structural security risk for all non-test, non-import symbols.
///
/// Must run AFTER `compute_change_risk_scores()` in the pipeline so that
/// `metadata["test_coverage"]` is available for the untested signal.
pub fn compute_security_risk(db: &SymbolDatabase) -> Result<SecurityRiskStats> {
    let mut stats = SecurityRiskStats::default();
    let sink_patterns = all_sink_patterns();

    // Pre-load P95 for blast radius normalization
    let p95: f64 = db.conn.query_row(
        "SELECT COALESCE(
            (SELECT reference_score FROM symbols
             WHERE reference_score > 0
             ORDER BY reference_score DESC
             LIMIT 1 OFFSET (SELECT MAX(0, CAST(COUNT(*) * 0.05 AS INTEGER))
                             FROM symbols WHERE reference_score > 0)),
            0.0)",
        [],
        |row| row.get(0),
    ).unwrap_or(0.0);

    debug!("Security risk P95 reference_score: {:.2}", p95);

    // Pre-load call identifiers grouped by symbol (batch)
    let call_identifiers = db.get_call_identifiers_grouped()?;

    // Pre-load relationship callees grouped by from_symbol_id (batch)
    let mut rel_stmt = db.conn.prepare(
        "SELECT r.from_symbol_id, s_callee.name
         FROM relationships r
         JOIN symbols s_callee ON r.to_symbol_id = s_callee.id
         WHERE r.kind = 'calls'"
    )?;
    let mut relationship_callees: HashMap<String, Vec<String>> = HashMap::new();
    let rel_rows = rel_stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rel_rows {
        let (from_id, callee_name) = row?;
        relationship_callees.entry(from_id).or_default().push(callee_name);
    }

    debug!(
        "Pre-loaded {} call identifiers, {} relationship callees",
        call_identifiers.len(),
        relationship_callees.len()
    );

    // Query all non-test symbols
    let mut stmt = db.conn.prepare(
        "SELECT id, kind, visibility, reference_score, signature, metadata
         FROM symbols
         WHERE (json_extract(metadata, '$.is_test') IS NULL
                OR json_extract(metadata, '$.is_test') != 1)"
    )?;

    let rows: Vec<(String, String, Option<String>, f64, Option<String>, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (id, kind_str, vis, ref_score, signature, metadata_json) in &rows {
            let kind = SymbolKind::from_string(kind_str);

            // Skip imports/exports
            if security_kind_weight(&kind).is_none() {
                continue;
            }

            // Compute signals
            let exposure = exposure_score(vis.as_deref(), &kind);
            let input_handling = if has_input_handling(signature.as_deref()) { 1.0 } else { 0.0 };

            let ident_callees = call_identifiers.get(id.as_str()).map(|v| v.as_slice()).unwrap_or(&[]);
            let rel_callees = relationship_callees.get(id.as_str()).map(|v| v.as_slice()).unwrap_or(&[]);
            let (sink_score, sink_names) = compute_sink_signal(ident_callees, rel_callees, &sink_patterns);

            // Scoring gate: skip if no security-relevant signals
            if exposure < 0.5 && input_handling == 0.0 && sink_score == 0.0 {
                stats.skipped_no_signals += 1;
                continue;
            }

            let blast_radius = normalize_blast_radius(*ref_score, p95);

            // Untested signal: binary
            let untested = metadata_json.as_ref()
                .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                .and_then(|v| v.get("test_coverage").cloned())
                .map(|_| 0.0) // has test_coverage → not untested
                .unwrap_or(1.0); // no test_coverage → untested

            let score = compute_score(exposure, input_handling, sink_score, blast_radius, untested);
            let label = risk_label(score);

            stats.total_scored += 1;
            match label {
                "HIGH" => stats.high_risk += 1,
                "MEDIUM" => stats.medium_risk += 1,
                _ => stats.low_risk += 1,
            }

            let risk_data = serde_json::json!({
                "score": (score * 100.0).round() / 100.0,
                "label": label,
                "signals": {
                    "exposure": (exposure * 100.0).round() / 100.0,
                    "input_handling": input_handling,
                    "sink_calls": sink_names,
                    "blast_radius": (blast_radius * 100.0).round() / 100.0,
                    "untested": untested == 1.0,
                }
            });

            // Merge into existing metadata
            let mut meta = match metadata_json {
                Some(json_str) => serde_json::from_str::<serde_json::Value>(json_str)
                    .unwrap_or_else(|_| serde_json::json!({})),
                None => serde_json::json!({}),
            };

            meta.as_object_mut()
                .unwrap()
                .insert("security_risk".to_string(), risk_data);

            db.conn.execute(
                "UPDATE symbols SET metadata = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::to_string(&meta)?, id],
            )?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => { db.conn.execute_batch("COMMIT")?; }
        Err(e) => {
            let _ = db.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    info!(
        "Security risk computed: {} scored ({} HIGH, {} MEDIUM, {} LOW), {} skipped (no signals)",
        stats.total_scored, stats.high_risk, stats.medium_risk, stats.low_risk, stats.skipped_no_signals
    );

    Ok(stats)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tests::analysis::security_risk_tests`
Expected: All tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/analysis/security_risk.rs src/tests/analysis/security_risk_tests.rs
git commit -m "feat(analysis): implement compute_security_risk with batch sink detection"
```

---

### Task 4: Hook into indexing pipeline

**Files:**
- Modify: `src/tools/workspace/indexing/processor.rs:528-530`

- [ ] **Step 1: Add pipeline call**

After the `compute_change_risk_scores` call (~line 528-530), add:

```rust
                // Compute structural security risk scores
                if let Err(e) = crate::analysis::compute_security_risk(&db_lock) {
                    warn!("Failed to compute security risk: {}", e);
                }
```

- [ ] **Step 2: Build to verify**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 3: Commit**

```bash
git add src/tools/workspace/indexing/processor.rs
git commit -m "feat(analysis): hook compute_security_risk into indexing pipeline"
```

---

## Chunk 2: Tool Integration

### Task 5: Deep dive — security risk formatting

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs`

- [ ] **Step 1: Add format_security_risk_info function**

Add after `format_change_risk_info` (~line 230, after the change risk function ends):

```rust
/// Format security risk section for production symbols.
/// Only shown when metadata contains security_risk key.
fn format_security_risk_info(out: &mut String, symbol: &crate::extractors::base::Symbol, incoming_count: usize) {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return,
    };

    // Skip test symbols
    if metadata.get("is_test").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }

    let security = match metadata.get("security_risk") {
        Some(r) => r,
        None => return,
    };

    let score = security.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let label = security.get("label").and_then(|v| v.as_str()).unwrap_or("LOW");
    let signals = security.get("signals");

    // Build summary: "Security Risk: HIGH (0.85) — calls execute, raw_sql; public; accepts string params"
    let mut summary_parts = Vec::new();

    if let Some(sigs) = signals {
        if let Some(sinks) = sigs.get("sink_calls").and_then(|v| v.as_array()) {
            if !sinks.is_empty() {
                let names: Vec<&str> = sinks.iter().filter_map(|v| v.as_str()).collect();
                summary_parts.push(format!("calls {}", names.join(", ")));
            }
        }
        if let Some(exp) = sigs.get("exposure").and_then(|v| v.as_f64()) {
            if exp >= 0.5 {
                summary_parts.push("public".to_string());
            }
        }
        if sigs.get("input_handling").and_then(|v| v.as_f64()).unwrap_or(0.0) > 0.0 {
            summary_parts.push("accepts string params".to_string());
        }
    }

    let summary = if summary_parts.is_empty() {
        String::new()
    } else {
        format!(" — {}", summary_parts.join("; "))
    };

    out.push_str(&format!(
        "\nSecurity Risk: {} ({:.2}){}\n",
        label, score, summary
    ));

    // Detail lines
    if let Some(sigs) = signals {
        let exposure = sigs.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if exposure >= 0.5 {
            out.push_str("  exposure: public\n");
        } else {
            out.push_str(&format!("  exposure: {:.2}\n", exposure));
        }

        let input = sigs.get("input_handling").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if input > 0.0 {
            out.push_str("  input handling: yes (signature contains input type patterns)\n");
        }

        if let Some(sinks) = sigs.get("sink_calls").and_then(|v| v.as_array()) {
            if !sinks.is_empty() {
                let names: Vec<&str> = sinks.iter().filter_map(|v| v.as_str()).collect();
                out.push_str(&format!("  sink calls: {}\n", names.join(", ")));
            }
        }

        let blast = sigs.get("blast_radius").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push_str(&format!("  blast radius: {:.2} ({} callers)\n", blast, incoming_count));

        let untested = sigs.get("untested").and_then(|v| v.as_bool()).unwrap_or(false);
        out.push_str(&format!("  untested: {}\n", if untested { "yes" } else { "no" }));
    }
}
```

- [ ] **Step 2: Wire into kind-specific formatters ONLY**

Add `format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);` AFTER each `format_change_risk_info` call in the kind-specific formatters. Do NOT add to `format_header` (line 69) — only to:

- `format_callable` (~line 320, after change_risk call)
- `format_trait_or_interface` (~line 358)
- `format_class_or_struct` (~line 447)
- `format_enum` (~line 495)
- `format_module` (~line 563)
- `format_generic` (~line 587)

- [ ] **Step 3: Build to verify**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 4: Commit**

```bash
git add src/tools/deep_dive/formatting.rs
git commit -m "feat(deep_dive): display security risk section with signals and factors"
```

---

### Task 6: Get context — security labels on pivots

**Files:**
- Modify: `src/tools/get_context/formatting.rs:44-63` (PivotEntry struct)
- Modify: `src/tools/get_context/pipeline.rs:365-381` (PivotEntry construction)
- Modify: `src/tools/get_context/formatting.rs:144-150,215-220` (both output formats)
- Modify: `src/tests/tools/get_context_formatting_tests.rs` (make_pivot helper)

- [ ] **Step 1: Add security_label field to PivotEntry**

In `src/tools/get_context/formatting.rs`, add after `risk_label` in PivotEntry (~line 62):

```rust
    /// Security risk label (HIGH/MEDIUM/LOW) from metadata, if available.
    pub security_label: Option<String>,
```

- [ ] **Step 2: Extract security_label in pipeline**

In `src/tools/get_context/pipeline.rs`, after the `risk_label` extraction (~line 370), add:

```rust
        let security_label = batch.full_symbols.get(&pivot.result.id)
            .and_then(|sym| sym.metadata.as_ref())
            .and_then(|m| m.get("security_risk"))
            .and_then(|r| r.get("label"))
            .and_then(|l| l.as_str())
            .map(String::from);
```

Add `security_label,` to the PivotEntry construction.

- [ ] **Step 3: Append to BOTH output formats**

In the **readable format** (~line 144-150), after the existing `risk_tag`:

```rust
        let security_tag = pivot.security_label.as_ref()
            .map(|l| format!("  [{} security]", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}:{} ({}){}{}\n",
            pivot.file_path, pivot.start_line, pivot.kind, risk_tag, security_tag
        ));
```

In the **compact format** (~line 215-220), after the existing `risk_tag`:

```rust
        let security_tag = pivot.security_label.as_ref()
            .map(|l| format!(" security={}", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "PIVOT {} {}:{} kind={} centrality={}{}{}\n",
            pivot.name, pivot.file_path, pivot.start_line, pivot.kind, label, risk_tag, security_tag
        ));
```

- [ ] **Step 4: Fix test helper**

In `src/tests/tools/get_context_formatting_tests.rs`, add `security_label: None,` to the `make_pivot` helper, after `risk_label: None,`.

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: Clean compile.

- [ ] **Step 6: Commit**

```bash
git add src/tools/get_context/formatting.rs src/tools/get_context/pipeline.rs \
        src/tests/tools/get_context_formatting_tests.rs
git commit -m "feat(get_context): display security risk labels on pivot symbols"
```

---

### Task 7: Final regression check and TODO update

**Files:**
- Modify: `TODO.md`

- [ ] **Step 1: Run dev tier**

Run: `cargo xtask test dev`
Expected: No new failures beyond known pre-existing ones.

- [ ] **Step 2: Update TODO.md**

Mark the structural security risk signals item as complete.

- [ ] **Step 3: Commit**

```bash
git add TODO.md
git commit -m "docs: mark security risk signals as complete in TODO"
```

---

## Verification

After all tasks complete:

1. **Build release**: `cargo build --release`
2. **Restart Claude Code** to pick up new binary
3. **Re-index**: `manage_workspace(operation="index", force=true)`
4. **Dogfood — deep_dive**: `deep_dive(symbol="delete_symbols_for_file")` → should show Security Risk section with execute sink
5. **Dogfood — get_context**: `get_context(query="database operations")` → pivots should show `security=HIGH` labels where applicable
6. **Sanity check**: Verify that private utility functions with no sinks have no security_risk key
7. **Regression**: `cargo xtask test dev` — no new failures
