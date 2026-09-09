use anyhow::Result;
use std::collections::{HashMap, HashSet};

use julie_core::Symbol;
use julie_core::database::{IdentifierRef, SymbolDatabase};
use julie_extractors::RelationshipKind;
use julie_index::search::scoring::is_test_path;

use super::lookup::{impl_type_name, qualified_name_leaf};
use super::types::RefEntry;

fn symbol_is_test(symbol: &Symbol) -> bool {
    julie_index::analysis::test_roles::is_test_related(symbol) || is_test_path(&symbol.file_path)
}

/// Build test location refs by querying identifiers linked from test symbols.
pub(crate) fn build_test_refs(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<RefEntry>> {
    let names = identifier_names_for_symbol(db, symbol)?;
    let bare_name_allowed = bare_identifier_name_allowed(symbol, &names);
    let ident_refs = db.get_identifiers_by_names(&names)?;

    // Batch-fetch all containing symbols in one query so metadata can drive test
    // detection before the path fallback kicks in.
    let containing_ids: Vec<String> = ident_refs
        .iter()
        .filter_map(|i| i.containing_symbol_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let symbol_map: HashMap<String, Symbol> = if containing_ids.is_empty() {
        HashMap::new()
    } else {
        db.get_symbols_by_ids(&containing_ids)
            .unwrap_or_default()
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    };

    let test_idents: Vec<_> = ident_refs
        .into_iter()
        .filter(|ident| {
            if ident.file_path == symbol.file_path && ident.start_line == symbol.start_line {
                return false;
            }
            if !identifier_matches_symbol(ident, symbol, &names, bare_name_allowed) {
                return false;
            }

            ident
                .containing_symbol_id
                .as_ref()
                .and_then(|id| symbol_map.get(id))
                .is_some_and(symbol_is_test)
                || is_test_path(&ident.file_path)
        })
        .collect();

    let mut test_refs = Vec::new();
    for ident in test_idents {
        let containing_symbol = ident
            .containing_symbol_id
            .as_ref()
            .and_then(|id| symbol_map.get(id).cloned());

        let rel_kind = match ident.kind.as_str() {
            "call" => RelationshipKind::Calls,
            "import" => RelationshipKind::Imports,
            _ => RelationshipKind::References,
        };

        test_refs.push(RefEntry {
            kind: rel_kind,
            file_path: ident.file_path,
            line_number: ident.start_line,
            symbol: containing_symbol,
        });
    }

    // Deduplicate by (file_path, containing symbol name) — keep first occurrence
    let mut seen = HashSet::new();
    test_refs.retain(|r| {
        let key = (
            r.file_path.clone(),
            r.symbol
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
        );
        seen.insert(key)
    });

    // Cap to prevent output bloat for common symbol names
    test_refs.truncate(10);

    Ok(test_refs)
}

/// Merge identifier-based references into the incoming list.
///
/// Identifiers catch usage sites that relationship extraction misses (struct type
/// annotations, function calls without extracted relationships, member accesses).
/// Deduplicates against existing relationship-based refs by (file_path, line_number)
/// and filters out the definition site itself.
pub(crate) fn merge_identifier_refs(
    db: &SymbolDatabase,
    symbol: &Symbol,
    mut incoming: Vec<RefEntry>,
    incoming_total: usize,
    incoming_calls_total: usize,
    incoming_cap: usize,
) -> Result<(Vec<RefEntry>, usize, usize)> {
    let names = identifier_names_for_symbol(db, symbol)?;
    let bare_name_allowed = bare_identifier_name_allowed(symbol, &names);
    let ident_refs = db.get_identifiers_by_names(&names)?;

    if ident_refs.is_empty() {
        return Ok((incoming, incoming_total, incoming_calls_total));
    }

    // Build dedup set from existing relationship refs; kept mutable so new entries
    // added below are tracked and don't get added twice from the identifier list.
    let mut existing: HashSet<(String, u32)> = incoming
        .iter()
        .map(|r| (r.file_path.clone(), r.line_number))
        .collect();

    let mut added = 0;
    let mut call_added = 0;
    for ident in ident_refs {
        if !identifier_matches_symbol(&ident, symbol, &names, bare_name_allowed) {
            continue;
        }

        // Skip identifier at the definition site itself
        if ident.file_path == symbol.file_path && ident.start_line == symbol.start_line {
            continue;
        }

        // Skip if already covered by a relationship
        let key = (ident.file_path.clone(), ident.start_line);
        if existing.contains(&key) {
            continue;
        }

        // Respect incoming cap
        if incoming.len() >= incoming_cap {
            added += 1; // Still count towards total
            if ident.kind == "call" {
                call_added += 1;
            }
            continue;
        }

        let rel_kind = match ident.kind.as_str() {
            "call" => RelationshipKind::Calls,
            "import" => RelationshipKind::Imports,
            _ => RelationshipKind::References,
        };
        let is_call = matches!(rel_kind, RelationshipKind::Calls);

        // Enrich with containing symbol if available
        let containing_symbol = ident
            .containing_symbol_id
            .as_ref()
            .and_then(|id| db.get_symbol_by_id(id).ok().flatten());

        incoming.push(RefEntry {
            kind: rel_kind,
            file_path: ident.file_path,
            line_number: ident.start_line,
            symbol: containing_symbol,
        });
        existing.insert(key);
        added += 1;
        if is_call {
            call_added += 1;
        }
    }

    let new_total = incoming_total + added;
    let new_call_total = incoming_calls_total + call_added;
    Ok((incoming, new_total, new_call_total))
}

fn identifier_names_for_symbol(db: &SymbolDatabase, symbol: &Symbol) -> Result<Vec<String>> {
    let mut names = Vec::new();
    push_unique(&mut names, symbol.name.clone());

    let mut qualifiers = Vec::new();
    if let Some(parent_id) = &symbol.parent_id {
        if let Some(parent) = db.get_symbol_by_id(parent_id)? {
            push_unique(&mut qualifiers, parent.extracted.name);
        }
    }
    if let Some(impl_type) = impl_type_name(symbol) {
        push_unique(&mut qualifiers, impl_type.to_string());
        push_unique(&mut qualifiers, qualified_name_leaf(impl_type).to_string());
    }

    for qualifier in qualifiers {
        push_unique(&mut names, format!("{}::{}", qualifier, symbol.name));
        push_unique(&mut names, format!("{}.{}", qualifier, symbol.name));
    }

    Ok(names)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

fn bare_identifier_name_allowed(symbol: &Symbol, names: &[String]) -> bool {
    symbol.parent_id.is_none()
        && impl_type_name(symbol).is_none()
        && names.iter().all(|name| name == &symbol.name)
}

fn identifier_matches_symbol(
    ident: &IdentifierRef,
    symbol: &Symbol,
    names: &[String],
    bare_name_allowed: bool,
) -> bool {
    if ident.target_symbol_id.as_deref() == Some(symbol.id.as_str()) {
        return true;
    }
    if ident.target_symbol_id.is_some() {
        return false;
    }
    if bare_name_allowed && ident.name == symbol.name {
        return true;
    }
    names
        .iter()
        .any(|name| name != &symbol.name && ident.name == *name)
}

/// Enrich ref entries with symbol data by looking up relationship symbol IDs.
pub(crate) fn enrich_refs(
    db: &SymbolDatabase,
    refs: &mut [RefEntry],
    symbol_ids: &[String],
) -> Result<()> {
    // Collect unique symbol IDs to fetch
    let unique_symbol_ids: Vec<String> = symbol_ids
        .iter()
        .filter(|id| !id.is_empty())
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    if unique_symbol_ids.is_empty() {
        return Ok(());
    }

    let symbols = db.get_symbols_by_ids(&unique_symbol_ids)?;
    let symbol_map: HashMap<String, Symbol> =
        symbols.into_iter().map(|s| (s.id.clone(), s)).collect();

    for (r, sym_id) in refs.iter_mut().zip(symbol_ids.iter()) {
        if !sym_id.is_empty() {
            r.symbol = symbol_map.get(sym_id).cloned();
        }
    }

    Ok(())
}
