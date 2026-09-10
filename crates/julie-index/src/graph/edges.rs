//! CSR adjacency in both directions plus the walk accessors tools call.

use super::{Edge, EdgeKind, Graph, SymbolId};

/// Deduplicated edges stored as offset arrays: `out[offsets[id]..offsets[id+1]]`.
pub struct Adjacency {
    out_offsets: Vec<u32>,
    out: Vec<(SymbolId, EdgeKind)>,
    in_offsets: Vec<u32>,
    incoming: Vec<(SymbolId, EdgeKind)>,
}

fn csr(
    count: usize,
    edges: &[Edge],
    key: impl Fn(&Edge) -> (SymbolId, SymbolId),
) -> (Vec<u32>, Vec<(SymbolId, EdgeKind)>) {
    let mut sorted: Vec<(SymbolId, SymbolId, EdgeKind)> = edges
        .iter()
        .map(|e| {
            let (owner, other) = key(e);
            (owner, other, e.kind)
        })
        .collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut offsets = vec![0u32; count + 1];
    for (owner, _, _) in &sorted {
        offsets[owner.0 as usize + 1] += 1;
    }
    for i in 0..count {
        offsets[i + 1] += offsets[i];
    }
    let entries = sorted
        .into_iter()
        .map(|(_, other, kind)| (other, kind))
        .collect();
    (offsets, entries)
}

impl Adjacency {
    pub fn build(count: usize, edges: &[Edge]) -> Self {
        let (out_offsets, out) = csr(count, edges, |e| (e.from, e.to));
        let (in_offsets, incoming) = csr(count, edges, |e| (e.to, e.from));
        Self {
            out_offsets,
            out,
            in_offsets,
            incoming,
        }
    }

    pub fn outgoing(&self, id: SymbolId) -> &[(SymbolId, EdgeKind)] {
        let i = id.0 as usize;
        &self.out[self.out_offsets[i] as usize..self.out_offsets[i + 1] as usize]
    }

    pub fn incoming(&self, id: SymbolId) -> &[(SymbolId, EdgeKind)] {
        let i = id.0 as usize;
        &self.incoming[self.in_offsets[i] as usize..self.in_offsets[i + 1] as usize]
    }

    pub fn edge_count(&self) -> usize {
        self.out.len()
    }

    pub fn edges(&self) -> impl Iterator<Item = Edge> + '_ {
        (0..self.out_offsets.len().saturating_sub(1)).flat_map(move |i| {
            let from = SymbolId(i as u32);
            self.outgoing(from).iter().map(move |(to, kind)| Edge {
                from,
                to: *to,
                kind: *kind,
            })
        })
    }

    pub fn resident_bytes(&self) -> usize {
        (self.out_offsets.capacity() + self.in_offsets.capacity()) * size_of::<u32>()
            + (self.out.capacity() + self.incoming.capacity()) * size_of::<(SymbolId, EdgeKind)>()
    }
}

fn with_kind<'a>(
    entries: &'a [(SymbolId, EdgeKind)],
    keep: impl Fn(EdgeKind) -> bool + 'a,
) -> impl Iterator<Item = SymbolId> + 'a {
    entries
        .iter()
        .filter(move |(_, kind)| keep(*kind))
        .map(|(id, _)| *id)
}

impl Graph {
    pub fn outgoing(&self, id: SymbolId) -> &[(SymbolId, EdgeKind)] {
        self.adjacency.outgoing(id)
    }

    pub fn incoming(&self, id: SymbolId) -> &[(SymbolId, EdgeKind)] {
        self.adjacency.incoming(id)
    }

    pub fn edges(&self) -> impl Iterator<Item = Edge> + '_ {
        self.adjacency.edges()
    }

    pub fn callers(&self, id: SymbolId) -> impl Iterator<Item = SymbolId> + '_ {
        with_kind(self.incoming(id), |k| k == EdgeKind::Calls)
    }

    pub fn callees(&self, id: SymbolId) -> impl Iterator<Item = SymbolId> + '_ {
        with_kind(self.outgoing(id), |k| k == EdgeKind::Calls)
    }

    /// Every symbol with a non-structural edge into `id`.
    pub fn references_to(&self, id: SymbolId) -> impl Iterator<Item = SymbolId> + '_ {
        with_kind(self.incoming(id), |k| k != EdgeKind::Contains)
    }

    /// Every symbol `id` has a non-structural edge to.
    pub fn references_from(&self, id: SymbolId) -> impl Iterator<Item = SymbolId> + '_ {
        with_kind(self.outgoing(id), |k| k != EdgeKind::Contains)
    }

    pub fn children(&self, id: SymbolId) -> impl Iterator<Item = SymbolId> + '_ {
        with_kind(self.outgoing(id), |k| k == EdgeKind::Contains)
    }

    pub fn parent(&self, id: SymbolId) -> Option<SymbolId> {
        with_kind(self.incoming(id), |k| k == EdgeKind::Contains).next()
    }

    /// Symbols that implement, extend, or override `id`.
    pub fn implementations(&self, id: SymbolId) -> impl Iterator<Item = SymbolId> + '_ {
        with_kind(self.incoming(id), |k| {
            matches!(k, EdgeKind::Implements | EdgeKind::Extends)
        })
    }
}
