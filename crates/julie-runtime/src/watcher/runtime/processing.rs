use super::*;

use julie_core::file_policy::detect_language_for_indexing_with_content;
use julie_index::checkout_store::PathChange;

impl QueueRuntime {
    pub(super) async fn drain_for_shutdown_inner(&self) {
        let remaining = self.index_queue.lock().await.len();
        if remaining > 0 {
            info!(
                "Queue processor shutting down, draining {} remaining events",
                remaining
            );
            // Acquire the mutation gate for the dispatch loop, then drop it before
            // calling retry_dirty_tantivy (which acquires its own permit).
            // Holding both simultaneously would deadlock on the same workspace_id.
            {
                let Some(guard) = self.acquire_gate_or_mark_rescan("shutdown drain").await else {
                    return;
                };
                let mut drained_any = false;
                let mut affected_paths = HashSet::new();
                let mut store_changes = Vec::new();
                while let Some(event) = self.index_queue.lock().await.pop_front() {
                    affected_paths.extend(self.projection_paths_for_event(&event));
                    store_changes.extend(self.store_changes_for_event(&event));
                    let provider_snapshot = self
                        .embedding_provider
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone();
                    crate::watcher::dispatch_file_event(event, &self.workspace_root);
                    drained_any = true;
                }
                if drained_any {
                    self.apply_store_changes(store_changes, guard).await;
                }
            }
        }
    }

    pub(super) async fn process_queue_batch(&self) -> usize {
        let queue_size = {
            let queue = self.index_queue.lock().await;
            queue.len()
        };

        if queue_size == 0 {
            return 0;
        }

        debug!("Processing {} queued file events", queue_size);

        // Acquire the mutation gate for the duration of the batch. Held until
        // all events in this tick are dispatched so catch-up indexing cannot
        // interleave writes mid-batch.
        let Some(guard) = self.acquire_gate_or_mark_rescan("queue batch").await else {
            return 0;
        };

        let mut processed_count = 0usize;
        let mut deletes = 0usize;
        let mut renames = 0usize;
        let mut affected_paths = HashSet::new();
        let mut store_changes = Vec::new();
        let max_this_tick = queue_size;
        let mut iterations = 0usize;

        while iterations < max_this_tick {
            let event = match {
                let mut queue = self.index_queue.lock().await;
                queue.pop_front()
            } {
                Some(event) => event,
                None => break,
            };
            iterations += 1;

            match event.change_type {
                FileChangeType::Deleted => deletes += 1,
                FileChangeType::Renamed { .. } => renames += 1,
                FileChangeType::Created | FileChangeType::Modified => {}
            }

            debug!("Background task processing: {:?}", event.path);
            affected_paths.extend(self.projection_paths_for_event(&event));
            store_changes.extend(self.store_changes_for_event(&event));

            let provider_snapshot = self
                .embedding_provider
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();

            let atomic_delete_path =
                crate::watcher::dispatch_file_event(event, &self.workspace_root);

            if let Some(path) = atomic_delete_path {
                self.last_processed.lock().await.remove(&path);
            }

            processed_count += 1;
        }

        let remaining_queue_len = self.index_queue.lock().await.len();
        if processed_count > 0 || deletes > 0 || renames > 0 || remaining_queue_len > 0 {
            info!(
                processed = processed_count,
                deletes, renames, remaining_queue_len, "Watcher batch summary"
            );
        }

        {
            let mut last_processed = self.last_processed.lock().await;
            last_processed.retain(|_, timestamp| {
                timestamp
                    .elapsed()
                    .map(|elapsed| elapsed < Duration::from_secs(2))
                    .unwrap_or(false)
            });
        }

        if processed_count > 0 {
            self.apply_store_changes(store_changes, guard).await;
        }

        processed_count
    }

    fn store_changes_for_event(&self, event: &FileChangeEvent) -> Vec<PathChange> {
        let relative = |path: &Path| {
            julie_core::paths::to_relative_unix_style(path, &self.workspace_root).ok()
        };
        let upsert = |path: &Path| {
            let relative = relative(path)?;
            let bytes = self.read_event_path(path)?;
            let language = detect_language_for_indexing_with_content(
                Path::new(&relative),
                &String::from_utf8_lossy(&bytes),
            );
            Some(PathChange::Upsert {
                path: relative,
                bytes,
                language,
            })
        };
        let remove = |path: &Path| {
            self.path_is_confirmed_absent(path)
                .then(|| relative(path))
                .flatten()
                .map(|path| PathChange::Remove { path })
        };
        match &event.change_type {
            FileChangeType::Created | FileChangeType::Modified => {
                upsert(&event.path).into_iter().collect()
            }
            FileChangeType::Deleted => remove(&event.path).into_iter().collect(),
            FileChangeType::Renamed { from, to } => {
                remove(from).into_iter().chain(upsert(to)).collect()
            }
        }
    }

    fn path_is_confirmed_absent(&self, path: &Path) -> bool {
        #[cfg(test)]
        {
            let relative = julie_core::paths::to_relative_unix_style(path, &self.workspace_root)
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            if self
                .rescan_read_failures
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&relative)
            {
                self.set_rescan_pending(true);
                return false;
            }
        }
        match path.try_exists() {
            Ok(exists) => !exists,
            Err(err) => {
                warn!(path = %path.display(), error = %err, "Watcher could not confirm path deletion; rescan scheduled");
                self.set_rescan_pending(true);
                false
            }
        }
    }

    fn read_event_path(&self, path: &Path) -> Option<Vec<u8>> {
        #[cfg(test)]
        {
            let relative = julie_core::paths::to_relative_unix_style(path, &self.workspace_root)
                .unwrap_or_else(|_| path.to_string_lossy().into_owned());
            if self
                .rescan_read_failures
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&relative)
            {
                self.set_rescan_pending(true);
                return None;
            }
        }
        match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(err) => {
                warn!(path = %path.display(), error = %err, "Watcher could not read queued path; rescan scheduled");
                self.set_rescan_pending(true);
                None
            }
        }
    }

    /// Runs the store write on the blocking pool with the gate still held. The
    /// guard is consumed here because nothing else in the batch needs it.
    async fn apply_store_changes(&self, changes: Vec<PathChange>, guard: MutationGuard<'static>) {
        if changes.is_empty() {
            return;
        }
        #[cfg(test)]
        if self.fail_commit_for_test {
            self.set_rescan_pending(true);
            return;
        }
        let store = Arc::clone(&self.store);
        let outcome = tokio::task::spawn_blocking(move || {
            let result = store.apply(&changes, &guard);
            drop(guard);
            result
        })
        .await;
        match outcome {
            Ok(Ok(applied)) => debug!(
                new_blobs = applied.new_blobs,
                reused_blobs = applied.reused_blobs,
                removed_paths = applied.removed_paths,
                "Checkout store applied watcher batch"
            ),
            Ok(Err(err)) => {
                warn!("Checkout store apply failed for watcher batch: {err:#}");
                self.set_rescan_pending(true);
            }
            Err(join) => {
                warn!("Checkout store apply task panicked: {join}");
                self.set_rescan_pending(true);
            }
        }
    }

    pub(super) async fn reconcile_workspace(&self) {
        let Some(guard) = self.acquire_gate_or_mark_rescan("watcher rescan").await else {
            return;
        };
        let store = Arc::clone(&self.store);
        let root = self.workspace_root.clone();
        #[cfg(test)]
        let read_failures = Arc::clone(&self.rescan_read_failures);
        let outcome = tokio::task::spawn_blocking(move || {
            #[cfg(not(test))]
            let reconciliation = crate::workspace::reconcile::reconcile_workspace(&store, &root)?;
            #[cfg(test)]
            let reconciliation =
                crate::workspace::reconcile::reconcile_workspace_with(&store, &root, |path| {
                    let relative = julie_core::paths::to_relative_unix_style(path, &root)
                        .unwrap_or_else(|_| path.to_string_lossy().into_owned());
                    if read_failures
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .remove(&relative)
                    {
                        return Err(std::io::Error::other("injected rescan read failure"));
                    }
                    std::fs::read(path)
                })?;
            let unreadable_paths = reconciliation.unreadable_paths;
            let applied = store.apply(&reconciliation.changes, &guard)?;
            drop(guard);
            anyhow::Ok((applied, unreadable_paths))
        })
        .await;

        match outcome {
            Ok(Ok((applied, unreadable_paths))) if unreadable_paths.is_empty() => {
                debug!(
                    new_blobs = applied.new_blobs,
                    reused_blobs = applied.reused_blobs,
                    removed_paths = applied.removed_paths,
                    "Watcher rescan reconciled checkout store"
                );
                *self
                    .rescan_failed_at
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
                self.set_rescan_status(self.needs_rescan.load(Ordering::Acquire));
            }
            Ok(Ok((_, unreadable_paths))) => {
                warn!(
                    unreadable_paths = unreadable_paths.len(),
                    "Watcher rescan retained unreadable paths; rescan remains pending"
                );
                self.mark_rescan_failed();
            }
            Ok(Err(err)) => {
                warn!("Watcher rescan failed: {err:#}");
                self.mark_rescan_failed();
            }
            Err(join) => {
                warn!("Watcher rescan task panicked: {join}");
                self.mark_rescan_failed();
            }
        }
    }

    fn projection_paths_for_event(&self, event: &FileChangeEvent) -> Vec<String> {
        let paths: Vec<&Path> = match &event.change_type {
            FileChangeType::Renamed { from, to } => vec![from.as_path(), to.as_path()],
            FileChangeType::Created | FileChangeType::Modified | FileChangeType::Deleted => {
                vec![event.path.as_path()]
            }
        };

        paths
            .into_iter()
            .filter_map(|path| {
                julie_core::paths::to_relative_unix_style(path, &self.workspace_root).ok()
            })
            .collect()
    }
}
