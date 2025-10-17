//! Exploration tools for codebase understanding and business logic discovery
//!
//! This module provides tools for:
//! - Fast codebase exploration (FastExploreTool)
//! - Business logic discovery (FindLogicTool)
//! - Architectural analysis and pattern recognition
//!
//! ## Module Structure
//! - `fast_explore` - FastExploreTool for codebase overview/dependencies/hotspots analysis
//! - `find_logic` - FindLogicTool with 5-tier intelligent search for business logic
//! - `types` - Shared data structures for results

pub mod fast_explore;
pub mod find_logic;
pub mod types;

// Re-export public API
pub use fast_explore::FastExploreTool;
pub use find_logic::FindLogicTool;
pub use types::{BusinessLogicSymbol, FastExploreResult, FindLogicResult};
