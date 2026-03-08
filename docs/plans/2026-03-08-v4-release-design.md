# Julie v4.0 Release Design

## Overview

Polish and harden the platform features from Phases 1-5 into a shippable v4.0 release. Four workstreams: Julie plugin (skills + hooks + adoption layer), OpenAPI documentation, worktree/parallel validation, and filewatcher decision.

## Workstream 1: Julie Plugin (Skills + Hooks + Adoption Layer)

### Problem

Julie has goldfish-equivalent memory tools (checkpoint, recall, plan) but lacks the **adoption layer** — the skills, hooks, and server instructions that make agents use the tools correctly without being told. Goldfish achieved this with 5 skills, 3 hooks, and detailed server instructions. Julie currently has none.

### Design

Ship a **lightweight Claude Code plugin** that contains only the adoption layer. The Julie binary is installed separately (GitHub release, `cargo install`, or build from source).

#### Plugin Structure

```
julie-plugin/
├── .claude-plugin/
│   └── plugin.json              # name: "julie", version, author, etc.
├── .mcp.json                    # Connects to daemon over HTTP
├── hooks/
│   └── hooks.json               # 3 hooks (SessionStart, PreCompact, ExitPlanMode)
├── skills/
│   ├── checkpoint/SKILL.md      # When/how to save memory checkpoints
│   ├── recall/SKILL.md          # Context restoration patterns
│   ├── plan/SKILL.md            # Plan creation and lifecycle
│   ├── plan-status/SKILL.md     # Progress assessment against plans
│   └── standup/SKILL.md         # Cross-project status reports
└── README.md                    # Installation: binary + plugin
```

#### .mcp.json

Default connection to daemon mode:

```json
{
  "mcpServers": {
    "julie": {
      "type": "http",
      "url": "http://localhost:3141/mcp"
    }
  }
}
```

User prerequisite: `julie daemon start` running. Daemon auto-start deferred to 4.1.

#### Hooks (3 — automatic behavior)

| Hook | Event | Action |
|---|---|---|
| Auto-recall | `SessionStart` (matcher: `startup\|clear`) | Call `recall()` with defaults to restore context |
| Pre-compact save | `PreCompact` | Call `checkpoint()` to preserve state before compaction |
| Plan persistence | `PostToolUse` (matcher: `ExitPlanMode`) | Call `plan(save)` to persist approved plans |

Hook types are all `"prompt"` — they instruct the agent to call the Julie MCP tools.

#### Skills (5 — user/agent-invocable)

**checkpoint** — Save developer context to Julie memory at meaningful milestones. Guidance on when to checkpoint (completed deliverables, key decisions, non-obvious discoveries) and when NOT to (every small edit, routine test passes, rapid-fire). Structured markdown descriptions with WHAT/WHY/HOW/IMPACT.

**recall** — Restore context from previous sessions. Default recall for session start, targeted queries with search/date filters, cross-project recall for standups.

**plan** — Create and manage persistent plans. Mandatory save after ExitPlanMode. Plan lifecycle: save, activate, update, complete. Plans survive context compaction and guide multi-session work.

**plan-status** — Assess progress against active plan using checkpoints. Cross-reference plan tasks against recent work. Report what's done, what's next, and whether the project is drifting.

**standup** — Generate cross-project status reports. Recall across all workspaces, review active plans, produce concise standup format (yesterday, today, blockers).

#### Server Instructions

Julie's MCP tool descriptions need enhancement to match goldfish's detailed guidance. Current descriptions are minimal struct doc comments. Update to include:

- **checkpoint tool**: When to/when not to checkpoint, markdown format requirements, type classification guidance, structured field descriptions
- **recall tool**: Parameter examples, trust recalled context, cross-project patterns
- **plan tool**: Urgency of saving after ExitPlanMode, plan vs checkpoint distinction, activation requirement

These are code changes in `src/tools/memory/checkpoint.rs`, `recall.rs`, and `plan.rs` — updating the `JsonSchema` derive descriptions and/or the tool handler's description strings.

### Distribution

- **Binary**: GitHub releases (macOS/Linux/Windows via existing CI workflow), `cargo install julie`, or build from source
- **Plugin**: Separate git repo (`julie-plugin`), distributed via Claude Code marketplace or `--plugin-dir`
- **Update independence**: Plugin versions and binary versions update independently

### What's deferred

- New skill opportunities leveraging Tantivy search + embeddings (4.1)
- Daemon auto-start on first connection (4.1)
- Monolithic plugin with bundled binary (not planned — two-piece approach is cleaner)

---

## Workstream 2: OpenAPI Documentation

### Problem

The HTTP API (8 modules: agents, common, dashboard, health, memories, projects, search, search_unified) has no formal documentation. Third-party tools cannot integrate without reading source code.

### Design

Generate an OpenAPI 3.0 spec from the axum routes. Options:

1. **utoipa** crate — derive macros on handler functions, auto-generates OpenAPI spec at compile time
2. **aide** crate — axum-native OpenAPI generation with route introspection
3. **Manual spec** — hand-written OpenAPI YAML

Recommendation: **utoipa** — it's the most mature Rust OpenAPI crate, works well with axum and serde, and keeps the spec in sync with the code via derive macros.

### Deliverables

- OpenAPI 3.0 JSON/YAML spec served at `/api/openapi.json`
- Swagger UI or Scalar docs served at `/api/docs` (optional but nice)
- All 8 API modules annotated with utoipa derives
- Request/response schemas derived from existing serde structs

---

## Workstream 3: Worktree & Parallel Validation

### Problem

Julie has not been validated in parallel/concurrent scenarios that are common in real development workflows.

### Test Scenarios

#### A. Multiple git worktrees, same repo
- Two worktrees of the same repo (e.g., `julie/` and `julie-feat/`) registered as separate projects
- Verify: separate workspace IDs, separate indexes, no cross-contamination
- Verify: file watchers for both work independently
- Verify: search results scoped correctly to each workspace

#### B. Concurrent MCP sessions to daemon
- Two Claude Code windows connected to the same daemon simultaneously
- Verify: concurrent search requests don't corrupt state
- Verify: concurrent indexing requests are safe (lock contention, write conflicts)
- Verify: memory operations (checkpoint, recall) are isolated per-project

#### C. Agent worktrees (razorback EnterWorktree)
- Agent spawns in a temporary worktree via razorback
- Verify: Julie either indexes the worktree as a separate workspace or gracefully ignores it
- Verify: no orphaned indexes/watchers after worktree cleanup
- Document expected behavior for users

#### D. Daemon lifecycle edge cases
- Daemon restart while MCP sessions are connected
- Project registration/deregistration while watchers are active
- Multiple `julie daemon start` attempts (already handled?)

### Deliverables

- Test plan executed manually against running daemon
- Bugs filed and fixed
- Any behavioral decisions documented

---

## Workstream 4: Filewatcher (Decision: Keep Current Behavior)

### Decision

Keep current behavior: daemon watches all ready projects on startup. OS-native watchers (FSEvents, ReadDirectoryChangesW, inotify) have negligible idle cost on macOS and Windows. Linux inotify watch limits could be an issue with many large projects but is an edge case.

### 4.0 Actions

- Document the behavior in the API docs / README
- Optionally: show active watcher count in dashboard stats

### Deferred to 4.1

- "Watch only projects with active MCP sessions" optimization
- Dashboard staleness indicator with manual re-index trigger

---

## 4.1 Backlog (from TODO triage)

Items triaged out of 4.0:

2. Project stats/insights in dashboard view
3. Advanced memory features (link memories to code/commits) leveraging Tantivy + embeddings
4. Dashboard integration with GitHub/DevOps
5. Auto project registration on startup
6. Agent opening browser to dashboard views
7. Visual code intelligence with JS graph libraries
8. Filewatcher "watch only active" optimization
11. GitHub Pages showcase for dashboard
12. Project view quick-access buttons (copy path, launch agent, open in VS Code)
13. Cross-tool token optimization approach

---

## Acceptance Criteria

### Plugin
- [ ] Plugin directory structure created with plugin.json manifest
- [ ] .mcp.json connects to daemon at localhost:3141
- [ ] 3 hooks implemented (SessionStart, PreCompact, ExitPlanMode)
- [ ] 5 skills implemented (checkpoint, recall, plan, plan-status, standup)
- [ ] Julie MCP tool descriptions enhanced with detailed usage guidance
- [ ] Plugin loads correctly via `claude --plugin-dir`
- [ ] All hooks fire correctly in manual testing
- [ ] All skills invoke correctly and produce expected tool calls

### OpenAPI
- [ ] utoipa (or chosen crate) integrated into build
- [ ] All 8 API modules annotated
- [ ] OpenAPI spec served at /api/openapi.json
- [ ] Spec validates against OpenAPI 3.0 standard
- [ ] All request/response schemas present

### Worktree/Parallel Validation
- [ ] Multiple worktrees tested — separate indexes, no cross-contamination
- [ ] Concurrent MCP sessions tested — no corruption or deadlocks
- [ ] Agent worktrees tested — behavior documented
- [ ] Daemon lifecycle edge cases tested
- [ ] All discovered bugs fixed

### Filewatcher
- [ ] Current behavior documented
- [ ] Dashboard shows active watcher count (optional)
