//! Navigation tools - Fast symbol navigation and reference finding
//!
//! This module provides high-performance tools for navigating code and finding references:
//! - **fast_goto**: Jump instantly to symbol definitions (<5ms)
//! - **fast_refs**: Find all references to a symbol (<20ms)
//!
//! Architecture:
//! - Uses multi-strategy symbol resolution (FTS5 → naming variants → semantic)
//! - Per-workspace database isolation
//! - CASCADE architecture for progressive enhancement
//! - Cross-language support through naming convention variants

mod fast_goto;
mod fast_refs;
mod formatting;
mod reference_workspace;
pub mod resolution; // Public for use by other tools
mod semantic_matching;
mod types;

// Re-export public APIs
pub use fast_goto::FastGotoTool;
pub use fast_refs::FastRefsTool;
pub use types::{DefinitionResult, FastGotoResult, FastRefsResult, ReferenceResult};
