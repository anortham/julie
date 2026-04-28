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
use crate::mcp_compat::CallToolResult;
use crate::tools::navigation::FastRefsTool;
use crate::workspace::registry::generate_workspace_id;

fn extract_text_from_result(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|content_block| {
            serde_json::to_value(content_block).ok().and_then(|json| {
                json.get("text")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

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
        annotations: Vec::new(),
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
        annotations: Vec::new(),
    }
}

fn make_file_info(path: &str, content: &str) -> crate::database::types::FileInfo {
    crate::database::types::FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash-{path}"),
        size: content.len() as i64,
        last_modified: 1,
        last_indexed: 1,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn make_struct_symbol(id: &str, name: &str, file_path: &str, line: u32) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Class,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        start_column: 0,
        end_line: line + 20,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        signature: Some(format!("pub struct {}", name)),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn make_method_symbol(id: &str, name: &str, file_path: &str, line: u32, parent_id: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Method,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        start_column: 0,
        end_line: line + 5,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        signature: Some(format!("pub fn {}()", name)),
        doc_comment: None,
        visibility: None,
        parent_id: Some(parent_id.to_string()),
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    }
}

fn make_identifier(
    id: &str,
    name: &str,
    file_path: &str,
    line: u32,
    start_column: u32,
    end_column: u32,
    containing_symbol_id: Option<&str>,
    target_symbol_id: Option<&str>,
) -> Identifier {
    Identifier {
        id: id.to_string(),
        name: name.to_string(),
        kind: IdentifierKind::Call,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: line,
        start_column,
        end_line: line,
        end_column,
        start_byte: start_column,
        end_byte: end_column,
        containing_symbol_id: containing_symbol_id.map(|value| value.to_string()),
        target_symbol_id: target_symbol_id.map(|value| value.to_string()),
        confidence: 1.0,
        code_context: None,
    }
}

async fn seed_primary_fast_refs_snapshot(
    handler: &JulieServerHandler,
    workspace_id: &str,
    file_infos: &[crate::database::types::FileInfo],
    symbols: &[Symbol],
    relationships: &[Relationship],
    identifiers: &[Identifier],
) -> Result<()> {
    let db = handler.primary_database().await?;
    let mut db = db.lock().unwrap();
    db.bulk_store_fresh_atomic(
        file_infos,
        symbols,
        relationships,
        identifiers,
        &[],
        workspace_id,
    )?;
    Ok(())
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

#[tokio::test]
async fn test_fast_refs_primary_qualified_identifier_fallback_respects_parent_filter() -> Result<()>
{
    let (handler, rebound_id, _rebound_path) = setup_rebound_primary_fast_refs_handler().await?;

    let fast_refs_tool =
        make_struct_symbol("tool-fast-refs", "FastRefsTool", "src/fast_refs.rs", 1);
    let fast_refs_call = make_method_symbol(
        "tool-fast-refs-call",
        "call_tool",
        "src/fast_refs.rs",
        10,
        "tool-fast-refs",
    );
    let other_tool = make_struct_symbol("tool-other", "OtherTool", "src/other.rs", 1);
    let other_call = make_method_symbol(
        "tool-other-call",
        "call_tool",
        "src/other.rs",
        10,
        "tool-other",
    );

    let caller = Symbol {
        id: "caller-fast-refs".to_string(),
        name: "caller_fast_refs".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/caller.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 6,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        signature: Some("pub fn caller_fast_refs()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    };

    let other_caller = Symbol {
        id: "caller-other".to_string(),
        name: "caller_other".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/other_caller.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 6,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        signature: Some("pub fn caller_other()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    };

    seed_primary_fast_refs_snapshot(
        &handler,
        &rebound_id,
        &[
            make_file_info(
                "src/rebound.rs",
                "pub fn rebound_primary_symbol() {}\n\npub fn rebound_primary_caller() {\n    rebound_primary_symbol();\n}\n",
            ),
            make_file_info(
                "src/fast_refs.rs",
                "pub struct FastRefsTool {}\nimpl FastRefsTool {\n    pub fn call_tool() {}\n}\n",
            ),
            make_file_info(
                "src/other.rs",
                "pub struct OtherTool {}\nimpl OtherTool {\n    pub fn call_tool() {}\n}\n",
            ),
            make_file_info(
                "src/caller.rs",
                "pub fn caller_fast_refs() {\n    FastRefsTool::call_tool();\n}\n",
            ),
            make_file_info(
                "src/other_caller.rs",
                "pub fn caller_other() {\n    OtherTool::call_tool();\n}\n",
            ),
        ],
        &[
            rebound_symbol(),
            rebound_caller_symbol(),
            fast_refs_tool,
            fast_refs_call,
            other_tool,
            other_call,
            caller,
            other_caller,
        ],
        &[],
        &[
            make_identifier(
                "ident-fast-refs-call",
                "call_tool",
                "src/caller.rs",
                2,
                4,
                27,
                Some("caller-fast-refs"),
                Some("tool-fast-refs-call"),
            ),
            make_identifier(
                "ident-other-call",
                "call_tool",
                "src/other_caller.rs",
                2,
                4,
                24,
                Some("caller-other"),
                Some("tool-other-call"),
            ),
        ],
    )
    .await?;

    let result = FastRefsTool {
        symbol: "FastRefsTool::call_tool".to_string(),
        include_definition: true,
        limit: 10,
        workspace: Some("primary".to_string()),
        reference_kind: None,
    }
    .call_tool(&handler)
    .await?;

    let result_text = extract_text_from_result(&result);
    assert!(
        result_text.contains("src/caller.rs:")
            && result_text.contains(":2  caller_fast_refs")
            && !result_text.contains("src/other_caller.rs:")
            && !result_text.contains(":2  caller_other"),
        "qualified fast_refs should stay on the matching definition set: {result_text}"
    );

    Ok(())
}

#[tokio::test]
async fn test_fast_refs_primary_identifier_fallback_dedupes_within_batch() -> Result<()> {
    let (handler, rebound_id, _rebound_path) = setup_rebound_primary_fast_refs_handler().await?;

    let fast_refs_tool =
        make_struct_symbol("tool-fast-refs", "FastRefsTool", "src/fast_refs.rs", 1);
    let fast_refs_call = make_method_symbol(
        "tool-fast-refs-call",
        "call_tool",
        "src/fast_refs.rs",
        10,
        "tool-fast-refs",
    );
    let caller = Symbol {
        id: "caller-fast-refs".to_string(),
        name: "caller_fast_refs".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/caller.rs".to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 6,
        end_column: 0,
        start_byte: 0,
        end_byte: 0,
        signature: Some("pub fn caller_fast_refs()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: None,
        content_type: None,
        annotations: Vec::new(),
    };

    seed_primary_fast_refs_snapshot(
        &handler,
        &rebound_id,
        &[
            make_file_info(
                "src/rebound.rs",
                "pub fn rebound_primary_symbol() {}\n\npub fn rebound_primary_caller() {\n    rebound_primary_symbol();\n}\n",
            ),
            make_file_info(
                "src/fast_refs.rs",
                "pub struct FastRefsTool {}\nimpl FastRefsTool {\n    pub fn call_tool() {\n        call_tool();\n        call_tool();\n    }\n}\n",
            ),
            make_file_info(
                "src/caller.rs",
                "pub fn caller_fast_refs() {\n    FastRefsTool::call_tool();\n}\n",
            ),
        ],
        &[rebound_symbol(), rebound_caller_symbol(), fast_refs_tool, fast_refs_call, caller],
        &[],
        &[
            make_identifier(
                "ident-fast-refs-call-a",
                "call_tool",
                "src/caller.rs",
                2,
                4,
                27,
                Some("caller-fast-refs"),
                Some("tool-fast-refs-call"),
            ),
            make_identifier(
                "ident-fast-refs-call-b",
                "call_tool",
                "src/caller.rs",
                2,
                5,
                28,
                Some("caller-fast-refs"),
                Some("tool-fast-refs-call"),
            ),
            make_identifier(
                "ident-fast-refs-call-c",
                "call_tool",
                "src/caller.rs",
                3,
                4,
                27,
                Some("caller-fast-refs"),
                Some("tool-fast-refs-call"),
            ),
        ],
    )
    .await?;

    let result = FastRefsTool {
        symbol: "FastRefsTool::call_tool".to_string(),
        include_definition: true,
        limit: 2,
        workspace: Some("primary".to_string()),
        reference_kind: None,
    }
    .call_tool(&handler)
    .await?;

    let result_text = extract_text_from_result(&result);
    let first_line_count = result_text.matches(":2  caller_fast_refs").count();
    let second_line_count = result_text.matches(":3  caller_fast_refs").count();

    assert_eq!(
        first_line_count, 1,
        "duplicate file:line refs should collapse before limit handling: {result_text}"
    );
    assert_eq!(
        second_line_count, 1,
        "limit should still leave room for the unique line after dedupe: {result_text}"
    );

    Ok(())
}
