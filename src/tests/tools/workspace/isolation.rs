/// Workspace Isolation Tests
///
/// Tests to ensure workspace operations properly maintain isolation between
/// primary and reference workspaces. Critical for preventing data loss bugs.

#[cfg(test)]
mod workspace_isolation {
    use anyhow::Result;
    use std::fs;
    use tempfile::TempDir;

    use crate::handler::JulieServerHandler;
    use julie_index::checkout_store::{CheckoutStore, PathChange};

    #[tokio::test(flavor = "multi_thread")]
    async fn test_force_reindex_preserves_reference_workspaces() -> Result<()> {
        let primary_workspace = TempDir::new()?;
        let reference_workspace = TempDir::new()?;

        fs::write(
            primary_workspace.path().join("primary.rs"),
            "fn primary_function() { println!(\"primary\"); }",
        )?;
        fs::write(
            reference_workspace.path().join("reference.rs"),
            "fn reference_function() { println!(\"reference\"); }",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(primary_workspace.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let workspace = handler.get_workspace().await?.unwrap();
        let reference_id = crate::workspace::registry::generate_workspace_id(
            &reference_workspace.path().to_string_lossy(),
        )?;

        let ref_index = workspace.workspace_index_path(&reference_id);
        fs::create_dir_all(&ref_index)?;
        let store = CheckoutStore::open(&ref_index, reference_workspace.path())?;
        let bytes = fs::read(reference_workspace.path().join("reference.rs"))?;
        {
            let guard = julie_core::workspace::mutation_gate::acquire_gate(&reference_id).await;
            store.apply(
                &[PathChange::Upsert {
                    path: "reference.rs".into(),
                    bytes,
                    language: "rust".into(),
                }],
                &guard,
            )?;
        }
        assert_eq!(
            store.status().graph.symbols,
            1,
            "Reference workspace should have 1 symbol before force reindex"
        );
        drop(store);

        handler
            .initialize_workspace_with_force(
                Some(primary_workspace.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        assert!(
            ref_index.join("facts.sqlite").exists(),
            "Reference workspace facts.sqlite was deleted during primary force reindex"
        );
        let store_after = CheckoutStore::open(&ref_index, reference_workspace.path())?;
        assert_eq!(
            store_after.status().graph.symbols,
            1,
            "Reference workspace lost its symbols during primary force reindex"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn test_reference_workspaces_get_hnsw_indexes() -> Result<()> {
        let reference_workspace = TempDir::new()?;
        for i in 0..10 {
            fs::write(
                reference_workspace.path().join(format!("file{}.rs", i)),
                format!(
                    r#"
                    pub fn function_{}() {{
                        println!("Function {{}}", {});
                    }}

                    pub struct Struct{} {{
                        field: String,
                    }}
                    "#,
                    i, i, i
                ),
            )?;
        }

        let handler = JulieServerHandler::new_for_test().await?;
        let primary_workspace = TempDir::new()?;
        fs::write(primary_workspace.path().join("main.rs"), "fn main() {}")?;

        handler
            .initialize_workspace_with_force(
                Some(primary_workspace.path().to_string_lossy().to_string()),
                true,
            )
            .await?;

        let reference_id = crate::workspace::registry::generate_workspace_id(
            &reference_workspace.path().to_string_lossy(),
        )?;
        let workspace = handler.get_workspace().await?.unwrap();
        let index_root = workspace.workspace_index_path(&reference_id);
        println!("Expected reference index path: {}", index_root.display());
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn index_root_holds_only_facts_and_tantivy_after_index() -> Result<()> {
        let workspace = TempDir::new()?;
        fs::write(
            workspace.path().join("lib.rs"),
            "pub fn layout_marker() {}\n",
        )?;
        let workspace_path = workspace.path().canonicalize()?;

        let handler = JulieServerHandler::new_for_test().await?;
        crate::tools::workspace::ManageWorkspaceTool {
            operation: "index".to_string(),
            path: Some(workspace_path.to_string_lossy().to_string()),
            force: Some(false),
            name: None,
            workspace_id: None,
            detailed: None,
        }
        .call_tool(&handler)
        .await?;

        let workspace_id =
            crate::workspace::registry::generate_workspace_id(&workspace_path.to_string_lossy())?;
        let loaded = handler.get_workspace().await?.unwrap();
        let index_root = loaded.workspace_index_path(&workspace_id);

        assert!(
            !index_root.join("db").exists(),
            "db/ is not accepted under {}",
            index_root.display()
        );
        assert!(
            !index_root.join("store").exists(),
            "store/ is not accepted under {}",
            index_root.display()
        );

        let mut entries: Vec<String> = fs::read_dir(&index_root)?
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        entries.sort();
        let required = ["facts.sqlite", "tantivy"];
        let optional = ["facts.sqlite-wal", "facts.sqlite-shm"];
        for name in required {
            assert!(
                entries.iter().any(|entry| entry == name),
                "missing {name} under {}: {entries:?}",
                index_root.display()
            );
        }
        for name in &entries {
            assert!(
                required.contains(&name.as_str()) || optional.contains(&name.as_str()),
                "unexpected entry {name} under {}",
                index_root.display()
            );
        }
        Ok(())
    }
}
