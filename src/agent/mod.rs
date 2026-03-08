//! Agent dispatch engine.
//!
//! Provides the backend trait, Claude CLI implementation, context assembly
//! from indexes + memories, and dispatch execution with output capture.
//!
//! ## Architecture
//!
//! - **Single-shot dispatch**: Assemble generous context -> `claude -p` -> capture output -> store as checkpoint
//! - **No interactive sessions**: Each dispatch is a one-shot prompt with full context
//! - **Results as checkpoints**: Completed dispatch output is stored via the memory system
//!
//! ## Modules
//!
//! - `backend` - `AgentBackend` trait and backend detection
//! - `claude_backend` - Claude CLI implementation of `AgentBackend`
//! - `context_assembly` - Context assembly from workspace search + memories
//! - `dispatch` - `DispatchManager` for tracking active/completed dispatches

pub mod backend;
pub mod claude_backend;
pub mod context_assembly;
pub mod dispatch;
