//! Symbol operations module
//!
//! Split into focused sub-modules (<500 lines each):
//! - storage: Store and delete symbols
//! - bulk: Bulk insert operations with index optimization  
//! - queries: Get symbol by ID, find by name/pattern
//! - search: Advanced search, statistics, batch queries

mod storage;
mod bulk;
mod queries;
mod search;

// Re-export all symbol methods (they're already implemented on SymbolDatabase via trait)
// No need to re-export - impl SymbolDatabase blocks in each file extend the type
