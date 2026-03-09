# Multi-Agent Dispatch — Design

**Date:** 2026-03-09
**Scope:** Add Codex, Gemini CLI, and Copilot CLI backends alongside the existing Claude backend

## Problem

The Agents page currently hardcodes Claude as the only dispatch backend. Users who have Codex, Gemini CLI, or GitHub Copilot CLI installed can't use them through the dashboard.

## Design

### Backend (Rust)

All four CLI agents follow the same pattern: spawn a child process with a prompt argument, stream stdout line-by-line via broadcast channel. The existing `AgentBackend` trait already abstracts this perfectly.

**New files (following `claude_backend.rs` pattern):**

| File | CLI Command | Key Flags |
|------|-------------|-----------|
| `src/agent/codex_backend.rs` | `codex exec "prompt"` | `--full-auto --color never` |
| `src/agent/gemini_backend.rs` | `gemini -p "prompt"` | (none needed) |
| `src/agent/copilot_backend.rs` | `copilot -p "prompt"` | `--autopilot` |

Each backend implements `AgentBackend` trait: `name()`, `is_available()`, `version()`, `dispatch()`.

**Modified files:**

1. `src/agent/mod.rs` — add `pub mod codex_backend; pub mod gemini_backend; pub mod copilot_backend;`
2. `src/agent/backend.rs` — update `detect_backends()` to check all four, add `create_backend(name) -> Option<Box<dyn AgentBackend>>`
3. `src/api/agents.rs` — add `backend: Option<String>` to `DispatchRequest`, replace hardcoded `ClaudeBackend::new()` with `create_backend(name)`
4. `ui/src/views/Agents.vue` — add backend selector dropdown, send `backend` field in dispatch body, show backend in history

### CLI Detection & Version

Each backend uses `which <cli>` for detection and `<cli> --version` for version string — same pattern as `ClaudeBackend`.

### Backend Selection Logic

1. If `backend` field is provided in request, use that specific backend (error if unavailable)
2. If omitted, use first available backend (current behavior)

## Acceptance Criteria

- [ ] `codex_backend.rs` implements `AgentBackend` — spawns `codex exec` with `--full-auto --color never`
- [ ] `gemini_backend.rs` implements `AgentBackend` — spawns `gemini -p`
- [ ] `copilot_backend.rs` implements `AgentBackend` — spawns `copilot -p` with `--autopilot`
- [ ] `detect_backends()` checks all four CLIs
- [ ] `create_backend(name)` factory function returns correct backend by name
- [ ] `DispatchRequest` accepts optional `backend` field
- [ ] `dispatch_agent()` uses requested backend (or first available)
- [ ] UI shows backend selector dropdown (only available backends)
- [ ] UI sends `backend` field in dispatch POST
- [ ] History/detail shows which backend was used
- [ ] All existing tests pass (no regressions)
