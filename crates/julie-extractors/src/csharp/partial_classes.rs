use crate::base::{Relationship, RelationshipKind, Symbol, SymbolKind};
use std::collections::{BTreeMap, HashMap};

pub(super) fn add_linkage_relationships(symbols: &[Symbol], relationships: &mut Vec<Relationship>) {
    let by_id: HashMap<&str, &Symbol> = symbols
        .iter()
        .map(|symbol| (symbol.id.as_str(), symbol))
        .collect();
    let mut groups: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();

    for symbol in symbols {
        if !is_partial_class(symbol) {
            continue;
        }

        let full_name = symbol_full_name(symbol, &by_id);
        groups.entry(full_name).or_default().push(symbol);
    }

    for (full_name, group) in groups {
        if group.len() < 2 {
            continue;
        }

        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let left = group[i];
                let right = group[j];

                relationships.push(partial_class_relationship(left, right, &full_name));
                relationships.push(partial_class_relationship(right, left, &full_name));
            }
        }
    }
}

fn partial_class_relationship(from: &Symbol, to: &Symbol, full_name: &str) -> Relationship {
    let mut metadata = HashMap::new();
    metadata.insert(
        "linkage".to_string(),
        serde_json::Value::String("partial_class".to_string()),
    );
    metadata.insert(
        "partial_full_name".to_string(),
        serde_json::Value::String(full_name.to_string()),
    );
    metadata.insert(
        "requires_partial_marker".to_string(),
        serde_json::Value::Bool(true),
    );

    Relationship {
        id: format!("{}_{}_partial_class", from.id, to.id),
        from_symbol_id: from.id.clone(),
        to_symbol_id: to.id.clone(),
        kind: RelationshipKind::References,
        file_path: from.file_path.clone(),
        line_number: from.start_line,
        confidence: 1.0,
        metadata: Some(metadata),
    }
}

fn is_partial_class(symbol: &Symbol) -> bool {
    symbol.kind == SymbolKind::Class
        && symbol
            .signature
            .as_deref()
            .is_some_and(contains_partial_modifier)
}

fn contains_partial_modifier(signature: &str) -> bool {
    signature.split_whitespace().any(|token| token == "partial")
}

fn symbol_full_name(symbol: &Symbol, by_id: &HashMap<&str, &Symbol>) -> String {
    let mut parts = vec![symbol.name.clone()];
    let mut current_parent = symbol.parent_id.as_deref();

    while let Some(parent_id) = current_parent {
        let Some(parent) = by_id.get(parent_id).copied() else {
            break;
        };
        if contributes_to_full_name(&parent.kind) {
            parts.push(parent.name.clone());
        }
        current_parent = parent.parent_id.as_deref();
    }

    parts.reverse();
    parts.join(".")
}

fn contributes_to_full_name(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Namespace
            | SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Interface
            | SymbolKind::Type
            | SymbolKind::Module
    )
}
