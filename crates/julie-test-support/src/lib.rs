//! Thin re-export of julie_core::test_support (see ADR-0006). The helpers live
//! in julie-core so its own tests can use them without a dep cycle.
//!
//! Also re-exports `FakeToolContext` — a hermetic test double for `ToolContext`
//! used by handler-free tool tests (T2b.5+).
pub use julie_core::test_support::*;
pub use julie_core::test_support::{cleanup, db, tempdir};

mod fake_tool_context;
pub use fake_tool_context::FakeToolContext;

pub mod workspace_markers;
pub use workspace_markers::{make_isolated_workspace_root, mark_workspace_root};
