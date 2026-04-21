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
use crate::extractors::{Identifier, IdentifierKind, Relationship, Symbol, SymbolKind, Visibility};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResult;
use crate::tools::impact::BlastRadiusTool;
use crate::tools::impact::walk::walk_impacts;

fn make_file(path: &str, hash: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "typescript".to_string(),
        hash: hash.to_string(),
        size: 256,
        last_modified: 1_700_000_000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: 10,
        content: None,
    }
}

fn make_symbol(
    id: &str,
    name: &str,
    file_path: &str,
    metadata: Option<HashMap<String, serde_json::Value>>,
) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "typescript".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        end_line: 3,
        start_column: 0,
        end_column: 0,
        start_byte: 0,
        end_byte: 42,
        parent_id: None,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: Some(Visibility::Public),
        metadata,
        semantic_group: None,
        confidence: Some(1.0),
        code_context: None,
        content_type: None,
    }
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
    Identifier {
        id: id.to_string(),
        name: name.to_string(),
        kind,
        language: "typescript".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        start_column: 0,
        end_line: line,
        end_column: name.len() as u32,
        start_byte: 0,
        end_byte: name.len() as u32,
        containing_symbol_id: containing_symbol_id.map(str::to_string),
        target_symbol_id: target_symbol_id.map(str::to_string),
        confidence,
        code_context: None,
    }
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

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_surfaces_identifier_only_callers() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 10,
        include_tests: true,
        // Explicit readable so we can assert on section headings.
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);

    assert!(
        !text.contains("No impacted symbols found"),
        "identifier-based callers must be reported: {text}"
    );
    for caller in ["setupHandler", "buildPipeline", "configureServer"] {
        assert!(
            text.contains(caller),
            "expected identifier-derived caller `{caller}` in output: {text}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_walk_impacts_preserves_identifier_call_kind_and_resolved_target() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;

    let files = vec![
        make_file("src/alpha.ts", "hash_alpha"),
        make_file("src/beta.ts", "hash_beta"),
        make_file("src/alpha_adapter.ts", "hash_alpha_adapter"),
        make_file("src/beta_adapter.ts", "hash_beta_adapter"),
    ];
    let symbols = vec![
        make_symbol("seed_alpha", "AlphaStore", "src/alpha.ts", None),
        make_symbol("seed_beta", "BetaStore", "src/beta.ts", None),
        make_symbol(
            "alpha_adapter",
            "alphaAdapter",
            "src/alpha_adapter.ts",
            None,
        ),
        make_symbol("beta_adapter", "betaAdapter", "src/beta_adapter.ts", None),
    ];
    let identifiers = vec![
        make_identifier(
            "alpha_ident",
            "AlphaStore",
            "src/alpha_adapter.ts",
            Some("alpha_adapter"),
            Some("seed_alpha"),
            IdentifierKind::TypeUsage,
            4,
            0.90,
        ),
        make_identifier(
            "beta_ident",
            "BetaStore",
            "src/beta_adapter.ts",
            Some("beta_adapter"),
            Some("seed_beta"),
            IdentifierKind::Call,
            7,
            0.95,
        ),
    ];

    let db = handler.primary_database().await?;
    {
        let mut guard = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.bulk_store_fresh_atomic(
            &files,
            &symbols,
            &Vec::<Relationship>::new(),
            &identifiers,
            &[],
            workspace_id.as_str(),
        )?;
        guard.compute_reference_scores()?;

        let seed_symbols =
            guard.get_symbols_by_ids(&["seed_alpha".to_string(), "seed_beta".to_string()])?;
        let impacts = walk_impacts(&guard, &seed_symbols, 1)?;

        let beta_adapter = impacts
            .iter()
            .find(|candidate| candidate.symbol.id == "beta_adapter")
            .expect("beta_adapter should be discovered via identifiers");
        assert_eq!(
            beta_adapter.relationship_kind,
            crate::extractors::RelationshipKind::Calls,
            "identifier kind=call should rank as a direct caller, not a generic reference"
        );
        assert_eq!(
            beta_adapter.via_symbol_name, "BetaStore",
            "identifier-derived impacts should use the resolved target symbol, not the first seed"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_is_deterministic_across_repeated_calls() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let tool = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 10,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    };

    let first = extract_text(&tool.call_tool(&handler).await?);
    let second = extract_text(&tool.call_tool(&handler).await?);

    assert_eq!(
        first, second,
        "two identical back-to-back calls must produce byte-identical output"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_renders_paths_and_related_symbol_headings() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let result = BlastRadiusTool {
        symbol_ids: vec!["seed".to_string()],
        file_paths: vec![],
        from_revision: None,
        to_revision: None,
        max_depth: 2,
        limit: 10,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let text = extract_text(&result);

    assert!(
        text.contains("Likely tests"),
        "expected Likely tests heading: {text}"
    );
    assert!(
        text.contains("tests/store_tests.ts"),
        "expected test path under Likely tests: {text}"
    );
    assert!(
        text.contains("Related test symbols"),
        "expected Related test symbols heading: {text}"
    );
    assert!(
        text.contains("testStoreSnapshot"),
        "expected linked test name under Related test symbols: {text}"
    );

    // Bare names must not leak into the "Likely tests" section. Parse the
    // block between the two headings to check it contains only path-like
    // entries (has a '/' or ends with an extension).
    let likely_start = text.find("Likely tests\n").expect("Likely tests heading");
    let slice_after_heading = &text[likely_start + "Likely tests\n".len()..];
    let likely_block_end = slice_after_heading
        .find("\n\n")
        .unwrap_or(slice_after_heading.len());
    let likely_block = &slice_after_heading[..likely_block_end];
    for line in likely_block.lines() {
        let entry = line.trim_start_matches("- ").trim();
        if entry.is_empty() {
            continue;
        }
        assert!(
            entry.contains('/') || entry.contains('.'),
            "Likely tests block must contain only paths, saw `{entry}`: {text}"
        );
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_blast_radius_defaults_to_compact_format() -> Result<()> {
    let (_temp_dir, handler, workspace_id) = setup_handler().await?;
    seed_identifier_walk_fixture(&handler, &workspace_id).await?;

    let readable = extract_text(
        &BlastRadiusTool {
            symbol_ids: vec!["seed".to_string()],
            file_paths: vec![],
            from_revision: None,
            to_revision: None,
            max_depth: 2,
            limit: 10,
            include_tests: true,
            format: Some("readable".to_string()),
            workspace: Some("primary".to_string()),
        }
        .call_tool(&handler)
        .await?,
    );

    let defaulted = extract_text(
        &BlastRadiusTool {
            symbol_ids: vec!["seed".to_string()],
            file_paths: vec![],
            from_revision: None,
            to_revision: None,
            max_depth: 2,
            limit: 10,
            include_tests: true,
            format: None,
            workspace: Some("primary".to_string()),
        }
        .call_tool(&handler)
        .await?,
    );

    let compact = extract_text(
        &BlastRadiusTool {
            symbol_ids: vec!["seed".to_string()],
            file_paths: vec![],
            from_revision: None,
            to_revision: None,
            max_depth: 2,
            limit: 10,
            include_tests: true,
            format: Some("compact".to_string()),
            workspace: Some("primary".to_string()),
        }
        .call_tool(&handler)
        .await?,
    );

    assert_eq!(
        defaulted, compact,
        "format=None must match compact, not readable — saw `{defaulted}` vs compact `{compact}`"
    );
    assert_ne!(
        defaulted, readable,
        "compact default should differ from readable (blank-line separators)"
    );

    Ok(())
}
