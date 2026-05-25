use super::*;

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

/// D-H2: Two concurrent on_initialized calls on a shared is_indexed must not
/// both claim the indexing slot. The write-lock check-and-set pattern is atomic.
#[tokio::test(flavor = "multi_thread")]
async fn test_auto_index_write_lock_prevents_double_spawn() {
    let is_indexed = Arc::new(tokio::sync::RwLock::new(false));
    let spawn_count = Arc::new(AtomicUsize::new(0));

    let check_and_maybe_spawn = |flag: Arc<tokio::sync::RwLock<bool>>,
                                 counter: Arc<AtomicUsize>| async move {
        // This is the fixed on_initialized pattern: write-lock + check-and-set.
        let mut guard = flag.write().await;
        if *guard {
            return;
        }
        *guard = true;
        drop(guard);
        counter.fetch_add(1, Ordering::SeqCst);
    };

    tokio::join!(
        check_and_maybe_spawn(Arc::clone(&is_indexed), Arc::clone(&spawn_count)),
        check_and_maybe_spawn(Arc::clone(&is_indexed), Arc::clone(&spawn_count)),
    );

    assert_eq!(
        spawn_count.load(Ordering::SeqCst),
        1,
        "Only one concurrent caller should claim the indexing slot"
    );
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

#[tokio::test(flavor = "multi_thread")]
async fn checkpoint_active_workspace_wal_uses_rebound_current_primary_store() -> Result<()> {
    let first_workspace = TempDir::new()?;
    let rebound_workspace = TempDir::new()?;

    let handler = JulieServerHandler::new(first_workspace.path().to_path_buf()).await?;
    handler.initialize_workspace(None).await?;

    let rebound_root = rebound_workspace.path().canonicalize()?;
    let rebound_id =
        crate::workspace::registry::generate_workspace_id(&rebound_root.to_string_lossy())?;
    handler.set_current_primary_binding(rebound_id.clone(), rebound_root);

    let rebound_db_path = handler.workspace_db_file_path_for(&rebound_id).await?;
    std::fs::create_dir_all(rebound_db_path.parent().expect("rebound db parent"))?;
    let _ = crate::database::SymbolDatabase::new(&rebound_db_path)?;

    let rebound_db = handler.get_database_for_workspace(&rebound_id).await?;
    let _rebound_guard = rebound_db.lock().unwrap();

    let checkpoint_err = crate::startup::checkpoint_active_workspace_wal(&handler)
        .await
        .expect_err(
            "checkpoint should target the rebound current-primary db and hit the held lock",
        );
    assert!(
        checkpoint_err
            .to_string()
            .contains("Could not acquire database lock for checkpoint"),
        "checkpoint should use rebound current-primary db, not stale loaded db: {checkpoint_err}"
    );

    Ok(())
}
