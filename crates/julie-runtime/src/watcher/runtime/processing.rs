use super::*;

impl QueueRuntime {
    pub(super) async fn drain_for_shutdown_inner(&self) {
        let remaining = self.index_queue.lock().await.len();
        if remaining > 0 {
            info!(
                "Queue processor shutting down, draining {} remaining events",
                remaining
            );
            // Acquire the gate for the dispatch loop, then drop it before
            // calling retry_dirty_tantivy (which acquires its own gate).
            // Holding both simultaneously would deadlock on the same workspace_id.
            {
                let Some(guard) = self.acquire_gate_or_mark_rescan("shutdown drain").await else {
                    return;
                };
                let mut drained_any = false;
                let mut affected_paths = HashSet::new();
                while let Some(event) = self.index_queue.lock().await.pop_front() {
                    affected_paths.extend(self.projection_paths_for_event(&event));
                    let provider_snapshot = self
                        .embedding_provider
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone();
                    crate::watcher::dispatch_file_event(
                        event,
                        &self.db,
                        &self.extractor_manager,
                        &self.search_index,
                        &provider_snapshot,
                        &self.workspace_root,
                        &self.lang_configs,
                        &self.tantivy_dirty,
                        &self.indexing_runtime,
                        &guard,
                    )
                    .await;
                    drained_any = true;
                }
                if drained_any {
                    self.commit_search_index("shutdown drain", &affected_paths)
                        .await;
                }
            } // guard dropped here, gate released
        }

        self.retry_dirty_tantivy().await;
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

        // Acquire the mutation gate for the duration of the batch.  Held until
        // all events in this tick are dispatched so catch-up indexing cannot
        // interleave writes mid-batch.
        let Some(guard) = self.acquire_gate_or_mark_rescan("queue batch").await else {
            return 0;
        };

        let mut processed_count = 0usize;
        let mut dropped_duplicates = 0usize;
        let mut deletes = 0usize;
        let mut renames = 0usize;
        let mut affected_paths = HashSet::new();
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

            let should_drop_duplicate = {
                let mut last_processed = self.last_processed.lock().await;
                let now = SystemTime::now();

                match event.change_type {
                    FileChangeType::Created | FileChangeType::Modified => {
                        if let Some(last_time) = last_processed.get(&event.path) {
                            if let Ok(elapsed) = now.duration_since(*last_time) {
                                if elapsed < DUPLICATE_DEBOUNCE_WINDOW {
                                    debug!(
                                        "Dropping duplicate event for {:?} (processed {}ms ago)",
                                        event.path,
                                        elapsed.as_millis()
                                    );
                                    true
                                } else {
                                    last_processed.insert(event.path.clone(), now);
                                    false
                                }
                            } else {
                                last_processed.insert(event.path.clone(), now);
                                false
                            }
                        } else {
                            last_processed.insert(event.path.clone(), now);
                            false
                        }
                    }
                    FileChangeType::Deleted | FileChangeType::Renamed { .. } => {
                        last_processed.insert(event.path.clone(), now);
                        false
                    }
                }
            };

            if should_drop_duplicate {
                dropped_duplicates += 1;
                continue;
            }

            match event.change_type {
                FileChangeType::Deleted => deletes += 1,
                FileChangeType::Renamed { .. } => renames += 1,
                FileChangeType::Created | FileChangeType::Modified => {}
            }

            debug!("Background task processing: {:?}", event.path);
            affected_paths.extend(self.projection_paths_for_event(&event));

            let provider_snapshot = self
                .embedding_provider
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();

            let atomic_delete_path = crate::watcher::dispatch_file_event(
                event,
                &self.db,
                &self.extractor_manager,
                &self.search_index,
                &provider_snapshot,
                &self.workspace_root,
                &self.lang_configs,
                &self.tantivy_dirty,
                &self.indexing_runtime,
                &guard,
            )
            .await;

            if let Some(path) = atomic_delete_path {
                self.last_processed.lock().await.remove(&path);
            }

            processed_count += 1;
        }

        let remaining_queue_len = self.index_queue.lock().await.len();
        if processed_count > 0
            || dropped_duplicates > 0
            || deletes > 0
            || renames > 0
            || remaining_queue_len > 0
        {
            info!(
                processed = processed_count,
                dropped_duplicates, deletes, renames, remaining_queue_len, "Watcher batch summary"
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
            self.commit_search_index("batch", &affected_paths).await;
        }

        processed_count
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
