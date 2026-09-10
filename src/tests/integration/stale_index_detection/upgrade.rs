use super::*;

/// A `symbols.db` whose `schema_version` is not current is never migrated. The
/// next `index` treats it like an engine-version mismatch: the index directory
/// is deleted, recreated at the current schema, and reindexed.
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn out_of_date_schema_version_recreates_index_directory_and_reindexes() -> Result<()> {
    use rusqlite::Connection;

    unsafe {
        std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
    }

    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path();
    fs::write(workspace_path.join("alpha.rs"), "fn alpha() {}\n")?;

    let handler = create_test_handler(workspace_path).await?;
    index_workspace(&handler, workspace_path).await?;
    let workspace_id = handler.require_primary_workspace_identity()?;
    let db_path = handler.workspace_db_file_path_for(&workspace_id).await?;
    drop(handler);

    let stale_version = crate::database::LATEST_SCHEMA_VERSION - 1;
    {
        let conn = Connection::open(&db_path)?;
        conn.execute("DELETE FROM schema_version", [])?;
        conn.execute(
            "INSERT INTO schema_version (version, applied_at, description)
             VALUES (?1, strftime('%s','now'), 'test downgrade')",
            [stale_version],
        )?;
    }
    let stale_marker = db_path.parent().unwrap().join("stale-directory-marker");
    fs::write(&stale_marker, "present before reindex")?;

    let reopened = JulieServerHandler::new_for_test().await?;
    reopened
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), false)
        .await?;
    assert_eq!(
        crate::database::SymbolDatabase::new(&db_path)?.get_schema_version()?,
        stale_version,
        "opening must not migrate the out-of-date database"
    );

    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&reopened)
    .await?;

    assert!(
        !stale_marker.exists(),
        "index directory must be deleted and recreated, not reused"
    );
    let rebuilt = crate::database::SymbolDatabase::new(&db_path)?;
    assert_eq!(
        rebuilt.get_schema_version()?,
        crate::database::LATEST_SCHEMA_VERSION
    );
    assert!(rebuilt.schema_version_matches()?);
    assert!(
        rebuilt.index_engine_version_matches(
            &workspace_id,
            SEMANTIC_INDEX_ENGINE_COMPONENT,
            SEMANTIC_INDEX_ENGINE_VERSION,
        )?,
        "rebuilt index must record the current engine version"
    );
    assert!(
        fast_search_text(&reopened, "alpha")
            .await?
            .contains("alpha"),
        "rebuilt index must be reindexed from source"
    );

    Ok(())
}

#[test]
fn engine_version_embeds_latest_schema_version() {
    let marker = format!("+schema={}", crate::database::LATEST_SCHEMA_VERSION);
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.ends_with(&marker),
        "SEMANTIC_INDEX_ENGINE_VERSION ({SEMANTIC_INDEX_ENGINE_VERSION}) must end with {marker}"
    );
}
