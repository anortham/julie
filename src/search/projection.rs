use anyhow::Result;

use crate::database::{ProjectionState, ProjectionStatus, SymbolDatabase};
use crate::search::{FileDocument, SearchIndex, SymbolDocument};

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
        let canonical = db.get_latest_canonical_revision(&self.workspace_id)?;
        let current_state = db.get_projection_state(self.projection, &self.workspace_id)?;

        let Some(canonical) = canonical else {
            if index.num_docs() > 0 {
                index.clear_all()?;
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

        let expected_docs = canonical.symbol_count + canonical.file_count;
        let docs_match = expected_docs == 0 || index.num_docs() == expected_docs as u64;
        let current_projected_revision =
            current_state.as_ref().and_then(projection_served_revision);
        if let Some(state) = current_state {
            if state.status == ProjectionStatus::Ready
                && state.canonical_revision == Some(canonical.revision)
                && state.projected_revision == Some(canonical.revision)
                && docs_match
            {
                return Ok(state);
            }
        }

        db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Building,
            Some(canonical.revision),
            current_projected_revision,
            None,
        )?;

        let symbols = db.get_all_symbols()?;
        let file_contents = db.get_all_file_contents_with_language()?;
        let symbol_docs: Vec<_> = symbols.iter().map(SymbolDocument::from_symbol).collect();
        let file_docs: Vec<_> = file_contents
            .iter()
            .map(|(path, language, content)| FileDocument {
                file_path: path.clone(),
                content: content.clone(),
                language: language.clone(),
            })
            .collect();

        if let Err(err) = self.rebuild(index, &symbol_docs, &file_docs) {
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

        db.upsert_projection_state(
            self.projection,
            &self.workspace_id,
            ProjectionStatus::Ready,
            Some(canonical.revision),
            Some(canonical.revision),
            None,
        )
    }

    pub fn project_documents(
        &self,
        db: &mut SymbolDatabase,
        index: &SearchIndex,
        symbol_docs: &[SymbolDocument],
        file_docs: &[FileDocument],
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

        if let Err(err) = apply_documents(index, symbol_docs, file_docs, files_to_clean) {
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
        symbol_docs: &[SymbolDocument],
        file_docs: &[FileDocument],
    ) -> Result<()> {
        index.clear_all()?;
        apply_documents(index, symbol_docs, file_docs, &[])
    }
}

fn projection_served_revision(state: &ProjectionState) -> Option<i64> {
    state.projected_revision.or_else(|| {
        if state.status == ProjectionStatus::Ready {
            state.canonical_revision
        } else {
            None
        }
    })
}

pub fn apply_documents(
    index: &SearchIndex,
    symbol_docs: &[SymbolDocument],
    file_docs: &[FileDocument],
    files_to_clean: &[String],
) -> Result<()> {
    for file_path in files_to_clean {
        index.remove_by_file_path(file_path)?;
    }

    for doc in symbol_docs {
        index.add_symbol(doc)?;
    }

    for doc in file_docs {
        index.add_file_content(doc)?;
    }

    index.commit()?;
    Ok(())
}
