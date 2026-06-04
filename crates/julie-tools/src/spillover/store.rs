//! Re-export shim — `SpilloverStore` has been relocated to `julie-context`.
//!
//! All importers of `crate::spillover::store::{SpilloverStore, ...}`
//! continue to resolve unchanged via these re-exports. The authoritative
//! source is now `crates/julie-context/src/spillover.rs`.
pub use julie_context::{SpilloverFormat, SpilloverPage, SpilloverStore};
