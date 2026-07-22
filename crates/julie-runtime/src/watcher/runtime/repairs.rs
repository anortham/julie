use super::*;

impl QueueRuntime {
    pub(super) async fn retry_persisted_repairs(&self, min_repair_age: Duration) -> usize {
        if !self.index_queue.lock().await.is_empty() {
            return 0;
        }

        let repairs = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard.list_indexing_repairs().unwrap_or_default()
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(i64::MAX);
        let due_repairs: Vec<_> = repairs
            .into_iter()
            .filter(|repair| {
                repair.reason == IndexingRepairReason::ExtractorFailure.as_str()
                    && now.saturating_sub(repair.updated_at) >= min_repair_age.as_secs() as i64
            })
            .collect();

        if due_repairs.is_empty() {
            return 0;
        }

        // Acquire the mutation gate before dispatching repair events.
        let Some(guard) = self
            .acquire_gate_or_mark_rescan("persisted repair retry")
            .await
        else {
            return 0;
        };

        let gitignore =
            match crate::watcher::filtering::build_gitignore_matcher(&self.workspace_root) {
                Ok(gitignore) => Some(gitignore),
                Err(err) => {
                    warn!(
                        "Repair retry failed to build gitignore matcher for {}: {}",
                        self.workspace_root.display(),
                        err
                    );
                    None
                }
            };

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_repair_reason(IndexingRepairReason::ExtractorFailure);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(IndexingOperation::WatcherRepair);

        let provider_snapshot = self
            .embedding_provider
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let mut replayed = 0usize;
        let mut affected_paths = HashSet::new();
        for repair in due_repairs {
            let repair_path = repair.path.clone();
            let absolute_path = self.workspace_root.join(&repair_path);
            if !absolute_path.is_file() {
                let db_guard = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Err(err) = db_guard.clear_indexing_repair(&repair_path) {
                    warn!(
                        "Failed to clear stale persisted repair for {}: {}",
                        repair_path, err
                    );
                }
                continue;
            }

            if !self.repair_path_is_retryable(&absolute_path, gitignore.as_ref()) {
                info!(
                    "Clearing repair for file unsupported by watcher extraction: {}",
                    repair_path
                );
                let db_guard = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if let Err(err) = db_guard.clear_indexing_repair(&repair_path) {
                    warn!(
                        "Failed to clear repair for unsupported file {}: {}",
                        repair_path, err
                    );
                }
                continue;
            }

            crate::watcher::dispatch_file_event(
                FileChangeEvent {
                    path: absolute_path,
                    change_type: FileChangeType::Modified,
                    timestamp: SystemTime::now(),
                },
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
            affected_paths.insert(repair_path);
            replayed += 1;
        }

        let remaining_extractor_repairs = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard
                .list_indexing_repairs()
                .unwrap_or_default()
                .into_iter()
                .filter(|repair| repair.reason == IndexingRepairReason::ExtractorFailure.as_str())
                .count()
        };

        {
            let mut runtime = self
                .indexing_runtime
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if remaining_extractor_repairs == 0 {
                runtime.clear_repair_reason(IndexingRepairReason::ExtractorFailure);
            } else {
                runtime.record_repair_reason(IndexingRepairReason::ExtractorFailure);
            }
            runtime.finish_operation();
        }

        if replayed > 0 {
            self.commit_search_index("repair replay", &affected_paths)
                .await;
        }

        replayed
    }

    fn repair_path_is_retryable(
        &self,
        absolute_path: &Path,
        gitignore: Option<&Gitignore>,
    ) -> bool {
        if !Self::path_has_registered_extractor(absolute_path) {
            return false;
        }

        let Some(gitignore) = gitignore else {
            return true;
        };

        crate::watcher::filtering::should_index_file(
            absolute_path,
            &self.supported_extensions,
            gitignore,
            &self.workspace_root,
        )
    }

    fn path_has_registered_extractor(path: &Path) -> bool {
        let Some(language) = path
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(julie_extractors::language::detect_language_from_extension)
        else {
            return false;
        };

        julie_extractors::registry::registry_entry(language).is_ok()
    }

    pub(super) async fn run_repair_scan_if_needed(&self) {
        let queue_now_empty = self.index_queue.lock().await.is_empty();
        let rescan_pending = self.needs_rescan.load(Ordering::Acquire);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_rescan_pending(rescan_pending);

        if !queue_now_empty || !rescan_pending {
            return;
        }

        // Acquire the mutation gate after early-return checks pass.
        let Some(guard) = self.acquire_gate_or_mark_rescan("repair scan").await else {
            return;
        };

        self.needs_rescan.store(false, Ordering::Release);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_watcher_rescan_pending(false);
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(IndexingOperation::WatcherRepair);
        warn!(
            reason = %IndexingRepairReason::WatcherOverflow,
            "Queue overflow detected, running repair scan for stale and new files"
        );

        let repair_started = Instant::now();
        let indexed_hashes = {
            let db_guard = self
                .db
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            db_guard.get_file_hashes_for_workspace().unwrap_or_default()
        };
        let indexed_set: HashSet<String> = indexed_hashes.keys().cloned().collect();

        let workspace_files =
            match julie_core::workspace_scan::scan_workspace_files(&self.workspace_root) {
                Ok(files) => files,
                Err(err) => {
                    warn!(
                        "Repair scan failed to enumerate workspace files for {}: {}",
                        self.workspace_root.display(),
                        err
                    );
                    self.needs_rescan.store(true, Ordering::Release);
                    self.indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .set_watcher_rescan_pending(true);
                    self.indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .finish_operation();
                    return;
                }
            };
        let gitignore =
            match crate::watcher::filtering::build_gitignore_matcher(&self.workspace_root) {
                Ok(gitignore) => gitignore,
                Err(err) => {
                    warn!(
                        "Repair scan failed to build gitignore matcher for {}: {}",
                        self.workspace_root.display(),
                        err
                    );
                    self.needs_rescan.store(true, Ordering::Release);
                    self.indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .set_watcher_rescan_pending(true);
                    self.indexing_runtime
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .finish_operation();
                    return;
                }
            };

        let provider_snapshot = self
            .embedding_provider
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();

        let mut checked_indexed_files = 0usize;
        let mut skipped_unchanged_files = 0usize;
        let mut deleted_files = 0usize;
        let mut modified_files = 0usize;
        let mut new_files = 0usize;
        let mut failed_hash_reads = 0usize;
        let mut dispatched_events = 0usize;
        let mut affected_paths = HashSet::new();

        for (rel_path, stored_hash) in &indexed_hashes {
            checked_indexed_files += 1;
            let abs_path = self.workspace_root.join(std::path::Path::new(rel_path));
            if !abs_path.is_file() {
                crate::watcher::dispatch_file_event(
                    FileChangeEvent {
                        path: abs_path,
                        change_type: FileChangeType::Deleted,
                        timestamp: SystemTime::now(),
                    },
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
                affected_paths.insert(rel_path.clone());
                deleted_files += 1;
                dispatched_events += 1;
                continue;
            }

            match julie_core::database::calculate_file_hash(&abs_path) {
                Ok(current_hash) if current_hash != *stored_hash => {
                    crate::watcher::dispatch_file_event(
                        FileChangeEvent {
                            path: abs_path,
                            change_type: FileChangeType::Modified,
                            timestamp: SystemTime::now(),
                        },
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
                    affected_paths.insert(rel_path.clone());
                    modified_files += 1;
                    dispatched_events += 1;
                }
                Ok(_) => {
                    skipped_unchanged_files += 1;
                }
                Err(err) => {
                    failed_hash_reads += 1;
                    warn!(
                        "Repair scan hash read failed for {}: {}",
                        abs_path.display(),
                        err
                    );
                }
            }
        }

        for rel_path in workspace_files.difference(&indexed_set) {
            let abs_path = self.workspace_root.join(std::path::Path::new(rel_path));
            if !crate::watcher::filtering::should_index_file(
                &abs_path,
                &self.supported_extensions,
                &gitignore,
                &self.workspace_root,
            ) {
                continue;
            }

            crate::watcher::dispatch_file_event(
                FileChangeEvent {
                    path: abs_path,
                    change_type: FileChangeType::Created,
                    timestamp: SystemTime::now(),
                },
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
            affected_paths.insert(rel_path.clone());
            new_files += 1;
            dispatched_events += 1;
        }

        info!(
            checked_indexed_files,
            skipped_unchanged_files,
            deleted_files,
            modified_files,
            new_files,
            failed_hash_reads,
            elapsed_ms = repair_started.elapsed().as_millis(),
            "Post-overflow repair scan summary"
        );

        if dispatched_events > 0 {
            self.commit_search_index("repair scan", &affected_paths)
                .await;
        }
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_operation();
    }
}
