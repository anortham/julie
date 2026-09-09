//! Shared application request engine and tool dispatcher.

pub mod binding;
pub mod catalog;
pub mod dispatch;
pub mod runtime_factory;
pub mod semantic;
pub mod semantic_qualification;
pub mod semantic_store;
pub mod types;

pub use binding::BindingResolver;
pub use catalog::{AVAILABLE_TOOLS, DecodedTool, ToolCatalog, ToolInfo};
pub use dispatch::RequestEngine;
pub use runtime_factory::{RequestRuntime, RuntimeFactory};
pub use semantic::{
    CURRENT_EMBEDDING_FORMAT_VERSION, DefaultSemanticRuntime, NoopSemanticRuntime,
    RuntimeProviderState, SemanticMode, SemanticReadiness, SemanticRequirement, SemanticRuntime,
    semantic_mode_needs_provider,
};
pub use semantic_qualification::{
    NativeQualificationRecord, QualificationValidationError, validate_native_qualification,
};
pub use types::*;
