use std::path::{Path, PathBuf};
use std::time::Instant;

use julie_extractors::{ExtractionResults, SymbolKind};
use julie_facts::rows::Normalization;
use julie_facts::{Extractor, FactsStore, FactsWriter, PathChange};

use super::fixture::{edges_of_kind, file, graph_of, id_of, store_with};
use crate::graph::{EdgeKind, Graph, SymbolId};

struct RealExtractor {
    root: PathBuf,
}

impl Extractor for RealExtractor {
    fn extract(&self, path: &str, content: &str, _language: &str) -> ExtractionResults {
        julie_extractors::extract_canonical(path, content, &self.root)
            .unwrap_or_else(|_| ExtractionResults::empty())
    }
}

fn seed_changes(root: &Path) -> Vec<PathChange> {
    let mut entries: Vec<_> = std::fs::read_dir(root).unwrap().flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = std::fs::read(entry.path()).unwrap();
            let content = String::from_utf8_lossy(&bytes);
            let language = julie_extractors::detect_language_for_source(&name, &content)?;
            Some(PathChange::Upsert {
                path: name,
                bytes: bytes.clone(),
                language: language.to_string(),
            })
        })
        .collect()
}

#[test]
fn load_on_seed_fixture_with_the_real_extractor_yields_calls_edges() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/seed/a");
    let extractor = RealExtractor { root: root.clone() };
    let mut store = FactsStore::in_memory().unwrap();
    FactsWriter::new(&mut store, &extractor, Normalization::default())
        .apply(&seed_changes(&root))
        .unwrap();

    let started = Instant::now();
    let graph = Graph::load(&store.reader(), None).unwrap();
    assert!(started.elapsed().as_secs() < 2);

    let calls = edges_of_kind(&graph, EdgeKind::Calls);
    assert!(
        calls.contains(&(
            "app.py:alpha_app".to_string(),
            "helpers.py:shared_helper_marker".to_string()
        )),
        "calls: {calls:?}"
    );
    let stats = graph.stats();
    assert!(stats.load_millis > 0);
    assert!(stats.symbols > 0 && stats.edges > 0 && stats.resident_bytes > 0);
    assert_eq!(stats.symbols, graph.len());
    assert_eq!(graph.paths().len(), 8);
}

#[test]
fn symbol_ids_follow_path_order_then_ordinal_order() {
    let (_, store) = store_with(vec![
        file("z.rs")
            .symbol("z1", "z_first", SymbolKind::Function)
            .symbol("z2", "z_second", SymbolKind::Function),
        file("a.rs").symbol("a1", "a_first", SymbolKind::Function),
    ]);
    let graph = graph_of(&store);

    assert_eq!(graph.paths(), ["a.rs", "z.rs"]);
    assert_eq!(graph.symbols_in_path("a.rs"), [SymbolId(0)]);
    assert_eq!(graph.symbols_in_path("z.rs"), [SymbolId(1), SymbolId(2)]);
    assert_eq!(graph.symbol(SymbolId(2)).name, "z_second");
    assert_eq!(graph.symbol(SymbolId(2)).ordinal, 1);
    assert!(graph.symbol(SymbolId(2)).id.ends_with(":1"));
    assert!(graph.symbols_in_path("missing.rs").is_empty());
}

#[test]
fn load_with_explicit_paths_limits_the_graph() {
    let (_, store) = store_with(vec![
        file("a.rs").symbol("a", "a", SymbolKind::Function),
        file("b.rs").symbol("b", "b", SymbolKind::Function),
    ]);
    let graph = Graph::load(&store.reader(), Some(&["b.rs".to_string()])).unwrap();

    assert_eq!(graph.paths(), ["b.rs"]);
    assert_eq!(graph.len(), 1);
}

#[test]
fn find_by_name_is_exact_and_suffix_lookup_prefers_the_named_parent() {
    let (_, store) = store_with(vec![
        file("a.rs")
            .symbol("foo", "Foo", SymbolKind::Class)
            .child("foo_run", "run", SymbolKind::Method, Some("foo"))
            .symbol("run_free", "run", SymbolKind::Function),
        file("b.rs").symbol("bar", "Bar", SymbolKind::Class).child(
            "bar_run",
            "run",
            SymbolKind::Method,
            Some("bar"),
        ),
    ]);
    let graph = graph_of(&store);

    assert_eq!(graph.find_by_name("run").len(), 3);
    assert!(graph.find_by_name("Foo::run").is_empty());
    assert_eq!(
        graph.find_by_name_suffix("Foo::run"),
        vec![id_of(&graph, "a.rs", "run")]
    );
    assert_eq!(
        graph.find_by_name_suffix("Bar.run"),
        vec![graph.symbols_in_path("b.rs")[1]]
    );
    assert_eq!(graph.find_by_name_suffix("Nope::run").len(), 3);
    assert_eq!(
        graph.find_by_name_suffix("Foo"),
        vec![id_of(&graph, "a.rs", "Foo")]
    );
}
