//! Cross-file relationship resolution
//!
//! Resolves `PendingRelationship`s (callee name only) into full `Relationship`s
//! (with target symbol ID) by looking up symbols in the database.
//!
//! ## Disambiguation Strategy
//! When multiple symbols share the same name, candidates are ranked by:
//! 1. **Kind filter** — must be a callable/referenceable symbol (not Import/Export)
//! 2. **Same language** — strongly preferred (cross-language calls within a project are rare)
//! 3. **Path proximity** — prefer symbols closer to the caller's directory

use julie_extractors::base::{PendingRelationship, Relationship, RelationshipKind, Symbol, SymbolKind};
use julie_extractors::language::detect_language_from_extension;
use tracing::info;

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
fn score_candidate(candidate: &Symbol, pending: &PendingRelationship) -> u32 {
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

    score
}

/// Select the best candidate from a list of symbols matching a callee name.
/// Returns None if no valid candidate exists.
pub fn select_best_candidate<'a>(
    candidates: &'a [Symbol],
    pending: &PendingRelationship,
) -> Option<&'a Symbol> {
    candidates
        .iter()
        .filter_map(|c| {
            let s = score_candidate(c, pending);
            if s > 0 { Some((c, s)) } else { None }
        })
        .max_by_key(|(_, score)| *score)
        .map(|(symbol, _)| symbol)
}

/// Build a resolved `Relationship` from a pending relationship and its resolved target.
pub fn build_resolved_relationship(
    pending: &PendingRelationship,
    target: &Symbol,
) -> Relationship {
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
            self.resolved, self.total, pct, self.no_candidates, self.no_valid_candidates, self.lookup_errors
        );
    }
}
