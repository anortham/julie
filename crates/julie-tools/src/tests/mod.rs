//! Handler-free tool tests relocated into julie-tools (T2b.6).
//!
//! Tests here have no dependency on `JulieServerHandler`, `crate::handler`,
//! `crate::daemon`, `crate::session`, or the `workspace` test helper.
//! They may use `julie_test_support::{db, tempdir, cleanup}`.

pub mod blast_radius_formatting_tests;
pub mod filtering_tests;
pub mod hybrid_search_tests;
pub mod phase4_token_savings;
pub mod query_classification_tests;
