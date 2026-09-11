//! Tool catalog registering all 12 tools with typed decoding, schemas, and access classes.

use crate::request_engine::semantic::SemanticRequirement;
use crate::request_engine::types::{AccessClass, RequestFailure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

macro_rules! register_tool_catalog {
    ($(
        $name:literal => {
            variant: $variant:ident,
            type: $param_ty:ty,
            description: $desc:literal,
            access: |$p_access:ident| $access_expr:expr,
            workspace: |$p_ws:ident| $ws_expr:expr,
            unbound: |$p_unbound:ident| $unbound_expr:expr,
            semantics: |$p_sem:ident| $sem_expr:expr,
        }
    ),* $(,)?) => {
        pub const AVAILABLE_TOOLS: &[&str] = &[
            $($name),*
        ];

        pub enum DecodedTool {
            $($variant($param_ty)),*
        }

        impl DecodedTool {
            pub fn name(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name),*
                }
            }

            pub fn access(&self) -> AccessClass {
                match self {
                    $(Self::$variant($p_access) => $access_expr),*
                }
            }

            pub fn workspace(&self) -> Option<&str> {
                match self {
                    $(Self::$variant($p_ws) => $ws_expr),*
                }
            }

            pub fn is_unbound(&self) -> bool {
                match self {
                    $(Self::$variant($p_unbound) => $unbound_expr),*
                }
            }

            pub fn semantic_requirement(&self) -> SemanticRequirement {
                match self {
                    $(Self::$variant($p_sem) => $sem_expr),*
                }
            }
        }


        #[derive(Debug, Clone, Default)]
        pub struct ToolCatalog;

        impl ToolCatalog {
            pub fn new() -> Self {
                Self
            }

            pub fn list() -> Vec<ToolInfo> {
                vec![
                    $(
                        ToolInfo {
                            name: $name,
                            description: $desc,
                            schema: serde_json::to_value(schemars::schema_for!($param_ty))
                                .unwrap_or_else(|_| serde_json::json!({})),
                        }
                    ),*
                ]
            }

            pub fn schema(name: &str) -> Result<Value, RequestFailure> {
                match name {
                    $(
                        $name => {
                            serde_json::to_value(schemars::schema_for!($param_ty))
                                .map_err(|e| RequestFailure::internal(format!("Schema generation error: {e}")))
                        }
                    ),*
                    _ => Err(RequestFailure::unknown_tool(name, AVAILABLE_TOOLS)),
                }
            }

            pub fn decode(name: &str, arguments: Map<String, Value>) -> Result<DecodedTool, RequestFailure> {
                let val = Value::Object(arguments);
                match name {
                    $(
                        $name => {
                            serde_json::from_value::<$param_ty>(val).map(DecodedTool::$variant).map_err(|e| {
                                RequestFailure::invalid_arguments(format!(
                                    "Failed to parse parameters for tool '{}': {}\nCheck field names and types against the tool schema.",
                                    name, e
                                ))
                            })
                        }
                    ),*
                    _ => Err(RequestFailure::unknown_tool(name, AVAILABLE_TOOLS)),
                }
            }
        }
    };
}

register_tool_catalog! {
    "blast_radius" => {
        variant: BlastRadius,
        type: crate::tools::BlastRadiusTool,
        description: "Deterministic impact analysis for changed symbols or files. With no arguments it reads the working-tree git diff. A next: line is returned when more rows exist than limit.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
    "call_path" => {
        variant: CallPath,
        type: crate::tools::navigation::CallPathTool,
        description: "Find shortest call paths between two functions.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
    "deep_dive" => {
        variant: DeepDive,
        type: crate::tools::DeepDiveTool,
        description: "Deep dive on a specific symbol: definition, callers, callees, references, types.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::Symbols,
    },
    "edit_file" => {
        variant: EditFile,
        type: crate::tools::editing::edit_file::EditFileTool,
        description: "Edit a file without reading it first. Preview with dry_run=true, apply with dry_run=false.",
        access: |p| if p.dry_run { AccessClass::Preview } else { AccessClass::SourceEdit },
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
    "fast_refs" => {
        variant: FastRefs,
        type: crate::tools::FastRefsTool,
        description: "Find symbol references (callers, usages, imports) with fast search.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
    "fast_search" => {
        variant: FastSearch,
        type: crate::tools::search::FastSearchParams,
        description: "Search code and symbols using unified code-aware full-text search.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.search.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |p| match p.search.backend {
            Some(crate::tools::search::SearchBackend::Lexical) => SemanticRequirement::None,
            Some(crate::tools::search::SearchBackend::Semantic)
            | Some(crate::tools::search::SearchBackend::Hybrid) => {
                SemanticRequirement::QueryAndSymbols
            }
            None => SemanticRequirement::QueryAndSymbols,
        },
    },
    "get_context" => {
        variant: GetContext,
        type: crate::tools::GetContextTool,
        description: "Get token-budgeted context for a concept or task.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::QueryAndSymbols,
    },
    "get_symbols" => {
        variant: GetSymbols,
        type: crate::tools::GetSymbolsTool,
        description: "List symbols in a file, directory, or whole workspace.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
    "manage_workspace" => {
        variant: ManageWorkspace,
        type: crate::tools::ManageWorkspaceTool,
        description: "Manage workspaces: index, list, open, remove, refresh, health, rebuild, and status.",
        access: |p| match p.operation.as_str() {
            "list" | "health" | "dashboard" | "status" => AccessClass::Read,
            _ => AccessClass::IndexMutation,
        },
        workspace: |p| p.path.as_deref().or(p.workspace_id.as_deref()),
        unbound: |p| {
            p.operation == "list"
                || (p.operation == "status" && p.path.is_none() && p.workspace_id.is_none())
        },
        semantics: |_p| SemanticRequirement::None,
    },
    "patterns" => {
        variant: Patterns,
        type: crate::tools::PatternsTool,
        description: "Detect, query, and inspect architectural patterns across the codebase.",
        access: |_p| AccessClass::Read,
        workspace: |p| p.workspace.as_deref(),
        unbound: |_p| false,
        semantics: |_p| SemanticRequirement::None,
    },
}
