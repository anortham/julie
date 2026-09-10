//! Change risk scoring: per-symbol 0.0–1.0 score representing
//! "how risky is it to change this?" based on centrality, visibility,
//! test linkage quality, and symbol kind.

use anyhow::{Result, anyhow};
use tracing::{debug, info};

use crate::analysis::test_linkage::test_linkage_entry;
use julie_extractors::SymbolKind;

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
pub fn compute_change_risk_scores() -> Result<ChangeRiskStats> {
    Ok(ChangeRiskStats::default())
}
