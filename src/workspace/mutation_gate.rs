//! Mutation gate — relocated to `julie_core::workspace::mutation_gate`.
//!
//! All items re-exported so existing `crate::workspace::mutation_gate::*` import
//! sites compile unchanged.
pub use julie_core::workspace::mutation_gate::{MutationGuard, Registry, acquire_gate};
