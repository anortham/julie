use serde::{Deserialize, Serialize};

use super::types::{PendingRelationship, RelationshipKind};

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
