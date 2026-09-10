//! Navigation tools - Symbol reference finding
//!
//! This module provides high-performance tools for finding references:
//! - **fast_refs**: Find all references to a symbol (<20ms)
//! - **call_path**: Find one bounded shortest path between two symbols
//!
//! Architecture:
//! - Walks the snapshot graph: name index for definitions, adjacency for references
//! - Per-workspace snapshot isolation
//! - Cross-language support through naming convention variants

pub mod call_path;
mod fast_refs;
pub(crate) mod fast_refs_semantic;
pub mod formatting;
pub mod resolution;
pub(crate) mod sites;

pub use call_path::CallPathTool;
pub use fast_refs::{FastRefsTool, FoundReferences, find_references};
