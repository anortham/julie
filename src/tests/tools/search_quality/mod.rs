//! Search Quality Tests - Systematic Search Validation
//!
//! This module tests search quality against Julie's own codebase (dogfooding).
//! Each test represents a real-world search scenario that should work correctly.
//!
//! ## Test Philosophy
//!
//! - **Dogfooding**: Tests run against Julie's actual workspace
//! - **Regression Detection**: Breaking changes fail tests immediately
//! - **Documentation**: Tests serve as search capability docs
//! - **Incremental Growth**: Add queries as edge cases are discovered
//!
//! ## Test Organization
//!
//! - `dogfood_tests.rs` - Core search quality tests against Julie codebase
//! - `helpers.rs` - Shared test utilities and assertions
//!
//! ## Adding New Tests
//!
//! When you find a search query that doesn't work:
//! 1. Add it as a test case (it should fail)
//! 2. Fix the underlying issue
//! 3. Test passes - regression prevented!

mod dogfood_tests;
mod helpers;
