//! julie-core: bottom leaf crate of the Julie workspace.
//!
//! This crate holds shared types and infrastructure that the top-level `julie`
//! crate (and any future sibling crates) depend on. It must remain a true leaf:
//! no references to `crate::handler`, `crate::tools`, or `crate::daemon`.

pub mod connection_pool;
pub mod cross_language_intelligence;
pub mod database;
pub mod embeddings_contract;
pub mod external_extract_paths;
pub mod file_utils;
pub mod glob;
pub mod health_types;
pub mod language;
pub mod mcp_compat;
pub mod paths;
pub mod serde_lenient;
pub mod shared;
pub mod string_similarity;
pub mod token_estimation;
pub mod workspace;
pub mod workspace_errors;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
