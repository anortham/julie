//! src/workspace_runtime/recovery.rs
//! Projection recovery coordinator; runs under the workspace mutation gate.

use std::sync::Arc;
use tracing::info;

use julie_core::database::SymbolDatabase;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_core::workspace::projection_stamp::needs_projection_recovery;
use julie_index::search::SearchIndex;
use julie_index::search::projection::SearchProjection;

pub struct ProjectionRecoveryCoordinator {
    workspace_id: String,
}

impl ProjectionRecoveryCoordinator {
    pub fn new(workspace_id: String) -> Self {
        Self { workspace_id }
    }

    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }

    /// Reconciles projection lag from canonical SQLite state under the mutation gate.
    pub async fn reconcile_if_needed(
        &self,
        db_arc: &Arc<std::sync::Mutex<SymbolDatabase>>,
        search_index: &Arc<SearchIndex>,
        _guard: &MutationGuard<'_>,
    ) -> Result<bool, anyhow::Error> {
        let (canonical_rev, projected_rev) = {
            let db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
            let canonical = db
                .get_latest_canonical_revision(&self.workspace_id)?
                .map(|r| r.revision as u64);
            let proj = db
                .get_projection_state("tantivy", &self.workspace_id)?
                .and_then(|p| p.projected_revision.map(|r| r as u64));
            (canonical, proj)
        };

        let Some(canonical) = canonical_rev else {
            return Ok(false);
        };

        if !needs_projection_recovery(canonical, projected_rev) {
            return Ok(false);
        }

        info!(
            workspace_id = %self.workspace_id,
            canonical,
            projected = ?projected_rev,
            "Reconciling projection gap from canonical SQLite truth"
        );

        let workspace_id = self.workspace_id.clone();
        let db_clone = Arc::clone(db_arc);
        let index_clone = Arc::clone(search_index);

        tokio::task::spawn_blocking(move || {
            let mut db = db_clone.lock().unwrap_or_else(|p| p.into_inner());
            SearchProjection::tantivy(&workspace_id)
                .ensure_current_from_database(&mut db, &index_clone)
        })
        .await??;

        Ok(true)
    }
}
