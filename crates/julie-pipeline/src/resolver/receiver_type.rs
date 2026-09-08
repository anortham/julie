use std::collections::HashSet;

use julie_core::Symbol;
use julie_core::database::SymbolDatabase;
use julie_extractors::{RelationshipKind, SymbolKind};

fn is_container_kind(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Interface
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Type
    )
}

fn find_enclosing_owner(scope_id: &str, db: &SymbolDatabase) -> Option<Symbol> {
    let mut current_id = scope_id.to_string();
    let mut visited = HashSet::new();

    while visited.insert(current_id.clone()) {
        let sym = db.get_symbol_by_id(&current_id).ok()??;
        if is_container_kind(&sym.kind) {
            return Some(sym);
        }
        match &sym.parent_id {
            Some(pid) if !pid.is_empty() => current_id = pid.clone(),
            _ => break,
        }
    }
    None
}

fn get_direct_base_ids(owner_id: &str, db: &SymbolDatabase) -> Vec<String> {
    let mut bases = Vec::new();
    if let Ok(rels) = db.get_outgoing_relationships(owner_id) {
        for rel in rels {
            if rel.kind == RelationshipKind::Extends || rel.kind == RelationshipKind::Implements {
                bases.push(rel.to_symbol_id);
            }
        }
    }
    bases
}

fn get_ancestor_type_ids(owner_id: &str, db: &SymbolDatabase) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut queue = vec![owner_id.to_string()];
    let mut visited = HashSet::new();
    visited.insert(owner_id.to_string());

    while let Some(current) = queue.pop() {
        for base_id in get_direct_base_ids(&current, db) {
            if visited.insert(base_id.clone()) {
                ancestors.push(base_id.clone());
                queue.push(base_id);
            }
        }
    }
    ancestors
}

fn owner_matches_namespace(owner: &Symbol, ns_prefix: &[&str], db: &SymbolDatabase) -> bool {
    // 1. Check parent container hierarchy if symbol is nested in modules/namespaces
    if let Some(pid) = owner.parent_id.as_deref() {
        let mut curr_pid = Some(pid.to_string());
        let mut parent_names = Vec::new();
        while let Some(id) = curr_pid {
            if let Ok(Some(parent_sym)) = db.get_symbol_by_id(&id) {
                parent_names.push(parent_sym.name.clone());
                curr_pid = parent_sym.parent_id.clone();
            } else {
                break;
            }
        }
        if !parent_names.is_empty() {
            if parent_names.len() >= ns_prefix.len() {
                return ns_prefix
                    .iter()
                    .rev()
                    .zip(&parent_names)
                    .all(|(req, actual)| actual == *req);
            } else {
                // If symbol has parent containers but fewer than ns_prefix requires, it does not match
                return false;
            }
        }
    }

    // 2. Check Rust module path segments from file for top-level symbols
    if owner.language == "rust" {
        let module_segments = super::namespace::rust_module_path_from_file(&owner.file_path);
        let ns_strings: Vec<String> = ns_prefix.iter().map(|s| s.to_string()).collect();
        if super::namespace::path_ends_with_segments(&module_segments, &ns_strings) {
            return true;
        }
    }

    // Directory name proximity matching is rejected to prevent contradictory namespace bindings
    false
}

fn resolve_named_owner(
    receiver: &str,
    caller_file_path: &str,
    caller_language: Option<&str>,
    db: &SymbolDatabase,
) -> Option<Symbol> {
    let segments: Vec<&str> = receiver
        .split(['.', ':'])
        .filter(|part| !part.is_empty())
        .collect();
    let terminal = segments.last()?;

    let potential_owners = db
        .find_symbols_by_names_batch(&[terminal.to_string()])
        .ok()?;
    let matching_owners = potential_owners.get(*terminal)?;

    let mut filtered = Vec::new();
    for owner in matching_owners {
        if !is_container_kind(&owner.kind) {
            continue;
        }
        if let Some(lang) = caller_language {
            if owner.language != lang {
                continue;
            }
        }
        if segments.len() > 1 {
            let ns_prefix = &segments[..segments.len() - 1];
            if !owner_matches_namespace(owner, ns_prefix, db) {
                continue;
            }
        }
        filtered.push(owner.clone());
    }

    if filtered.len() == 1 {
        return Some(filtered.remove(0));
    }

    if filtered.len() > 1 {
        let mut same_file: Vec<Symbol> = filtered
            .iter()
            .filter(|o| o.file_path == caller_file_path)
            .cloned()
            .collect();
        if same_file.len() == 1 {
            return Some(same_file.remove(0));
        }

        // Conservative refusal when ambiguous without scoped file evidence
        return None;
    }

    None
}

pub(super) fn filter_candidates_by_receiver<'a>(
    candidates: &'a [Symbol],
    receiver: &str,
    caller_scope_symbol_id: Option<&str>,
    caller_file_path: &str,
    caller_language: Option<&str>,
    db: &SymbolDatabase,
) -> Vec<&'a Symbol> {
    let receiver_lower = receiver.trim().to_lowercase();
    let is_self = matches!(receiver_lower.as_str(), "self" | "this");
    let is_base = matches!(receiver_lower.as_str(), "base" | "super");

    if is_self {
        let Some(scope_id) = caller_scope_symbol_id else {
            return Vec::new();
        };
        let Some(owner) = find_enclosing_owner(scope_id, db) else {
            return Vec::new();
        };

        let direct_matches: Vec<&'a Symbol> = candidates
            .iter()
            .filter(|c| c.parent_id.as_deref() == Some(&owner.id))
            .collect();
        if !direct_matches.is_empty() {
            return direct_matches;
        }

        let ancestors = get_ancestor_type_ids(&owner.id, db);
        let inherited_matches: Vec<&'a Symbol> = candidates
            .iter()
            .filter(|c| c.parent_id.as_ref().is_some_and(|p| ancestors.contains(p)))
            .collect();

        let parent_set: HashSet<_> = inherited_matches
            .iter()
            .filter_map(|c| c.parent_id.as_deref())
            .collect();
        if parent_set.len() > 1 {
            return Vec::new();
        }

        return inherited_matches;
    }

    if is_base {
        let Some(scope_id) = caller_scope_symbol_id else {
            return Vec::new();
        };
        let Some(owner) = find_enclosing_owner(scope_id, db) else {
            return Vec::new();
        };

        let ancestors = get_ancestor_type_ids(&owner.id, db);
        if ancestors.is_empty() {
            return Vec::new();
        }

        let base_matches: Vec<&'a Symbol> = candidates
            .iter()
            .filter(|c| c.parent_id.as_ref().is_some_and(|p| ancestors.contains(p)))
            .collect();

        let parent_set: HashSet<_> = base_matches
            .iter()
            .filter_map(|c| c.parent_id.as_deref())
            .collect();
        if parent_set.len() > 1 {
            return Vec::new();
        }

        return base_matches;
    }

    let Some(owner) = resolve_named_owner(receiver, caller_file_path, caller_language, db) else {
        return Vec::new();
    };

    let direct_matches: Vec<&'a Symbol> = candidates
        .iter()
        .filter(|c| c.parent_id.as_deref() == Some(&owner.id))
        .collect();
    if !direct_matches.is_empty() {
        return direct_matches;
    }

    let ancestors = get_ancestor_type_ids(&owner.id, db);
    let inherited_matches: Vec<&'a Symbol> = candidates
        .iter()
        .filter(|c| c.parent_id.as_ref().is_some_and(|p| ancestors.contains(p)))
        .collect();

    let parent_set: HashSet<_> = inherited_matches
        .iter()
        .filter_map(|c| c.parent_id.as_deref())
        .collect();
    if parent_set.len() > 1 {
        return Vec::new();
    }

    inherited_matches
}
