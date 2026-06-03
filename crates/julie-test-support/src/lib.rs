//! Thin re-export of julie_core::test_support (see ADR-0006). The helpers live
//! in julie-core so its own tests can use them without a dep cycle.
pub use julie_core::test_support::*;
pub use julie_core::test_support::{cleanup, db, tempdir};
