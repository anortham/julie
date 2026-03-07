//! Federation module: cross-workspace search with RRF merge.
//!
//! Provides parallel fan-out of search queries across multiple loaded workspaces,
//! merging results using Reciprocal Rank Fusion (RRF) for unified ranking.

pub mod rrf;
pub mod search;

pub use rrf::{multi_rrf_merge, RrfItem, RRF_K};
pub use search::{
    FederatedContentResult, FederatedSymbolResult, WorkspaceSearchEntry,
    federated_content_search, federated_symbol_search,
};
