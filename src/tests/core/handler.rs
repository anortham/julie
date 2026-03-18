// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.

use crate::handler::JulieServerHandler;
use anyhow::Result;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_sets_workspace_root() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    // workspace_root should be set to cwd (the default for new_for_test)
    assert!(handler.workspace_root.is_absolute() || handler.workspace_root.as_os_str() == ".");
    // workspace should start as None (lazy init)
    let ws = handler.workspace.read().await;
    assert!(
        ws.is_none(),
        "workspace should be None before initialization"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_returns_none_before_initialization() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;

    let checkpoint = crate::startup::checkpoint_active_workspace_wal(&handler).await?;

    assert!(
        checkpoint.is_none(),
        "no workspace should mean no checkpoint"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_runs_after_workspace_initialization() -> Result<()> {
    let workspace = TempDir::new()?;
    let handler = JulieServerHandler::new(workspace.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;

    let checkpoint = crate::startup::checkpoint_active_workspace_wal(&handler).await?;

    assert!(
        checkpoint.is_some(),
        "initialized workspace should expose a database for checkpointing"
    );
    Ok(())
}
