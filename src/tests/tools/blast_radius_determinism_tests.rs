use std::sync::Arc;

use anyhow::Result;
use julie_context::{ToolContext, WorkspaceTarget};
use julie_extractors::{RelationshipKind, SymbolKind};
use julie_index::graph::{Graph, SymbolId};
use julie_index::snapshot::Snapshot;
use julie_test_support::FakeToolContext;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::snapshot::snapshot_context_from_files;
use crate::tools::impact::BlastRadiusTool;
use crate::tools::impact::ranking::rank_impacts;
use crate::tools::impact::seed::resolve_seed_context;
use crate::tools::impact::walk::{
    ImpactCandidate, WalkBudget, walk_impacts, walk_impacts_with_budget,
};
use crate::tools::navigation::resolution::find_symbols;
use julie_core::Symbol;

fn make_symbol(id: &str, name: &str, file_path: &str, kind: SymbolKind) -> Symbol {
    Symbol {
        extracted: julie_extractors::Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            language: "typescript".to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            end_line: 3,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 42,
            parent_id: None,
            signature: Some(format!("fn {name}()")),
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: Some(1.0),
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
    }
}

async fn snapshot_of(context: &FakeToolContext) -> Result<Arc<Snapshot>> {
    context.snapshot(&WorkspaceTarget::Primary).await
}

fn definition(graph: &Graph, name: &str) -> SymbolId {
    find_symbols(graph, name, None)[0]
}

fn find_impact<'a>(impacts: &'a [ImpactCandidate], name: &str) -> Option<&'a ImpactCandidate> {
    impacts
        .iter()
        .find(|candidate| candidate.symbol.name == name)
}

fn identifier_walk_fixture() -> Result<(TempDir, FakeToolContext)> {
    snapshot_context_from_files(&[
        ("src/store.ts", "export class SnapshotStore {}\n"),
        (
            "src/handler.ts",
            "import { SnapshotStore } from \"./store\";\nexport function setupHandler() { return new SnapshotStore(); }\n",
        ),
        (
            "src/pipeline.ts",
            "import { SnapshotStore } from \"./store\";\nexport function buildPipeline(store: SnapshotStore) { return store; }\n",
        ),
        (
            "src/other.ts",
            "import { SnapshotStore } from \"./store\";\nexport function configureServer(store: SnapshotStore) { return store; }\n",
        ),
        (
            "tests/store_tests.ts",
            "import { SnapshotStore } from \"../src/store\";\nexport function testStoreSnapshot() { return new SnapshotStore(); }\n",
        ),
    ])
}

fn seed_tool(format: Option<&str>) -> BlastRadiusTool {
    BlastRadiusTool {
        symbol_ids: vec!["SnapshotStore".to_string()],
        max_depth: 2,
        limit: 10,
        format: format.map(str::to_string),
        ..Default::default()
    }
}

mod relationship_walk;
mod rendering;
mod seed_resolution;
mod walk_semantics;
