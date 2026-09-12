//! In-memory symbol graph resolved from `facts.sqlite` rows.
//!
//! Identifiers and unresolved relationship targets are matched to symbol
//! definitions by name; ambiguous matches drop the edge. The result is
//! adjacency arrays over dense [`SymbolId`]s that tools walk directly.

mod edges;
mod load;
mod reexports;
mod resolve;
mod scores;
mod web_edges;

use std::collections::HashMap;
use std::ops::Range;

use julie_facts::rows::SymbolRow;

pub use edges::Adjacency;
pub use load::FileRows;
pub use resolve::resolve;
pub use web_edges::{
    HTTP_CLIENT_CALL_PATTERN_IDS, ROUTE_HANDLER_PATTERN_IDS, WebRouteResolution, sql_query_edges,
    web_route_edges,
};

/// Dense graph id: assigned at load in path order, then ordinal order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgeKind {
    Calls,
    References,
    Imports,
    Implements,
    Extends,
    Contains,
    WebRoute,
    SqlQuery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Edge {
    pub from: SymbolId,
    pub to: SymbolId,
    pub kind: EdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphStats {
    pub symbols: usize,
    pub edges: usize,
    pub load_millis: u64,
    /// Estimated heap held by the graph's arrays (row string heaps excluded).
    pub resident_bytes: usize,
}

/// Symbol rows of every file plus the name and path indexes resolution uses.
pub struct SymbolTable {
    files: Vec<FileRows>,
    paths: Vec<String>,
    file_base: Vec<u32>,
    path_index: HashMap<String, u32>,
    by_name: HashMap<String, Vec<SymbolId>>,
    by_row_id: HashMap<String, SymbolId>,
}

impl SymbolTable {
    /// `files` must be sorted by path with no duplicates.
    pub(crate) fn new(files: Vec<FileRows>) -> Self {
        let mut file_base = Vec::with_capacity(files.len() + 1);
        let mut by_name: HashMap<String, Vec<SymbolId>> = HashMap::new();
        let mut by_row_id = HashMap::new();
        let mut next = 0u32;
        for file in &files {
            file_base.push(next);
            for row in file.symbols.iter() {
                by_name
                    .entry(row.name.clone())
                    .or_default()
                    .push(SymbolId(next));
                by_row_id.insert(row.id.clone(), SymbolId(next));
                next += 1;
            }
        }
        file_base.push(next);
        let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        let path_index = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (p.clone(), i as u32))
            .collect();
        Self {
            files,
            paths,
            file_base,
            path_index,
            by_name,
            by_row_id,
        }
    }

    pub fn len(&self) -> usize {
        *self.file_base.last().unwrap_or(&0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn files(&self) -> &[FileRows] {
        &self.files
    }

    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    pub fn file_index(&self, path: &str) -> Option<u32> {
        self.path_index.get(path).copied()
    }

    pub fn file_of(&self, id: SymbolId) -> u32 {
        (self.file_base.partition_point(|base| *base <= id.0) - 1) as u32
    }

    pub fn symbols_in_file(&self, file: u32) -> Range<u32> {
        self.file_base[file as usize]..self.file_base[file as usize + 1]
    }

    pub fn symbol(&self, id: SymbolId) -> &SymbolRow {
        let file = self.file_of(id);
        &self.files[file as usize].symbols[(id.0 - self.file_base[file as usize]) as usize]
    }

    /// Graph id of the symbol with `ordinal` inside `file`.
    pub fn id_at(&self, file: u32, ordinal: u32) -> Option<SymbolId> {
        let symbols = &self.files[file as usize].symbols;
        let local = match symbols.get(ordinal as usize) {
            Some(row) if row.ordinal == ordinal => ordinal as usize,
            _ => symbols
                .binary_search_by_key(&ordinal, |row| row.ordinal)
                .ok()?,
        };
        Some(SymbolId(self.file_base[file as usize] + local as u32))
    }

    pub fn parent(&self, id: SymbolId) -> Option<SymbolId> {
        let parent_ordinal = self.symbol(id).parent_ordinal?;
        self.id_at(self.file_of(id), parent_ordinal)
    }

    pub fn find_by_name(&self, name: &str) -> &[SymbolId] {
        self.by_name.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Lookup by the row id `"<blob_hash>:<ordinal>"`.
    pub fn symbol_by_row_id(&self, row_id: &str) -> Option<SymbolId> {
        self.by_row_id.get(row_id).copied()
    }

    /// `Parent::leaf` / `Parent.leaf` lookup: symbols named `leaf`, narrowed to
    /// those whose parent is named `Parent` when any such symbol exists.
    pub fn find_by_name_suffix(&self, qualified: &str) -> Vec<SymbolId> {
        let Some((qualifier, leaf)) = parse_qualified_name(qualified) else {
            return self.find_by_name(qualified).to_vec();
        };
        let parent_name = parse_qualified_name(qualifier).map_or(qualifier, |(_, leaf)| leaf);
        let candidates = self.find_by_name(leaf);
        let under_parent: Vec<SymbolId> = candidates
            .iter()
            .copied()
            .filter(|id| {
                self.parent(*id)
                    .is_some_and(|parent| self.symbol(parent).name == parent_name)
            })
            .collect();
        if under_parent.is_empty() {
            candidates.to_vec()
        } else {
            under_parent
        }
    }
}

/// Split `a::b::c` or `a.b.c` into `("a::b", "c")` at the last separator.
pub fn parse_qualified_name(symbol: &str) -> Option<(&str, &str)> {
    let split = |pos: usize, width: usize| {
        let (parent, child) = (&symbol[..pos], &symbol[pos + width..]);
        (!parent.is_empty() && !child.is_empty()).then_some((parent, child))
    };
    symbol
        .rfind("::")
        .and_then(|pos| split(pos, 2))
        .or_else(|| symbol.rfind('.').and_then(|pos| split(pos, 1)))
}

/// Immutable resolved graph. Build one with [`Graph::load`]; derive the next
/// one with [`Graph::apply_paths`].
pub struct Graph {
    table: SymbolTable,
    symbol_ids: Vec<SymbolId>,
    adjacency: Adjacency,
    scores: Vec<f64>,
    stats: GraphStats,
}

impl Graph {
    pub fn resolved_web_routes(&self, facts: &[julie_facts::rows::StructuralFactRow]) -> Vec<WebRouteResolution> {
        web_edges::resolve_web_route_facts(&self.table, facts)
    }
    pub fn symbol(&self, id: SymbolId) -> &SymbolRow {
        self.table.symbol(id)
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub fn find_by_name(&self, name: &str) -> &[SymbolId] {
        self.table.find_by_name(name)
    }

    pub fn find_by_name_suffix(&self, qualified: &str) -> Vec<SymbolId> {
        self.table.find_by_name_suffix(qualified)
    }

    pub fn symbol_by_row_id(&self, row_id: &str) -> Option<SymbolId> {
        self.table.symbol_by_row_id(row_id)
    }

    pub fn symbols_in_path(&self, path: &str) -> &[SymbolId] {
        match self.table.file_index(path) {
            Some(file) => {
                let range = self.table.symbols_in_file(file);
                &self.symbol_ids[range.start as usize..range.end as usize]
            }
            None => &[],
        }
    }

    pub fn paths(&self) -> &[String] {
        self.table.paths()
    }

    pub fn file_rows(&self, path: &str) -> Option<&FileRows> {
        self.table
            .file_index(path)
            .map(|file| &self.table.files()[file as usize])
    }

    pub fn stats(&self) -> GraphStats {
        self.stats
    }

    pub fn reference_score(&self, id: SymbolId) -> f64 {
        self.scores[id.0 as usize]
    }
}
