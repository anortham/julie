# Machine Service Phase 2 Verification Ledger

Plan: `docs/plans/2026-09-09-machine-service-phase2-plan.md`. Finding:
`docs/findings/2026-09-10-machine-service-phase2-gate.md`.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Dev tier after Task 9 (durable roots, complexity words, init lock deleted) | `cargo xtask test dev` | dev-tier | 7944312f | pass (29 buckets, 54 s warm) | 2026-09-10T01:20:00Z | no |
| System tier after the root-swap and boundary tripwire deletions | `cargo xtask test system` | branch-gate | e508994d | pass (5 buckets, 11.0 s) | 2026-09-10T01:48:00Z | no |
| Search-quality suite on the on-demand snapshot | `cargo xtask test bucket search-quality` | expensive-specialist | 4f82d55a (tree identical to the committed change) | pass (2 commands, 352 s: 40 s build + 312 s suite) | 2026-09-10T02:40:53Z | no |
| Full tier at the last code change before the doc comment fix | `cargo xtask test full` | branch-gate | 608750cf | pass (51 buckets, 470.5 s warm) | 2026-09-10T02:50:17Z | no |
| Dev tier at the final code commit | `cargo xtask test dev` | dev-tier | 46bb53da | pass (29 buckets, 57.9 s warm) | 2026-09-10T03:03:25Z | no |
| System tier at the final code commit | `cargo xtask test system` | branch-gate | 46bb53da | pass (5 buckets, 11.2 s) | 2026-09-10T03:03:37Z | no |
| Multi-process service lifecycle at the final code commit | `cargo xtask test bucket service-process` | branch-gate | 46bb53da | pass (3.1 s) | 2026-09-10T03:03:40Z | no |
| Full tier at the final code commit | `cargo xtask test full` | branch-gate | 46bb53da | pass (51 buckets, 471.3 s warm) | 2026-09-10T03:11:37Z | no |
| Fast tier wall time (report-only) | `cargo xtask test fast` | report-only | 608750cf | pass (2 buckets, 25.3 s warm) | 2026-09-10T02:53:42Z | no |
| Design section 4 words in added product code | `cargo xtask test bucket complexity-words` (`scripts/complexity-words.sh main`) | on-demand | 46bb53da | 15 hits, all ruled accepted in Task 9 (4 moved lines, 11 pre-existing names); bucket exits non-zero by design | 2026-09-10T03:02:00Z | no |
| Dependency audit | `cargo audit` | security-deps | 46bb53da | 4 vulnerabilities and 8 warnings, identical to `Cargo.lock` at main 8b07074b (crossbeam-epoch, h2, memmap2, lru, scc, anyhow, chacha20, rustls-pemfile, quick-xml, git2); none introduced by this branch | 2026-09-10T02:51:00Z | no |
| Secrets scan | `rg -n '(AKIA|BEGIN (RSA|OPENSSH) PRIVATE|api[_-]?key\s*=\s*"[A-Za-z0-9])'` over the diff | security-secrets | 76d64aff | pass (no matches) | 2026-09-10T01:30:00Z | no |
| Leaked test processes after the tiers | `ps -eo etimes,cmd \| grep -c '[m]ock-broker\|[j]ulie-embedding-host'` | branch-gate | 46bb53da | 0 from this worktree; 1 `julie-embedding-host` (24 h old) from the main checkout's `target/debug`, left alone | 2026-09-10T02:51:00Z | no |

## Reuse Rule

Reuse evidence only when the scope label matches and the commit SHA matches
the current HEAD exactly. Add a row with `Evidence Reused` set to `yes` when
you do.
