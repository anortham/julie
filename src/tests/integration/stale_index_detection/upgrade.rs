use super::*;

use julie_facts::version::FACTS_SCHEMA_VERSION;

/// A facts.sqlite whose schema_version is not current is never migrated.
/// The next `index` deletes the index directory and reindexes.
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
    let index_dir = handler.workspace_index_dir_for(&workspace_id).await?;
    drop(handler);

    let facts_path = index_dir.join("facts.sqlite");
    {
        let conn = Connection::open(&facts_path)?;
        conn.execute(
            "UPDATE meta SET value = '0' WHERE key = 'schema_version'",
            [],
        )?;
    }
    let stale_marker = index_dir.join("stale-directory-marker");
    fs::write(&stale_marker, "present before reindex")?;

    let reopened = JulieServerHandler::new_for_test().await?;
    reopened
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), false)
        .await?;

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
    let conn = Connection::open(&facts_path)?;
    let schema: String = conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(schema, FACTS_SCHEMA_VERSION.to_string());
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
    let marker = format!("+facts={FACTS_SCHEMA_VERSION}");
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains(&marker),
        "SEMANTIC_INDEX_ENGINE_VERSION ({SEMANTIC_INDEX_ENGINE_VERSION}) must contain {marker}"
    );
}
