use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::database::{ProjectionState, ProjectionStatus, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol};
use crate::search::{FileDocument, SearchIndex, SymbolDocument};

pub const TANTIVY_PROJECTION_NAME: &str = "tantivy";

pub struct SearchProjection {
    workspace_id: String,
    projection: &'static str,
}

#[derive(Debug, Clone, Default)]
struct SymbolIndexContext {
    annotation_keys: Vec<String>,
    annotations_text: String,
    owner_names_text: String,
}

#[derive(Debug, Clone)]
struct SymbolContextSource {
    name: String,
    parent_id: Option<String>,
    annotations: Vec<AnnotationMarker>,
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
        let docs_match = expected_docs == 0 || index.num_docs() == expected_docs as u64;
        let current_projected_revision =
            current_state.as_ref().and_then(projection_served_revision);

        if current_state.is_none() && docs_match && index.num_docs() > 0 {
            let state = db.upsert_projection_state(
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
            return Ok(state);
        }

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

        let symbols = db.get_all_symbols()?;
        let file_contents = db.get_all_files_for_search_projection()?;
        let symbol_docs: Vec<_> = symbols.iter().map(SymbolDocument::from_symbol).collect();
        let symbol_contexts = symbol_contexts_from_symbols(&symbols);
        let file_docs: Vec<_> = file_contents
            .iter()
            .map(|(path, language, content)| FileDocument {
                file_path: path.clone(),
                content: content.clone(),
                language: language.clone(),
            })
            .collect();

        if let Err(err) = self.rebuild(index, &symbol_docs, &file_docs, &symbol_contexts) {
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

        let symbol_contexts = load_symbol_contexts_from_database(db, symbol_docs)?;
        if let Err(err) = apply_documents_with_context(
            index,
            symbol_docs,
            file_docs,
            files_to_clean,
            &symbol_contexts,
        ) {
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
        symbol_contexts: &HashMap<String, SymbolIndexContext>,
    ) -> Result<()> {
        index.clear_all()?;
        apply_documents_with_context(index, symbol_docs, file_docs, &[], symbol_contexts)
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
    apply_documents_with_context(
        index,
        symbol_docs,
        file_docs,
        files_to_clean,
        &HashMap::new(),
    )
}

fn apply_documents_with_context(
    index: &SearchIndex,
    symbol_docs: &[SymbolDocument],
    file_docs: &[FileDocument],
    files_to_clean: &[String],
    symbol_contexts: &HashMap<String, SymbolIndexContext>,
) -> Result<()> {
    for file_path in files_to_clean {
        index.remove_by_file_path(file_path)?;
    }

    for doc in symbol_docs {
        let context = symbol_contexts.get(&doc.id).cloned().unwrap_or_default();
        index.add_symbol_with_context(
            doc,
            &context.annotation_keys,
            &context.annotations_text,
            &context.owner_names_text,
        )?;
    }

    for doc in file_docs {
        index.add_file_content(doc)?;
    }

    index.commit()?;
    Ok(())
}

fn symbol_contexts_from_symbols(symbols: &[Symbol]) -> HashMap<String, SymbolIndexContext> {
    let sources = symbols
        .iter()
        .map(|symbol| (symbol.id.clone(), SymbolContextSource::from(symbol)))
        .collect::<HashMap<_, _>>();

    symbols
        .iter()
        .map(|symbol| {
            (
                symbol.id.clone(),
                build_symbol_index_context(&SymbolContextSource::from(symbol), &sources),
            )
        })
        .collect()
}

fn load_symbol_contexts_from_database(
    db: &SymbolDatabase,
    symbol_docs: &[SymbolDocument],
) -> Result<HashMap<String, SymbolIndexContext>> {
    let target_ids = symbol_docs
        .iter()
        .map(|doc| doc.id.clone())
        .collect::<Vec<_>>();
    if target_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut sources = HashMap::new();
    let mut requested = HashSet::new();
    let mut ids_to_load = target_ids.clone();

    while !ids_to_load.is_empty() {
        let batch = ids_to_load
            .into_iter()
            .filter(|id| !sources.contains_key(id) && requested.insert(id.clone()))
            .collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }

        let symbols = db.get_symbols_by_ids(&batch)?;
        let mut parent_ids = Vec::new();
        for symbol in symbols {
            if let Some(parent_id) = &symbol.parent_id {
                if !sources.contains_key(parent_id) && !requested.contains(parent_id) {
                    parent_ids.push(parent_id.clone());
                }
            }
            sources.insert(symbol.id.clone(), SymbolContextSource::from(&symbol));
        }
        ids_to_load = parent_ids;
    }

    Ok(target_ids
        .into_iter()
        .filter_map(|id| {
            sources
                .get(&id)
                .map(|source| (id, build_symbol_index_context(source, &sources)))
        })
        .collect())
}

fn build_symbol_index_context(
    symbol: &SymbolContextSource,
    sources: &HashMap<String, SymbolContextSource>,
) -> SymbolIndexContext {
    let (annotation_keys, annotations_text) = annotation_index_text(&symbol.annotations);
    SymbolIndexContext {
        annotation_keys,
        annotations_text,
        owner_names_text: owner_names_text(symbol, sources),
    }
}

fn annotation_index_text(annotations: &[AnnotationMarker]) -> (Vec<String>, String) {
    let mut annotation_keys = Vec::new();
    let mut seen_keys = HashSet::new();
    let mut text_parts = Vec::new();

    for marker in annotations {
        let key = marker.annotation_key.trim().to_ascii_lowercase();
        if !key.is_empty() && seen_keys.insert(key.clone()) {
            annotation_keys.push(key.clone());
        }
        push_nonempty(&mut text_parts, marker.annotation.as_str());
        push_nonempty(&mut text_parts, marker.annotation_key.as_str());
        if let Some(raw_text) = &marker.raw_text {
            push_nonempty(&mut text_parts, raw_text.as_str());
        }
    }

    (annotation_keys, text_parts.join(" "))
}

fn owner_names_text(
    symbol: &SymbolContextSource,
    sources: &HashMap<String, SymbolContextSource>,
) -> String {
    let mut owner_names = Vec::new();
    let mut seen = HashSet::new();
    let mut current_parent_id = symbol.parent_id.as_deref();

    while let Some(parent_id) = current_parent_id {
        if !seen.insert(parent_id.to_string()) {
            break;
        }
        let Some(parent) = sources.get(parent_id) else {
            break;
        };
        push_nonempty(&mut owner_names, parent.name.as_str());
        current_parent_id = parent.parent_id.as_deref();
    }

    owner_names.join(" ")
}

fn push_nonempty(parts: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        parts.push(value.to_string());
    }
}

impl From<&Symbol> for SymbolContextSource {
    fn from(symbol: &Symbol) -> Self {
        Self {
            name: symbol.name.clone(),
            parent_id: symbol.parent_id.clone(),
            annotations: symbol.annotations.clone(),
        }
    }
}
