# Julie Projection Freshness Verification Ledger

Record one row per verification run. Every column is required. Reuse evidence only when the required scope label and current HEAD SHA both match.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Isolated worktree starts from a green Julie nano baseline | `CARGO_TARGET_DIR=/Users/murphy/source/julie/target cargo xtask test nano` | baseline-nano | `e564bc1f7f5a70678b193a9007acec5ceada0877` | pass: 2 buckets, 42.3s warm, 175.4s prebuild, 217.8s cold wall; known macOS object-version warning only | 2026-07-22T02:00:59Z | no |
