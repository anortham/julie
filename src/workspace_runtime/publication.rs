//! src/workspace_runtime/publication.rs
//! Coherent read snapshot acquisition.

use std::sync::Arc;

use tantivy::Searcher;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{IndexRecordOption, Value};
use thiserror::Error;

use crate::paths::RegistryPaths;
use crate::request_engine::types::WorkspaceBinding;
use julie_core::database::{ProjectionStatus, SymbolDatabase};
use julie_core::workspace::projection_stamp::{
    PublicationStamp, TantivyCommitPayload, needs_projection_recovery,
};
use julie_index::search::SearchIndex;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error(
        "PROJECTION_LAG: canonical revision {canonical}, projected {projected:?}, tantivy {tantivy_revision:?}"
    )]
    ProjectionLag {
        canonical: u64,
        projected: Option<u64>,
        tantivy_revision: Option<u64>,
    },
    #[error("Database error: {0}")]
    Database(#[from] anyhow::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Retained coherent read snapshot that pins SQLite and Tantivy point-in-time state.
pub struct WorkspaceReadSnapshot {
    pub stamp: PublicationStamp,
    pub searcher: Searcher,
    pub db: Arc<std::sync::Mutex<SymbolDatabase>>,
}

impl std::fmt::Debug for WorkspaceReadSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkspaceReadSnapshot")
            .field("stamp", &self.stamp)
            .finish()
    }
}

impl WorkspaceReadSnapshot {
    pub fn stamp(&self) -> &PublicationStamp {
        &self.stamp
    }

    pub fn searcher(&self) -> &Searcher {
        &self.searcher
    }

    pub fn db(&self) -> &Arc<std::sync::Mutex<SymbolDatabase>> {
        &self.db
    }

    /// Acquire a coherent read snapshot of the SQLite and Tantivy stores.
    pub async fn acquire(
        binding: &WorkspaceBinding,
        _paths: &RegistryPaths,
    ) -> Result<Self, SnapshotError> {
        let db_path = binding.index_root.join("db").join("symbols.db");
        let db = SymbolDatabase::new(&db_path).map_err(SnapshotError::Database)?;
        let db_arc = Arc::new(std::sync::Mutex::new(db));

        let (canonical_rev, projected_rev, status) = {
            let db = db_arc.lock().unwrap_or_else(|p| p.into_inner());
            let canonical = db
                .get_latest_canonical_revision(&binding.workspace_id)
                .map_err(SnapshotError::Database)?
                .map(|r| r.revision as u64)
                .unwrap_or(0);
            let proj = db
                .get_projection_state("tantivy", &binding.workspace_id)
                .map_err(SnapshotError::Database)?;
            let projected_rev = proj
                .as_ref()
                .and_then(|p| p.projected_revision.map(|r| r as u64));
            let status = proj.as_ref().map(|p| p.status);
            (canonical, projected_rev, status)
        };

        let tantivy_path = binding.index_root.join("tantivy");
        let search_index = SearchIndex::open_or_create(&tantivy_path)
            .map_err(|e| SnapshotError::Database(anyhow::anyhow!(e)))?;
        let searcher = search_index.reader().searcher();

        let metas = searcher
            .index()
            .load_metas()
            .map_err(|e| SnapshotError::Database(anyhow::anyhow!(e)))?;
        let tantivy_payload: Option<TantivyCommitPayload> = metas
            .payload
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        let tantivy_rev = tantivy_payload.as_ref().map(|p| p.revision);

        // Verification invariant:
        // Canonical matches projected, projected matches tantivy payload, status is ready
        let has_lag = needs_projection_recovery(canonical_rev, projected_rev)
            || (canonical_rev > 0 && projected_rev != tantivy_rev && tantivy_rev.is_some())
            || (canonical_rev > 0 && status != Some(ProjectionStatus::Ready));

        if has_lag {
            return Err(SnapshotError::ProjectionLag {
                canonical: canonical_rev,
                projected: projected_rev,
                tantivy_revision: tantivy_rev,
            });
        }

        let stamp = PublicationStamp {
            epoch: tantivy_payload.as_ref().map(|p| p.epoch).unwrap_or(0),
            canonical_revision: canonical_rev,
            projected_revision: projected_rev.unwrap_or(0),
            generation: tantivy_payload.as_ref().map(|p| p.generation).unwrap_or(1),
        };

        Ok(Self {
            stamp,
            searcher,
            db: db_arc,
        })
    }

    /// Search symbols in the retained searcher point-in-time view.
    pub fn search_symbols(&self, query_str: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        let schema = self.searcher.schema();
        let name_field = schema.get_field("name")?;

        let mut results = Vec::new();

        // 1. Exact term query
        let term_query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(name_field, query_str),
            IndexRecordOption::Basic,
        );
        if let Ok(top_docs) = self
            .searcher
            .search(&term_query, &TopDocs::with_limit(limit).order_by_score())
        {
            for (_score, doc_address) in top_docs {
                let doc: tantivy::TantivyDocument = self.searcher.doc(doc_address)?;
                if let Some(val) = doc.get_first(name_field).and_then(|v| v.as_str()) {
                    results.push(val.to_string());
                }
            }
        }

        if !results.is_empty() {
            return Ok(results);
        }

        // 2. Query parser fallback for prefix/fuzzy match
        let query_parser = QueryParser::for_index(self.searcher.index(), vec![name_field]);
        if let Ok(parsed) = query_parser.parse_query(query_str) {
            if let Ok(top_docs) = self
                .searcher
                .search(&parsed, &TopDocs::with_limit(limit).order_by_score())
            {
                for (_score, doc_address) in top_docs {
                    let doc: tantivy::TantivyDocument = self.searcher.doc(doc_address)?;
                    if let Some(val) = doc.get_first(name_field).and_then(|v| v.as_str()) {
                        results.push(val.to_string());
                    }
                }
            }
        }

        Ok(results)
    }
}
