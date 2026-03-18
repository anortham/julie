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

use crate::database::SymbolDatabase;
use julie_extractors::base::{
    PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind,
};
use julie_extractors::language::detect_language_from_extension;
use std::collections::{HashMap, HashSet};
use tracing::{info, trace, warn};

/// Pre-computed context about which caller files reference which parent type names.
/// Keeps `score_candidate` pure (no DB access during scoring).
pub struct ParentReferenceContext {
    /// Map from candidate parent_id → parent symbol name
    parent_names: HashMap<String, String>,
    /// Set of (caller_file_path, parent_name) pairs with confirmed identifier references
    file_refs: HashSet<(String, String)>,
    /// Files known to have at least one identifier in the DB.
    /// Used to distinguish "no match" from "no data" for negative filtering.
    files_with_identifiers: HashSet<String>,
}

impl ParentReferenceContext {
    /// Create an empty context (no disambiguation data available).
    pub fn empty() -> Self {
        Self {
            parent_names: HashMap::new(),
            file_refs: HashSet::new(),
            files_with_identifiers: HashSet::new(),
        }
    }

    /// Create a context with pre-computed data.
    pub fn new(
        parent_names: HashMap<String, String>,
        file_refs: HashSet<(String, String)>,
        files_with_identifiers: HashSet<String>,
    ) -> Self {
        Self {
            parent_names,
            file_refs,
            files_with_identifiers,
        }
    }

    /// Check if a caller file references a candidate's parent type.
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

    /// Check if we have identifier data for this file.
    /// Returns false when we have no data (can't make negative judgments).
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

/// Score a candidate symbol for disambiguation against a pending relationship.
/// Higher score = better match. Returns 0 if the candidate should be excluded entirely.
fn score_candidate(
    candidate: &Symbol,
    pending: &PendingRelationship,
    parent_ctx: &ParentReferenceContext,
) -> u32 {
    if !is_resolvable_target(&candidate.kind) {
        return 0;
    }

    let mut score: u32 = 1; // Base score for being a valid target

    // Strong preference for same language (cross-language calls are rare in a single project)
    if let Some(caller_lang) = language_of(&pending.file_path) {
        if candidate.language == caller_lang {
            score += 100;
        }
    }

    // Prefer symbols in the same directory tree (closer = more likely the right target)
    let caller_dir = dir_of(&pending.file_path);
    let candidate_dir = dir_of(&candidate.file_path);
    if caller_dir == candidate_dir {
        score += 50; // Same directory
    } else if candidate_dir.starts_with(caller_dir) || caller_dir.starts_with(candidate_dir) {
        score += 25; // Parent/child directory
    }

    // Prefer callable kinds for Calls relationships
    if pending.kind == RelationshipKind::Calls
        && matches!(
            candidate.kind,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor
        )
    {
        score += 10;
    }

    // Prefer type kinds for Instantiates relationships (DI registrations target types, not constructors)
    if pending.kind == RelationshipKind::Instantiates
        && matches!(
            candidate.kind,
            SymbolKind::Class | SymbolKind::Interface | SymbolKind::Struct | SymbolKind::Type
        )
    {
        score += 10;
    }

    // Import-constrained disambiguation: if the caller file references the candidate's
    // parent type (via identifiers), this is the strongest signal for correct resolution.
    // Dominates all other heuristics combined (max existing: 160).
    if parent_ctx.caller_references_parent(&pending.file_path, candidate.parent_id.as_deref()) {
        score += 200;
    } else if candidate.parent_id.is_some()
        && parent_ctx.caller_has_identifiers(&pending.file_path)
    {
        // Negative filtering: candidate has a parent type, we have identifier data for
        // the caller file, but the caller doesn't reference this parent. This is likely
        // a phantom edge — the real target is an external/framework type not in the index.
        return 0;
    }

    // Penalize candidates in test files. Test subclasses (e.g., `class Flask(flask.Flask)`
    // in tests/test_config.py) can otherwise win disambiguation via path proximity when
    // the caller is also in a test directory, causing centrality to accumulate on the
    // wrong symbol. The -75 penalty is strong enough to override proximity (+50) but
    // not parent-reference context (+200).
    if crate::search::scoring::is_test_path(&candidate.file_path) {
        score = score.saturating_sub(75);
    }

    score
}

/// Select the best candidate from a list of symbols matching a callee name.
/// Returns None if no valid candidate exists.
pub fn select_best_candidate<'a>(
    candidates: &'a [Symbol],
    pending: &PendingRelationship,
    parent_ctx: &ParentReferenceContext,
) -> Option<&'a Symbol> {
    candidates
        .iter()
        .filter_map(|c| {
            let s = score_candidate(c, pending, parent_ctx);
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

/// Resolve pending relationships in batch: group by callee_name, query once per
/// unique name, then disambiguate per pending relationship.
///
/// This is O(unique_callee_names) DB queries instead of O(total_pendings) —
/// a massive win when many relationships share the same callee name.
pub fn resolve_batch(
    pendings: &[PendingRelationship],
    db: &SymbolDatabase,
) -> (Vec<Relationship>, ResolutionStats) {
    let mut stats = ResolutionStats {
        total: pendings.len(),
        ..Default::default()
    };

    if pendings.is_empty() {
        return (Vec::new(), stats);
    }

    // Collect unique callee names
    let unique_names: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        pendings
            .iter()
            .filter(|p| seen.insert(p.callee_name.as_str()))
            .map(|p| p.callee_name.clone())
            .collect()
    };

    info!(
        "🔗 Batch resolving {} pending relationships ({} unique callee names)",
        pendings.len(),
        unique_names.len()
    );

    // Single batch query for all unique names
    let candidates_map = match db.find_symbols_by_names_batch(&unique_names) {
        Ok(map) => map,
        Err(e) => {
            warn!("Batch symbol lookup failed: {}", e);
            stats.lookup_errors = pendings.len();
            return (Vec::new(), stats);
        }
    };

    // Build parent reference context for import-constrained disambiguation
    let parent_ctx = build_parent_reference_context(&candidates_map, pendings, db);

    // Resolve each pending against the cached candidates
    let mut resolved = Vec::with_capacity(pendings.len());
    for pending in pendings {
        match candidates_map.get(&pending.callee_name) {
            Some(candidates) if !candidates.is_empty() => {
                if let Some(target) = select_best_candidate(candidates, pending, &parent_ctx) {
                    resolved.push(build_resolved_relationship(pending, target));
                    stats.resolved += 1;
                } else {
                    stats.no_valid_candidates += 1;
                    trace!(
                        "Could not resolve '{}' - no valid target among {} candidates",
                        pending.callee_name,
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
///
/// Checks which caller files reference which candidate parent types (via identifiers),
/// enabling the resolver to prefer candidates whose parent type the caller actually uses.
fn build_parent_reference_context(
    candidates_map: &HashMap<String, Vec<Symbol>>,
    pendings: &[PendingRelationship],
    db: &SymbolDatabase,
) -> ParentReferenceContext {
    // 1. Collect unique parent_ids from all candidates
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

    // 2. Resolve parent_ids to names
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

    // 3. Collect unique caller file_paths and unique parent names
    let caller_files: Vec<&str> = {
        let mut seen = HashSet::new();
        pendings
            .iter()
            .filter(|p| seen.insert(p.file_path.as_str()))
            .map(|p| p.file_path.as_str())
            .collect()
    };

    let unique_parent_names: Vec<&str> = {
        let set: HashSet<&str> = parent_names.values().map(|n| n.as_str()).collect();
        set.into_iter().collect()
    };

    // 4. Query identifiers: which (file, parent_name) pairs exist?
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

    // 5. Query which caller files have any identifiers at all.
    // Needed to distinguish "no match" (reject phantom edges) from "no data" (allow fallback).
    let files_with_identifiers =
        match db.has_identifiers_for_files(&caller_files) {
            Ok(files) => files,
            Err(e) => {
                warn!(
                    "Failed to query identifier existence for disambiguation: {}",
                    e
                );
                HashSet::new()
            }
        };

    ParentReferenceContext::new(parent_names, file_refs, files_with_identifiers)
}
