//! Per-tool MCP handler modules.
//!
//! Each tool's `#[tool]` method lives in its own file with a dedicated
//! `#[tool_router(router = tool_router_<name>, vis = "pub(crate)")]` impl
//! block. The composer in `crate::handler` builds the final `ToolRouter` by
//! adding all per-tool routers together. The split keeps each per-tool
//! router small and separately reviewable.

pub(crate) mod blast_radius;
pub(crate) mod call_path;
pub(crate) mod deep_dive;
pub(crate) mod edit_file;
pub(crate) mod error;
pub(crate) mod fast_refs;
pub(crate) mod fast_search;
pub(crate) mod get_context;
pub(crate) mod get_symbols;
pub(crate) mod manage_workspace;
pub(crate) mod patterns;
