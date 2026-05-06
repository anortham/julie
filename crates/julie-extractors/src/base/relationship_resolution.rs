use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{PendingRelationship, RelationshipKind, Symbol, SymbolKind};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UnresolvedTarget {
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "terminalName")]
    pub terminal_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
    #[serde(
        rename = "namespacePath",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub namespace_path: Vec<String>,
    #[serde(
        rename = "importContext",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub import_context: Option<String>,
}

impl UnresolvedTarget {
    pub fn simple(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            display_name: name.clone(),
            terminal_name: name,
            receiver: None,
            namespace_path: Vec::new(),
            import_context: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructuredPendingRelationship {
    pub pending: PendingRelationship,
    pub target: UnresolvedTarget,
    #[serde(
        rename = "callerScopeSymbolId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub caller_scope_symbol_id: Option<String>,
}

impl StructuredPendingRelationship {
    pub fn new(
        from_symbol_id: String,
        target: UnresolvedTarget,
        caller_scope_symbol_id: Option<String>,
        kind: RelationshipKind,
        file_path: String,
        line_number: u32,
        confidence: f32,
    ) -> Self {
        let display_name = target.display_name.clone();
        Self {
            target,
            caller_scope_symbol_id,
            pending: PendingRelationship {
                from_symbol_id,
                callee_name: display_name,
                kind,
                file_path,
                line_number,
                confidence,
            },
        }
    }

    pub fn into_pending_relationship(self) -> PendingRelationship {
        self.pending
    }
}

impl PendingRelationship {
    pub fn legacy(
        from_symbol_id: String,
        callee_name: String,
        kind: RelationshipKind,
        file_path: String,
        line_number: u32,
        confidence: f32,
    ) -> Self {
        Self {
            from_symbol_id,
            callee_name,
            kind,
            file_path,
            line_number,
            confidence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LocalTargetResolution<'a> {
    Resolved(&'a Symbol),
    Import(&'a Symbol),
    Ambiguous,
    ReceiverQualified,
    Missing,
}

impl<'a> LocalTargetResolution<'a> {
    pub fn as_symbol(self) -> Option<&'a Symbol> {
        match self {
            LocalTargetResolution::Resolved(symbol) | LocalTargetResolution::Import(symbol) => {
                Some(symbol)
            }
            LocalTargetResolution::Ambiguous
            | LocalTargetResolution::ReceiverQualified
            | LocalTargetResolution::Missing => None,
        }
    }
}

pub struct ScopedSymbolIndex<'a> {
    by_name: HashMap<&'a str, Vec<&'a Symbol>>,
}

impl<'a> ScopedSymbolIndex<'a> {
    pub fn new(symbols: &'a [Symbol]) -> Self {
        let mut by_name: HashMap<&'a str, Vec<&'a Symbol>> = HashMap::new();
        for symbol in symbols {
            by_name
                .entry(symbol.name.as_str())
                .or_default()
                .push(symbol);
        }
        Self { by_name }
    }

    pub fn unique_symbol_map(symbols: &'a [Symbol]) -> HashMap<String, &'a Symbol> {
        let index = Self::new(symbols);
        index
            .by_name
            .into_iter()
            .filter_map(|(name, candidates)| match candidates.as_slice() {
                [symbol] => Some((name.to_string(), *symbol)),
                _ => None,
            })
            .collect()
    }

    pub fn first_by_name(&self, name: &str) -> Option<&'a Symbol> {
        self.by_name
            .get(name)
            .and_then(|candidates| candidates.first().copied())
    }

    pub fn candidates_by_name(&self, name: &str) -> impl Iterator<Item = &'a Symbol> + '_ {
        self.by_name
            .get(name)
            .into_iter()
            .flat_map(|candidates| candidates.iter().copied())
    }

    pub fn resolve_call_target(
        &self,
        terminal_name: &str,
        caller: Option<&Symbol>,
        receiver: Option<&str>,
    ) -> LocalTargetResolution<'a> {
        let Some(candidates) = self.by_name.get(terminal_name) else {
            return LocalTargetResolution::Missing;
        };

        if receiver.is_some_and(|receiver| !is_self_receiver(receiver)) {
            return LocalTargetResolution::ReceiverQualified;
        }

        let callable: Vec<&Symbol> = candidates
            .iter()
            .copied()
            .filter(|symbol| is_callable_or_import(&symbol.kind))
            .collect();
        if callable.is_empty() {
            return LocalTargetResolution::Missing;
        }

        if receiver.is_some() {
            return resolve_self_receiver_target(&callable, caller);
        }

        unique_candidate(&callable)
    }
}

fn resolve_self_receiver_target<'a>(
    candidates: &[&'a Symbol],
    caller: Option<&Symbol>,
) -> LocalTargetResolution<'a> {
    let Some(caller_parent_id) = caller.and_then(|caller| caller.parent_id.as_deref()) else {
        return LocalTargetResolution::Missing;
    };

    let same_parent: Vec<&Symbol> = candidates
        .iter()
        .copied()
        .filter(|symbol| symbol.parent_id.as_deref() == Some(caller_parent_id))
        .collect();
    if same_parent.is_empty() {
        return LocalTargetResolution::Missing;
    }

    unique_candidate(&same_parent)
}

fn unique_candidate<'a>(candidates: &[&'a Symbol]) -> LocalTargetResolution<'a> {
    if let Some(symbol) = unique_concrete_definition(candidates) {
        return LocalTargetResolution::Resolved(symbol);
    }

    match candidates {
        [] => LocalTargetResolution::Missing,
        [symbol] if symbol.kind == SymbolKind::Import => LocalTargetResolution::Import(symbol),
        [symbol] => LocalTargetResolution::Resolved(symbol),
        _ => LocalTargetResolution::Ambiguous,
    }
}

fn unique_concrete_definition<'a>(candidates: &[&'a Symbol]) -> Option<&'a Symbol> {
    let mut definition = None;
    for symbol in candidates {
        match symbol_definition_status(symbol)? {
            true if definition.replace(*symbol).is_some() => return None,
            true => {}
            false => {}
        }
    }
    definition
}

fn symbol_definition_status(symbol: &Symbol) -> Option<bool> {
    let value = symbol.metadata.as_ref()?.get("isDefinition")?;
    match value {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::String(value) => value.parse::<bool>().ok(),
        _ => None,
    }
}

fn is_self_receiver(receiver: &str) -> bool {
    matches!(receiver, "self" | "this" | "Self")
}

fn is_callable_or_import(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor | SymbolKind::Import
    )
}
