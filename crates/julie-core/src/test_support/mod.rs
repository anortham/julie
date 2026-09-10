//! Handler-free test helpers for the Julie workspace.
//!
//! These helpers live in `julie-core` itself (behind the `test-support` feature or
//! `cfg(test)`) so that julie-core's own test binary can use them without a
//! dev-dependency cycle.  See ADR-0006 for the full rationale.
//!
//! `julie-test-support` is a thin re-export of this module for downstream consumers.

pub mod cleanup;
pub mod tempdir;

pub use cleanup::atomic_cleanup_julie_dir;
pub use tempdir::unique_temp_dir;
