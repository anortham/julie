# Rustfmt Normalization Verification

## Contract

- Hard gate: the pinned Rust 1.97.0 formatter reports a clean repository.
- Hard gate: the mechanical source commit contains exactly the plan's 85 Rust paths and no manual or non-Rust changes.
- Hard gate: compile, toolchain-contract, affected-change escalation, dev, warning-free release build, and full verification pass.
- Report only: formatting line churn and command durations.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Pre-normalization formatter baseline is red on the exact 85-file manifest | `cargo fmt --check` plus unique-path extraction | worker-exact-red | `c3f54d74596817ba420fc4097a5ab25356254db9` | expected fail: exit 1; 85 unique Rust files match plan Appendix A | 2026-07-22T19:18:31Z | no |
| Approved-plan baseline remains red on the exact 85-file manifest | `cargo fmt --check` plus manifest comparison | lead-working-tree-red | `441836f06fe4ba63926ff4a14647e3518b1b1d15` | expected fail: exit 1; exact 85-file match | 2026-07-22T19:27:08Z | no |
| One canonical formatter command produces a clean, Rust-only diff | `cargo fmt`; `cargo fmt --check`; `git diff --check`; exact manifest and extension checks | lead-working-tree-green | `441836f06fe4ba63926ff4a14647e3518b1b1d15` plus working tree | pass: 85 Rust files; 304 insertions and 268 deletions; no non-Rust paths | 2026-07-22T19:28:02Z | no |
| Mechanical diff preserves interfaces and behavior | token-level diff audit plus Julie impact and public-symbol inspection | lead-working-tree-semantic-audit | `441836f06fe4ba63926ff4a14647e3518b1b1d15` plus working tree | pass: layout and deterministic ordering only; inspected public signatures and callers unchanged | 2026-07-22T19:31:44Z | no |
| Diff-scoped regression union passes before the mechanical commit | `cargo xtask test changed` | affected-working-tree | `441836f06fe4ba63926ff4a14647e3518b1b1d15` plus working tree | pass: 39 buckets; 1075.4s warm; 68.7s prebuild; 1144.1s cold wall | 2026-07-22T19:44:41Z | no |
