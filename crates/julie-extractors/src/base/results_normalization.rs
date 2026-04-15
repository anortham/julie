use std::collections::HashMap;

use super::span::{NormalizedSpan, RecordOffset};
use super::types::ExtractionResults;

fn relationship_id(
    from_symbol_id: &str,
    to_symbol_id: &str,
    kind: &impl std::fmt::Debug,
    line_number: u32,
) -> String {
    format!(
        "{}_{}_{:?}_{}",
        from_symbol_id, to_symbol_id, kind, line_number
    )
}

impl ExtractionResults {
    pub fn empty() -> Self {
        Self {
            symbols: Vec::new(),
            relationships: Vec::new(),
            pending_relationships: Vec::new(),
            structured_pending_relationships: Vec::new(),
            types: HashMap::new(),
            identifiers: Vec::new(),
        }
    }

    pub fn extend(&mut self, mut other: Self) {
        self.symbols.append(&mut other.symbols);
        self.relationships.append(&mut other.relationships);
        self.pending_relationships
            .append(&mut other.pending_relationships);
        self.structured_pending_relationships
            .append(&mut other.structured_pending_relationships);
        self.types.extend(other.types);
        self.identifiers.append(&mut other.identifiers);
    }

    pub fn apply_record_offset(&mut self, offset: RecordOffset) {
        for symbol in &mut self.symbols {
            let span = NormalizedSpan {
                start_line: symbol.start_line,
                start_column: symbol.start_column,
                end_line: symbol.end_line,
                end_column: symbol.end_column,
                start_byte: symbol.start_byte,
                end_byte: symbol.end_byte,
            }
            .with_offset(offset);
            symbol.apply_normalized_span(span);
        }

        for identifier in &mut self.identifiers {
            let span = NormalizedSpan {
                start_line: identifier.start_line,
                start_column: identifier.start_column,
                end_line: identifier.end_line,
                end_column: identifier.end_column,
                start_byte: identifier.start_byte,
                end_byte: identifier.end_byte,
            }
            .with_offset(offset);
            identifier.apply_normalized_span(span);
        }

        for relationship in &mut self.relationships {
            relationship.line_number += offset.line_delta;
        }

        for pending_relationship in &mut self.pending_relationships {
            pending_relationship.line_number += offset.line_delta;
        }

        for pending_relationship in &mut self.structured_pending_relationships {
            pending_relationship.pending.line_number += offset.line_delta;
        }
    }

    pub fn rekey_normalized_locations(&mut self) {
        let mut symbol_id_map = HashMap::new();

        for symbol in &mut self.symbols {
            let old_id = symbol.refresh_id();
            symbol_id_map.insert(old_id, symbol.id.clone());
        }

        for symbol in &mut self.symbols {
            if let Some(parent_id) = symbol.parent_id.as_mut() {
                if let Some(new_parent_id) = symbol_id_map.get(parent_id) {
                    *parent_id = new_parent_id.clone();
                }
            }
        }

        for identifier in &mut self.identifiers {
            identifier.refresh_id();

            if let Some(containing_symbol_id) = identifier.containing_symbol_id.as_mut() {
                if let Some(new_symbol_id) = symbol_id_map.get(containing_symbol_id) {
                    *containing_symbol_id = new_symbol_id.clone();
                }
            }

            if let Some(target_symbol_id) = identifier.target_symbol_id.as_mut() {
                if let Some(new_symbol_id) = symbol_id_map.get(target_symbol_id) {
                    *target_symbol_id = new_symbol_id.clone();
                }
            }
        }

        for relationship in &mut self.relationships {
            if let Some(new_from_symbol_id) = symbol_id_map.get(&relationship.from_symbol_id) {
                relationship.from_symbol_id = new_from_symbol_id.clone();
            }
            if let Some(new_to_symbol_id) = symbol_id_map.get(&relationship.to_symbol_id) {
                relationship.to_symbol_id = new_to_symbol_id.clone();
            }

            relationship.id = relationship_id(
                relationship.from_symbol_id.as_str(),
                relationship.to_symbol_id.as_str(),
                &relationship.kind,
                relationship.line_number,
            );
        }

        for pending_relationship in &mut self.pending_relationships {
            if let Some(new_from_symbol_id) =
                symbol_id_map.get(&pending_relationship.from_symbol_id)
            {
                pending_relationship.from_symbol_id = new_from_symbol_id.clone();
            }
        }

        for pending_relationship in &mut self.structured_pending_relationships {
            if let Some(new_from_symbol_id) =
                symbol_id_map.get(&pending_relationship.pending.from_symbol_id)
            {
                pending_relationship.pending.from_symbol_id = new_from_symbol_id.clone();
            }

            let Some(scope_symbol_id) = pending_relationship.caller_scope_symbol_id.as_ref() else {
                continue;
            };

            if let Some(new_scope_symbol_id) = symbol_id_map.get(scope_symbol_id) {
                pending_relationship.caller_scope_symbol_id = Some(new_scope_symbol_id.clone());
            }
        }

        let mut rekeyed_types = HashMap::with_capacity(self.types.len());
        for (symbol_id, mut type_info) in std::mem::take(&mut self.types) {
            let new_symbol_id = symbol_id_map.get(&symbol_id).cloned().unwrap_or(symbol_id);
            type_info.symbol_id = new_symbol_id.clone();
            rekeyed_types.insert(new_symbol_id, type_info);
        }
        self.types = rekeyed_types;
    }
}
