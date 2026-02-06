//! Data types for call path tracing

use crate::extractors::RelationshipKind;

/// Represents a node in the call path tree
#[derive(Debug, Clone)]
pub struct CallPathNode {
    pub symbol: crate::extractors::Symbol,
    pub level: u32,
    pub match_type: MatchType,
    pub relationship_kind: Option<RelationshipKind>,
    pub children: Vec<CallPathNode>,
}

/// Type of match found during tracing
#[derive(Debug, Clone, PartialEq)]
pub enum MatchType {
    /// Same language, direct relationship in database
    Direct,
    /// Cross-language via naming convention variants
    NamingVariant,
}

// Default value functions for serde defaults
pub fn default_upstream() -> String {
    "upstream".to_string()
}

pub fn default_depth() -> u32 {
    3
}

pub fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}
