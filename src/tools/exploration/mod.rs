//! Exploration tools for codebase understanding and business logic discovery
//!
//! This module provides tools for:
//! - Multi-mode exploration (FastExploreTool) - NEW unified exploration tool
//! - Business logic discovery (FindLogicTool) - Legacy, use fast_explore instead
//! - Architectural analysis and pattern recognition
//!
//! ## Module Structure
//! - `fast_explore` - FastExploreTool with multi-mode exploration (logic/similar/tests/deps)
//! - `find_logic` - FindLogicTool with 5-tier intelligent search (legacy, use fast_explore)
//! - `types` - Shared data structures for results

pub mod fast_explore;
pub mod find_logic;
pub mod types;

// Re-export public API
pub use fast_explore::{ExploreMode, FastExploreTool};
pub use find_logic::FindLogicTool;
pub use types::BusinessLogicSymbol;
