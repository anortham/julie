use super::*;

/// Upgrade regression for Blocker 1 in `docs/PRE-RELEASE-FINDINGS.md`.
///
/// Simulates a v6.9.0 workspace by taking a freshly indexed workspace,
/// removing the post-v6.9 metadata tables, and pinning `schema_version`
/// back to 14 while keeping the existing SQLite rows and Tantivy docs.
///
/// The first non-force index after reopening on v6.10.0 must preserve
/// untouched Tantivy docs from other files, not wipe the search index.
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn test_v690_upgrade_preserves_existing_tantivy_docs_on_first_edit() -> Result<()> {
    use rusqlite::Connection;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();

    let alpha_file = workspace_path.join("alpha.rs");
    fs::write(&alpha_file, "fn alpha() {}\n")?;

    let beta_file = workspace_path.join("beta.rs");
    fs::write(&beta_file, "fn beta() {}\n")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;

    let workspace_id = handler.require_primary_workspace_identity()?;
    let db_path = handler.workspace_db_file_path_for(&workspace_id).await?;

    let (_, search_index) = handler.primary_pooled_database_and_search_index().await?;
    let initial_doc_count = {
        let index = search_index.lock().unwrap();
        let alpha_results =
            index.search_symbols("alpha", &crate::search::SearchFilter::default(), 10)?;
        assert!(
            alpha_results
                .results
                .iter()
                .any(|result| result.name == "alpha"),
            "baseline index should contain alpha before simulated downgrade"
        );

        let beta_results =
            index.search_symbols("beta", &crate::search::SearchFilter::default(), 10)?;
        assert!(
            beta_results
                .results
                .iter()
                .any(|result| result.name == "beta"),
            "baseline index should contain beta before simulated downgrade"
        );

        index.num_docs()
    };
    drop(handler);

    {
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "DROP TABLE IF EXISTS projection_states;
             DROP TABLE IF EXISTS canonical_revisions;
             DROP TABLE IF EXISTS indexing_repairs;
             DELETE FROM schema_version;
             INSERT INTO schema_version (version, applied_at, description)
             VALUES (14, strftime('%s','now'), 'test downgrade to v6.9.0');",
        )?;
    }

    let upgraded = JulieServerHandler::new_for_test().await?;
    upgraded
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), false)
        .await?;

    let upgraded_db = crate::database::SymbolDatabase::new(db_path.clone())?;
    assert_eq!(
        upgraded_db.get_schema_version()?,
        crate::database::LATEST_SCHEMA_VERSION,
        "opening the downgraded workspace should migrate it back to the latest schema"
    );

    {
        let (_, search_index) = upgraded.primary_pooled_database_and_search_index().await?;
        let index = search_index.lock().unwrap();
        let beta_results =
            index.search_symbols("beta", &crate::search::SearchFilter::default(), 10)?;
        assert!(
            beta_results
                .results
                .iter()
                .any(|result| result.name == "beta"),
            "baseline Tantivy docs should still be visible immediately after upgrade reopen"
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    fs::write(&alpha_file, "fn alpha() {}\nfn alpha_new() {}\n")?;

    let non_force_index = crate::tools::workspace::ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    non_force_index.call_tool(&upgraded).await?;

    let (upgraded_db, upgraded_search_index) =
        upgraded.primary_pooled_database_and_search_index().await?;
    {
        let canonical = upgraded_db
            .get_latest_canonical_revision(&workspace_id)?
            .expect("first post-upgrade edit should bootstrap canonical metadata");
        assert!(
            canonical.revision >= 1,
            "canonical revision should exist after the first post-upgrade edit"
        );
        let alpha_new_count: i64 = upgraded_db.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE name = 'alpha_new'",
            [],
            |row| row.get(0),
        )?;
        assert!(
            alpha_new_count > 0,
            "first post-upgrade edit should persist alpha_new in SQLite before Tantivy projection checks"
        );
    }

    let doc_count_after_first_edit = {
        let index = upgraded_search_index.lock().unwrap();
        let beta_results =
            index.search_symbols("beta", &crate::search::SearchFilter::default(), 10)?;
        assert!(
            beta_results
                .results
                .iter()
                .any(|result| result.name == "beta"),
            "first post-upgrade edit must not wipe untouched beta docs from Tantivy"
        );

        index.num_docs()
    };
    assert!(
        doc_count_after_first_edit >= initial_doc_count,
        "first post-upgrade edit should preserve existing docs instead of shrinking Tantivy"
    );

    let upgraded_fast_search_beta = fast_search_text(&upgraded, "beta").await?;
    assert!(
        upgraded_fast_search_beta.contains("beta"),
        "fast_search should still return beta after the first post-upgrade edit"
    );

    drop(upgraded);

    let reopened = JulieServerHandler::new_for_test().await?;
    reopened
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), false)
        .await?;

    let reopened_fast_search_beta = fast_search_text(&reopened, "beta").await?;
    assert!(
        reopened_fast_search_beta.contains("beta"),
        "beta should remain searchable after reopening the upgraded workspace"
    );
    let reopened_fast_search_alpha_new = fast_search_text(&reopened, "alpha_new").await?;
    assert!(
        reopened_fast_search_alpha_new.contains("alpha_new"),
        "new symbol should remain searchable after reopening the upgraded workspace"
    );

    Ok(())
}
