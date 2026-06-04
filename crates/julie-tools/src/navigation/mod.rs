//! Navigation tools - Symbol reference finding
//!
//! This module provides high-performance tools for finding references:
//! - **fast_refs**: Find all references to a symbol (<20ms)
//! - **call_path**: Find one bounded shortest path between two symbols
//!
//! Architecture:
//! - Uses multi-strategy symbol resolution (Tantivy → naming variants)
//! - Per-workspace database isolation
//! - Cross-language support through naming convention variants

pub mod call_path;
mod fast_refs;
pub mod formatting;
pub mod resolution;
pub mod target_workspace; // Public for use by other tools

// Re-export public APIs
pub use call_path::CallPathTool;
pub use fast_refs::FastRefsTool;
