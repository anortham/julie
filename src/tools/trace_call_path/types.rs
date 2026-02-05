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

pub fn default_output_format() -> Option<String> {
    None // None = lean format (ASCII tree). Override with "json", "toon", or "auto"
}

// ============================================================================
// Phase 5: Hierarchical TOON Flattening (Optimized)
// ============================================================================

/// Completely flat structure for TOON encoding.
///
/// This struct has ALL fields explicitly defined at the top level with
/// NO `#[serde(skip_serializing_if)]` attributes to ensure uniform key sets
/// required for tabular TOON encoding.
///
/// # Performance
///
/// Direct conversion from `SerializablePathNode` → `ToonFlatCallPathNode`
/// in a single pass eliminates intermediate allocations.
#[derive(Debug, Clone, Serialize)]
pub struct ToonFlatCallPathNode {
    pub id: usize,
    pub parent_id: Option<usize>, // Always serialized (null for root nodes)
    pub group: usize,
    pub level: u32,
    pub symbol_name: String,
    pub file_path: String,
    pub language: String,
    pub line: u32,
    pub match_type: String,
    pub relationship_kind: Option<String>, // Always serialized
    pub similarity: Option<f32>,           // Always serialized
}

impl TraceCallPathResult {
    /// Convert to completely flat structure for efficient TOON encoding.
    ///
    /// Performs direct conversion from `SerializablePathNode` to `ToonFlatCallPathNode`
    /// in a single pass, eliminating intermediate allocations.
    ///
    /// Uses depth-first traversal to maintain tree relationships via `parent_id` references.
    pub fn to_toon_flat(&self) -> Vec<ToonFlatCallPathNode> {
        let mut result = Vec::new();
        let mut id_counter = 0;

        if let Some(ref call_paths) = self.call_paths {
            for (group_id, path) in call_paths.iter().enumerate() {
                for node in &path.nodes {
                    flatten_to_toon_recursive(node, &mut result, &mut id_counter, None, group_id);
                }
            }
        }

        result
    }
}

/// Recursive helper to flatten `SerializablePathNode` directly into `ToonFlatCallPathNode`.
///
/// Performs depth-first traversal, assigning unique IDs and tracking parent_id references.
/// This single-pass conversion eliminates intermediate struct allocations for better performance.
fn flatten_to_toon_recursive(
    node: &SerializablePathNode,
    result: &mut Vec<ToonFlatCallPathNode>,
    id_counter: &mut usize,
    parent_id: Option<usize>,
    group: usize,
) {
    let current_id = *id_counter;
    *id_counter += 1;

    // Create final TOON-ready node directly (no intermediate structs)
    result.push(ToonFlatCallPathNode {
        id: current_id,
        parent_id,
        group,
        level: node.level,
        symbol_name: node.symbol_name.clone(),
        file_path: node.file_path.clone(),
        language: node.language.clone(),
        line: node.line,
        match_type: node.match_type.clone(),
        relationship_kind: node.relationship_kind.clone(),
        similarity: node.similarity,
    });

    // Recursively flatten children
    for child in &node.children {
        flatten_to_toon_recursive(child, result, id_counter, Some(current_id), group);
    }
}
