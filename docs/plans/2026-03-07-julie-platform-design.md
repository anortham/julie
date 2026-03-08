# Julie Platform: Personal Developer Intelligence Service

## Context

Julie is currently a per-session MCP server that provides code intelligence for a single project (plus manually-added reference workspaces). Goldfish is a separate Claude Code plugin providing developer memory (checkpoints, recall, plans). Both die when the session ends, operate independently, and are effectively locked to Claude Code.

This design transforms Julie from a per-session tool into a **persistent daemon** — a personal developer intelligence platform that knows all your code, remembers your work, serves any client, and can dispatch agents to act on its knowledge.

**Why now:** Without the daemon pivot, Julie remains a good code search tool. With it, Julie becomes infrastructure — a compounding intelligence layer that gets smarter over time. The daemon is the foundation; without it, the cross-project, memory, UI, and agent dispatch capabilities aren't possible.

---

## Vision

Julie runs as a persistent background service on your machine. It indexes all your projects, integrates developer memory, exposes a web management UI, serves any MCP client via HTTP, and can dispatch AI agents with full context assembled from its knowledge base.

```
Julie Daemon (always running)
  |
  +-- HTTP Server (single port)
  |     +-- /mcp        -> MCP Streamable HTTP (agents connect here)
  |     +-- /api/...    -> REST API (management, search, memories)
  |     +-- /ui/...     -> Web UI (dashboard, search playground, timeline)
  |
  +-- Indexing Engine
  |     +-- Per-project Tantivy indexes (federated, not shared)
  |     +-- Per-project SQLite databases
  |     +-- Cross-project file watchers (notify crate)
  |     +-- Background embedding pipeline (Python sidecar)
  |
  +-- Memory Module (Goldfish successor)
  |     +-- Per-project .memories/ (git-tracked, shareable)
  |     +-- User-level ~/.julie/memories/ (personal, cross-project)
  |     +-- Tantivy-indexed + optionally embedded
  |
  +-- Agent Dispatch
        +-- Context assembly from indexes + memories
        +-- Pluggable backends: claude CLI, codex CLI, API keys, Ollama
        +-- Results stored as memories (feedback loop)
```

---

## Key Architectural Decisions

### 1. Per-Project Indexes (Federated, Not Shared)

Each project keeps its own Tantivy index + SQLite database at `<project>/.julie/indexes/{workspace_id}/` (unchanged from current layout). A global registry at `~/.julie/registry.toml` tracks all known projects. Cross-project search queries all relevant indexes in parallel and merges with RRF.

**Rationale:** 95% of queries are single-project. A shared index would add filtering overhead to every query. Per-project indexes preserve isolation (add/remove projects without touching others), enable independent lifecycle, match the current architecture, and keep index data on the same drive as the source code (important for multi-drive setups like Windows with source on D:).

### 2. MCP over Streamable HTTP

Replace stdio transport with MCP Streamable HTTP. Clients connect to `http://localhost:PORT/mcp`. Same MCP protocol, different wire format. Most modern MCP clients (Claude Code, Cursor, etc.) already support this.

**Rationale:** Stdio requires the client to spawn Julie as a subprocess — incompatible with a daemon model. Streamable HTTP lets the daemon serve multiple concurrent MCP clients on the same port that hosts the web UI.

### 3. Dual-Level Memory Storage

- **Per-project:** `<project>/.memories/` — git-tracked, shareable. New team members clone the repo and get institutional knowledge.
- **User-level:** `~/.julie/memories/` — personal cross-project learnings, standup data, career decisions.

**Rationale:** Project memories should travel with the code (they explain *why* the code is the way it is). Personal memories should stay private.

### 4. Single Embedding Model, Multiple Content Types

Keep BGE-small (or upgrade to a general-purpose model) for all content types: code symbols, memories, documentation. One model = one embedding space = unified cross-content search.

**Rationale:** Julie embeds symbol metadata (names, signatures, doc comments), not raw code. These are semi-natural-language — a text model handles them well. Memories and docs are natural language. A shared embedding space enables cross-content queries ("find code related to this memory").

### 5. Dynamic Embedding Dimensions

Store model dimension in config, create vector tables dynamically. Enable model experimentation without schema migration.

**Rationale:** Learned from Miller's LanceDB approach. Hardcoding 384D in sqlite-vec schema locks you to one model. Dynamic dimensions unblock experimentation.

### 6. Cross-Platform Daemon (No Platform-Specific Core)

The daemon is a background process (`julie daemon start`), not a platform-specific service. Auto-start is optional and platform-specific (launchd/systemd/Task Scheduler), isolated from core logic.

- macOS: `~/.julie/`, launchd plist for auto-start
- Linux: `~/.julie/`, systemd unit for auto-start
- Windows: `%APPDATA%\julie\`, Startup folder or Task Scheduler for auto-start

**Rationale:** Julie is cross-platform. Core daemon logic must have zero platform-specific code. Platform specifics are isolated to optional auto-start installers.

### 7. Agent Dispatch via CLI Backends

Agent dispatch shells out to user-authenticated CLI tools (`claude -p`, `codex -q`, etc.) rather than managing API auth directly. API keys supported as an alternative. Local models via Ollama.

**Rationale:** CLI tools handle their own auth (OAuth for subscriptions). Julie doesn't need to store credentials or manage tokens. Auto-detection (`which claude`) enables zero-config setup.

---

## Roadmap

### Phase 1: Daemon + HTTP Foundation

The big pivot. Julie becomes a persistent service with an HTTP interface.

**Deliverables:**
- Daemon lifecycle: `julie daemon start|stop|status|logs`
- HTTP server (axum): management API + static web UI
- MCP Streamable HTTP transport (replacing stdio for daemon mode)
- Global project registry at `~/.julie/registry.toml`
- Multi-project awareness: register, discover, list projects
- Background indexing of registered projects (per-project indexes)
- Cross-project file watchers via `notify` crate
- Basic web UI: project list, index status, health dashboard
- Cross-platform path resolution (Unix vs Windows home dirs)
- Stdio MCP still supported for backward compatibility (non-daemon mode)

**Architecture:**
- axum HTTP server with tower middleware
- tokio runtime for async I/O
- Existing Tantivy + SQLite per workspace (no changes to index format)
- PID file at `~/.julie/daemon.pid` for lifecycle management
- Configurable port (default 7890, env: `JULIE_PORT`)

### Phase 2: Cross-Project Intelligence

Unlock the value of knowing all your projects.

**Deliverables:**
- Federated search: `fast_search(workspace="all")` queries all indexes in parallel, RRF merge
- Cross-project references: `fast_refs(workspace="all")`
- Cross-project deep_dive and get_context
- Auto-reference discovery: analyze dependency manifests (Cargo.toml, package.json, go.mod, etc.), suggest related local projects
- Search results tagged with source project
- Web UI: cross-project search interface, project relationship graph

### Phase 3: Memory Integration

Bring Goldfish's functionality into Julie as a native module.

**Deliverables:**
- Checkpoint, recall, plan tools — Rust implementation guided by Goldfish's design
- Per-project `.memories/` storage (git-tracked, markdown + YAML frontmatter)
- User-level `~/.julie/memories/` storage (personal, cross-project)
- Memory indexing in Tantivy (full-text searchable)
- Memory embedding (optional, same model as code symbols)
- Memory-enriched `get_context`: surface relevant memories alongside code results
- Cross-project recall: aggregate memories across all projects
- Web UI: memory timeline, plan viewer, checkpoint browser
- Fuzzy search over memories (port fuse.js-style matching or use Tantivy)

### Phase 4: Management UI + Agent Dispatch

The command center.

**Deliverables:**
- Full web dashboard: project dependency graph (visualized), memory timeline, search playground with scoring debug, configuration management
- Agent backend configuration: register CLI tools, API keys, local models
- Agent dispatch from UI: assemble context from indexes + memories, construct prompt, shell out to agent, stream results
- Feedback loop: agent results stored as memory checkpoints
- Auto-detection of available agent backends (`which claude`, `which codex`, etc.)
- Event notifications: breaking changes detected, stale indexes, etc.

### Phase 5: Search Enhancement

Polish the search experience across content types.

**Deliverables:**
- Dynamic embedding dimensions (support model swapping)
- Embed memories + documentation alongside code symbols
- Weighted RRF per tool: fast_search weights code higher, recall weights memories higher, get_context balances both
- Search playground in UI: test queries, see scoring breakdown, compare hybrid vs keyword-only
- Content type filters in search: code / memories / docs / all

---

## Constraints

- **File size limit**: No implementation file over 500 lines (existing project standard)
- **Cross-platform**: macOS, Linux, Windows. No platform-specific code in core.
- **Language-agnostic**: All features must work for any programming language/project layout.
- **Backward compatible**: Stdio MCP mode still works for non-daemon usage.
- **TDD**: All phases follow test-driven development (existing project standard).
- **Existing test suite**: ~265s full suite must continue passing throughout.

---

## Resolved Decisions

1. **Web UI framework**: **Vue + PrimeVue**, built with Vite, `dist/` output embedded in binary via `include_dir` or `rust-embed`, served as static assets by axum. Chosen over Leptos (younger ecosystem, no PrimeVue equivalent) because the developer already knows Vue and PrimeVue provides the exact components needed (DataTable, Timeline, Tree, Charts).

2. **Daemon auto-start**: **Manual only for Phase 1.** Just `julie daemon start|stop|status`. Auto-start scripts (launchd/systemd/Task Scheduler) are a later polish item.

3. **Port conflict handling**: **Fail with clear error.** "Port 7890 in use. Set JULIE_PORT or pass --port to use a different port." Simple, predictable, no magic.

4. **Index storage**: **Keep per-project `<project>/.julie/` unchanged.** Add global registry at `~/.julie/registry.toml` to track known projects. Indexes stay on the same drive as source code (important for multi-drive setups). The registry is lightweight metadata only.

5. **Goldfish migration path**: **Full compatibility.** Julie's Rust memory module reads and writes the exact same format (markdown + YAML frontmatter, date-based directories). Drop-in replacement, no migration needed.

6. **Embedding sidecar lifecycle**: **One sidecar for the entire daemon, on-demand with idle timeout.** Starts on first embedding request from any project, serves all projects, shuts down after idle. Eliminates the current problem of multiple sessions spawning multiple sidecars.
