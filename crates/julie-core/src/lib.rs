//! julie-core: bottom leaf crate of the Julie workspace.
//!
//! This crate holds shared types and infrastructure that the top-level `julie`
//! crate (and any future sibling crates) depend on. It must remain a true leaf:
//! no references to `crate::handler`, `crate::tools`, or `crate::daemon`.

pub mod connection_pool;
pub mod cross_language_intelligence;
pub mod database;
pub mod embeddings_contract;
pub use embeddings_contract::CURRENT_EMBEDDING_FORMAT_VERSION;
pub mod embeddings_identity;
pub use embeddings_identity::EncoderIdentity;
pub mod external_extract_paths;
pub mod file_policy;
pub mod file_utils;
pub mod glob;
pub mod health_types;
pub mod indexing_state;
pub mod language;
pub mod mcp_compat;
pub mod paths;
pub mod serde_lenient;
pub mod shared;
pub mod string_similarity;
pub mod symbol;
pub mod token_estimation;

pub use symbol::Symbol;
pub mod walk;
pub mod workspace;
pub mod workspace_errors;
pub mod workspace_scan;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
