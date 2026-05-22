use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::database::{FileInfo, SymbolDatabase};
use crate::extractors::{AnnotationMarker, Symbol};
use crate::search::index::{SearchDocument, truncate_utf8_bytes};
use crate::search::tokenizer::pretokenize_code;
use crate::search::SearchIndex;
use crate::search::scoring::{classify_role, test_subrole};

/// Maximum byte length for `relationship_text` per symbol.
pub(super) const RELATIONSHIP_TEXT_MAX_BYTES: usize = 512;

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

/// Collect related-symbol names for a batch of symbol IDs.
///
/// For each symbol in `symbol_ids`, fetches all edges where
/// `from_symbol_id IN ids OR to_symbol_id IN ids` (one query each direction),
/// resolves the partner symbol's name, deduplicates, joins with spaces, and
/// truncates at `max_bytes_per` on the last whitespace boundary.
///
/// Returns a `HashMap<symbol_id, relationship_text_blob>`.
/// Symbols with no relationships are omitted from the map (callers treat
/// missing keys as empty string).
pub(crate) fn collect_relationship_names_bounded(
    db: &SymbolDatabase,
    symbol_ids: &[String],
    max_bytes_per: usize,
) -> Result<HashMap<String, String>> {
    if symbol_ids.is_empty() {
        return Ok(HashMap::new());
    }

    // --- ONE batch call each direction ----------------------------------
    let outgoing = db.get_outgoing_relationships_for_symbols(symbol_ids)?;
    let incoming = db.get_relationships_to_symbols(symbol_ids)?;

    // Collect all partner IDs we need to resolve to names.
    // For a given focal symbol_id, the partner is the OTHER end of the edge.
    // outgoing: from_symbol_id == focal → partner is to_symbol_id
    // incoming: to_symbol_id == focal → partner is from_symbol_id
    let focal_set: HashSet<&str> = symbol_ids.iter().map(String::as_str).collect();

    // Build a map: focal_id → set of partner_ids (deduped)
    let mut focal_to_partners: HashMap<String, HashSet<String>> = HashMap::new();

    for rel in &outgoing {
        if focal_set.contains(rel.from_symbol_id.as_str()) {
            focal_to_partners
                .entry(rel.from_symbol_id.clone())
                .or_default()
                .insert(rel.to_symbol_id.clone());
        }
    }
    for rel in &incoming {
        if focal_set.contains(rel.to_symbol_id.as_str()) {
            focal_to_partners
                .entry(rel.to_symbol_id.clone())
                .or_default()
                .insert(rel.from_symbol_id.clone());
        }
    }

    if focal_to_partners.is_empty() {
        return Ok(HashMap::new());
    }

    // Gather all unique partner IDs for a single name-lookup batch.
    let all_partner_ids: Vec<String> = focal_to_partners
        .values()
        .flat_map(|s| s.iter().cloned())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    let partner_symbols = db.get_symbols_by_ids(&all_partner_ids)?;
    let id_to_name: HashMap<&str, &str> = partner_symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    // Build the relationship_text blobs, one per focal symbol.
    let mut result = HashMap::with_capacity(focal_to_partners.len());
    for (focal_id, partner_ids) in focal_to_partners {
        let mut names: Vec<&str> = partner_ids
            .iter()
            .filter_map(|pid| id_to_name.get(pid.as_str()).copied())
            .collect();
        if names.is_empty() {
            continue;
        }
        names.sort_unstable();
        let joined = names.join(" ");
        let blob = truncate_to_whitespace_boundary(&joined, max_bytes_per).to_string();
        if !blob.is_empty() {
            result.insert(focal_id, blob);
        }
    }

    Ok(result)
}

/// Truncate `s` to at most `max_bytes` bytes on a whitespace boundary.
///
/// If `s` fits within `max_bytes`, returns `s` unchanged. Otherwise truncates
/// to `max_bytes` bytes (respecting UTF-8 char boundaries via
/// [`truncate_utf8_bytes`]) then backtracks to the last whitespace so partial
/// identifiers are never left in the index.
fn truncate_to_whitespace_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let truncated = truncate_utf8_bytes(s, max_bytes);
    if let Some(idx) = truncated.rfind(char::is_whitespace) {
        &truncated[..idx]
    } else {
        truncated
    }
}

pub(crate) fn apply_uncommitted_documents_from_symbols(
    index: &SearchIndex,
    symbols: &[Symbol],
    file_path: &str,
    file_content: &str,
    file_language: &str,
    files_to_clean: &[String],
    db: &SymbolDatabase,
) -> Result<()> {
    let symbol_contexts = symbol_contexts_from_symbols(symbols);
    let symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
    let relationship_map =
        collect_relationship_names_bounded(db, &symbol_ids, RELATIONSHIP_TEXT_MAX_BYTES)
            .unwrap_or_default();

    for file_path_to_clean in files_to_clean {
        index.remove_by_file_path(file_path_to_clean)?;
    }

    for symbol in symbols {
        let context = symbol_contexts.get(&symbol.id).cloned().unwrap_or_default();
        let rel_text = relationship_map.get(&symbol.id).cloned().unwrap_or_default();
        let search_doc = symbol_to_search_document(symbol, &context, rel_text);
        index.add_search_doc(&search_doc)?;
    }

    // Index the file row so line-mode search can find content matches.
    let file_search_doc = raw_file_to_search_document(file_path, file_content, file_language);
    index.add_search_doc(&file_search_doc)?;

    Ok(())
}

pub(crate) fn apply_documents_with_context(
    index: &SearchIndex,
    symbols: &[Symbol],
    file_infos: &[FileInfo],
    files_to_clean: &[String],
    symbol_contexts: &HashMap<String, SymbolIndexContext>,
    relationship_map: &HashMap<String, String>,
    commit: bool,
) -> Result<()> {
    for file_path in files_to_clean {
        index.remove_by_file_path(file_path)?;
    }

    for symbol in symbols {
        let context = symbol_contexts.get(&symbol.id).cloned().unwrap_or_default();
        let rel_text = relationship_map.get(&symbol.id).cloned().unwrap_or_default();
        let search_doc = symbol_to_search_document(symbol, &context, rel_text);
        index.add_search_doc(&search_doc)?;
    }

    for file_info in file_infos {
        let search_doc = file_info_to_search_document(file_info);
        index.add_search_doc(&search_doc)?;
    }

    if commit {
        index.commit()?;
    }
    Ok(())
}

/// Apply documents with full DB-backed relationship enrichment.
/// Used by the watcher paths that don't go through `project_documents`.
///
/// This is the canonical production entry point for relationship_text: it
/// precomputes the map once per batch then delegates to
/// `apply_documents_with_context`.
pub(crate) fn apply_documents_with_db(
    index: &SearchIndex,
    symbols: &[Symbol],
    file_infos: &[FileInfo],
    files_to_clean: &[String],
    db: &SymbolDatabase,
    commit: bool,
) -> Result<()> {
    let symbol_ids: Vec<String> = symbols.iter().map(|s| s.id.clone()).collect();
    let relationship_map =
        collect_relationship_names_bounded(db, &symbol_ids, RELATIONSHIP_TEXT_MAX_BYTES)
            .unwrap_or_default();
    let symbol_contexts = symbol_contexts_from_symbols(symbols);
    apply_documents_with_context(
        index,
        symbols,
        file_infos,
        files_to_clean,
        &symbol_contexts,
        &relationship_map,
        commit,
    )
}

/// Build a `SearchDocument` (union shape) for a file row from a `FileInfo`.
/// `code_body` is empty for file rows; `pretokenized_code` is built from the
/// first ≤ 2000 bytes of content (T4).  `relationship_text` is always empty
/// for file rows.
fn file_info_to_search_document(file_info: &FileInfo) -> SearchDocument {
    let normalized_path = file_info.path.replace('\\', "/");
    let basename = normalized_path.rsplit('/').next().unwrap_or(&normalized_path).to_string();
    let name = if basename.contains('.') {
        basename[..basename.rfind('.').unwrap()].to_string()
    } else {
        basename.clone()
    };
    let content = file_info.content.as_deref().unwrap_or("");
    let language = &file_info.language;
    let role = classify_role(&normalized_path, language);
    let test_role_str = test_subrole(&normalized_path);

    // pretokenized_code: CamelCase/snake_case-split of the first ≤ 2000 bytes of content.
    let content_truncated = truncate_utf8_bytes(content, 2000);
    let pretokenized_code = pretokenize_code(content_truncated);

    SearchDocument {
        doc_type: "file".to_string(),
        id: String::new(),
        name,
        language: language.clone(),
        file_path: normalized_path.clone(),
        basename,
        kind: "file".to_string(),
        role: role.to_string(),
        test_role: test_role_str.to_string(),
        signature: String::new(),
        doc_comment: String::new(),
        code_body: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        start_line: 0,
        content: content.to_string(),
        path_text: normalized_path,
        pretokenized_code,
        relationship_text: String::new(),
    }
}

/// Build a `SearchDocument` from a `Symbol` and its indexing context.
fn symbol_to_search_document(
    symbol: &Symbol,
    context: &SymbolIndexContext,
    relationship_text: String,
) -> SearchDocument {
    let normalized_path = symbol.file_path.replace('\\', "/");
    let basename = normalized_path.rsplit('/').next().unwrap_or(&normalized_path).to_string();
    let path_role = classify_role(&normalized_path, &symbol.language);
    let path_test_role_str = test_subrole(&normalized_path);

    // Inline test helpers live in non-test files (e.g. `#[cfg(test)]` blocks
    // inside `src/lib.rs`).  Path heuristics can't detect them; check the
    // extractor's metadata override so the role and test_role fields carry
    // the correct classification for the unified reranker and the
    // `exclude_tests` filter.
    let meta = symbol.metadata.as_ref();
    let metadata_is_test = meta
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let metadata_test_role = meta
        .and_then(|m| m.get("test_role"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let (role, test_role) = if metadata_is_test && path_role != "test" {
        let tr = metadata_test_role
            .unwrap_or_else(|| path_test_role_str.to_string());
        ("test".to_string(), tr)
    } else {
        (
            path_role.to_string(),
            metadata_test_role.unwrap_or_else(|| path_test_role_str.to_string()),
        )
    };

    let raw_body = symbol.code_context.as_deref().unwrap_or("");
    let code_body = truncate_utf8_bytes(raw_body, 2000).to_string();
    let signature = symbol.signature.clone().unwrap_or_default();
    let pretok_input = format!("{} {} {}", symbol.name, signature, code_body);
    let pretokenized_code = pretokenize_code(&pretok_input);

    SearchDocument {
        doc_type: "symbol".to_string(),
        id: symbol.id.clone(),
        name: symbol.name.clone(),
        language: symbol.language.clone(),
        file_path: normalized_path,
        basename,
        kind: symbol.kind.to_string(),
        role,
        test_role,
        signature,
        doc_comment: symbol.doc_comment.clone().unwrap_or_default(),
        code_body,
        annotation_keys: context.annotation_keys.clone(),
        annotations_text: context.annotations_text.clone(),
        owner_names_text: context.owner_names_text.clone(),
        start_line: symbol.start_line,
        content: String::new(),
        path_text: String::new(),
        pretokenized_code,
        relationship_text,
    }
}

/// Build a `SearchDocument` for a file row from raw path/content/language.
fn raw_file_to_search_document(file_path: &str, content: &str, language: &str) -> SearchDocument {
    let normalized_path = file_path.replace('\\', "/");
    let basename = normalized_path.rsplit('/').next().unwrap_or(&normalized_path).to_string();
    let name = if let Some(dot) = basename.rfind('.') {
        basename[..dot].to_string()
    } else {
        basename.clone()
    };
    let role = classify_role(&normalized_path, language);
    let test_role_str = test_subrole(&normalized_path);
    let content_truncated = truncate_utf8_bytes(content, 2000);
    let pretokenized_code = pretokenize_code(content_truncated);

    SearchDocument {
        doc_type: "file".to_string(),
        id: String::new(),
        name,
        language: language.to_string(),
        file_path: normalized_path.clone(),
        basename,
        kind: "file".to_string(),
        role: role.to_string(),
        test_role: test_role_str.to_string(),
        signature: String::new(),
        doc_comment: String::new(),
        code_body: String::new(),
        annotation_keys: vec![],
        annotations_text: String::new(),
        owner_names_text: String::new(),
        start_line: 0,
        content: content.to_string(),
        path_text: normalized_path,
        pretokenized_code,
        relationship_text: String::new(),
    }
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
    symbols: &[Symbol],
) -> Result<HashMap<String, SymbolIndexContext>> {
    let target_ids = symbols
        .iter()
        .map(|s| s.id.clone())
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
    symbols: &[Symbol],
    file_infos: &[FileInfo],
    files_to_clean: &[String],
) -> Result<()> {
    apply_documents_with_context(
        index,
        symbols,
        file_infos,
        files_to_clean,
        &HashMap::new(),
        &HashMap::new(),
        true,
    )
}
