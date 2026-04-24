use super::walk::ImpactCandidate;
use crate::extractors::{RelationshipKind, Symbol, Visibility};
use crate::search::scoring::is_test_path;

#[derive(Debug, Clone)]
pub struct RankedImpact {
    pub symbol: Symbol,
    pub distance: u32,
    pub relationship_kind: RelationshipKind,
    pub reference_score: f64,
    pub why: String,
}

pub fn rank_impacts(candidates: Vec<ImpactCandidate>, include_tests: bool) -> Vec<RankedImpact> {
    let mut ranked: Vec<RankedImpact> = candidates
        .into_iter()
        .filter(|candidate| include_tests || !is_test_symbol(&candidate.symbol))
        .map(|candidate| RankedImpact {
            why: build_reason(&candidate),
            symbol: candidate.symbol,
            distance: candidate.distance,
            relationship_kind: candidate.relationship_kind,
            reference_score: candidate.reference_score,
        })
        .collect();

    ranked.sort_by(|left, right| {
        ranking_key(left)
            .cmp(&ranking_key(right))
            .then_with(|| left.symbol.file_path.cmp(&right.symbol.file_path))
            .then_with(|| left.symbol.start_line.cmp(&right.symbol.start_line))
            .then_with(|| left.symbol.name.cmp(&right.symbol.name))
    });

    ranked
}

pub fn relationship_priority(kind: &RelationshipKind) -> u8 {
    match kind {
        RelationshipKind::Calls => 0,
        RelationshipKind::Overrides => 1,
        RelationshipKind::Implements => 2,
        RelationshipKind::Instantiates => 3,
        RelationshipKind::References => 4,
        RelationshipKind::Imports => 5,
        _ => 6,
    }
}

fn ranking_key(
    candidate: &RankedImpact,
) -> (u32, u8, std::cmp::Reverse<u8>, std::cmp::Reverse<u64>) {
    (
        candidate.distance,
        relationship_priority(&candidate.relationship_kind),
        std::cmp::Reverse(visibility_rank(candidate.symbol.visibility.as_ref())),
        std::cmp::Reverse(candidate.reference_score.to_bits()),
    )
}

fn visibility_rank(visibility: Option<&Visibility>) -> u8 {
    match visibility {
        Some(Visibility::Public) => 2,
        Some(Visibility::Protected) => 1,
        _ => 0,
    }
}

fn build_reason(candidate: &ImpactCandidate) -> String {
    let centrality = if candidate.reference_score >= 10.0 {
        "high"
    } else if candidate.reference_score > 0.0 {
        "medium"
    } else {
        "low"
    };

    if candidate.distance == 1 {
        format!(
            "{}, 1 hop, centrality={}",
            relationship_label(&candidate.relationship_kind),
            centrality
        )
    } else {
        format!(
            "reaches {} in {} hops, centrality={}",
            candidate.via_symbol_name, candidate.distance, centrality
        )
    }
}

fn relationship_label(kind: &RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::Calls => "direct caller",
        RelationshipKind::Overrides => "override",
        RelationshipKind::Implements => "implementation",
        RelationshipKind::Instantiates => "constructor path",
        RelationshipKind::References => "reference",
        RelationshipKind::Imports => "importer",
        _ => "incoming edge",
    }
}

fn is_test_symbol(symbol: &Symbol) -> bool {
    crate::analysis::test_roles::is_test_related(symbol) || is_test_path(&symbol.file_path)
}
