//! Read rows per path, resolve once, and build the graph. `apply_paths` keeps
//! the row arrays of unchanged files and re-reads only the changed ones.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use julie_facts::FactsReader;
use julie_facts::rows::{
    IdentifierRow, RelationshipRow, StructuralFactQuery, StructuralFactRow, SymbolRow,
};

use super::edges::Adjacency;
use super::scores::reference_scores;
use super::web_edges::{edge_pattern_ids, sql_query_edges, web_route_edges};
use super::{Graph, GraphStats, SymbolId, SymbolTable, resolve};

const READ_CHUNK: usize = 500;

/// One path's rows, shared between graphs that both contain the file unchanged.
#[derive(Debug, Clone)]
pub struct FileRows {
    pub path: String,
    pub symbols: Arc<[SymbolRow]>,
    pub identifiers: Arc<[IdentifierRow]>,
    pub relationships: Arc<[RelationshipRow]>,
}

fn group_by_path<T>(rows: Vec<T>, path_of: impl Fn(&T) -> &str) -> HashMap<String, Vec<T>> {
    let mut grouped: HashMap<String, Vec<T>> = HashMap::new();
    for row in rows {
        grouped
            .entry(path_of(&row).to_string())
            .or_default()
            .push(row);
    }
    grouped
}

fn read_files(reader: &FactsReader<'_>, paths: &[String]) -> Result<Vec<FileRows>> {
    let mut files = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(READ_CHUNK) {
        let keys: Vec<&str> = chunk.iter().map(String::as_str).collect();
        let mut symbols = group_by_path(reader.symbols_for_paths(&keys)?, |r| &r.path);
        let mut identifiers = group_by_path(reader.identifiers_for_paths(&keys)?, |r| &r.path);
        let mut relationships = group_by_path(reader.relationships_for_paths(&keys)?, |r| &r.path);
        for path in chunk {
            files.push(FileRows {
                path: path.clone(),
                symbols: symbols.remove(path).unwrap_or_default().into(),
                identifiers: identifiers.remove(path).unwrap_or_default().into(),
                relationships: relationships.remove(path).unwrap_or_default().into(),
            });
        }
    }
    Ok(files)
}

fn read_edge_facts(reader: &FactsReader<'_>) -> Result<Vec<StructuralFactRow>> {
    reader.structural_facts(&StructuralFactQuery {
        pattern_ids: edge_pattern_ids(),
        path_pattern: None,
        language: None,
        limit: i64::MAX as usize,
    })
}

fn sorted_unique(mut paths: Vec<String>) -> Vec<String> {
    paths.sort_unstable();
    paths.dedup();
    paths
}

impl Graph {
    /// Build from every path in the store, or only `paths` when given.
    pub fn load(reader: &FactsReader<'_>, paths: Option<&[String]>) -> Result<Graph> {
        let started = Instant::now();
        let paths = match paths {
            Some(paths) => sorted_unique(paths.to_vec()),
            None => reader.paths()?.into_iter().map(|p| p.path).collect(),
        };
        let files = read_files(reader, &paths)?;
        let facts = read_edge_facts(reader)?;
        Ok(Self::build(files, &facts, started))
    }

    /// New graph with `removed` dropped and `changed` re-read; every other
    /// file's row arrays are shared with `self`. Edges are re-resolved in
    /// memory for all files, so callers in unchanged files follow a rename.
    pub fn apply_paths(
        &self,
        reader: &FactsReader<'_>,
        changed: &[String],
        removed: &[String],
    ) -> Result<Graph> {
        let started = Instant::now();
        let changed = sorted_unique(changed.to_vec());
        let mut files: Vec<FileRows> = self
            .table
            .files()
            .iter()
            .filter(|file| {
                changed.binary_search(&file.path).is_err() && !removed.contains(&file.path)
            })
            .cloned()
            .collect();
        files.extend(read_files(reader, &changed)?);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let facts = read_edge_facts(reader)?;
        Ok(Self::build(files, &facts, started))
    }

    fn build(files: Vec<FileRows>, facts: &[StructuralFactRow], started: Instant) -> Graph {
        let table = SymbolTable::new(files);
        let mut edges = resolve(
            &table,
            table.files().iter().flat_map(|f| f.identifiers.iter()),
            table.files().iter().flat_map(|f| f.relationships.iter()),
        );
        edges.extend(web_route_edges(&table, facts));
        edges.extend(sql_query_edges(&table, facts));
        let scores = reference_scores(&table, &edges);
        let adjacency = Adjacency::build(table.len(), &edges);
        let symbol_ids: Vec<SymbolId> = (0..table.len() as u32).map(SymbolId).collect();
        let stats = GraphStats {
            symbols: table.len(),
            edges: adjacency.edge_count(),
            load_millis: (started.elapsed().as_secs_f64() * 1000.0).ceil() as u64,
            resident_bytes: resident_bytes(&table, &adjacency, &symbol_ids, &scores),
        };
        Graph {
            table,
            symbol_ids,
            adjacency,
            scores,
            stats,
        }
    }
}

fn resident_bytes(
    table: &SymbolTable,
    adjacency: &Adjacency,
    symbol_ids: &[SymbolId],
    scores: &[f64],
) -> usize {
    let rows: usize = table
        .files()
        .iter()
        .map(|f| {
            f.symbols.len() * size_of::<SymbolRow>()
                + f.identifiers.len() * size_of::<IdentifierRow>()
                + f.relationships.len() * size_of::<RelationshipRow>()
        })
        .sum();
    rows + adjacency.resident_bytes()
        + size_of_val(symbol_ids)
        + size_of_val(scores)
        + table.len()
            * (2 * size_of::<String>() + size_of::<Vec<SymbolId>>() + size_of::<SymbolId>())
}
