# TODO

## Bugs

- [ ] **F1. Connect bridge drops long-running MCP/SSE requests after 300s** — `reqwest` timeout applies to the whole bridged request; `forward_sse_stream()` returns `Ok(())` on stream error instead of a JSON-RPC error, so long tool calls can silently hang (`src/connect.rs:268`, `src/connect.rs:540-543`)
- [ ] **F3. `daemon_stop()` removes PID file even if the daemon is still alive** — timed-out shutdowns (>5s) unconditionally call `remove_pid_file()`, leaving a live daemon invisible to `is_daemon_running()` and enabling duplicate daemon spawns (`src/daemon.rs:334-349`)
- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)

## Security

- [ ] **F2. Windows `launch_terminal` shells unquoted project paths** — `cmd /c start cmd /k cd /d <path>` with metacharacters (`&`, `|`) in valid Windows paths can cause command injection (`src/api/projects.rs:597-613`)
- [ ] **CORS + unauthenticated destructive endpoints** — `launch_editor`/`launch_terminal` execute system commands with no auth; localhost-only but any local process can trigger via CORS (`src/server.rs`, `src/api/projects.rs`)

## Tech Debt

- [ ] **Refactor `src/api/projects.rs` (616 lines)** — exceeds 500-line implementation limit
- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas

## Enhancements

- [ ] **Server-side federated multi-project search** — dashboard "All projects" currently merges raw per-workspace scores client-side; scores aren't comparable across indices (`ui/src/views/Search.vue:239-241`)
- [ ] **Load existing `.julie` workspaces immediately on registration** — projects with existing `.julie/` dir get `Stale` status, forcing reindex; could load existing index immediately then refresh in background. Note: `Stale` is intentional to catch file changes while daemon was down.
- [ ] **Windows Python launcher `py -3.12` / `py -3.13` probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax, which is the standard way to request a specific version on Windows (`src/embeddings/sidecar_bootstrap.rs:197-208`)

## Ideas (Unscoped)

- [ ] Dashboard ↔ GitHub/DevOps repo integration — what can we build with project repo access?
- [ ] Agent-triggered dashboard views — can the agent open the browser to a specific dashboard view as part of a tool call?
- [ ] Visual code intelligence — leverage JS graphing/diagram libs to surface code intelligence visually
- [ ] GitHub Pages showcase for dashboard functionality
- [x] Add OpenCode MCP config example to README.md:
  ```json
  {
    "$schema": "https://opencode.ai/config.json",
    "mcp": {
      "julie": {
        "type": "remote",
        "url": "http://localhost:7890/mcp",
        "enabled": true
      }
    }
  }
  ```
