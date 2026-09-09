use super::*;

impl QueueRuntime {
    pub(super) async fn retry_dirty_tantivy(&self) {
        let dirty_paths: Vec<String> = {
            let dirty = self
                .tantivy_dirty
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            dirty.iter().cloned().collect()
        };

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_dirty_projection_count(dirty_paths.len());

        if dirty_paths.is_empty() {
            return;
        }

        let Some(search_index) = self.search_index.as_ref() else {
            warn!(
                reason = %IndexingRepairReason::TantivyDirty,
                dirty_files = dirty_paths.len(),
                "Skipping dirty Tantivy retry because no search index is attached"
            );
            return;
        };

        // Acquire the mutation gate before writing to Tantivy.
        let Some(_guard) = self
            .acquire_gate_or_mark_rescan("dirty Tantivy retry")
            .await
        else {
            return;
        };

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .begin_operation(IndexingOperation::WatcherRepair);

        warn!(
            reason = %IndexingRepairReason::TantivyDirty,
            dirty_files = dirty_paths.len(),
            "Retrying dirty Tantivy projection entries"
        );

        for rel_path in dirty_paths {
            let (symbols, file_content, file_language) = {
                let db_guard = self
                    .db
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let symbols = db_guard.get_symbols_for_file(&rel_path).unwrap_or_default();
                let content = db_guard
                    .get_file_content(&rel_path)
                    .unwrap_or(None)
                    .unwrap_or_default();
                let language = symbols
                    .first()
                    .map(|symbol| symbol.language.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                (symbols, content, language)
            };

            let search_index = Arc::clone(search_index);
            let db_for_retry = Arc::clone(&self.db);
            let rel_clone = rel_path.clone();
            let retry_result = tokio::task::spawn_blocking(move || {
                let db_guard = db_for_retry
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let symbol_ids: Vec<String> =
                    symbols.iter().map(|symbol| symbol.id.clone()).collect();
                let partner_symbol_ids =
                    julie_index::search::projection::collect_relationship_partner_symbol_ids(
                        &db_guard,
                        &symbol_ids,
                    )?;
                julie_index::search::projection::apply_uncommitted_documents_from_symbols(
                    &search_index,
                    &symbols,
                    &rel_clone,
                    &file_content,
                    &file_language,
                    std::slice::from_ref(&rel_clone),
                    &db_guard,
                )?;
                if !partner_symbol_ids.is_empty() {
                    julie_index::search::projection::reproject_partner_symbols(
                        &search_index,
                        &db_guard,
                        &partner_symbol_ids,
                    )?;
                }
                search_index.commit()?;
                Ok::<(), anyhow::Error>(())
            })
            .await;

            match retry_result {
                Ok(Ok(())) => {
                    let remaining_dirty = {
                        let mut dirty = self
                            .tantivy_dirty
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        dirty.remove(&rel_path);
                        dirty.len()
                    };
                    self.tantivy_failure_attempts
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .remove(&rel_path);
                    {
                        let mut runtime = self
                            .indexing_runtime
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        runtime.set_dirty_projection_count(remaining_dirty);
                        runtime.clear_abandoned_projection(&rel_path);
                    }
                    info!("Tantivy retry succeeded for {}", rel_path);
                    if remaining_dirty == 0 {
                        self.persist_projection_state(ProjectionStatus::Ready, None);
                    } else {
                        self.persist_projection_state(
                            ProjectionStatus::Stale,
                            Some("Tantivy projection repair remains pending"),
                        );
                    }
                }
                Ok(Err(err)) => {
                    self.handle_tantivy_retry_failure(&rel_path, &err.to_string());
                }
                Err(err) => {
                    self.handle_tantivy_retry_failure(
                        &rel_path,
                        &format!("retry task panicked: {}", err),
                    );
                }
            }
        }

        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish_operation();
    }

    /// Bumps the per-file retry counter and decides whether to keep retrying,
    /// log a warning, or abandon the file entirely. After
    /// MAX_TANTIVY_RETRY_ATTEMPTS we drop the file from the dirty set, emit a
    /// single ERROR with remediation guidance, and record a repair reason so
    /// the health report surfaces the projection failure instead of letting it
    /// hide behind a silent retry loop.
    fn handle_tantivy_retry_failure(&self, rel_path: &str, error_text: &str) {
        let attempts = {
            let mut attempts = self
                .tantivy_failure_attempts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let count = attempts.entry(rel_path.to_string()).or_insert(0);
            *count += 1;
            *count
        };

        if attempts == 1 {
            warn!(
                "Tantivy retry failed for {} (will retry up to {} times): {}",
                rel_path, MAX_TANTIVY_RETRY_ATTEMPTS, error_text
            );
        } else if attempts >= MAX_TANTIVY_RETRY_ATTEMPTS {
            let remaining_dirty = {
                let mut dirty = self
                    .tantivy_dirty
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                dirty.remove(rel_path);
                dirty.len()
            };
            self.tantivy_failure_attempts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(rel_path);

            let mut runtime = self
                .indexing_runtime
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            runtime.set_dirty_projection_count(remaining_dirty);
            runtime.record_abandoned_projection(rel_path.to_string());
            drop(runtime);

            error!(
                file = %rel_path,
                attempts = attempts,
                last_error = %error_text,
                "Tantivy projection abandoned for {} after {} retries — index directory may be missing on disk. Run manage_workspace operation=index force=true to rebuild.",
                rel_path,
                MAX_TANTIVY_RETRY_ATTEMPTS
            );
        }
    }

    fn persist_projection_state(&self, status: ProjectionStatus, detail: Option<&str>) {
        let db_guard = self
            .db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let canonical = match db_guard.get_latest_canonical_revision(&self.workspace_id) {
            Ok(Some(canonical)) => canonical,
            Ok(None) => return,
            Err(err) => {
                warn!("Failed to read canonical revision for watcher projection state: {err}");
                return;
            }
        };
        let projected_revision = if status == ProjectionStatus::Ready {
            Some(canonical.revision)
        } else {
            match db_guard.get_projection_state(TANTIVY_PROJECTION_NAME, &self.workspace_id) {
                Ok(state) => state.and_then(|state| state.projected_revision),
                Err(err) => {
                    warn!("Failed to read existing watcher projection state: {err}");
                    None
                }
            }
        };

        if let Err(err) = db_guard.upsert_projection_state(
            TANTIVY_PROJECTION_NAME,
            &self.workspace_id,
            status,
            Some(canonical.revision),
            projected_revision,
            detail,
        ) {
            warn!("Failed to persist watcher projection state: {err}");
        }
    }

    fn record_commit_failure(
        &self,
        context: &str,
        affected_paths: &HashSet<String>,
        error_text: &str,
    ) {
        let dirty_count = {
            let mut dirty = self
                .tantivy_dirty
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            dirty.extend(affected_paths.iter().cloned());
            dirty.len()
        };
        self.indexing_runtime
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .set_dirty_projection_count(dirty_count);
        let detail = format!("Tantivy {context} commit failed: {error_text}");
        self.persist_projection_state(ProjectionStatus::Stale, Some(&detail));
        warn!("{detail}");
    }

    pub(super) async fn commit_search_index(
        &self,
        context: &str,
        affected_paths: &HashSet<String>,
    ) {
        let Some(search_index) = self.search_index.as_ref() else {
            return;
        };

        #[cfg(test)]
        if self.fail_commit_for_test {
            self.record_commit_failure(context, affected_paths, "injected watcher commit failure");
            return;
        }

        let search_index = Arc::clone(search_index);
        let commit_result = tokio::task::spawn_blocking(move || search_index.commit()).await;

        match commit_result {
            Ok(Ok(())) => {
                let dirty_count = self
                    .tantivy_dirty
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .len();
                self.indexing_runtime
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .set_dirty_projection_count(dirty_count);
                if dirty_count == 0 {
                    self.persist_projection_state(ProjectionStatus::Ready, None);
                } else {
                    self.persist_projection_state(
                        ProjectionStatus::Stale,
                        Some("Tantivy projection repair remains pending"),
                    );
                }
            }
            Ok(Err(err)) => {
                self.record_commit_failure(context, affected_paths, &err.to_string());
            }
            Err(err) => {
                self.record_commit_failure(
                    context,
                    affected_paths,
                    &format!("commit task panicked: {err}"),
                );
            }
        }
    }
}
