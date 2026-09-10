use std::path::Path;

use anyhow::Result;
use julie_test_support::{FakeToolContext, SnapshotFixture};

/// A `FakeToolContext` that serves one snapshot built from every file under
/// `tree`, with `tree` as the primary workspace root.
pub fn snapshot_context(tree: impl AsRef<Path>) -> Result<FakeToolContext> {
    let fixture = SnapshotFixture::from_tree(tree)?;
    Ok(FakeToolContext::new()
        .with_workspace_id("snapshot-fixture")
        .with_primary_root(fixture.root().to_path_buf())
        .with_snapshot_fixture(fixture))
}

/// Write `files` (relative path, content) into a temp tree and serve it as
/// the primary snapshot. Keep the returned `TempDir` alive for the test.
pub fn snapshot_context_from_files(
    files: &[(&str, &str)],
) -> Result<(tempfile::TempDir, FakeToolContext)> {
    let tree = tempfile::TempDir::new()?;
    for (path, content) in files {
        let full = tree.path().join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, content)?;
    }
    let context = snapshot_context(tree.path())?;
    Ok((tree, context))
}
