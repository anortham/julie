//! Tests for the edit_symbol tool's pure editing functions.
//!
//! These test replace_symbol_body and insert_near_symbol directly,
//! not the full MCP tool flow (which requires an indexed workspace).

use crate::tools::editing::edit_symbol::{insert_near_symbol, replace_symbol_body};
use crate::tools::editing::validation::check_bracket_balance;

#[test]
fn test_replace_symbol_body() {
    let source =
        "fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";

    let result = replace_symbol_body(source, 1, 3, "fn hello() {\n    println!(\"goodbye\");\n}")
        .expect("Replace should succeed");

    assert!(result.contains("goodbye"), "Should contain new body");
    assert!(
        result.contains("fn world()"),
        "Should preserve other functions"
    );
    assert!(
        !result.contains("println!(\"hello\")"),
        "Should not contain old body"
    );
}

#[test]
fn test_insert_after_symbol() {
    let source = "struct Foo {\n    x: i32,\n}\n\nfn bar() {}\n";

    let result = insert_near_symbol(
        source,
        3,
        "\nimpl Foo {\n    fn new() -> Self { Self { x: 0 } }\n}",
        "after",
    )
    .expect("Insert after should succeed");

    assert!(result.contains("impl Foo"), "Should contain inserted code");
    let struct_pos = result.find("struct Foo").unwrap();
    let impl_pos = result.find("impl Foo").unwrap();
    let bar_pos = result.find("fn bar").unwrap();
    assert!(struct_pos < impl_pos, "impl should be after struct");
    assert!(impl_pos < bar_pos, "impl should be before bar");
}

#[test]
fn test_insert_before_symbol() {
    let source = "fn process() {\n    // work\n}\n";

    let result = insert_near_symbol(source, 1, "/// Process all items.", "before")
        .expect("Insert before should succeed");

    let doc_pos = result.find("/// Process all items.").unwrap();
    let fn_pos = result.find("fn process()").unwrap();
    assert!(doc_pos < fn_pos, "Doc comment should be before function");
}

#[test]
fn test_replace_preserves_surrounding_content() {
    let source = "// header comment\n\nfn target() {\n    old_code();\n}\n\n// footer comment\n";

    let result = replace_symbol_body(source, 3, 5, "fn target() {\n    new_code();\n}")
        .expect("Replace should succeed");

    assert!(
        result.contains("// header comment"),
        "Should preserve header"
    );
    assert!(
        result.contains("// footer comment"),
        "Should preserve footer"
    );
    assert!(result.contains("new_code()"), "Should contain new code");
}

#[test]
fn test_invalid_line_range() {
    let source = "fn hello() {}\n";
    let result = replace_symbol_body(source, 5, 10, "new code");
    assert!(result.is_err(), "Should fail for out-of-range lines");
}

#[test]
fn test_insert_at_invalid_line() {
    let source = "fn hello() {}\n";
    let result = insert_near_symbol(source, 100, "new code", "after");
    assert!(result.is_err(), "Should fail for out-of-range line");
}

#[test]
fn test_replace_helper_is_unguarded() {
    // replace_symbol_body is a pure line-manipulation helper with no freshness check.
    // The freshness guard lives in EditSymbolTool::call_tool (blake3 hash comparison).
    // This test documents that the helper applies blindly -- callers must verify freshness.
    let modified_file = "line1\nnew_line_inserted\nfn foo() {\n    bar()\n}\nline5\n";
    let result = replace_symbol_body(modified_file, 2, 4, "fn foo() {\n    baz()\n}");
    assert!(result.is_ok());
    let content = result.unwrap();
    // The helper replaces lines 2-4 regardless of what's there.
    // In a stale-index scenario, this produces wrong output.
    // call_tool's freshness check prevents this from happening in practice.
    assert!(
        !content.contains("fn foo() {\n    bar()"),
        "Old foo body should be replaced"
    );
}

/// insert_near_symbol must preserve the source's trailing-newline behavior.
/// If source has no trailing newline, result must also have none.
#[test]
fn test_insert_near_symbol_no_trailing_newline_preserved() {
    let source = "fn a() {}\nfn b() {}"; // no trailing newline
    let result =
        insert_near_symbol(source, 1, "// inserted", "before").expect("Insert should succeed");
    assert!(
        !result.ends_with('\n'),
        "insert_near_symbol must not add trailing newline when source has none, got: {:?}",
        result
    );
}

/// insert_near_symbol must preserve the source's trailing newline when present.
#[test]
fn test_insert_near_symbol_trailing_newline_preserved_when_present() {
    let source = "fn a() {}\nfn b() {}\n"; // has trailing newline
    let result =
        insert_near_symbol(source, 1, "// inserted", "before").expect("Insert should succeed");
    assert!(
        result.ends_with('\n'),
        "insert_near_symbol must keep trailing newline when source has one, got: {:?}",
        result
    );
}

#[test]
fn test_bracket_in_string_warns_instead_of_rejecting() {
    let before = "fn foo() {\n    println!(\"hello\");\n}\n";
    let after = "fn foo() {\n    println!(\"hello {\");\n}\n";

    let result = check_bracket_balance(before, after);
    assert!(result.is_some(), "Should warn about bracket change");
    assert!(
        result.unwrap().contains("Warning"),
        "Should be advisory warning"
    );
}

#[test]
fn test_balanced_edit_no_warning() {
    let before = "fn foo() {\n    bar();\n}\n";
    let after = "fn foo() {\n    baz();\n}\n";

    let result = check_bracket_balance(before, after);
    assert!(result.is_none(), "Balanced edit should produce no warning");
}

#[cfg(test)]
mod integration {
    use crate::handler::JulieServerHandler;
    use crate::mcp_compat::CallToolResult;
    use crate::tools::editing::edit_symbol::EditSymbolTool;
    use crate::tools::workspace::ManageWorkspaceTool;
    use anyhow::Result;
    use futures::poll;
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Extract all text from a CallToolResult by walking the content blocks.
    fn extract_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .filter_map(|block| {
                serde_json::to_value(block).ok().and_then(|json| {
                    json.get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Create a temp workspace with one Rust source file, index it, and return
    /// the (TempDir, handler, relative-path-to-file) triple.
    /// TempDir must stay alive for the duration of the test.
    async fn setup_indexed_workspace(
        content: &str,
    ) -> Result<(TempDir, JulieServerHandler, String)> {
        let temp_dir = TempDir::new()?;
        let workspace_path = temp_dir.path().to_path_buf();

        let src_dir = workspace_path.join("src");
        fs::create_dir_all(&src_dir)?;
        let file_path = src_dir.join("test.rs");
        fs::write(&file_path, content)?;

        // Construct the handler with the temp dir as workspace_root so that
        // secure_path_resolution (used by the freshness guard) resolves relative
        // paths like "src/test.rs" against the correct base.
        let handler = JulieServerHandler::new(workspace_path.clone()).await?;

        let index_tool = ManageWorkspaceTool {
            operation: "index".to_string(),
            workspace_id: None,
            path: Some(workspace_path.to_string_lossy().to_string()),
            name: None,
            force: Some(false),
            detailed: None,
        };
        index_tool.call_tool(&handler).await?;

        Ok((temp_dir, handler, "src/test.rs".to_string()))
    }

    /// Index, replace one function body via call_tool, verify the on-disk file changed
    /// correctly and the untouched function is preserved.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_replace_via_index() -> Result<()> {
        let source = "pub fn greet() {\n    println!(\"hello\");\n}\n\npub fn farewell() {\n    println!(\"goodbye\");\n}\n";
        let (_temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

        let tool = EditSymbolTool {
            symbol: "greet".to_string(),
            operation: "replace".to_string(),
            content: "pub fn greet() {\n    println!(\"hi there\");\n}".to_string(),
            file_path: None,
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = extract_text(&result);

        // The tool should confirm the apply, not report an error.
        assert!(
            !text.contains("Error:"),
            "Expected successful apply, got: {}",
            text
        );
        assert!(
            text.contains("Applied replace"),
            "Expected 'Applied replace' in response, got: {}",
            text
        );

        // Verify on-disk file reflects the change.
        let workspace = handler
            .get_workspace()
            .await?
            .expect("workspace must exist");
        let abs_path = workspace.root.join("src").join("test.rs");
        let on_disk = fs::read_to_string(&abs_path)?;

        assert!(
            on_disk.contains("hi there"),
            "On-disk file should contain new body, got: {}",
            on_disk
        );
        assert!(
            !on_disk.contains("println!(\"hello\")"),
            "Old body should be gone, got: {}",
            on_disk
        );
        assert!(
            on_disk.contains("fn farewell()"),
            "Untouched function should be preserved, got: {}",
            on_disk
        );

        Ok(())
    }

    /// Index a file, then mutate it on disk (simulating out-of-band change),
    /// then attempt edit_symbol. The freshness guard should refuse with a stale-index error.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_rejects_stale_index() -> Result<()> {
        let source = "pub fn stable() {\n    // original\n}\n";
        let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

        // Mutate the file on disk without re-indexing.
        let abs_path = temp_dir.path().join("src").join("test.rs");
        fs::write(
            &abs_path,
            "pub fn stable() {\n    // mutated after indexing\n}\n",
        )?;

        let tool = EditSymbolTool {
            symbol: "stable".to_string(),
            operation: "replace".to_string(),
            content: "pub fn stable() {\n    // new body\n}".to_string(),
            file_path: None,
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = extract_text(&result);

        assert!(
            text.contains("changed since last indexing"),
            "Expected stale-index error, got: {}",
            text
        );

        Ok(())
    }

    /// Dry run with insert_after: response must contain the preview, but the
    /// on-disk file must remain untouched.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_insert_after_dry_run() -> Result<()> {
        let source = "pub fn compute() {\n    let x = 1;\n}\n";
        let (temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

        let inserted = "pub fn helper() {\n    // assists compute\n}";
        let tool = EditSymbolTool {
            symbol: "compute".to_string(),
            operation: "insert_after".to_string(),
            content: inserted.to_string(),
            file_path: None,
            dry_run: true,
        };

        let result = tool.call_tool(&handler).await?;
        let text = extract_text(&result);

        assert!(
            text.contains("Dry run preview"),
            "Expected dry-run preview header, got: {}",
            text
        );
        assert!(
            text.contains("helper"),
            "Preview should show inserted content, got: {}",
            text
        );

        // File must be unchanged on disk.
        let abs_path = temp_dir.path().join("src").join("test.rs");
        let on_disk = fs::read_to_string(&abs_path)?;
        assert_eq!(on_disk, source, "Dry run must not modify the file on disk");

        Ok(())
    }

    /// Attempt to edit a symbol that was never indexed. Should get a "not found" error.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_not_found() -> Result<()> {
        let source = "pub fn real_function() {\n    // exists\n}\n";
        let (_temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

        let tool = EditSymbolTool {
            symbol: "ghost_function_xyz".to_string(),
            operation: "replace".to_string(),
            content: "pub fn ghost_function_xyz() {}".to_string(),
            file_path: None,
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = extract_text(&result);

        assert!(
            text.contains("not found"),
            "Expected 'not found' error for missing symbol, got: {}",
            text
        );

        Ok(())
    }

    /// After edit_symbol writes a file, the DB hash must NOT be updated (hash poisoning fix).
    /// A second edit call before the watcher re-indexes must fail the freshness guard because
    /// the file hash changed but the indexed hash is still the pre-edit value.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_does_not_poison_watcher_hash() -> Result<()> {
        let source = "pub fn target() {\n    let x = 1;\n}\n";
        let (_temp_dir, handler, _rel_path) = setup_indexed_workspace(source).await?;

        let first_edit = EditSymbolTool {
            symbol: "target".to_string(),
            operation: "replace".to_string(),
            content: "pub fn target() {\n    let x = 2;\n}".to_string(),
            file_path: None,
            dry_run: false,
        };
        let result = first_edit.call_tool(&handler).await?;
        let text = extract_text(&result);
        assert!(
            text.contains("Applied replace"),
            "First edit should succeed, got: {}",
            text
        );

        // Second edit with no watcher re-index must fail the freshness guard.
        // Before the fix, update_file_hash() poisoned the DB so the second edit
        // incorrectly succeeded. After the fix the DB still holds the pre-edit hash
        // and the guard fires.
        let second_edit = EditSymbolTool {
            symbol: "target".to_string(),
            operation: "replace".to_string(),
            content: "pub fn target() {\n    let x = 3;\n}".to_string(),
            file_path: None,
            dry_run: false,
        };
        let result2 = second_edit.call_tool(&handler).await?;
        let text2 = extract_text(&result2);
        assert!(
            text2.contains("changed since last indexing"),
            "Second edit without re-index must fail (hash must not be poisoned), got: {}",
            text2
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_uses_current_primary_db_after_rebind() -> Result<()> {
        use crate::daemon::database::DaemonDatabase;
        use crate::daemon::workspace_pool::WorkspacePool;
        use crate::workspace::registry::generate_workspace_id;

        let temp_dir = TempDir::new()?;
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir)?;

        let original_root = temp_dir.path().join("original-primary");
        let rebound_root = temp_dir.path().join("rebound-primary");
        fs::create_dir_all(original_root.join("src"))?;
        fs::create_dir_all(rebound_root.join("src"))?;
        fs::write(
            original_root.join("src").join("test.rs"),
            "pub fn original_only() { println!(\"original\"); }\n",
        )?;
        fs::write(
            rebound_root.join("src").join("test.rs"),
            "pub fn rebound_target() { println!(\"before\"); }\n",
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

        let rebound_path = rebound_root.canonicalize()?;
        let rebound_path_str = rebound_path.to_string_lossy().to_string();
        let rebound_id = generate_workspace_id(&rebound_path_str)?;
        daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
        daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;

        let rebound_ws = pool.get_or_init(&rebound_id, rebound_path.clone()).await?;
        {
            let rebound_file_path = rebound_root.join("src").join("test.rs");
            let rebound_hash = crate::database::calculate_file_hash(&rebound_file_path)?;
            let rebound_db = rebound_ws
                .db
                .as_ref()
                .expect("rebound workspace db should exist")
                .clone();
            let mut rebound_db = rebound_db.lock().unwrap();
            let file_info = crate::database::types::FileInfo {
                path: "src/test.rs".to_string(),
                language: "rust".to_string(),
                hash: rebound_hash,
                size: 64,
                last_modified: 1,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 1,
                content: Some("pub fn rebound_target() { println!(\"before\"); }\n".to_string()),
            };
            let symbol = crate::extractors::Symbol {
                id: "rebound_symbol".to_string(),
                name: "rebound_target".to_string(),
                kind: crate::extractors::SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/test.rs".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 1,
                end_column: 24,
                start_byte: 0,
                end_byte: 24,
                signature: Some("pub fn rebound_target()".to_string()),
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
            };
            rebound_db.bulk_store_fresh_atomic(
                &[file_info],
                &[symbol],
                &[],
                &[],
                &[],
                &rebound_id,
            )?;
        }

        {
            let rebound_db = rebound_ws
                .db
                .as_ref()
                .expect("rebound workspace db should exist")
                .lock()
                .unwrap();
            let rebound_symbols = crate::tools::deep_dive::data::find_symbol(
                &rebound_db,
                "rebound_target",
                Some("src/test.rs"),
            )?;
            assert!(
                !rebound_symbols.is_empty(),
                "test setup should index rebound_target into rebound workspace db"
            );
        }

        handler.set_current_primary_binding(rebound_id, rebound_path.clone());

        {
            let rebound_workspace = pool
                .get(
                    handler
                        .current_workspace_id()
                        .as_deref()
                        .expect("current workspace id should be rebound"),
                )
                .await
                .expect("rebound workspace should exist in pool");
            let rebound_db = rebound_workspace
                .db
                .as_ref()
                .expect("rebound workspace db should exist")
                .lock()
                .unwrap();
            let rebound_symbols = crate::tools::deep_dive::data::find_symbol(
                &rebound_db,
                "rebound_target",
                Some("src/test.rs"),
            )?;
            assert!(
                !rebound_symbols.is_empty(),
                "rebound current-primary db should expose rebound_target to the editing handler"
            );
        }

        let tool = EditSymbolTool {
            symbol: "rebound_target".to_string(),
            operation: "replace".to_string(),
            content: "pub fn rebound_target() {\n    println!(\"after\");\n}".to_string(),
            file_path: Some("src/test.rs".to_string()),
            dry_run: false,
        };

        let result = tool.call_tool(&handler).await?;
        let text = extract_text(&result);
        assert!(
            text.contains("Applied replace"),
            "edit_symbol after current-primary rebind should use rebound DB/index: {text}"
        );

        let rebound_on_disk = fs::read_to_string(rebound_root.join("src").join("test.rs"))?;
        assert!(rebound_on_disk.contains("after"));
        let original_on_disk = fs::read_to_string(original_root.join("src").join("test.rs"))?;
        assert!(original_on_disk.contains("original"));

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_edit_symbol_keeps_primary_binding_snapshot_across_swap_window() -> Result<()> {
        use crate::daemon::database::DaemonDatabase;
        use crate::daemon::workspace_pool::WorkspacePool;
        use crate::workspace::registry::generate_workspace_id;

        let temp_dir = TempDir::new()?;
        let indexes_dir = temp_dir.path().join("indexes");
        fs::create_dir_all(&indexes_dir)?;

        let original_root = temp_dir.path().join("original-primary");
        let rebound_root = temp_dir.path().join("rebound-primary");
        fs::create_dir_all(original_root.join("src"))?;
        fs::create_dir_all(rebound_root.join("src"))?;
        fs::write(
            original_root.join("src").join("test.rs"),
            "pub fn original_target() { println!(\"before\"); }\n",
        )?;
        fs::write(
            rebound_root.join("src").join("test.rs"),
            "pub fn rebound_target() { println!(\"rebound\"); }\n",
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
        daemon_db.upsert_workspace(&original_id, &original_path_str, "ready")?;
        let original_ws = pool
            .get_or_init(&original_id, original_path.clone())
            .await?;
        {
            let original_file_path = original_root.join("src").join("test.rs");
            let original_hash = crate::database::calculate_file_hash(&original_file_path)?;
            let original_db = original_ws
                .db
                .as_ref()
                .expect("original workspace db should exist")
                .clone();
            let mut original_db = original_db.lock().unwrap();
            let file_info = crate::database::types::FileInfo {
                path: "src/test.rs".to_string(),
                language: "rust".to_string(),
                hash: original_hash,
                size: 64,
                last_modified: 1,
                last_indexed: 0,
                symbol_count: 1,
                line_count: 1,
                content: Some("pub fn original_target() { println!(\"before\"); }\n".to_string()),
            };
            let symbol = crate::extractors::Symbol {
                id: "original_symbol".to_string(),
                name: "original_target".to_string(),
                kind: crate::extractors::SymbolKind::Function,
                language: "rust".to_string(),
                file_path: "src/test.rs".to_string(),
                start_line: 1,
                start_column: 0,
                end_line: 1,
                end_column: 25,
                start_byte: 0,
                end_byte: 25,
                signature: Some("pub fn original_target()".to_string()),
                doc_comment: None,
                visibility: None,
                parent_id: None,
                metadata: None,
                semantic_group: None,
                confidence: None,
                code_context: None,
                content_type: None,
            };
            original_db.bulk_store_fresh_atomic(
                &[file_info],
                &[symbol],
                &[],
                &[],
                &[],
                &original_id,
            )?;
        }

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

        let rebound_path = rebound_root.canonicalize()?;
        let rebound_path_str = rebound_path.to_string_lossy().to_string();
        let rebound_id = generate_workspace_id(&rebound_path_str)?;
        daemon_db.upsert_workspace(&rebound_id, &rebound_path_str, "ready")?;
        pool.get_or_init(&rebound_id, rebound_path.clone()).await?;

        let workspace_write_guard = handler.workspace.write().await;
        let edit_tool = EditSymbolTool {
            symbol: "original_target".to_string(),
            operation: "replace".to_string(),
            content: "pub fn original_target() {\n    println!(\"after\");\n}".to_string(),
            file_path: Some("src/test.rs".to_string()),
            dry_run: false,
        };
        let mut edit_future = Box::pin(edit_tool.call_tool(&handler));
        assert!(
            poll!(edit_future.as_mut()).is_pending(),
            "edit_symbol should block on the first await while the workspace lock is held"
        );

        handler.set_current_primary_binding(rebound_id, rebound_path.clone());
        drop(workspace_write_guard);

        let result = edit_future.await?;
        let text = extract_text(&result);
        assert!(
            text.contains("Applied replace"),
            "snapshot-bound edit should still succeed across the swap window: {text}"
        );

        let original_on_disk = fs::read_to_string(original_root.join("src").join("test.rs"))?;
        assert!(
            original_on_disk.contains("after"),
            "edit should apply to the original root selected at call start"
        );

        let rebound_on_disk = fs::read_to_string(rebound_root.join("src").join("test.rs"))?;
        assert!(
            rebound_on_disk.contains("rebound"),
            "swap-window edit must not leak into the rebound root"
        );

        Ok(())
    }
}
