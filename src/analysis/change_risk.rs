//! Change risk scoring: per-symbol 0.0–1.0 score representing
//! "how risky is it to change this?" based on centrality, visibility,
//! test linkage quality, and symbol kind.

use anyhow::Result;
use tracing::{debug, info};

use crate::analysis::test_linkage::test_linkage_entry;
use crate::database::SymbolDatabase;
use crate::extractors::SymbolKind;

/// Weights for the change risk formula.
const W_CENTRALITY: f64 = 0.35;
const W_VISIBILITY: f64 = 0.25;
const W_TEST_WEAKNESS: f64 = 0.30;
const W_KIND: f64 = 0.10;

/// Summary stats from running change risk analysis.
#[derive(Debug, Clone, Default)]
pub struct ChangeRiskStats {
    pub total_scored: usize,
    pub high_risk: usize,
    pub medium_risk: usize,
    pub low_risk: usize,
}

/// Map visibility string to 0.0–1.0 score.
pub fn visibility_score(vis: Option<&str>) -> f64 {
    match vis {
        Some("public") => 1.0,
        Some("protected") => 0.5,
        Some("private") => 0.2,
        _ => 0.5, // NULL or unknown → moderate exposure
    }
}

/// Map symbol kind to 0.0–1.0 weight.
/// Returns None for Import/Export (excluded from scoring).
pub fn kind_weight(kind: &SymbolKind) -> Option<f64> {
    match kind {
        // Callable: highest risk surface
        SymbolKind::Function
        | SymbolKind::Method
        | SymbolKind::Constructor
        | SymbolKind::Destructor
        | SymbolKind::Operator => Some(1.0),
        // Container: moderate risk
        SymbolKind::Class
        | SymbolKind::Struct
        | SymbolKind::Interface
        | SymbolKind::Trait
        | SymbolKind::Enum
        | SymbolKind::Union
        | SymbolKind::Module
        | SymbolKind::Namespace
        | SymbolKind::Type
        | SymbolKind::Delegate => Some(0.7),
        // Data: lower risk
        SymbolKind::Variable
        | SymbolKind::Constant
        | SymbolKind::Property
        | SymbolKind::Field
        | SymbolKind::EnumMember
        | SymbolKind::Event => Some(0.3),
        // Import/Export: skip
        SymbolKind::Import | SymbolKind::Export => None,
    }
}

/// Map linked-test best_tier to a "test weakness" score, gated by confidence.
/// Higher = weaker linkage = more risk.  Low confidence pulls the score
/// toward the neutral midpoint (0.5) so uncertain tiers neither inflate
/// nor deflate risk.
pub fn test_weakness_score(best_tier: Option<&str>, confidence: f64) -> f64 {
    let raw_weakness = match best_tier {
        None => 1.0,
        Some("stub") => 0.9,
        Some("thin") => 0.6,
        Some("adequate") => 0.3,
        Some("thorough") => 0.1,
        Some("unknown") => 0.5,
        Some("n/a") => 0.5,
        _ => 0.5, // Unrecognized tier -> neutral
    };
    let confidence = confidence.clamp(0.0, 1.0);
    let neutral = 0.5;
    neutral + (raw_weakness - neutral) * confidence
}

/// Normalize reference_score to 0.0–1.0 using log sigmoid.
pub fn normalize_centrality(reference_score: f64, p95: f64) -> f64 {
    if p95 <= 0.0 {
        return 0.0;
    }
    let normalized = (1.0 + reference_score).ln() / (1.0 + p95).ln();
    normalized.min(1.0)
}

/// Compute final change risk score from normalized signals.
pub fn compute_risk_score(centrality: f64, visibility: f64, test_weakness: f64, kind: f64) -> f64 {
    W_CENTRALITY * centrality
        + W_VISIBILITY * visibility
        + W_TEST_WEAKNESS * test_weakness
        + W_KIND * kind
}

/// Map score to tier label.
pub fn risk_label(score: f64) -> &'static str {
    if score >= 0.7 {
        "HIGH"
    } else if score >= 0.4 {
        "MEDIUM"
    } else {
        "LOW"
    }
}

/// Compute change risk scores for all non-test, non-import/export symbols.
///
/// Must run AFTER `compute_test_linkage()` so that `metadata["test_linkage"]`
/// is available for the test weakness signal.
pub fn compute_change_risk_scores(db: &SymbolDatabase) -> Result<ChangeRiskStats> {
    let mut stats = ChangeRiskStats::default();

    // Compute P95 of reference_score for centrality normalization
    let p95: f64 = db
        .conn
        .query_row(
            "SELECT COALESCE(
            (SELECT reference_score FROM symbols
             WHERE reference_score > 0
             ORDER BY reference_score DESC
             LIMIT 1 OFFSET (SELECT MAX(0, CAST(COUNT(*) * 0.05 AS INTEGER))
                             FROM symbols WHERE reference_score > 0)),
            0.0)",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0.0);

    debug!("Change risk P95 reference_score: {:.2}", p95);

    // Query all non-test symbols with their scoring inputs
    let mut stmt = db.conn.prepare(
        "SELECT id, kind, visibility, reference_score, metadata
         FROM symbols
         WHERE (json_extract(metadata, '$.is_test') IS NULL
                OR json_extract(metadata, '$.is_test') != 1)",
    )?;

    let rows: Vec<(String, String, Option<String>, f64, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    db.conn.execute_batch("BEGIN")?;
    let result = (|| -> Result<()> {
        for (id, kind_str, vis, ref_score, metadata_json) in &rows {
            let kind = SymbolKind::from_string(kind_str);

            // Skip imports/exports
            let kw = match kind_weight(&kind) {
                Some(w) => w,
                None => continue,
            };

            let centrality = normalize_centrality(*ref_score, p95);
            let vis_score = visibility_score(vis.as_deref());

            let metadata = metadata_json
                .as_ref()
                .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());
            let tl_entry = metadata.as_ref().and_then(test_linkage_entry);
            let best_tier = tl_entry
                .and_then(|v| v.get("best_tier"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let confidence = tl_entry
                .and_then(|v| v.get("best_confidence"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5); // Default for old data without confidence
            let tw = test_weakness_score(best_tier.as_deref(), confidence);

            let score = compute_risk_score(centrality, vis_score, tw, kw);
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
                "factors": {
                    "centrality": (centrality * 100.0).round() / 100.0,
                    "visibility": vis.as_deref().unwrap_or("unknown"),
                    "test_weakness": tw,
                    "kind": kind_str,
                }
            });

            // Merge into existing metadata
            let mut meta = match metadata_json {
                Some(json_str) => serde_json::from_str::<serde_json::Value>(json_str)
                    .unwrap_or_else(|_| serde_json::json!({})),
                None => serde_json::json!({}),
            };

            if let Some(obj) = meta.as_object_mut() {
                obj.insert("change_risk".to_string(), risk_data);
            }

            db.conn.execute(
                "UPDATE symbols SET metadata = ?1 WHERE id = ?2",
                rusqlite::params![serde_json::to_string(&meta)?, id],
            )?;
        }
        Ok(())
    })();

    match result {
        Ok(()) => {
            db.conn.execute_batch("COMMIT")?;
        }
        Err(e) => {
            let _ = db.conn.execute_batch("ROLLBACK");
            return Err(e);
        }
    }

    info!(
        "Change risk computed: {} scored ({} HIGH, {} MEDIUM, {} LOW)",
        stats.total_scored, stats.high_risk, stats.medium_risk, stats.low_risk
    );

    Ok(stats)
}
