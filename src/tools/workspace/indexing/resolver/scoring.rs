use super::{ParentReferenceContext, namespace};
use julie_extractors::base::{
    PendingRelationship, RelationshipKind, Symbol, SymbolKind, UnresolvedTarget,
};
use julie_extractors::language::detect_language_from_extension;

pub(super) fn is_resolvable_target(kind: &SymbolKind) -> bool {
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

pub(super) fn language_of(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?;
    detect_language_from_extension(ext)
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

pub(super) fn score_candidate(
    candidate: &Symbol,
    pending: &PendingRelationship,
    target: Option<&UnresolvedTarget>,
    caller_scope_symbol_id: Option<&str>,
    parent_ctx: &ParentReferenceContext,
) -> u32 {
    if !is_resolvable_target(&candidate.kind) {
        return 0;
    }

    let mut score: u32 = 1;
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
        score += 50;
    } else if candidate_dir.starts_with(caller_dir) || caller_dir.starts_with(candidate_dir) {
        score += 25;
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
