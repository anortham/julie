---
id: julie-daemon-restart-listener-fix-awaiting-live-va
title: Julie Daemon Restart Listener Fix — Awaiting Live Validation Before v7.10.0
status: active
created: 2026-05-17T22:56:29.320Z
updated: 2026-05-17T22:56:29.320Z
tags:
  - daemon
  - bug-fix
  - v7.10.0
  - blocker
  - live-validation
---

## Julie Daemon Restart Listener Fix — Awaiting Live Validation Before v7.10.0

### Status
- Current release: **v7.9.3** (the prior brief incorrectly referenced v7.8.0).
- Daemon fragility blocker: **fix implemented and gate-passed**, awaiting user live validation before v7.10.0.

### What was fixed (HEAD = `1bd973c0` after the 5 fix commits + 2 docs commits)

The Julie MCP daemon entered `restart_pending=true` after binary rebuild and rejected every new HTTP session indefinitely (22-min outages observed, 637 daily rejections at peak). Recovery required external SIGKILL.

Root cause: `restart_notify: Arc<Notify>` at `src/daemon/lifecycle.rs:152` had **no `.notified()` consumer in src/**. The notification channel was dead infrastructure — `notify_restart()` calls fired into a void.

Fix shipped across:
- `219ce80e` T2: `spawn_restart_bridge` listener task in `DaemonApp::serve`
- `6a0ead78` T1: `mark_restart_pending` now signals on first transition; redundant calls deleted
- `2fee56aa`, `c1e0abf5`: cleanup
- `781884b7` T3: integration test for active-session bounded recovery + `DaemonConfig::current_binary_mtime` testing seam
- `240ea0d0`: plan doc + verification ledger

Worst-case user-visible outage: indefinite → ~95-100s bounded.

### Active blocker

Live validation by user (Task 7). Procedure documented in `docs/plans/2026-05-17-daemon-restart-listener-fix.md` section 6.
Key observable: new log line `Restart channel signaled; triggering daemon shutdown via stop_notify` (from `src/daemon/app/helpers.rs`) — this confirms the new path fires when binary mtime advances during an active session.

### Once live validation passes

- Tag `v7.10.0`
- Per `feedback_release_approval.md`: get explicit user approval before tagging
- Per `feedback_no_human_hour_estimates.md`: do not quote times for the release process

### Risks to watch in validation
- 60s drain may cut off long-running tool calls (>60s). Acceptable per design; revisit only on real evidence.
- Bridge task is fire-and-forget — fires once, then exits. If a future refactor needs multi-shot, that's a follow-up.
- The daemon-split rewrite (`docs/plans/2026-05-15-daemon-split-and-search-reranker-design.md`) plans to delete `DaemonLifecycleController` entirely; this fix is a minimal interim.

### Team workflow that worked (for future similar bugs)
1. Two-investigator pattern (logs + code) caught the divergence early — log-archaeologist confirmed H1 from data, code-cartographer overturned H1 with cleaner root cause.
2. Codex adversarial review on the FIX DESIGN (not just the diff) — caught the symmetric-vs-asymmetric trade-off before implementation.
3. Cartographer inline review at every task — caught real Task 3 logic errors that would have shipped a non-functional test.
4. Fresh-implementer-per-task discipline with TDD — clean RED→GREEN evidence, justified deviations.
