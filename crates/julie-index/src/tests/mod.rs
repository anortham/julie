//! Tests for julie-index: search and analysis layer.
//!
//! Handler-free tests relocated from the top-level crate's src/tests/ tree.
//! These tests only depend on julie-index internals plus julie-core and
//! julie-test-support — no handler, tools, daemon, or watcher imports.

pub mod analysis;
pub mod search;
