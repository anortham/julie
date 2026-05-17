# Daemon Fragility — Log Archaeology Report

**Author:** subagent (log-archaeology), 2026-05-17
**Scope:** `~/.julie/daemon.log.2026-05-{10..17}`
**Method:** Read-only log analysis + source code at `src/daemon/mcp_session.rs:378-468` (state machine confirmation only, no code changes).

---

## Timeline: 2026-05-17 14:00-14:30 (the worst incident)

The 10:13:02 daemon (v7.9.3) entered `restart_pending` no later than 12:53:54 (first `AcceptWithRestartPending` log at `mcp_session.rs:432`). It then went silent on the HTTP admission path until manually killed at ~14:24-14:25:

| Timestamp                | Source                       | Event                                                                 | Key fields                                                       |
|--------------------------|------------------------------|-----------------------------------------------------------------------|------------------------------------------------------------------|
| 2026-05-17T13:38:44.365  | `watcher::runtime:690`       | Last watcher activity before the bad window                          | normal background work                                           |
| 2026-05-17T14:01:44.994  | `mcp_session.rs:459`         | **Rejecting HTTP session while daemon waits to restart**             | `reason=StaleBinary first_request=false adapter_version=<none>`  |
| 2026-05-17T14:01:46.000  | `mcp_session.rs:459`         | Reject (retry)                                                       | `first_request=false` (+1.0s)                                    |
| 2026-05-17T14:01:48.005  | `mcp_session.rs:459`         | Reject (retry)                                                       | `first_request=false` (+2.0s)                                    |
| 2026-05-17T14:01:52.009  | `mcp_session.rs:459`         | Reject (retry)                                                       | `first_request=false` (+4.0s)                                    |
| 2026-05-17T14:02:00.013  | `mcp_session.rs:459`         | Reject (retry)                                                       | `first_request=false` (+8.0s) — backoff series ends              |
| 2026-05-17T14:03:10.654-.776 | `mcp_session.rs:459`     | **3 parallel adapter chains kick in** (rejections clustered ~60ms)    | `first_request=false`                                            |
| 14:03:11.657-.788        | `mcp_session.rs:459`         | 3 rejects (+1s on each chain)                                        | `first_request=false`                                            |
| 14:03:13.662-.792        | `mcp_session.rs:459`         | 3 rejects (+2s)                                                      | `first_request=false`                                            |
| 14:03:17.666-.796        | `mcp_session.rs:459`         | 3 rejects (+4s)                                                      | `first_request=false`                                            |
| 14:03:25.672-.800        | `mcp_session.rs:459`         | 3 rejects (+8s); all chains hit backoff ceiling                      | `first_request=false`                                            |
| 14:19:40.30x             | `watcher::handlers:374-430`  | Watcher event processed (daemon process is alive, background ok)     | filewatcher continues fine                                       |
| 14:22:54.723             | `mcp_session.rs:459`         | Reject (new burst)                                                   | `first_request=false`                                            |
| 14:22:55, 14:22:57, 14:23:01, 14:23:09 | `mcp_session.rs:459` | Reject (1, 2, 4, 8s backoff)                                       | `first_request=false`                                            |
| 14:23:24.742-14:23:39.754 | `mcp_session.rs:459`        | 5 more rejections (multi-chain, ceiling reached)                     | `first_request=false`                                            |
| **14:23:55.754**         | `mcp_session.rs:459`         | **Last rejection from 10:13 daemon**                                 | `first_request=false`                                            |
| **(silence 14:23:55 → 14:25:58 = ~123s)** |               | **Old daemon process exits without writing any shutdown log**       | —                                                                |
| 2026-05-17T14:25:58.757  | `daemon::cli:126`            | **New daemon starts: v7.9.3**                                        | —                                                                |
| 14:25:58.769             | `daemon::app:159`            | Binary mtime captured                                                | —                                                                |
| 14:25:58.769             | `daemon::app:181`            | "Recovery markers from previous daemon run detected" `count=1`       | likely stale marker from 09:23:48 force-abort, not new           |
| 14:25:58.949             | `mcp_session.rs:197`         | New session accepted (hermes-agent workspace)                        | normal recovery                                                  |

**`Last HTTP session disconnected and restart is pending` (`mcp_session.rs:410`) → 0 occurrences in this window. 0 occurrences on the entire 2026-05-17 log.**
**`Rejecting HTTP session and triggering daemon restart` (`mcp_session.rs:445`) → 0 occurrences in this window. 0 occurrences on the entire 2026-05-17 log.**

---

## Recovery analysis

The 10:13 daemon died at some point between **14:23:55** (last rejection log) and **14:25:58** (new daemon startup), a ~123s gap. **There is no graceful shutdown evidence:**

- No `Draining active sessions` log (`app/handle.rs:89`)
- No `Session drain timed out; recovery marker written` (`shutdown.rs:131`)
- No `Daemon shutting down` (`app/handle.rs:122`)
- No `Daemon stopped` (`app/handle.rs:169`)
- No panic backtrace
- No `Last HTTP session disconnected and restart is pending. Triggering restart.` (`mcp_session.rs:410`)

The new daemon's `Recovery markers ... count=1` is almost certainly the leftover marker file from the **09:23:48** drain timeout (the only logged force-abort of the day) — the marker file persists until cleared via the dashboard, and `count=1` shows up at every startup that day (09:24:33, 10:13:02, 14:25:58, 16:53:01).

**Most likely cause of the 10:13 daemon's death: external SIGKILL** (either the user issued `kill -9`, or a `dev-restart --force` couldn't rebuild while the process held the binary lock and the user manually killed it). The watcher was still running normally at 14:19:40, so the daemon process wasn't deadlocked end-to-end — only the HTTP admission gate was stuck. The kill closed the gap by destroying the process; the auto-restart only worked because something external removed the stuck process.

---

## H1 verdict: **CONFIRMED with one refinement**

H1 as stated ("rejected init requests may not be decrementing active_sessions, so count never hits 0, so notify_restart never fires") is essentially correct, **but the more precise statement is:**

> `mcp_session.rs:459` (the `RejectForRestart` action path) is the only admission outcome the daemon ever takes once it enters `restart_pending`. It calls `mark_restart_pending` (idempotent) but **never calls `notify_restart()`**. The two paths that DO call `notify_restart()` are:
>   1. **`mcp_session.rs:410-411`** — fires only when an *existing* HTTP session disconnects AND `remaining == 0`.
>   2. **`mcp_session.rs:445-452`** — `ShutdownForRestart` action (rejects current request AND triggers restart).
>
> **In 8 days of logs, path 2 has fired 0 times. Path 1 has fired 7 times in 8 days while path 459 has fired 687 times.** Recovery via the disconnect handler is statistically rare; rejected init requests do not increment `active_sessions`, so they cannot pull the count down to 0, and they take no other action that ends the daemon.

**Cross-day evidence table:**

| Day        | starts | L432 accept+pending | L459 reject+wait | L445 reject+trigger | L410 last-disconnect (only path that recovers from rejections) |
|------------|--------|---------------------|------------------|---------------------|-----------------------------------------------------------------|
| 2026-05-10 | 30     | 0                   | **637**          | **0**               | 3                                                               |
| 2026-05-11 | 12     | 1                   | 17               | **0**               | 3                                                               |
| 2026-05-12 | 1      | 0                   | 0                | 0                   | 0                                                               |
| 2026-05-13 | 2      | 0                   | 0                | **0**               | 1                                                               |
| 2026-05-14 | 0      | 0                   | 0                | 0                   | 0                                                               |
| 2026-05-15 | 0      | 0                   | 0                | 0                   | 0                                                               |
| 2026-05-16 | 0      | 0                   | 0                | 0                   | 0                                                               |
| 2026-05-17 | 7      | 3                   | **33**           | **0**               | **0**                                                           |

**Interpretation:**
- `L445` (the deterministic recovery trigger) is dead code in production. Never fires.
- `L410` (disconnect-driven recovery) is non-deterministic; fires only when an already-accepted session happens to send an explicit close. Adapter retries on rejected sessions never produce an L410.
- `L459` (the broken path) is the dominant admission outcome by ~98:1 ratio on a busy day (05-10: 637 vs 3).
- On 2026-05-17 specifically, L459 fired 33 times and L410/L445 fired 0 times — pure deadlock, recovery only via external kill.

So:
- **H1 confirmed.** active_sessions doesn't go to 0; notify_restart is never called by any path the daemon actually exercises during the rejection storm.
- **H2 (partial confirm).** The 60s drain timeout is irrelevant — drain never starts because notify_restart never fires. The "60s timeout" in the brief is academic; the daemon would sit forever.
- **H3 confirmed.** The adapter retries indefinitely with exponential backoff (1s → 2s → 4s → 8s ceiling per chain) and never kills the daemon process. On 2026-05-17 there were at least 3 parallel adapter chains all stuck in the same loop. The `adapter_version="<none>"` field suggests the adapter doesn't even identify itself, let alone implement a respawn-on-restart-required policy.

---

## Session accounting

The `active_sessions` counter (visible in `mcp_session.rs:432` warnings) reached the following values during the 10:13:02 daemon's lifetime:

- 12:53:54 — `active_sessions=1, first_request=true` (an init request accepted while restart_pending — this is the "transient" accept path; it sets the lifecycle flag and lets the session continue). **This is when active_sessions last became visible as a non-zero number, and it never gets logged decrementing.**

No disconnect event ever logs `Last HTTP session disconnected and restart is pending` in the 10:13→14:25 daemon lifetime, which means **the disconnect handler at `mcp_session.rs:378-412` never observed `remaining == 0` while `restart_pending() == true`.** Either:
- The accepted session at 12:53:54 (and the earlier one at 10:10:47 that did the same thing) never invoked the rmcp disconnect callback path, OR
- It disconnected before restart_pending was set and so didn't trigger L410, OR
- More sessions accumulated and at least one was always alive.

Without per-session telemetry I cannot say which. But the **net is the same: active_sessions never hit 0 + restart_pending in the same observation, so L410 never fires.**

---

## Adapter retry pattern

Two distinct burst patterns on 2026-05-17:

**Single-chain bursts** (14:01:44 → 14:02:00 and 14:22:54 → 14:23:09):
- Inter-request gaps: 1.0s → 2.0s → 4.0s → 8.0s
- Classic exponential backoff with cap ~8s
- All `first_request=false`

**Multi-chain bursts** (14:03:10 → 14:03:25 and 14:23:24 → 14:23:55):
- 3 simultaneous rejections within ~150ms (e.g. `14:03:10.654 / .712 / .776`)
- Each chain independently backs off 1→2→4→8s (clear "trio" pattern at every step)
- All `first_request=false`
- **3 parallel adapter chains running at the same time** — likely 3 separate MCP client sessions (Claude Code + Codex CLI + OpenCode per the dev-workflow notes in CLAUDE.md, all routed at `target/release/julie-server` and all bridging to the daemon).

**`first_request=false` on every single rejection** is significant: it means the lifecycle's `first_transition` flag was already consumed by a prior `mark_restart_pending` call. The first time restart_pending was flipped, it was flipped silently (no L432, L445, or L459 log for first_request=true at that moment) — the flip happened during a transient that succeeded. From then on, every subsequent admission decision saw the flag already set.

**The adapter has no kill-and-respawn path on `restart_required_error`. It just retries the HTTP POST forever.**

---

## Surprises / additional findings

1. **L445 (`ShutdownForRestart` — the documented "deterministic recovery" path) has fired 0 times across all 8 daemon logs.** Either the gate function (`stale_binary_accept_action`) never returns `ShutdownForRestart`, or it does but never under stale-binary conditions. This path is the "right" recovery — reject the current request AND shut down the daemon — and it never executes. Worth grepping `stale_binary_accept_action` to see when it's supposed to return `ShutdownForRestart`.

2. **The daemon process itself is fine during the rejection storm.** Filewatcher events at 14:19:40 (deletion + re-extraction of a `.tmp.32859` swap file) processed normally in the middle of the 22-minute outage. Bulk operations, WAL checkpoints, runtime batches — all healthy. **Only the HTTP admission gate is broken.** This is important: the user's edits are still being indexed, they just can't ASK Julie anything.

3. **The "recovery markers count=1" log at every startup is misleading.** It carries over from 09:23:48 and persists at count=1 indefinitely. Each new daemon reports it but no daemon clears it. This is noise, not signal — don't chase it as evidence of recent force-abort.

4. **`adapter_version="<none>"` field is suspicious.** It suggests the rmcp adapter (Claude Code's MCP client) does not send a version header the daemon can read. Without that, any version-skew based gate logic in `version_gate_action` can't differentiate between adapter and daemon versions. Worth verifying whether the adapter SHOULD be sending a version.

5. **Three accepted-while-pending events on 2026-05-17** (09:16:44, 10:10:47, 12:53:54) — all `first_request=true`. Each of these "succeeded" in letting one MCP client through, but each contributed to the active_sessions count that prevents recovery. The fact that they're spaced ~1-2 hours apart suggests the user was getting brief windows of usability between rebuilds.

6. **The 22-minute outage at 14:01-14:23 was preceded by an earlier 47-minute silent rejection window at 13:14:45** (single rejection log) — the gate was already set at 13:14, and the adapter eventually gave up retrying for ~47 minutes before resuming at 14:01:44. This suggests the user wasn't actively using the daemon between 13:14 and 14:01 — when they tried to use it at 14:01, the retry storm began.

---

## Bottom line for the fix design

H1 confirmed. The fix needs to:

1. **Make `RejectForRestart` (line 459) call `notify_restart()` when the gate has been stuck for too long, OR**
2. **Make the gate emit `ShutdownForRestart` (line 445) instead of `RejectForRestart` after N consecutive rejections / T seconds in restart_pending, OR**
3. **Add a periodic "if restart_pending and stuck for >T, force shutdown" watchdog in the daemon main loop (independent of HTTP traffic).**

Don't rely on the disconnect handler — adapter retries don't disconnect, they just keep posting.

The adapter side (H3) is a complementary fix: it should kill the local daemon process on receiving `restart_required_error` 2-3 times in a row, not retry forever. But fixing the daemon side is sufficient on its own.
