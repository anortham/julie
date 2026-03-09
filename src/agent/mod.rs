//! Agent dispatch engine.
//!
//! Provides the backend trait, CLI implementations for multiple agent backends,
//! context assembly from indexes + memories, and dispatch execution with output capture.
//!
//! ## Architecture
//!
//! - **Single-shot dispatch**: Assemble generous context -> agent CLI -> capture output -> store as checkpoint
//! - **No interactive sessions**: Each dispatch is a one-shot prompt with full context
//! - **Results as checkpoints**: Completed dispatch output is stored via the memory system
//! - **Multi-backend**: Supports Claude, Codex, Gemini CLI, and GitHub Copilot CLI
//!
//! ## Modules
//!
//! - `backend` - `AgentBackend` trait, backend detection, and factory
//! - `claude_backend` - Claude Code CLI implementation
//! - `codex_backend` - OpenAI Codex CLI implementation
//! - `gemini_backend` - Google Gemini CLI implementation
//! - `copilot_backend` - GitHub Copilot CLI implementation
//! - `context_assembly` - Context assembly from workspace search + memories
//! - `dispatch` - `DispatchManager` for tracking active/completed dispatches

pub mod backend;
pub mod claude_backend;
pub mod codex_backend;
pub mod context_assembly;
pub mod copilot_backend;
pub mod dispatch;
pub mod gemini_backend;
