# Julie Control Plane Lifecycle Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:executing-plans to implement this plan task-by-task.

**Goal:** Make daemon startup, adapter handoff, IPC transport, session lifecycle, and stale-binary replacement deterministic across Unix and Windows.

**Architecture:** Move lifecycle authority out of scattered retry logic and into explicit control-plane state transitions. Keep platform transport narrow, lifecycle decisions centralized, and dashboard visibility tied to the same state machine used by the runtime.

**Tech Stack:** Rust, Tokio, named pipes and Unix sockets, Axum dashboard, rmcp

---

**Execution rules:** Use @razorback:test-driven-development and @razorback:verification-before-completion on each task. Start with the daemon and adapter tests that pin the current failures, then move code toward explicit state transitions and transport boundaries.

### Task 1: Extract The Lifecycle State Machine

**Files:**
- Modify: `src/daemon/lifecycle.rs:11-97`
- Modify: `src/daemon/mod.rs:296-911`
- Modify: `src/daemon/ipc_session.rs:24-61`
- Test: `src/tests/daemon/lifecycle.rs`
- Test: `src/tests/daemon/state.rs`
- Test: `src/tests/integration/daemon_lifecycle.rs`

**What to build:** Expand `src/daemon/lifecycle.rs` from stop-status helpers into the canonical lifecycle state machine for daemon startup, ready, draining, restart-required, and shutdown transitions. Move decision logic out of `run_daemon`, `accept_loop`, and the version gate helper until those functions mostly orchestrate transitions rather than invent them.

**Approach:** Keep the first pass focused on state and transition helpers, plus thin integration points in `src/daemon/mod.rs`. Preserve current behavior while pulling the decisions into named states that tests can assert on.

**Acceptance criteria:**
- [ ] lifecycle states are defined in one place and reused by daemon startup and shutdown paths
- [ ] `run_daemon` and `accept_loop` delegate restart and drain decisions to lifecycle helpers
- [ ] version-gate tests assert lifecycle outcomes, not free-form side effects
- [ ] daemon lifecycle tests cover ready, restart-required, and draining behavior

### Task 2: Introduce A Narrow Platform Transport Contract

**Files:**
- Create: `src/daemon/transport.rs`
- Modify: `src/daemon/ipc.rs:8-236`
- Modify: `src/adapter/launcher.rs:19-365`
- Test: `src/tests/daemon/ipc.rs`
- Test: `src/tests/daemon/paths.rs`
- Test: `src/tests/integration/daemon_lifecycle.rs`

**What to build:** Isolate Unix socket and Windows named-pipe behavior behind a transport contract that exposes readiness probing, bind semantics, and connect semantics without leaking platform detail into the lifecycle state machine.

**Approach:** Keep `src/daemon/ipc.rs` as the low-level transport implementation, but move the lifecycle-facing contract into `src/daemon/transport.rs`. Update `DaemonLauncher` to depend on the contract rather than bespoke pipe and socket checks.

**Acceptance criteria:**
- [ ] control-plane code can reason about transport readiness without platform branches in lifecycle logic
- [ ] Windows named-pipe probing and Unix socket probing share the same contract-level outcomes
- [ ] adapter launcher uses the transport contract for readiness and wait loops
- [ ] transport tests cover both successful probe and restart boundary conditions

### Task 3: Unify Adapter Retry, Version Gate, And Restart Handoff

**Files:**
- Modify: `src/adapter/mod.rs:42-101`
- Modify: `src/adapter/launcher.rs:72-328`
- Modify: `src/daemon/ipc_session.rs:80-408`
- Modify: `src/daemon/mod.rs:699-911`
- Test: `src/tests/daemon/ipc_session.rs`
- Test: `src/tests/daemon/handler.rs`
- Test: `src/tests/integration/daemon_lifecycle.rs`

**What to build:** Replace sleep-driven restart handoff logic with lifecycle-driven retry behavior. Adapter connection failure, immediate disconnect after handshake, version mismatch, and stale-binary replacement should all move through one restart-required path with explicit reasons.

**Approach:** Keep the existing version-gate helper pure, then route its outcomes through lifecycle and transport helpers rather than flag flips in multiple files. Make the adapter retry because the lifecycle says restart is pending, not because two seconds felt lucky.

**Acceptance criteria:**
- [ ] adapter retry behavior follows explicit lifecycle outcomes
- [ ] version mismatch, stale binary, and transport replacement use one restart-required path
- [ ] accept-loop and adapter tests cover immediate restart handoff without timing folklore
- [ ] Windows and Unix share the same contract-level restart behavior

### Task 4: Surface Lifecycle State Through The Dashboard And Session Layer

**Files:**
- Modify: `src/dashboard/state.rs:41-164`
- Modify: `src/dashboard/routes/status.rs:11-87`
- Modify: `src/daemon/session.rs`
- Modify: `src/handler.rs:643-667`
- Test: `src/tests/dashboard/integration.rs`
- Test: `src/tests/daemon/session.rs`
- Test: `src/tests/daemon/session_workspace.rs`

**What to build:** Expose lifecycle state, restart-required state, and session binding state through the dashboard and the session layer so primary-workspace rebinding and daemon replacement stop being invisible side effects.

**Approach:** Reuse the shared health vocabulary from the program plan. Keep session-binding changes tied to the same lifecycle names used by the daemon and adapter so the dashboard is reporting runtime truth, not parallel guesses.

**Acceptance criteria:**
- [ ] dashboard shows daemon lifecycle state and restart-required state
- [ ] session state distinguishes connecting, bound, serving, and closing
- [ ] request-time primary binding paths use lifecycle-aware session state
- [ ] session and dashboard tests pin the new lifecycle fields

