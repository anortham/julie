use std::cell::RefCell;
use std::collections::HashMap;

use julie_extractors::{
    ExtractionResults, Identifier, IdentifierKind, NormalizedSpan, PendingRelationship,
    Relationship, RelationshipKind, StructuralFact, Symbol, SymbolKind,
};
use julie_facts::rows::Normalization;
use julie_facts::{Extractor, FactsStore, FactsWriter, PathChange};
use serde_json::json;

use crate::graph::{Edge, EdgeKind, Graph, SymbolId};

fn span(line: u32) -> NormalizedSpan {
    NormalizedSpan {
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: 10,
        start_byte: line * 20,
        end_byte: line * 20 + 10,
    }
}

pub struct FileBuilder {
    path: String,
    results: ExtractionResults,
    line: u32,
}

pub fn file(path: &str) -> FileBuilder {
    FileBuilder {
        path: path.to_string(),
        results: ExtractionResults::empty(),
        line: 0,
    }
}

impl FileBuilder {
    fn next_line(&mut self) -> u32 {
        self.line += 1;
        self.line
    }

    pub fn symbol(self, id: &str, name: &str, kind: SymbolKind) -> Self {
        self.child(id, name, kind, None)
    }

    pub fn child(mut self, id: &str, name: &str, kind: SymbolKind, parent: Option<&str>) -> Self {
        let s = span(self.next_line());
        self.results.symbols.push(Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: self.path.clone(),
            start_line: s.start_line,
            start_column: s.start_column,
            end_line: s.end_line,
            end_column: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            body_span: None,
            body_hash: None,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: parent.map(str::to_string),
            metadata: None,
            annotations: Vec::new(),
            semantic_group: None,
            confidence: None,
            content_type: None,
        });
        self
    }

    pub fn identifier(mut self, from: &str, name: &str, kind: IdentifierKind) -> Self {
        let s = span(self.next_line());
        let ordinal = self.results.identifiers.len();
        self.results.identifiers.push(Identifier {
            id: format!("{from}-ident-{ordinal}"),
            name: name.to_string(),
            kind,
            language: "rust".to_string(),
            file_path: self.path.clone(),
            start_line: s.start_line,
            start_column: s.start_column,
            end_line: s.end_line,
            end_column: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            containing_symbol_id: Some(from.to_string()),
            target_symbol_id: None,
            confidence: 1.0,
            receiver_type: None,
            code_context: None,
        });
        self
    }

    pub fn call(self, from: &str, name: &str) -> Self {
        self.identifier(from, name, IdentifierKind::Call)
    }

    pub fn type_usage(self, from: &str, name: &str) -> Self {
        self.identifier(from, name, IdentifierKind::TypeUsage)
    }

    pub fn relation(mut self, from: &str, to: &str, kind: RelationshipKind) -> Self {
        let line = self.next_line();
        self.results.relationships.push(Relationship {
            id: format!("{from}-{to}-{line}"),
            from_symbol_id: from.to_string(),
            to_symbol_id: to.to_string(),
            kind,
            file_path: self.path.clone(),
            line_number: line,
            span: None,
            reference_site_is_exact: false,
            confidence: 1.0,
            metadata: None,
        });
        self
    }

    pub fn pending(mut self, from: &str, callee: &str, kind: RelationshipKind) -> Self {
        let line = self.next_line();
        self.results
            .pending_relationships
            .push(PendingRelationship {
                from_symbol_id: from.to_string(),
                callee_name: callee.to_string(),
                kind,
                file_path: self.path.clone(),
                line_number: line,
                confidence: 1.0,
            });
        self
    }

    pub fn fact(
        mut self,
        pattern_id: &str,
        containing: Option<&str>,
        confidence: f32,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        let s = span(self.next_line());
        self.results.structural_facts.push(StructuralFact {
            id: format!("fact-{}", self.results.structural_facts.len()),
            file_path: self.path.clone(),
            language: "rust".to_string(),
            pattern_id: pattern_id.to_string(),
            capture_name: "route".to_string(),
            node_kind: "call".to_string(),
            containing_symbol_id: containing.map(str::to_string),
            start_line: s.start_line,
            start_column: s.start_column,
            end_line: s.end_line,
            end_column: s.end_column,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            confidence,
            metadata: Some(metadata),
        });
        self
    }

    pub fn route(
        self,
        containing: &str,
        verb: Option<&str>,
        template: &str,
        confidence: f32,
    ) -> Self {
        let mut meta = HashMap::new();
        meta.insert("route_template".to_string(), json!(template));
        if let Some(verb) = verb {
            meta.insert("verb".to_string(), json!(verb));
        }
        self.fact("axum.route.v1", Some(containing), confidence, meta)
    }

    pub fn client_call(
        self,
        containing: &str,
        verb: Option<&str>,
        target: &str,
        confidence: f32,
    ) -> Self {
        let mut meta = HashMap::new();
        meta.insert("target_path".to_string(), json!(target));
        if let Some(verb) = verb {
            meta.insert("verb".to_string(), json!(verb));
        }
        self.fact("http.client_request.v1", Some(containing), confidence, meta)
    }
}

/// Test extractor: hands back the hand-written results registered for a path.
#[derive(Default)]
pub struct Fixture {
    files: RefCell<HashMap<String, ExtractionResults>>,
    versions: RefCell<HashMap<String, u32>>,
}

impl Extractor for Fixture {
    fn extract(&self, path: &str, _content: &str, _language: &str) -> ExtractionResults {
        self.files.borrow()[path].clone()
    }
}

impl Fixture {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the rows for a path and write them to `store`
    /// under fresh bytes so the writer extracts again.
    pub fn put(&self, store: &mut FactsStore, builder: FileBuilder) {
        let path = builder.path.clone();
        self.files
            .borrow_mut()
            .insert(path.clone(), builder.results);
        let version = {
            let mut versions = self.versions.borrow_mut();
            let v = versions.entry(path.clone()).or_insert(0);
            *v += 1;
            *v
        };
        let mut writer = FactsWriter::new(store, self, Normalization::default());
        writer
            .apply(&[PathChange::Upsert {
                path: path.clone(),
                bytes: format!("{path}#{version}").into_bytes(),
                language: "rust".to_string(),
            }])
            .unwrap();
    }

    pub fn remove(&self, store: &mut FactsStore, path: &str) {
        let mut writer = FactsWriter::new(store, self, Normalization::default());
        writer
            .apply(&[PathChange::Remove {
                path: path.to_string(),
            }])
            .unwrap();
    }
}

pub fn store_with(files: Vec<FileBuilder>) -> (Fixture, FactsStore) {
    let fixture = Fixture::new();
    let mut store = FactsStore::in_memory().unwrap();
    for builder in files {
        fixture.put(&mut store, builder);
    }
    (fixture, store)
}

pub fn graph_of(store: &FactsStore) -> Graph {
    Graph::load(&store.reader(), None).unwrap()
}

pub fn label(graph: &Graph, id: SymbolId) -> String {
    let row = graph.symbol(id);
    format!("{}:{}", row.path, row.name)
}

/// Every edge as `(from, to, kind)` labels, sorted, for whole-list assertions.
pub fn edge_labels(graph: &Graph) -> Vec<(String, String, EdgeKind)> {
    let mut out: Vec<_> = graph
        .edges()
        .map(|Edge { from, to, kind }| (label(graph, from), label(graph, to), kind))
        .collect();
    out.sort();
    out
}

pub fn edges_of_kind(graph: &Graph, kind: EdgeKind) -> Vec<(String, String)> {
    edge_labels(graph)
        .into_iter()
        .filter(|(_, _, k)| *k == kind)
        .map(|(from, to, _)| (from, to))
        .collect()
}

pub fn id_of(graph: &Graph, path: &str, name: &str) -> SymbolId {
    *graph
        .symbols_in_path(path)
        .iter()
        .find(|id| graph.symbol(**id).name == name)
        .unwrap_or_else(|| panic!("no symbol {name} in {path}"))
}

pub fn labels(graph: &Graph, ids: impl Iterator<Item = SymbolId>) -> Vec<String> {
    let mut out: Vec<_> = ids.map(|id| label(graph, id)).collect();
    out.sort();
    out
}
