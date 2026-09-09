use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::search::SearchIndex;
use julie_core::Symbol;
use julie_core::database::{FileInfo, ProjectionState, ProjectionStatus, SymbolDatabase};

use super::apply::{
    SymbolIndexContext, apply_documents_with_context, load_enriched_relationship_text,
    load_symbol_contexts_from_database,
};
use super::{SearchProjection, projection_served_revision};

impl SearchProjection {
    pub fn project_documents(
        &self,
        db: &mut SymbolDatabase,
        index: &SearchIndex,
        symbols: &[Symbol],
        file_infos: &[FileInfo],
        files_to_clean: &[String],
        target_revision: Option<i64>,
    ) -> Result<ProjectionState> {
        let Some(target_revision) = target_revision else {
            return Ok(db
                .get_projection_state(self.projection, &self.workspace_id)?
                .unwrap_or(db.upsert_projection_state(
                    self.projection,
                    &self.workspace_id,
                    ProjectionStatus::Missing,
                    None,
                    None,
                    None,
                )?));
        };

        let current_projected_revision = db
            .get_projection_state(self.projection, &self.workspace_id)?
            .as_ref()
            .and_then(projection_served_revision);

        db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Building,
            Some(target_revision),
            current_projected_revision,
            None,
        )?;

        let load_start = std::time::Instant::now();
        let symbol_contexts = load_symbol_contexts_from_database(db, symbols)?;
        let symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
        let relationship_map = match load_enriched_relationship_text(db, &symbol_ids) {
            Ok(map) => map,
            Err(e) => {
                let msg = e.to_string();
                let detail = format!("load_enriched_relationship_text: {}", msg);
                let _ = db.upsert_projection_state(
                    self.projection,
                    &self.workspace_id,
                    ProjectionStatus::Stale,
                    Some(target_revision),
                    current_projected_revision,
                    Some(&detail),
                );
                return Err(e);
            }
        };
        info!(
            "⏱️  projection.load_contexts: {:.2}s ({} symbols)",
            load_start.elapsed().as_secs_f64(),
            symbols.len()
        );

        let apply_start = std::time::Instant::now();
        let apply_result = apply_documents_with_context(
            index,
            symbols,
            file_infos,
            files_to_clean,
            &symbol_contexts,
            &relationship_map,
            false,
        );
        info!(
            "⏱️  projection.apply_documents: {:.2}s ({} symbols, {} files, {} cleaned)",
            apply_start.elapsed().as_secs_f64(),
            symbols.len(),
            file_infos.len(),
            files_to_clean.len()
        );
        if let Err(err) = apply_result {
            let detail = err.to_string();
            let _ = db.upsert_projection_state(
                self.projection,
                &self.workspace_id,
                ProjectionStatus::Stale,
                Some(target_revision),
                current_projected_revision,
                Some(&detail),
            );
            return Err(err);
        }

        let payload = serde_json::to_string(
            &julie_core::workspace::projection_stamp::TantivyCommitPayload {
                epoch: 1,
                revision: target_revision as u64,
                generation: target_revision as u64,
            },
        )?;
        index.commit_with_payload(&payload)?;

        db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Ready,
            Some(target_revision),
            Some(target_revision),
            None,
        )
    }

    pub fn project_documents_with_locks(
        &self,
        db: &Arc<Mutex<SymbolDatabase>>,
        index: &Arc<SearchIndex>,
        symbols: &[Symbol],
        file_infos: &[FileInfo],
        files_to_clean: &[String],
        target_revision: Option<i64>,
    ) -> Result<ProjectionState> {
        let Some(target_revision) = target_revision else {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            return Ok(db
                .get_projection_state(self.projection, &self.workspace_id)?
                .unwrap_or(db.upsert_projection_state(
                    self.projection,
                    &self.workspace_id,
                    ProjectionStatus::Missing,
                    None,
                    None,
                    None,
                )?));
        };

        let (current_projected_revision, symbol_contexts, relationship_map) = {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let current_projected_revision = db
                .get_projection_state(self.projection, &self.workspace_id)?
                .as_ref()
                .and_then(projection_served_revision);
            db.upsert_projection_state(
                self.projection,
                &self.workspace_id,
                ProjectionStatus::Building,
                Some(target_revision),
                current_projected_revision,
                None,
            )?;
            let load_start = std::time::Instant::now();
            let symbol_contexts = load_symbol_contexts_from_database(&db, symbols)?;
            let symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
            let relationship_map = match load_enriched_relationship_text(&db, &symbol_ids) {
                Ok(map) => map,
                Err(e) => {
                    let msg = e.to_string();
                    let detail = format!("load_enriched_relationship_text: {}", msg);
                    let _ = db.upsert_projection_state(
                        self.projection,
                        &self.workspace_id,
                        ProjectionStatus::Stale,
                        Some(target_revision),
                        current_projected_revision,
                        Some(&detail),
                    );
                    return Err(e);
                }
            };
            info!(
                "⏱️  projection.load_contexts: {:.2}s ({} symbols)",
                load_start.elapsed().as_secs_f64(),
                symbols.len()
            );
            (
                current_projected_revision,
                symbol_contexts,
                relationship_map,
            )
        };

        let apply_start = std::time::Instant::now();
        let apply_result = apply_documents_with_context(
            index,
            symbols,
            file_infos,
            files_to_clean,
            &symbol_contexts,
            &relationship_map,
            false,
        );
        info!(
            "⏱️  projection.apply_documents: {:.2}s ({} symbols, {} files, {} cleaned)",
            apply_start.elapsed().as_secs_f64(),
            symbols.len(),
            file_infos.len(),
            files_to_clean.len()
        );

        if let Err(err) = apply_result {
            let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let detail = err.to_string();
            let _ = db.upsert_projection_state(
                self.projection,
                &self.workspace_id,
                ProjectionStatus::Stale,
                Some(target_revision),
                current_projected_revision,
                Some(&detail),
            );
            return Err(err);
        }

        let payload = serde_json::to_string(
            &julie_core::workspace::projection_stamp::TantivyCommitPayload {
                epoch: 1,
                revision: target_revision as u64,
                generation: target_revision as u64,
            },
        )?;
        index.commit_with_payload(&payload)?;

        let db = db.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Ready,
            Some(target_revision),
            Some(target_revision),
            None,
        )
    }

    pub(crate) fn rebuild(
        &self,
        index: &SearchIndex,
        symbols: &[Symbol],
        file_infos: &[FileInfo],
        symbol_contexts: &HashMap<String, SymbolIndexContext>,
        relationship_map: &HashMap<String, String>,
        target_revision: Option<u64>,
    ) -> Result<()> {
        let apply_result = (|| -> Result<()> {
            index.clear_all_uncommitted()?;
            #[cfg(any(test, feature = "test-support"))]
            index.wait_after_rebuild_delete_for_test();
            apply_documents_with_context(
                index,
                symbols,
                file_infos,
                &[],
                symbol_contexts,
                relationship_map,
                false,
            )?;
            #[cfg(any(test, feature = "test-support"))]
            if index.take_rebuild_failure_for_test() {
                anyhow::bail!("injected rebuild failure");
            }
            Ok(())
        })();

        match apply_result {
            Ok(()) => {
                let payload = target_revision.map(|rev| {
                    serde_json::to_string(
                        &julie_core::workspace::projection_stamp::TantivyCommitPayload {
                            epoch: 1,
                            revision: rev,
                            generation: rev,
                        },
                    )
                    .unwrap_or_default()
                });
                index.release_writer_with_payload(payload.as_deref())?;
                Ok(())
            }
            Err(apply_error) => match index.rollback_and_release_writer() {
                Ok(()) => Err(apply_error),
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "{apply_error:#}; rebuild rollback failed: {rollback_error}"
                )),
            },
        }
    }
}
