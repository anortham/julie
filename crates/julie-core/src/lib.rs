//! julie-core: bottom leaf crate of the Julie workspace.
//!
//! This crate holds shared types and infrastructure that the top-level `julie`
//! crate (and any future sibling crates) depend on. It must remain a true leaf:
//! no references to `crate::handler`, `crate::tools`, or `crate::daemon`.

pub mod connection_pool;
pub mod database;
pub mod embeddings_contract;
pub mod glob;
pub mod paths;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
