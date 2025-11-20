//! Data types for call path tracing

use crate::extractors::RelationshipKind;
use serde::{Deserialize, Serialize};

/// Structured result from trace_call_path operation
#[derive(Debug, Clone, Serialize)]
pub struct TraceCallPathResult {
    pub tool: String,
    pub symbol: String,
    pub direction: String,
    pub max_depth: u32,
    pub cross_language: bool,
    pub success: bool,
    pub paths_found: usize,
    pub next_actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_paths: Option<Vec<CallPath>>,
}

/// Serializable call path for structured output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallPath {
    pub root_symbol: String,
    pub root_file: String,
    pub root_language: String,
    pub nodes: Vec<SerializablePathNode>,
    pub total_depth: u32,
}

/// Serializable path node for structured output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializablePathNode {
    pub symbol_name: String,
    pub file_path: String,
    pub language: String,
    pub line: u32,
    pub match_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<f32>,
    pub level: u32,
    pub children: Vec<SerializablePathNode>,
}

/// Represents a node in the call path tree
#[derive(Debug, Clone)]
pub struct CallPathNode {
    pub symbol: crate::extractors::Symbol,
    #[allow(dead_code)]
    pub level: u32,
    #[allow(dead_code)]
    pub match_type: MatchType,
    #[allow(dead_code)]
    pub relationship_kind: Option<RelationshipKind>,
    #[allow(dead_code)]
    pub similarity: Option<f32>,
    pub children: Vec<CallPathNode>,
}

/// Represents a semantic match found via embeddings
#[derive(Clone)]
pub struct SemanticMatch {
    pub symbol: crate::extractors::Symbol,
    pub relationship_kind: RelationshipKind,
    pub similarity: f32,
}

/// Type of match found during tracing
#[derive(Debug, Clone, PartialEq)]
pub enum MatchType {
    /// Same language, direct relationship in database
    Direct,
    /// Cross-language via naming convention variants
    NamingVariant,
    /// Via embedding similarity
    Semantic,
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

pub fn default_output_format() -> Option<String> {
    None // Default to JSON for backwards compatibility
}
