use super::http_api::Running;

fn entries(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn assert_only(dir: &std::path::Path, required: &[&str], optional: &[&str]) {
    let found = entries(dir);
    let stray: Vec<&String> = found
        .iter()
        .filter(|name| !required.contains(&name.as_str()) && !optional.contains(&name.as_str()))
        .collect();
    let missing: Vec<&&str> = required
        .iter()
        .filter(|name| !found.iter().any(|f| f == *name))
        .collect();
    assert!(
        stray.is_empty() && missing.is_empty(),
        "{}: stray {stray:?}, missing {missing:?}, found {found:?}",
        dir.display()
    );
}

#[tokio::test]
async fn durable_roots_hold_only_the_registry_service_record_and_indexes() {
    let running = Running::start(None).await;
    let root_dir = tempfile::tempdir().unwrap();
    let root = root_dir.path().canonicalize().unwrap();
    std::fs::write(root.join(".git"), "gitdir: nowhere\n").unwrap();
    std::fs::write(root.join("lib.rs"), "pub fn durable_probe() {}\n").unwrap();
    let indexed = running
        .client()
        .post(format!("{}/api/manage_workspace", running.base))
        .bearer_auth(&running.token)
        .json(&serde_json::json!({"operation": "index", "path": root.to_string_lossy()}))
        .send()
        .await
        .unwrap();
    assert_eq!(indexed.status(), 200);

    let id = crate::workspace::registry::generate_workspace_id(&root.to_string_lossy()).unwrap();
    let home = running.paths.julie_home();
    assert_only(
        &home,
        &["registry.db", "service.json", "indexes"],
        &["registry.db-wal", "registry.db-shm"],
    );
    assert_only(&home.join("indexes"), &[id.as_str()], &[]);
    let index_dir = running.paths.workspace_index_dir(&id);
    assert_only(&index_dir, &["db", "store", "tantivy"], &[]);
    assert_only(
        &index_dir.join("db"),
        &["symbols.db"],
        &["symbols.db-wal", "symbols.db-shm"],
    );
    assert_only(
        &index_dir.join("store"),
        &["facts.sqlite", "tantivy"],
        &["facts.sqlite-wal", "facts.sqlite-shm"],
    );
    assert!(index_dir.join("store/tantivy/julie.meta.json").is_file());
}
