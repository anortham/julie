# Machine Service Phase 3 Verification Ledger

HEAD at last recorded row: see table. Reuse only when SHA matches current HEAD exactly.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Durable roots are facts.sqlite + tantivy/ | `cargo nextest run --lib tests::service::durable_roots` | worker-ceiling | ca8b7149 | pass | 2026-09-10T17:00:00Z | no |
| `cargo xtask test dev` | `cargo xtask test dev` | lead-dev | ec3c16cd | pass (30 buckets, 57.5s warm) | 2026-09-10T17:45:00Z | no |
| `cargo xtask test system` | `cargo xtask test system` | lead-system | ec3c16cd | pass (4 buckets, 7.0s warm) | 2026-09-10T17:43:00Z | no |
| `service-process` under 20 s | `cargo xtask test bucket service-process` (via `dev`) | lead-service-process | ec3c16cd | pass (3.1s warm) | 2026-09-10T17:45:00Z | no |
| `cargo xtask test dogfood` | `cargo xtask test dogfood` | lead-expensive-gate | bfb8b7f1 | pass (2 buckets, 14.0s warm) | 2026-09-10T18:05:00Z | no |
| `cargo xtask test full` | `cargo xtask test full` | lead-full | bfb8b7f1 | fail (`tools-search-format-quality`: empty `search_annotation_search_tests`) | 2026-09-10T18:10:00Z | no |
| `cargo xtask test full` after empty-filter drops | `cargo xtask test full` | lead-full | d77f0810 | pass, 147.2s warm (still included dogfood) | 2026-09-10T18:20:00Z | no |
| Fast unit-only under 10s | `cargo xtask test fast` ×3 | lead-fast | 255f9b59 | pass, median 3.3s warm | 2026-09-10T18:27:00Z | no |
| Trimmed full under 120s | `cargo xtask test full` ×3 | lead-full | 255f9b59 | pass, median 93.2s warm | 2026-09-10T18:30:00Z | no |
| Miller `/status` | temp `JULIE_HOME`; index `/home/murphy/source/miller`; `service status --json` | lead-status | 255f9b59 | pass. rss 939,204,608; graph 330,214,728 bytes; 220,334 symbols | 2026-09-10T18:28:00Z | no |
| Julie `/status` | temp `JULIE_HOME`; index machine-service worktree; `service status --json` | lead-status | 255f9b59 | pass. rss 257,634,304; graph 85,980,600 bytes; 56,230 symbols | 2026-09-10T18:30:00Z | no |
| `cargo xtask test dev` at finding commit | `cargo xtask test dev` | lead-dev | cdefbcd8 | pass, 30 buckets, 49.7s warm | 2026-09-10T18:32:00Z | no |
| Net lines vs `63cefbcf` | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` | lead-tokei | fd707d9e | 145,367 → 106,940 (net −38,427) | 2026-09-10T17:20:00Z | no |
| Complexity words | `sh scripts/complexity-words.sh main` | lead-complexity | ca8b7149 | SourceEditCoordinator only (ADR-0004) | 2026-09-10T17:00:00Z | no |
