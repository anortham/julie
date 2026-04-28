//! Cross-file relationship resolution
//!
//! Resolves `PendingRelationship`s (callee name only) into full `Relationship`s
//! (with target symbol ID) by looking up symbols in the database.
//!
//! ## Disambiguation Strategy
//! When multiple symbols share the same name, candidates are ranked by:
//! 1. **Kind filter** — must be a callable/referenceable symbol (not Import/Export)
//! 2. **Parent type reference** (+200) — caller file's identifiers reference the candidate's parent type
//! 3. **Same language** (+100) — strongly preferred (cross-language calls within a project are rare)
//! 4. **Path proximity** (+50/+25) — prefer symbols closer to the caller's directory
//! 5. **Kind match** (+10) — prefer callable kinds for Calls, type kinds for Instantiates
//! 6. **Test-file penalty** (−75) — candidates in test paths are penalized to prevent
//!    test subclasses from stealing centrality from production symbols

mod namespace;

use crate::database::SymbolDatabase;
use julie_extractors::base::{
    PendingRelationship, Relationship, RelationshipKind, StructuredPendingRelationship, Symbol,
    SymbolKind, UnresolvedTarget,
};
use julie_extractors::language::detect_language_from_extension;
use std::collections::{HashMap, HashSet};
use tracing::{info, trace, warn};

/// Pre-computed parent-type references used by candidate scoring.
pub struct ParentReferenceContext {
    parent_names: HashMap<String, String>,
    file_refs: HashSet<(String, String)>,
    scope_refs: HashSet<(String, String)>,
    files_with_identifiers: HashSet<String>,
}

impl ParentReferenceContext {
    pub fn empty() -> Self {
        Self {
            parent_names: HashMap::new(),
            file_refs: HashSet::new(),
            scope_refs: HashSet::new(),
            files_with_identifiers: HashSet::new(),
        }
    }

    pub fn new(
        parent_names: HashMap<String, String>,
        file_refs: HashSet<(String, String)>,
        files_with_identifiers: HashSet<String>,
    ) -> Self {
        Self {
            parent_names,
            file_refs,
            scope_refs: HashSet::new(),
            files_with_identifiers,
        }
    }

    fn with_scope_refs(mut self, scope_refs: HashSet<(String, String)>) -> Self {
        self.scope_refs = scope_refs;
        self
    }

    pub fn caller_references_parent(
        &self,
        caller_file: &str,
        candidate_parent_id: Option<&str>,
    ) -> bool {
        let parent_id = match candidate_parent_id {
            Some(id) => id,
            None => return false,
        };
        let parent_name = match self.parent_names.get(parent_id) {
            Some(name) => name,
            None => return false,
        };
        self.file_refs
            .contains(&(caller_file.to_string(), parent_name.clone()))
    }

    pub fn caller_scope_references_parent(
        &self,
        caller_scope_symbol_id: Option<&str>,
        candidate_parent_id: Option<&str>,
    ) -> bool {
        let (Some(scope_id), Some(parent_id)) = (caller_scope_symbol_id, candidate_parent_id)
        else {
            return false;
        };
        let Some(parent_name) = self.parent_names.get(parent_id) else {
            return false;
        };
        self.scope_refs
            .contains(&(scope_id.to_string(), parent_name.clone()))
    }

    pub fn caller_has_identifiers(&self, caller_file: &str) -> bool {
        self.files_with_identifiers.contains(caller_file)
    }
}

/// Symbols that are valid resolution targets for cross-file relationships.
/// Excludes Import, Export, Variable, Field, EnumMember — these aren't definitions you call or extend.
fn is_resolvable_target(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Constructor
            | SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Interface
            | SymbolKind::Enum
            | SymbolKind::Type
            | SymbolKind::Module
            | SymbolKind::Namespace
            | SymbolKind::Constant
            | SymbolKind::Delegate
            | SymbolKind::Event
    )
}

/// Infer language from a file path's extension.
fn language_of(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?;
    detect_language_from_extension(ext)
}

/// Extract directory portion of a path (everything before the last `/`).
fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn add_parent_mentions_from_text(
    scope_id: &str,
    text: &str,
    parent_name_set: &HashSet<&str>,
    refs: &mut HashSet<(String, String)>,
) {
    for token in text.split(|ch: char| !(ch.is_alphanumeric() || ch == '_')) {
        if parent_name_set.contains(token) {
            refs.insert((scope_id.to_string(), token.to_string()));
        }
    }
}

fn add_symbol_parent_mentions(
    symbol: &Symbol,
    parent_name_set: &HashSet<&str>,
    refs: &mut HashSet<(String, String)>,
) {
    if let Some(signature) = symbol.signature.as_deref() {
        add_parent_mentions_from_text(&symbol.id, signature, parent_name_set, refs);
    }
    if let Some(code_context) = symbol.code_context.as_deref() {
        add_parent_mentions_from_text(&symbol.id, code_context, parent_name_set, refs);
    }
}

/// Score a candidate symbol for disambiguation. Higher score wins; 0 excludes it.
fn score_candidate(
    candidate: &Symbol,
    pending: &PendingRelationship,
    target: Option<&UnresolvedTarget>,
    caller_scope_symbol_id: Option<&str>,
    parent_ctx: &ParentReferenceContext,
) -> u32 {
    if !is_resolvable_target(&candidate.kind) {
        return 0;
    }

    let mut score: u32 = 1; // Base score for being a valid target
    let Some(namespace_bonus) = namespace::score(candidate, pending, target, parent_ctx) else {
        return 0;
    };
    score += namespace_bonus;

    if let Some(caller_lang) = language_of(&pending.file_path) {
        if candidate.language == caller_lang {
            score += 100;
        }
    }

    let caller_dir = dir_of(&pending.file_path);
    let candidate_dir = dir_of(&candidate.file_path);
    if caller_dir == candidate_dir {
        score += 50; // Same directory
    } else if candidate_dir.starts_with(caller_dir) || caller_dir.starts_with(candidate_dir) {
        score += 25; // Parent/child directory
    }

    if pending.kind == RelationshipKind::Calls
        && matches!(
            candidate.kind,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
        )
    {
        score += 10;
    }

    if pending.kind == RelationshipKind::Instantiates
        && matches!(
            candidate.kind,
            SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct | SymbolKind::Type
        )
    {
        score += 10;
    }

    if target.and_then(|t| t.receiver.as_ref()).is_some()
        && parent_ctx
            .caller_scope_references_parent(caller_scope_symbol_id, candidate.parent_id.as_deref())
    {
        score += 150;
    }

    if parent_ctx.caller_references_parent(&pending.file_path, candidate.parent_id.as_deref()) {
        score += 200;
    } else if candidate.parent_id.is_some() && parent_ctx.caller_has_identifiers(&pending.file_path)
    {
        score = score.saturating_sub(75);
    }

    if crate::search::scoring::is_test_path(&candidate.file_path) {
        score = score.saturating_sub(75);
    }

    score
}

/// Select the best candidate from a list of symbols matching a callee name.
/// Returns None if no valid candidate exists.
#[cfg(test)]
pub fn select_best_candidate<'a>(
    candidates: &'a [Symbol],
    pending: &PendingRelationship,
    parent_ctx: &ParentReferenceContext,
) -> Option<&'a Symbol> {
    select_best_candidate_for_target(candidates, pending, None, None, parent_ctx)
}

fn select_best_candidate_for_target<'a>(
    candidates: &'a [Symbol],
    pending: &PendingRelationship,
    target: Option<&UnresolvedTarget>,
    caller_scope_symbol_id: Option<&str>,
    parent_ctx: &ParentReferenceContext,
) -> Option<&'a Symbol> {
    candidates
        .iter()
        .filter_map(|c| {
            let s = score_candidate(c, pending, target, caller_scope_symbol_id, parent_ctx);
            if s > 0 { Some((c, s)) } else { None }
        })
        .max_by_key(|(_, score)| *score)
        .map(|(symbol, _)| symbol)
}

/// Build a resolved `Relationship` from a pending relationship and its resolved target.
pub fn build_resolved_relationship(pending: &PendingRelationship, target: &Symbol) -> Relationship {
    Relationship {
        id: format!(
            "{}_{}_{:?}_resolved",
            pending.from_symbol_id, target.id, pending.kind
        ),
        from_symbol_id: pending.from_symbol_id.clone(),
        to_symbol_id: target.id.clone(),
        kind: pending.kind.clone(),
        file_path: pending.file_path.clone(),
        line_number: pending.line_number,
        confidence: pending.confidence,
        metadata: None,
    }
}

/// Statistics from a resolution pass.
#[derive(Debug, Default)]
pub struct ResolutionStats {
    pub total: usize,
    pub resolved: usize,
    pub no_candidates: usize,
    pub no_valid_candidates: usize,
    pub lookup_errors: usize,
}

impl ResolutionStats {
    pub fn log_summary(&self) {
        if self.total == 0 {
            return;
        }
        let pct = (self.resolved as f64 / self.total as f64 * 100.0) as u32;
        info!(
            "Resolution: {}/{} resolved ({}%), {} no candidates, {} no valid candidates, {} errors",
            self.resolved,
            self.total,
            pct,
            self.no_candidates,
            self.no_valid_candidates,
            self.lookup_errors
        );
    }
}

/// Resolve pending relationships in batch.
pub fn resolve_batch(
    pendings: &[PendingRelationship],
    db: &SymbolDatabase,
) -> (Vec<Relationship>, ResolutionStats) {
    let structured: Vec<StructuredPendingRelationship> = pendings
        .iter()
        .map(|pending| StructuredPendingRelationship {
            pending: pending.clone(),
            target: UnresolvedTarget::simple(pending.callee_name.clone()),
            caller_scope_symbol_id: None,
        })
        .collect();
    resolve_structured_batch(&structured, db)
}

/// Resolve structured pending relationships in batch. Uses target terminal names
/// for lookup while preserving namespace context for candidate selection.
pub fn resolve_structured_batch(
    pendings: &[StructuredPendingRelationship],
    db: &SymbolDatabase,
) -> (Vec<Relationship>, ResolutionStats) {
    let mut stats = ResolutionStats {
        total: pendings.len(),
        ..Default::default()
    };

    if pendings.is_empty() {
        return (Vec::new(), stats);
    }

    let unique_names: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        pendings
            .iter()
            .filter(|p| seen.insert(p.target.terminal_name.as_str()))
            .map(|p| p.target.terminal_name.clone())
            .collect()
    };

    info!(
        "🔗 Batch resolving {} pending relationships ({} unique callee names)",
        pendings.len(),
        unique_names.len()
    );

    let candidates_map = match db.find_symbols_by_names_batch(&unique_names) {
        Ok(map) => map,
        Err(e) => {
            warn!("Batch symbol lookup failed: {}", e);
            stats.lookup_errors = pendings.len();
            return (Vec::new(), stats);
        }
    };

    let legacy_pendings: Vec<PendingRelationship> = pendings
        .iter()
        .map(|pending| pending.pending.clone())
        .collect();
    let caller_scope_ids: Vec<&str> = pendings
        .iter()
        .filter_map(|pending| pending.caller_scope_symbol_id.as_deref())
        .collect();
    let parent_ctx =
        build_parent_reference_context(&candidates_map, &legacy_pendings, &caller_scope_ids, db);

    let mut resolved = Vec::with_capacity(pendings.len());
    for structured in pendings {
        match candidates_map.get(&structured.target.terminal_name) {
            Some(candidates) if !candidates.is_empty() => {
                if let Some(target) = select_best_candidate_for_target(
                    candidates,
                    &structured.pending,
                    Some(&structured.target),
                    structured.caller_scope_symbol_id.as_deref(),
                    &parent_ctx,
                ) {
                    resolved.push(build_resolved_relationship(&structured.pending, target));
                    stats.resolved += 1;
                } else {
                    stats.no_valid_candidates += 1;
                    trace!(
                        "Could not resolve '{}' - no valid target among {} candidates",
                        structured.target.display_name,
                        candidates.len()
                    );
                }
            }
            _ => {
                stats.no_candidates += 1;
            }
        }
    }

    (resolved, stats)
}

/// Build parent reference context for import-constrained disambiguation.
fn build_parent_reference_context(
    candidates_map: &HashMap<String, Vec<Symbol>>,
    pendings: &[PendingRelationship],
    caller_scope_ids: &[&str],
    db: &SymbolDatabase,
) -> ParentReferenceContext {
    let parent_ids: Vec<String> = {
        let mut seen = HashSet::new();
        candidates_map
            .values()
            .flat_map(|syms| syms.iter())
            .filter_map(|s| s.parent_id.as_ref())
            .filter(|id| seen.insert(id.as_str()))
            .cloned()
            .collect()
    };

    if parent_ids.is_empty() {
        return ParentReferenceContext::empty();
    }

    let parent_symbols = match db.get_symbols_by_ids(&parent_ids) {
        Ok(syms) => syms,
        Err(e) => {
            warn!("Failed to resolve parent symbols for disambiguation: {}", e);
            return ParentReferenceContext::empty();
        }
    };

    let parent_names: HashMap<String, String> = parent_symbols
        .into_iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect();

    if parent_names.is_empty() {
        return ParentReferenceContext::empty();
    }

    let caller_files: Vec<&str> = {
        let mut seen = HashSet::new();
        pendings
            .iter()
            .filter(|p| seen.insert(p.file_path.as_str()))
            .map(|p| p.file_path.as_str())
            .collect()
    };

    let parent_name_queries: Vec<String> = {
        let mut seen = HashSet::new();
        parent_names
            .values()
            .filter(|name| seen.insert(name.as_str()))
            .cloned()
            .collect()
    };
    let unique_parent_names: Vec<&str> = parent_name_queries.iter().map(String::as_str).collect();

    let file_refs = match db.get_identifier_presence(&caller_files, &unique_parent_names) {
        Ok(refs) => refs,
        Err(e) => {
            warn!(
                "Failed to query identifier presence for disambiguation: {}",
                e
            );
            HashSet::new()
        }
    };

    let files_with_identifiers = match db.has_identifiers_for_files(&caller_files) {
        Ok(files) => files,
        Err(e) => {
            warn!(
                "Failed to query identifier existence for disambiguation: {}",
                e
            );
            HashSet::new()
        }
    };

    let parent_name_set: HashSet<&str> = parent_name_queries.iter().map(String::as_str).collect();
    let mut scope_refs = if caller_scope_ids.is_empty() {
        HashSet::new()
    } else {
        let scope_ids: HashSet<&str> = caller_scope_ids.iter().copied().collect();
        match db.get_identifiers_by_names(&parent_name_queries) {
            Ok(refs) => refs
                .into_iter()
                .filter_map(|identifier| {
                    let scope_id = identifier.containing_symbol_id?;
                    (scope_ids.contains(scope_id.as_str())
                        && parent_name_set.contains(identifier.name.as_str()))
                    .then_some((scope_id, identifier.name))
                })
                .collect(),
            Err(e) => {
                warn!(
                    "Failed to query scoped identifier presence for disambiguation: {}",
                    e
                );
                HashSet::new()
            }
        }
    };

    if !caller_scope_ids.is_empty() {
        let scope_ids: Vec<String> = {
            let mut seen = HashSet::new();
            caller_scope_ids
                .iter()
                .filter(|id| seen.insert(**id))
                .map(|id| (*id).to_string())
                .collect()
        };
        match db.get_symbols_by_ids(&scope_ids) {
            Ok(symbols) => {
                for symbol in symbols {
                    add_symbol_parent_mentions(&symbol, &parent_name_set, &mut scope_refs);
                }
            }
            Err(e) => {
                warn!("Failed to query caller symbols for disambiguation: {}", e);
            }
        }
    }

    ParentReferenceContext::new(parent_names, file_refs, files_with_identifiers)
        .with_scope_refs(scope_refs)
}
