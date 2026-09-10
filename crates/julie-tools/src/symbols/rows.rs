//! Symbol rows from the snapshot graph for the filtering, body-extraction,
//! and formatting stages.

use julie_core::Symbol;
use julie_index::snapshot::Snapshot;

use crate::snapshot_rows::to_symbol;

/// Every symbol stored for `path`, in source order.
pub(super) fn symbols_in_path(snapshot: &Snapshot, path: &str) -> Vec<Symbol> {
    let graph = snapshot.graph();
    let mut symbols: Vec<Symbol> = graph
        .symbols_in_path(path)
        .iter()
        .map(|id| to_symbol(graph, *id))
        .collect();
    symbols.sort_by_key(|symbol| (symbol.start_line, symbol.start_column));
    symbols
}
