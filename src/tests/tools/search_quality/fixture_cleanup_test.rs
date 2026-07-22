use super::helpers::setup_handler_with_fixture;

#[tokio::test(flavor = "multi_thread")]
async fn fixture_workspace_is_removed_when_handler_guard_drops() {
    let handler = setup_handler_with_fixture().await;
    let fixture_root = handler
        .workspace
        .read()
        .await
        .as_ref()
        .expect("fixture workspace should be installed")
        .root
        .clone();

    assert!(fixture_root.exists());
    drop(handler);

    let removed_on_drop = !fixture_root.exists();
    if !removed_on_drop {
        let _ = std::fs::remove_dir_all(&fixture_root);
    }
    assert!(
        removed_on_drop,
        "fixture workspace remained after handler drop: {}",
        fixture_root.display()
    );
}
