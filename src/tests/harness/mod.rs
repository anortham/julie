//! Test harness fixtures.
//!
//! Reusable infrastructure for tests that need a running daemon. The
//! canonical fixture is [`in_process::InProcessDaemon`] (Plan Task B.3):
//! it spins a `DaemonApp` inside the test process without going through
//! `tokio::process::Command`, so tests run in-process and `cargo xtask
//! test dev` doesn't pay subprocess startup cost per test.
//!
//! Plan reference: `docs/plans/2026-05-16-daemon-split-and-search-reranker-plan.md`
//! Task B.3.

#[cfg(test)]
pub mod in_process;
