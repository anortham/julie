# Deployment Story Verification Ledger

Plan: `docs/plans/2026-09-11-deployment-story-plan.md`. Reuse a row only when the scope label matches and the commit SHA equals the current HEAD.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Batch A (tasks 1 to 5) leaves the whole workspace green: 2208 passed, 13 ignored CLI tests passed | `cargo xtask test dev` | dev | 622c616d | pass (39.2 s wall) | 2026-09-11T21:02:00Z | no |
| Plugin tests green after the manifest and hook change: 20 pass, 0 fail | `node --test --test-reporter=tap hooks/` (julie-plugin) | plugin-tests | bbd2c53 (julie-plugin) | pass | 2026-09-11T20:56:45Z | no (worker run, Task 5 report) |
| Packed Linux archive runs through `run.cjs`: sidecar beside the binary, embedding child ready on poll 1, `fast_search` 6 hits, service stopped | Task 6 steps (see `.razorback` report and the finding) | live | 622c616d (julie), bbd2c53 (julie-plugin) | pass, except `initialize` lacked `instructions` (fixed below) | 2026-09-11T21:06:00Z | no |
| Service `initialize` carries the agent instructions | `cargo nextest run --lib initialize_over_http_carries_the_agent_instructions` | worker-red-green | working tree on 622c616d | RED then pass | 2026-09-11T21:12:00Z | no |
