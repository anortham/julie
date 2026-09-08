//! Shared application request engine and tool dispatcher.

pub mod binding;
pub mod catalog;
pub mod dispatch;
pub mod runtime_factory;
pub mod semantic;
pub mod types;

pub use binding::BindingResolver;
pub use catalog::{AVAILABLE_TOOLS, DecodedTool, ToolCatalog, ToolInfo};
pub use dispatch::RequestEngine;
pub use runtime_factory::{RequestRuntime, RuntimeFactory};
pub use semantic::{
    CURRENT_EMBEDDING_FORMAT_VERSION, DefaultSemanticRuntime, NoopSemanticRuntime, SemanticMode,
    SemanticReadiness, SemanticRequirement, SemanticRuntime,
};
pub use types::*;
