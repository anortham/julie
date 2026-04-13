use std::fs;
use std::sync::Arc;

use anyhow::Result;
use tempfile::TempDir;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::extractors::{
    Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind,
};
use crate::handler::JulieServerHandler;
use crate::tools::navigation::FastRefsTool;
use crate::workspace::registry::generate_workspace_id;

fn rebound_symbol() -> Symbol {
    Symbol {
        id: "rebound-primary-symbol-id".to_string(),
        name: "rebound_primary_symbol".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/rebound.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 1,
        end_column: 32,
        start_byte: 0,
        end_byte: 32,
        signature: Some("pub fn rebound_primary_symbol()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }
}

fn rebound_caller_symbol() -> Symbol {
    Symbol {
        id: "rebound-primary-caller-id".to_string(),
        name: "rebound_primary_caller".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/rebound.rs".to_string(),
        start_line: 3,
        start_column: 0,
        end_line: 3,
        end_column: 32,
        start_byte: 35,
        end_byte: 67,
        signature: Some("pub fn rebound_primary_caller()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
    }
}

async fn setup_rebound_primary_fast_refs_handler()
-> Result<(JulieServerHandler, String, std::path::PathBuf)> {
    let temp_dir = TempDir::new()?;
    let indexes_dir = temp_dir.path().join("indexes");
    fs::create_dir_all(&indexes_dir)?;

    let original_root = temp_dir.path().join("original-primary");
    let rebound_root = temp_dir.path().join("rebound-primary");
    fs::create_dir_all(original_root.join("src"))?;
    fs::create_dir_all(rebound_root.join("src"))?;
    fs::write(
        original_root.join("src").join("old.rs"),
        "fn old_root_only() {}\n",
    )?;
    fs::write(
        rebound_root.join("src").join("rebound.rs"),
        "pub fn rebound_primary_symbol() {}\n",
    )?;

    let daemon_db = Arc::new(DaemonDatabase::open(&temp_dir.path().join("daemon.db"))?);
    let pool = Arc::new(WorkspacePool::new(
        indexes_dir,
        Some(Arc::clone(&daemon_db)),
        None,
        None,
    ));

    let original_path = original_root.canonicalize()?;
    let original_path_str = original_path.to_string_lossy().to_string();
    let original_id = generate_workspace_id(&original_path_str)?;
    let original_ws = pool
        .get_or_init(&original_id, original_path.clone())
        .await?;

    let handler = JulieServerHandler::new_with_shared_workspace(
        original_ws,
        original_path.clone(),
        Some(Arc::clone(&daemon_db)),
        Some(original_id.clone()),
        None,
        None,
        None,
        None,
        Some(Arc::clone(&pool)),
    )
    .await?;

    daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;

    let rebound_path = rebound_root.canonicalize()?;
    let rebound_path_str = rebound_path.to_string_lossy().to_string();
    let rebound_id = generate_workspace_id(&rebound_path_str)?;
    daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

    let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
    {
        let rebound_db = rebound_ws.db.as_ref().unwrap().clone();
        let mut rebound_db = rebound_db.lock().unwrap();
        let file_info = crate::database::types::FileInfo {
            path: "src/rebound.rs".to_string(),
            language: "rust".to_string(),
            hash: "rebound-primary-hash".to_string(),
            size: 1,
            last_modified: 1,
            last_indexed: 1,
            symbol_count: 2,
            line_count: 4,
            content: Some(
                "pub fn rebound_primary_symbol() {}\n\npub fn rebound_primary_caller() {\n    rebound_primary_symbol();\n}\n"
                    .to_string(),
            ),
        };
        let relationship = Relationship {
            id: "rebound-primary-call-rel".to_string(),
            from_symbol_id: rebound_caller_symbol().id.clone(),
            to_symbol_id: rebound_symbol().id.clone(),
            kind: RelationshipKind::Calls,
            file_path: "src/rebound.rs".to_string(),
            line_number: 4,
            confidence: 1.0,
            metadata: None,
        };
        let identifier = Identifier {
            id: "rebound-primary-call-ident".to_string(),
            name: "rebound_primary_symbol".to_string(),
            kind: IdentifierKind::Call,
            language: "rust".to_string(),
            file_path: "src/rebound.rs".to_string(),
            start_line: 4,
            start_column: 4,
            end_line: 4,
            end_column: 26,
            start_byte: 71,
            end_byte: 93,
            containing_symbol_id: Some(rebound_caller_symbol().id.clone()),
            target_symbol_id: Some(rebound_symbol().id.clone()),
            confidence: 1.0,
            code_context: None,
        };
        rebound_db.bulk_store_fresh_atomic(
            &[file_info],
            &[rebound_symbol(), rebound_caller_symbol()],
            &[relationship],
            &[identifier],
            &[],
            &rebound_id,
        )?;
    }

    handler.set_current_primary_binding(rebound_id.clone(), rebound_path.clone());

    std::mem::forget(temp_dir);

    Ok((handler, rebound_id, rebound_path))
}

#[tokio::test]
async fn test_fast_refs_primary_uses_rebound_current_primary_store() -> Result<()> {
    let (handler, _rebound_id, _rebound_path) = setup_rebound_primary_fast_refs_handler().await?;

    let result = FastRefsTool {
        symbol: "rebound_primary_symbol".to_string(),
        include_definition: true,
        limit: 10,
        workspace: Some("primary".to_string()),
        reference_kind: None,
    }
    .call_tool(&handler)
    .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("src/rebound.rs:1") && !result_text.contains("No references found"),
        "fast_refs should use the rebound current-primary store instead of the stale loaded workspace: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_primary_rejects_swap_gap() -> Result<()> {
    let (handler, _rebound_id, _rebound_path) = setup_rebound_primary_fast_refs_handler().await?;
    handler.publish_loaded_workspace_swap_intent_for_test();

    let err = FastRefsTool {
        symbol: "rebound_primary_symbol".to_string(),
        include_definition: true,
        limit: 10,
        workspace: Some("primary".to_string()),
        reference_kind: None,
    }
    .call_tool(&handler)
    .await
    .expect_err("swap gap should reject primary fast_refs");

    assert!(
        err.to_string()
            .contains("Primary workspace identity unavailable during swap"),
        "unexpected error: {err:#}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_primary_keeps_rebound_source_name_resolution_after_rebind() -> Result<()> {
    let (handler, _rebound_id, _rebound_path) = setup_rebound_primary_fast_refs_handler().await?;

    let result = FastRefsTool {
        symbol: "rebound_primary_symbol".to_string(),
        include_definition: true,
        limit: 10,
        workspace: Some("primary".to_string()),
        reference_kind: Some("call".to_string()),
    }
    .call_tool(&handler)
    .await?;

    let result_text = format!("{:?}", result);
    assert!(
        result_text.contains("rebound_primary_caller (Calls)"),
        "fast_refs should resolve source names from the same rebound primary snapshot used for the main lookup: {result_text}"
    );

    Ok(())
}
