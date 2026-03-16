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
pub(crate) const EXECUTION_SINKS: &[&str] = &[
    "exec", "eval", "system", "popen", "spawn", "fork",
    "shell_exec", "child_process", "subprocess", "shellexecute", "createprocess",
];

/// Category B: Database/query operation sinks.
pub(crate) const DATABASE_SINKS: &[&str] = &[
    // Raw SQL execution
    "execute", "raw_sql", "exec_query", "executequery",
    "executeupdate", "rawquery", "runsql",
    // EF Core / .NET (include Async variants for exact matching)
    "savechanges", "savechangesasync",
    "executedelete", "executedeleteasync",
    "executesqlraw", "executesqlrawasync",
    "executesqlinterpolated", "executesqlinterpolatedasync",
    "fromsqlraw", "fromsql",
    // Django / SQLAlchemy / Python ORMs
    "raw", "commit", "cursor",
    // Rails / ActiveRecord
    "destroy", "find_by_sql", "update_all", "delete_all",
    // Prisma / TypeORM / JS ORMs
    "findmany", "findunique", "createmany", "deletemany",
    "getrepository", "createquerybuilder",
    // JPA / Hibernate
    "persist", "merge", "createquery", "createnativequery",
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
// DI / framework type exclusions for input handling
// =============================================================================

/// Types that appear in DI constructor signatures but are not user input.
/// These are stripped from the parameter portion before checking INPUT_PATTERNS
/// to avoid false positives (e.g., `RequestDelegate` matching `Request`).
const DI_EXCLUSION_PATTERNS: &[&str] = &[
    "RequestDelegate", "ILogger", "IOptions", "IConfiguration",
    "IServiceProvider", "IHostEnvironment", "IWebHostEnvironment",
    "IMemoryCache", "CancellationToken",
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
/// Strips DI framework types before checking to avoid false positives
/// (e.g., `RequestDelegate` falsely matching the `Request` pattern).
pub fn has_input_handling(signature: Option<&str>) -> bool {
    let sig = match signature {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };

    // Extract parameter portion only (before return type delimiter)
    let param_portion = extract_parameter_portion(sig);

    let has_match = INPUT_PATTERNS.iter().any(|pattern| param_portion.contains(pattern));
    if !has_match {
        return false;
    }

    // If all matches are explained by DI exclusions, it's a false positive.
    // Strip DI type names from the param portion and re-check.
    let mut remaining = param_portion.to_string();
    for excl in DI_EXCLUSION_PATTERNS {
        remaining = remaining.replace(excl, "");
    }
    INPUT_PATTERNS.iter().any(|pattern| remaining.contains(pattern))
}

/// Extract the parameter portion of a signature, excluding return type.
/// Handles: `-> Type` (Rust), `: Type` after `)` (TS/Python), `returns` keyword.
pub fn extract_parameter_portion(signature: &str) -> &str {
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
                    "visibility": vis.as_deref().unwrap_or("default"),
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
