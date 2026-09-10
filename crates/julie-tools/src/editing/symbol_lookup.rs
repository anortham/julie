//! Name lookup over the snapshot graph for the editing tools.

use julie_core::Symbol;
use julie_index::graph::Graph;

use crate::navigation::resolution::find_symbols;
use crate::snapshot_rows::to_symbol;

/// Look up a symbol by name, optionally disambiguated by file path.
///
/// Resolution order matches [`find_symbols`]: the full name first (flat
/// namespaces), then `Parent::leaf` under a matching parent, then the plain
/// name. Import and export stubs never match.
pub fn find_symbol(graph: &Graph, name: &str, context_file: Option<&str>) -> Vec<Symbol> {
    find_symbols(graph, name, context_file)
        .into_iter()
        .map(|id| to_symbol(graph, id))
        .collect()
}
