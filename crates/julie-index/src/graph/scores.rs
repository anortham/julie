//! Reference scores: weighted incoming edges plus the propagation steps of
//! the SQL-era `compute_reference_scores` (interface -> implementation,
//! constructor -> class, test-file de-weighting, C header -> implementation).

use julie_extractors::SymbolKind;

use super::{Edge, EdgeKind, SymbolId, SymbolTable};
use crate::search::scoring::is_test_path;

const PROPAGATION: f64 = 0.7;
const TEST_FILE_FACTOR: f64 = 0.1;

fn weight(kind: EdgeKind) -> f64 {
    match kind {
        EdgeKind::Calls => 3.0,
        EdgeKind::Implements | EdgeKind::Imports | EdgeKind::Extends => 2.0,
        EdgeKind::References => 1.0,
        EdgeKind::Contains | EdgeKind::WebRoute | EdgeKind::SqlQuery => 0.0,
    }
}

fn is_header(path: &str) -> bool {
    path.ends_with(".h") || path.ends_with(".hpp") || path.ends_with(".hh")
}

fn is_c_source(path: &str) -> bool {
    path.ends_with(".c")
        || path.ends_with(".cpp")
        || path.ends_with(".cc")
        || path.ends_with(".cxx")
}

/// Score every symbol from the raw (not deduplicated) edge list, so each call
/// site counts once, as each relationship row did before.
pub fn reference_scores(symbols: &SymbolTable, edges: &[Edge]) -> Vec<f64> {
    let count = symbols.len();
    let mut scores = vec![0.0f64; count];
    for edge in edges {
        scores[edge.to.0 as usize] += weight(edge.kind);
    }

    let direct = scores.clone();
    for edge in edges {
        if matches!(edge.kind, EdgeKind::Implements | EdgeKind::Extends) {
            scores[edge.from.0 as usize] += direct[edge.to.0 as usize] * PROPAGATION;
        }
    }

    let mut best_constructor = vec![0.0f64; count];
    for edge in edges {
        let child = symbols.symbol(edge.to);
        if edge.kind == EdgeKind::Contains && child.kind == SymbolKind::Constructor {
            let parent = edge.from.0 as usize;
            best_constructor[parent] = best_constructor[parent].max(scores[edge.to.0 as usize]);
        }
    }
    for (i, score) in scores.iter_mut().enumerate() {
        let row = symbols.symbol(SymbolId(i as u32));
        if *score == 0.0
            && matches!(row.kind, SymbolKind::Class | SymbolKind::Struct)
            && best_constructor[i] > 0.0
        {
            *score += best_constructor[i] * PROPAGATION;
        }
    }

    for (i, score) in scores.iter_mut().enumerate() {
        if *score > 0.0 && is_test_path(&symbols.symbol(SymbolId(i as u32)).path) {
            *score *= TEST_FILE_FACTOR;
        }
    }

    let before_headers = scores.clone();
    for (i, score) in scores.iter_mut().enumerate() {
        let row = symbols.symbol(SymbolId(i as u32));
        if *score != 0.0 || row.kind != SymbolKind::Function || !is_c_source(&row.path) {
            continue;
        }
        let best_header = symbols
            .find_by_name(&row.name)
            .iter()
            .filter(|id| {
                let header = symbols.symbol(**id);
                header.kind == SymbolKind::Function && is_header(&header.path)
            })
            .map(|id| before_headers[id.0 as usize])
            .fold(0.0f64, f64::max);
        if best_header > 0.0 {
            *score += best_header * PROPAGATION;
        }
    }
    scores
}
