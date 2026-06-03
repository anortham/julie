//! julie-core: bottom leaf crate of the Julie workspace.
//!
//! This crate holds shared types and infrastructure that the top-level `julie`
//! crate (and any future sibling crates) depend on. It must remain a true leaf:
//! no references to `crate::handler`, `crate::tools`, or `crate::daemon`.

pub mod embeddings_contract;
