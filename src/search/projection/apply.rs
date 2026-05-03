use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::database::SymbolDatabase;
use crate::extractors::{AnnotationMarker, Symbol};
use crate::search::{FileDocument, SearchIndex, SymbolDocument};

#[derive(Debug, Clone, Default)]
pub(crate) struct SymbolIndexContext {
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

pub(crate) fn apply_uncommitted_documents_from_symbols(
    index: &SearchIndex,
    symbols: &[Symbol],
    file_docs: &[FileDocument],
    files_to_clean: &[String],
) -> Result<()> {
    let symbol_docs = symbols
        .iter()
        .map(SymbolDocument::from_symbol)
        .collect::<Vec<_>>();
    let symbol_contexts = symbol_contexts_from_symbols(symbols);
    apply_documents_with_context(
        index,
        &symbol_docs,
        file_docs,
        files_to_clean,
        &symbol_contexts,
        false,
    )
}

pub(crate) fn apply_documents_with_context(
    index: &SearchIndex,
    symbol_docs: &[SymbolDocument],
    file_docs: &[FileDocument],
    files_to_clean: &[String],
    symbol_contexts: &HashMap<String, SymbolIndexContext>,
    commit: bool,
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

    if commit {
        index.commit()?;
    }
    Ok(())
}

pub(crate) fn symbol_contexts_from_symbols(
    symbols: &[Symbol],
) -> HashMap<String, SymbolIndexContext> {
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

pub(crate) fn load_symbol_contexts_from_database(
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
        true,
    )
}
