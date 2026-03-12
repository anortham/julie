// Tests for src/handler.rs — JulieServerHandler construction and lifecycle.

use crate::handler::JulieServerHandler;
use anyhow::Result;

#[tokio::test(flavor = "multi_thread")]
async fn handler_construction_sets_workspace_root() -> Result<()> {
    let handler = JulieServerHandler::new_for_test().await?;
    // workspace_root should be set to cwd (the default for new_for_test)
    assert!(handler.workspace_root.is_absolute() || handler.workspace_root.as_os_str() == ".");
    // workspace should start as None (lazy init)
    let ws = handler.workspace.read().await;
    assert!(ws.is_none(), "workspace should be None before initialization");
    Ok(())
}
