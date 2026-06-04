//! `julie-context` — the ToolContext facade crate (Phase 2 PR 2b).
//!
//! Position in the workspace DAG:
//!   julie-core → julie-index → julie-context → julie-tools → julie (top)
//!   julie-pipeline (sibling; neither depends on the other)
//!
//! This crate holds exactly:
//! - `ToolContext` trait (18 methods) — the facade that `julie-tools` uses
//!   instead of `&JulieServerHandler` so tools can be extracted without
//!   naming the top-crate handler, daemon, or runtime types.
//! - `SpilloverStore` / `SpilloverFormat` / `SpilloverPage` — relocated from
//!   `src/tools/spillover/store.rs` (pure: std + anyhow + blake3).
//! - `WorkspaceTarget` enum — relocated from
//!   `src/tools/navigation/resolution.rs` (return type of `resolve_workspace_target`).
//!
//! Do NOT add anything that names `julie::`, `julie_pipeline`, `julie_runtime`,
//! or `julie_tools` — the tripwire (`tests/no_upward_deps.rs`) enforces this.

pub mod spillover;
pub mod tool_context;
pub mod workspace_target;

// Flat re-exports so downstream code can write `julie_context::SpilloverStore`
// etc. rather than navigating the module tree.
pub use spillover::{SpilloverFormat, SpilloverPage, SpilloverStore};
pub use tool_context::ToolContext;
pub use workspace_target::WorkspaceTarget;
