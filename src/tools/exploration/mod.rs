//! Exploration tools for codebase understanding and business logic discovery
//!
//! This module provides tools for:
//! - Business logic discovery (FindLogicTool)
//! - Architectural analysis and pattern recognition
//!
//! ## Module Structure
//! - `find_logic` - FindLogicTool with 5-tier intelligent search for business logic
//! - `types` - Shared data structures for results

pub mod find_logic;
pub mod types;

// Re-export public API
pub use find_logic::FindLogicTool;
pub use types::{BusinessLogicSymbol, FindLogicResult};
