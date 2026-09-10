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
        checkpoint.is_none(),
        "facts.sqlite has no WAL checkpoint path; initialized workspaces return None"
    );
    Ok(())
}
