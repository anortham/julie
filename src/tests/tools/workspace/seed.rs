use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::handler::JulieServerHandler;
use crate::registry::database::DaemonDatabase;
use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tests::helpers::workspace::{make_isolated_workspace_root, mark_workspace_root};
use crate::tools::workspace::ManageWorkspaceTool;
use crate::tools::workspace::indexing::seed::copy_dir;
use crate::tools::workspace::indexing::seed::sqlite_read_only_uri;
use crate::workspace::registry::generate_workspace_id;

fn fixture_copy(parent: &Path, name: &str) -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/seed")
        .join(name);
    let root = parent.join(name);
    copy_dir(&source, &root).unwrap();
    mark_workspace_root(&root);
    root.canonicalize().unwrap()
}

fn index_tool(path: &Path) -> ManageWorkspaceTool {
    ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
}

#[test]
fn sqlite_read_only_uri_encodes_a_native_absolute_path() {
    let temp = tempfile::tempdir().unwrap();
    let uri = sqlite_read_only_uri(&temp.path().join("seed #?.sqlite")).unwrap();
    assert!(uri.starts_with("file:///"), "{uri}");
    assert!(uri.contains("seed%20%23%3F.sqlite"), "{uri}");
    assert!(uri.ends_with("?mode=ro"), "{uri}");
}

#[cfg(unix)]
#[test]
fn sqlite_read_only_uri_preserves_a_unix_backslash_as_path_data() {
    assert_eq!(
        sqlite_read_only_uri(Path::new(r"/tmp/seed\name.sqlite")).unwrap(),
        "file:///tmp/seed%5Cname.sqlite?mode=ro"
    );
}

#[tokio::test]
async fn seeding_from_a_sibling_copies_shared_files_and_reextracts_changed_ones() {
    let temp = tempfile::tempdir().unwrap();
    let primary_root = make_isolated_workspace_root(temp.path(), "primary");
    fs::write(primary_root.join("main.rs"), "fn primary() {}\n").unwrap();
    let a_root = fixture_copy(temp.path(), "a");
    let b_root = fixture_copy(temp.path(), "b");

    let daemon_db = Arc::new(DaemonDatabase::open(&temp.path().join("daemon.db")).unwrap());
    let primary_path = primary_root.canonicalize().unwrap();
    let primary_id = generate_workspace_id(&primary_path.to_string_lossy()).unwrap();
    let primary_ws = Arc::new(
        crate::workspace::JulieWorkspace::initialize(primary_path.clone())
            .await
            .unwrap(),
    );
    let handler = JulieServerHandler::new_with_shared_workspace(
        primary_ws,
        primary_path,
        Some(Arc::clone(&daemon_db)),
        Some(primary_id),
        None,
    )
    .await
    .unwrap();

    let a_result = index_tool(&a_root).call_tool(&handler).await.unwrap();
    assert!(
        call_tool_result_text(&a_result).contains("8 files"),
        "sibling must be fully indexed first: {}",
        call_tool_result_text(&a_result)
    );

    let b_id = generate_workspace_id(&b_root.to_string_lossy()).unwrap();
    let b_index_root = handler.workspace_index_dir_for(&b_id).await.unwrap();
    let guard = handler.acquire_mutation_guard(&b_id).await;
    let (report, _) = index_tool(&b_root)
        .seed_from_sibling(&handler, &b_root, &b_index_root, Some(&a_root), &guard)
        .await
        .unwrap()
        .expect("a sibling with an index must seed");
    drop(guard);

    assert_eq!(report.sibling_root, a_root);
    assert_eq!(report.copied, 6);
    assert_eq!(report.extracted, 2);
    assert_eq!(report.removed, 0);
    assert_eq!(
        report.to_string(),
        format!(
            "Seeded from {}: 6 blobs copied, 2 extracted, 0 removed in {} ms",
            a_root.display(),
            report.elapsed_ms
        )
    );

    let opened = ManageWorkspaceTool {
        operation: "open".to_string(),
        path: Some(b_root.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    }
    .call_tool(&handler)
    .await
    .unwrap();
    assert!(
        call_tool_result_text(&opened).contains(&b_id),
        "open must bind the seeded index: {}",
        call_tool_result_text(&opened)
    );

    let store = handler
        .checkout_store_for_workspace(&b_id, &b_root)
        .await
        .unwrap();
    let snapshot = store.current();
    let graph = snapshot.graph();
    let shared = graph.find_by_name("shared_only_marker_symbol");
    assert!(
        !shared.is_empty(),
        "copied blob must keep the shared-file symbol"
    );
    assert_eq!(graph.symbol(shared[0]).path, "lib.rs");
    assert!(
        !graph.find_by_name("beta_only_entry").is_empty(),
        "extracted blob must contain the new-checkout symbol"
    );
}

#[tokio::test]
async fn seeding_without_a_sibling_returns_none() {
    let temp = tempfile::tempdir().unwrap();
    let handler = JulieServerHandler::new_for_test().await.unwrap();
    let root = make_isolated_workspace_root(temp.path(), "lonely");
    let id = generate_workspace_id(&root.to_string_lossy()).unwrap();
    let index_root = handler.workspace_index_dir_for(&id).await.unwrap();
    let guard = handler.acquire_mutation_guard(&id).await;

    let seeded = index_tool(&root)
        .seed_from_sibling(&handler, &root, &index_root, None, &guard)
        .await
        .unwrap();

    assert!(seeded.is_none());
}
