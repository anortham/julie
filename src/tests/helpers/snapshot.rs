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
