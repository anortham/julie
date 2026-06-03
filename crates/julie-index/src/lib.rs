//! julie-index: search and analysis layer above julie-core.
//!
//! Provides Tantivy-powered code search with code-aware tokenization,
//! post-indexing analysis (test linkage, change risk, early warnings),
//! and language configuration. Depends on julie-core (the leaf) and
//! julie-extractors; must NOT depend on the top-level julie crate
//! (no handler, tools, daemon, watcher, or workspace references).

pub mod analysis;
pub mod search;

#[cfg(test)]
mod tests;
