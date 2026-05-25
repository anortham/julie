//! Per-tool MCP handler modules.
//!
//! Each tool's `#[tool]` method lives in its own file with a dedicated
//! `#[tool_router(router = tool_router_<name>, vis = "pub(crate)")]` impl
//! block. The composer in `crate::handler` builds the final `ToolRouter` by
//! adding all per-tool routers together. This split lets `xtask test changed`
//! map a tool-only edit to a single test bucket instead of falling back to
//! the whole dev tier.

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
pub(crate) mod rename_symbol;
pub(crate) mod rewrite_symbol;
pub(crate) mod spillover_get;
