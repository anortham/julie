//! Determinism and identifier-walk regression tests for blast_radius.
//!
//! These tests lock down the fixes from the 2026-04-21 blast_radius-fixup plan:
//! - `walk_impacts` must surface identifier-based callers (not just the
//!   relationships table) so references like TypeScript type usages stop
//!   appearing as "No impacted symbols found."
//! - Two back-to-back calls with identical inputs must produce byte-identical
//!   output (no HashMap-iteration leakage).
//! - "Likely tests" (paths) and "Related test symbols" (names) must render
//!   under two distinct headings, with paths taking priority.

use std::collections::HashMap;

use anyhow::Result;
use tempfile::TempDir;

use crate::database::types::FileInfo;
use crate::extractors::{
    Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind, Visibility,
};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tests::helpers::db::{
    file_info_builder, identifier_builder, relationship_builder, symbol_builder,
};
use crate::tools::impact::BlastRadiusTool;
use crate::tools::impact::ranking::rank_impacts;
use crate::tools::impact::seed::resolve_seed_context;
use crate::tools::impact::walk::{
    ImpactCandidate, WalkBudget, walk_impacts, walk_impacts_with_budget,
};

fn make_file(path: &str, hash: &str) -> FileInfo {
    file_info_builder(path)
        .language("typescript")
        .hash(hash)
        .size(256)
        .last_modified(1_700_000_000)
        .last_indexed(0)
        .line_count(10)
        .build()
}

fn make_symbol(
    id: &str,
    name: &str,
    file_path: &str,
    metadata: Option<HashMap<String, serde_json::Value>>,
) -> Symbol {
    let mut builder = symbol_builder(id, name, file_path)
        .language("typescript")
        .span(1, 0, 3, 0)
        .bytes(0, 42)
        .signature(format!("fn {name}()"))
        .visibility(Visibility::Public)
        .confidence(1.0);
    if let Some(metadata) = metadata {
        builder = builder.metadata(metadata);
    }
    builder.build()
}

fn make_symbol_with_kind(id: &str, name: &str, file_path: &str, kind: SymbolKind) -> Symbol {
    let mut symbol = make_symbol(id, name, file_path, None);
    symbol.kind = kind;
    symbol
}

fn make_relationship(
    id: &str,
    from_symbol_id: &str,
    to_symbol_id: &str,
    kind: RelationshipKind,
    file_path: &str,
) -> Relationship {
    relationship_builder(id, from_symbol_id, to_symbol_id)
        .kind(kind)
        .file_path(file_path)
        .build()
}

#[allow(clippy::too_many_arguments)]
fn make_identifier(
    id: &str,
    name: &str,
    file_path: &str,
    containing_symbol_id: Option<&str>,
    target_symbol_id: Option<&str>,
    kind: IdentifierKind,
    line: u32,
    confidence: f32,
) -> Identifier {
    let mut builder = identifier_builder(id, name, file_path)
        .kind(kind)
        .language("typescript")
        .line(line)
        .confidence(confidence);
    if let Some(containing_symbol_id) = containing_symbol_id {
        builder = builder.containing_symbol_id(containing_symbol_id);
    }
    if let Some(target_symbol_id) = target_symbol_id {
        builder = builder.target_symbol_id(target_symbol_id);
    }
    builder.build()
}

fn extract_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|item| {
            serde_json::to_value(item).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(|text| text.to_string())
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn setup_handler() -> Result<(TempDir, JulieServerHandler, String)> {
    let temp_dir = TempDir::new()?;
    let handler = JulieServerHandler::new(temp_dir.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;
    let workspace_id = handler
        .current_workspace_id()
        .expect("initialized workspace should bind a primary workspace id");
    Ok((temp_dir, handler, workspace_id))
}

/// Seed a workspace that exercises the three fixes together:
/// - Identifier-only references (no relationship rows) so walk_impacts must
///   consult the identifiers table to find callers.
/// - Multiple identifier-derived callers across files so ordering matters.
/// - Two linked tests (one path-form, one bare name) so formatting must emit
///   two distinct sections.
async fn seed_identifier_walk_fixture(
    handler: &JulieServerHandler,
    workspace_id: &str,
) -> Result<()> {
    let mut linkage = HashMap::new();
    linkage.insert(
        "test_linkage".to_string(),
        serde_json::json!({
            "test_count": 2,
            "best_tier": "thorough",
            "worst_tier": "basic",
            "linked_tests": ["testStoreSnapshot"],
            "linked_test_paths": ["tests/store_tests.ts"],
            "evidence_sources": ["relationship"],
        }),
    );

    let files = vec![
        make_file("src/store.ts", "hash_store"),
        make_file("src/handler.ts", "hash_handler"),
        make_file("src/pipeline.ts", "hash_pipeline"),
        make_file("src/other.ts", "hash_other"),
        make_file("tests/store_tests.ts", "hash_tests"),
    ];

    let symbols = vec![
        make_symbol("seed", "SpilloverStore", "src/store.ts", Some(linkage)),
        make_symbol("handler_caller", "setupHandler", "src/handler.ts", None),
        make_symbol("pipeline_caller", "buildPipeline", "src/pipeline.ts", None),
        make_symbol("other_caller", "configureServer", "src/other.ts", None),
        make_symbol(
            "test_caller",
            "testStoreSnapshot",
            "tests/store_tests.ts",
            None,
        ),
    ];

    // No relationships — every caller is discovered only through the
    // identifiers table. This mirrors the TypeScript failure reported in the
    // dogfood.
    let relationships: Vec<Relationship> = Vec::new();

    // Build identifiers. Three type_usage references, one call reference, one
    // test reference (pointing at the seed via target_symbol_id). Deliberately
    // ordered alphabetically unfriendly (so the code has to sort).
    let identifiers = vec![
        make_identifier(
            "id1",
            "SpilloverStore",
            "src/pipeline.ts",
            Some("pipeline_caller"),
            Some("seed"),
            IdentifierKind::TypeUsage,
            7,
            0.85,
        ),
        make_identifier(
            "id2",
            "SpilloverStore",
            "src/other.ts",
            Some("other_caller"),
            Some("seed"),
            IdentifierKind::TypeUsage,
            4,
            0.90,
        ),
        make_identifier(
            "id3",
            "SpilloverStore",
            "src/handler.ts",
            Some("handler_caller"),
            Some("seed"),
            IdentifierKind::Call,
            11,
            0.95,
        ),
        make_identifier(
            "id4",
            "SpilloverStore",
            "tests/store_tests.ts",
            Some("test_caller"),
            Some("seed"),
            IdentifierKind::TypeUsage,
            3,
            0.95,
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &relationships,
            &identifiers,
            &[],
            workspace_id,
        )?;
        guard.compute_reference_scores()?;
    }

    Ok(())
}

mod relationship_walk;
mod rendering;
mod seed_resolution;
mod walk_semantics;
