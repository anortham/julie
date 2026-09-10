use std::fs;

use crate::deep_dive::data::{build_symbol_context, find_symbol};
use crate::deep_dive::deep_dive_query;
use julie_extractors::SymbolKind;
use julie_index::graph::SymbolId;
use julie_index::snapshot::Snapshot;
use julie_test_support::SnapshotFixture;
use tempfile::TempDir;

fn fixture(files: &[(&str, &str)]) -> (TempDir, SnapshotFixture) {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    let fixture = SnapshotFixture::from_tree(dir.path()).unwrap();
    (dir, fixture)
}

fn at_line(line: usize, body: &str) -> String {
    format!("{}{body}", "\n".repeat(line - 1))
}

fn only(snapshot: &Snapshot, name: &str) -> SymbolId {
    let found = find_symbol(snapshot.graph(), name, None);
    assert_eq!(found.len(), 1, "expected one symbol named {name}");
    found[0]
}

mod build_context;
mod find_symbol;
mod refs_query_similarity;
