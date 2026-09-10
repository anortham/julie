#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use anyhow::Result;
    use serial_test::serial;
    use tempfile::TempDir;

    use crate::handler::JulieServerHandler;
    use crate::health::{HealthChecker, HealthLevel};
    use crate::tests::test_helpers::create_test_file;
    use crate::tools::workspace::ManageWorkspaceTool;
    use crate::workspace::registry::generate_workspace_id;

    struct SkipEmbeddingsGuard;

    impl SkipEmbeddingsGuard {
        fn new() -> Self {
            unsafe {
                std::env::set_var("JULIE_SKIP_EMBEDDINGS", "1");
            }
            Self
        }
    }

    impl Drop for SkipEmbeddingsGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("JULIE_SKIP_EMBEDDINGS");
            }
        }
    }

    async fn prepare_indexed_workspace() -> Result<(
        SkipEmbeddingsGuard,
        TempDir,
        JulieServerHandler,
        PathBuf,
        String,
    )> {
        let guard = SkipEmbeddingsGuard::new();
        let temp_dir = tempfile::tempdir()?;
        let workspace_path = temp_dir.path().to_path_buf();

        std::fs::create_dir_all(workspace_path.join("src"))?;
        create_test_file(
            &workspace_path.join("src"),
            "lib.rs",
            "pub fn repair_target() {}\n",
        )?;

        let handler = JulieServerHandler::new_for_test().await?;
        handler
            .initialize_workspace_with_force(
                Some(workspace_path.to_string_lossy().to_string()),
                true,
            )
            .await?;

        ManageWorkspaceTool {
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
            generate_workspace_id(&workspace_path.to_string_lossy()).expect("workspace id");

        Ok((guard, temp_dir, handler, workspace_path, workspace_id))
    }

    #[serial(embedding_env)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_system_health_ignores_unconfigured_runtime_plane_for_overall_level() -> Result<()>
    {
        let (_guard, _temp_dir, handler, _workspace_path, _workspace_id) =
            prepare_indexed_workspace().await?;

        {
            let mut workspace = handler.workspace.write().await;
            let workspace = workspace.as_mut().expect("workspace should be initialized");
            workspace.embedding_provider = None;
            workspace.embedding_runtime_status = None;
        }

        let snapshot = HealthChecker::system_snapshot(&handler).await?;

        assert_eq!(
            snapshot.data_plane.level,
            HealthLevel::Ready,
            "indexed workspace should report a healthy data plane"
        );
        assert_eq!(
            snapshot.runtime_plane.level,
            HealthLevel::Unavailable,
            "embedding runtime is intentionally unconfigured in this harness"
        );
        assert_eq!(
            snapshot.overall,
            HealthLevel::Ready,
            "unconfigured runtime should not downgrade an otherwise healthy system"
        );

        Ok(())
    }
}
