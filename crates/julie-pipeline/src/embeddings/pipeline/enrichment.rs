use std::collections::HashMap;

use julie_core::Symbol;
use julie_core::database::SymbolDatabase;
use julie_extractors::{RelationshipKind, SymbolKind};

/// Build a map of symbol_id -> callee names from the relationship graph.
/// Only includes `Calls` relationships to avoid noise from imports/type refs.
pub(crate) fn build_callee_map(
    db: &SymbolDatabase,
    symbols: &[Symbol],
) -> HashMap<String, Vec<String>> {
    let func_ids: Vec<String> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|s| s.id.clone())
        .collect();

    if func_ids.is_empty() {
        return HashMap::new();
    }

    let relationships = match db.get_outgoing_relationships_for_symbols(&func_ids) {
        Ok(rels) => rels,
        Err(err) => {
            tracing::warn!("Failed to load callees for embedding enrichment: {err:#}");
            return HashMap::new();
        }
    };

    let id_to_name: HashMap<&str, &str> = symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    let mut callees: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &relationships {
        if rel.kind == RelationshipKind::Calls {
            if let Some(name) = id_to_name.get(rel.to_symbol_id.as_str()) {
                callees
                    .entry(rel.from_symbol_id.clone())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }

    for names in callees.values_mut() {
        names.sort();
        names.dedup();
    }

    callees
}

/// Build a map of symbol_id -> field access names from the identifiers table.
/// Captures domain vocabulary from member accesses like `self.session_metrics` or `this.db`.
pub(crate) fn build_field_access_map(db: &SymbolDatabase) -> HashMap<String, Vec<String>> {
    match db.get_member_access_identifiers_grouped() {
        Ok(fields) => fields,
        Err(err) => {
            tracing::warn!("Failed to load field accesses for embedding enrichment: {err:#}");
            HashMap::new()
        }
    }
}

/// Build a map of symbol_id -> implementor names from the relationship graph.
/// Finds `Implements` and `Extends` relationships pointing TO trait/interface symbols.
pub(crate) fn build_implementor_map(
    db: &SymbolDatabase,
    symbols: &[Symbol],
) -> HashMap<String, Vec<String>> {
    let trait_interface_ids: Vec<String> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Trait | SymbolKind::Interface))
        .map(|s| s.id.clone())
        .collect();

    if trait_interface_ids.is_empty() {
        return HashMap::new();
    }

    let relationships = match db.get_relationships_to_symbols(&trait_interface_ids) {
        Ok(rels) => rels,
        Err(err) => {
            tracing::warn!("Failed to load implementors for embedding enrichment: {err:#}");
            return HashMap::new();
        }
    };

    let id_to_name: HashMap<&str, &str> = symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &relationships {
        if matches!(
            rel.kind,
            RelationshipKind::Implements | RelationshipKind::Extends
        ) {
            let impl_name = id_to_name
                .get(rel.from_symbol_id.as_str())
                .copied()
                .unwrap_or(rel.from_symbol_id.as_str());
            implementors
                .entry(rel.to_symbol_id.clone())
                .or_default()
                .push(impl_name.to_string());
        }
    }

    for names in implementors.values_mut() {
        names.sort();
        names.dedup();
        names.truncate(8);
    }

    implementors
}
