use anyhow::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::search::SearchIndex;
use julie_core::database::{FileInfo, ProjectionState, ProjectionStatus, SymbolDatabase};
use julie_extractors::Symbol;

mod apply;
mod facts_text;

pub use apply::apply_documents;
#[cfg(any(test, feature = "test-support"))]
pub use apply::apply_documents_with_db;
pub use apply::apply_uncommitted_documents_from_symbols;
pub use apply::collect_relationship_names_bounded;
pub use apply::collect_relationship_partner_symbol_ids;
pub use apply::reproject_partner_symbols;
use apply::{
    SymbolIndexContext, apply_documents_with_context, load_enriched_relationship_text,
    load_symbol_contexts_from_database, symbol_contexts_from_symbols,
};
#[cfg(any(test, feature = "test-support"))]
pub use facts_text::collect_structural_facts_text_bounded;

pub const TANTIVY_PROJECTION_NAME: &str = "tantivy";

pub struct SearchProjection {
    workspace_id: String,
    projection: &'static str,
}

impl SearchProjection {
    pub fn tantivy(workspace_id: impl Into<String>) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            projection: TANTIVY_PROJECTION_NAME,
        }
    }

    pub fn ensure_current_from_database(
        &self,
        db: &mut SymbolDatabase,
        index: &SearchIndex,
    ) -> Result<ProjectionState> {
        self.ensure_current_inner(db, index, None)
    }

    /// Same as `ensure_current_from_database`, but gates `search_ready` so that
    /// consumers don't read an empty index during the `clear_all` → `apply_documents`
    /// window that a rebuild opens. The flag is flipped to FALSE only if real work
    /// happens; Ready-and-matching short-circuits leave it untouched.
    pub fn ensure_current_with_gate(
        &self,
        db: &mut SymbolDatabase,
        index: &SearchIndex,
        search_ready: &AtomicBool,
    ) -> Result<ProjectionState> {
        self.ensure_current_inner(db, index, Some(search_ready))
    }

    /// Rebuild Tantivy from canonical SQLite data when an index open operation
    /// reports that the on-disk index had to be recreated due to incompatibility.
    pub fn repair_recreated_open_if_needed(
        &self,
        db: &mut SymbolDatabase,
        index: &SearchIndex,
        repair_required: bool,
        search_ready: Option<&AtomicBool>,
    ) -> Result<()> {
        if !repair_required {
            return Ok(());
        }

        match search_ready {
            Some(gate) => {
                self.ensure_current_with_gate(db, index, gate)?;
            }
            None => {
                self.ensure_current_from_database(db, index)?;
            }
        }

        Ok(())
    }

    fn ensure_current_inner(
        &self,
        db: &mut SymbolDatabase,
        index: &SearchIndex,
        search_ready: Option<&AtomicBool>,
    ) -> Result<ProjectionState> {
        let canonical = db.ensure_canonical_revision(&self.workspace_id)?;
        let current_state = db.get_projection_state(self.projection, &self.workspace_id)?;

        let Some(canonical) = canonical else {
            if index.num_docs() > 0 {
                if let Some(gate) = search_ready {
                    gate.store(false, Ordering::Release);
                }
                index.clear_all()?;
            } else if let Some(gate) = search_ready {
                gate.store(false, Ordering::Release);
            }
            return db.upsert_projection_state(
                self.projection,
                &self.workspace_id,
                ProjectionStatus::Missing,
                None,
                None,
                None,
            );
        };

        let expected_docs = db.count_projection_source_docs()?;
        let docs_match = index.num_docs() == expected_docs as u64;
        let current_projected_revision =
            current_state.as_ref().and_then(projection_served_revision);

        if let Some(state) = current_state {
            if state.status == ProjectionStatus::Ready
                && state.canonical_revision == Some(canonical.revision)
                && state.projected_revision == Some(canonical.revision)
                && docs_match
            {
                if let Some(gate) = search_ready {
                    gate.store(true, Ordering::Release);
                }
                return Ok(state);
            }
        }

        // We're about to open the empty-index window. Gate reads first.
        if let Some(gate) = search_ready {
            gate.store(false, Ordering::Release);
        }

        db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Building,
            Some(canonical.revision),
            current_projected_revision,
            None,
        )?;

        let phase_start = std::time::Instant::now();
        let symbols = db.get_all_symbols()?;
        info!(
            "projection.ensure_current.load_symbols: {:.2}s ({} symbols)",
            phase_start.elapsed().as_secs_f64(),
            symbols.len()
        );

        let phase_start = std::time::Instant::now();
        let raw_file_tuples = db.get_all_files_for_search_projection()?;
        let file_infos: Vec<FileInfo> = raw_file_tuples
            .into_iter()
            .map(|(path, language, content)| FileInfo {
                path,
                language,
                content: Some(content),
                hash: String::new(),
                size: 0,
                last_modified: 0,
                last_indexed: 0,
                symbol_count: 0,
                line_count: 0,
            })
            .collect();
        info!(
            "projection.ensure_current.load_files: {:.2}s ({} files)",
            phase_start.elapsed().as_secs_f64(),
            file_infos.len()
        );

        let phase_start = std::time::Instant::now();
        let symbol_contexts = symbol_contexts_from_symbols(&symbols);
        info!(
            "projection.ensure_current.build_symbol_contexts: {:.2}s ({} symbols)",
            phase_start.elapsed().as_secs_f64(),
            symbols.len()
        );

        let phase_start = std::time::Instant::now();
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
                    Some(canonical.revision),
                    current_projected_revision,
                    Some(&detail),
                );
                return Err(e);
            }
        };
        info!(
            "projection.ensure_current.collect_relationship_text: {:.2}s ({} focal symbols, {} relationship_text rows)",
            phase_start.elapsed().as_secs_f64(),
            symbol_ids.len(),
            relationship_map.len()
        );

        let phase_start = std::time::Instant::now();
        if let Err(err) = self.rebuild(
            index,
            &symbols,
            &file_infos,
            &symbol_contexts,
            &relationship_map,
        ) {
            let detail = err.to_string();
            let _ = db.upsert_projection_state(
                self.projection,
                &self.workspace_id,
                ProjectionStatus::Stale,
                Some(canonical.revision),
                current_projected_revision,
                Some(&detail),
            );
            return Err(err);
        }
        info!(
            "projection.ensure_current.apply_documents_uncommitted: {:.2}s ({} symbols, {} files)",
            phase_start.elapsed().as_secs_f64(),
            symbols.len(),
            file_infos.len()
        );

        let ready_state = db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Ready,
            Some(canonical.revision),
            Some(canonical.revision),
            None,
        )?;
        if let Some(gate) = search_ready {
            gate.store(true, Ordering::Release);
        }
        Ok(ready_state)
    }

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
        index.release_writer()?;

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
        index.release_writer()?;

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

    fn rebuild(
        &self,
        index: &SearchIndex,
        symbols: &[Symbol],
        file_infos: &[FileInfo],
        symbol_contexts: &HashMap<String, SymbolIndexContext>,
        relationship_map: &HashMap<String, String>,
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
                index.release_writer()?;
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

pub(crate) fn projection_served_revision(state: &ProjectionState) -> Option<i64> {
    state.projected_revision.or_else(|| {
        if state.status == ProjectionStatus::Ready {
            state.canonical_revision
        } else {
            None
        }
    })
}

#[cfg(test)]
mod merge_tests {
    include!("../tests/search/structural_facts_merge_tests.rs");
}
