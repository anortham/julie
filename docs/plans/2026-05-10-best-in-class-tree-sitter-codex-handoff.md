# Julie tree-sitter "best-in-class" — Codex handoff

## Codex follow-up status — 2026-05-11

Codex completed the stale items in this handoff through offline release gates
and daemon-mode live dogfood:

- `lua.types` and `r.types` are now `exception`; `capabilities.json` has 0 open
  gaps.
- Phase 6 release-profile evidence was regenerated with 22 verified repos,
  including VB.NET `samples`, 0 skipped repos, and 0 hard failures.
- `fixtures/extraction/tree-sitter-real-world-corpus.toml` now has 110
  representative specs enforced by `xtask`.
- Phase 5.4 rustdoc cleanup is done; `cargo doc -p julie-extractors --no-deps`
  emits no warnings.
- The stale historical-matrix section now reports that historical evidence is
  deprecated instead of listing every registry row as missing.
- Offline release gates passed; see
  `docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md` and
  `docs/TREE_SITTER_QUALITY_BAR.md` ledgers.
- A startup repair health regression was fixed at `88998e69`.
- Daemon-mode live dogfood passed at `88998e69`: health READY, one-hop
  `call_path`, `fast_refs`, semantic engine-version SQLite row, and
  already-up-to-date refresh.

Still pending:

- Merge PR #20 to `main`. Do not rebase; ledger SHAs matter.

Use `docs/plans/2026-05-10-best-in-class-tree-sitter-handoff.md` as the current
handoff for the direct-connector caveat and integration step. Direct Codex
`mcp__julie__` still returns `Transport closed`; daemon HTTP transport is
verified, and the in-process connector is tracked as a separate transport
concern rather than an extractor/data-plane blocker.

## What this is

Julie is a Rust-based code-intelligence MCP server with 34 tree-sitter language extractors (36 registry rows counting TSX/JSX). A multi-day autonomous run drove the "best-in-class tree-sitter" program from spec to near-merge. The work lives on branch `best-in-class-treesitter` in a worktree.

## Where to start

```bash
cd /Users/murphy/source/julie/.worktrees/best-in-class-treesitter
git log --oneline main..HEAD | head -20
```

Read these in order. They are the source of truth — do **not** rederive from commit messages or memory:

1. `docs/TREE_SITTER_QUALITY_BAR.md` — fixed-target rubric. The bar never moves down.
2. `docs/plans/2026-05-10-best-in-class-tree-sitter-design.md` — approved design.
3. `docs/plans/2026-05-10-best-in-class-tree-sitter-rubric.md` — grading criteria.
4. `docs/plans/2026-05-10-best-in-class-tree-sitter-plan.md` — executable plan. **The Verification Ledger at the bottom is the audit trail** — every gate run is there with commit SHA + timestamp + scope label.
5. `docs/plans/2026-05-10-best-in-class-tree-sitter-handoff.md` — what the autonomous run handed back.
6. `docs/EXTRACTION_CONTRACT.md` — downstream-facing contract for the `julie-extractors` crate.
7. `fixtures/extraction/capabilities.json` — machine-checked capability matrix. Single source of truth for per-language extraction.
8. `CLAUDE.md` / `AGENTS.md` — project conventions (TDD, narrow tests, Julie tool rules, file-size limits, language-agnostic design). Kept in sync by a pre-commit hook.

## What's done

Phases 1–8 closed for offline gates; daemon-mode live dogfood is recorded at
evidence commit `88998e69`.

- **24 languages** emit `StructuredPendingRelationship` with `target.terminal_name` + `import_context` for cross-file calls/imports (Phase 4a). Each has a real fixture in `fixtures/extraction/<lang>/cross_file/` and a locking test in `crates/julie-extractors/src/tests/<lang>/cross_file_pending.rs`.
- **7 languages** have Recipe B no-wrong-edge locks (CSS, regex, Markdown, YAML, Razor, TOML, JSON). `capability_matrix_negative_cases_emit_no_wrong_edges` de-ignored.
- **Capability matrix** migrated from bare-string dead-doc evidence to typed `EvidenceRef` (Test / Fixture / Commit) with machine-checked resolver. Open gaps are 0 after `lua.types` and `r.types` were reclassified as intrinsic-N/A exceptions.
- **Pillar 3 public API**: `extract_canonical`, `capability_snapshot()`, `EXTRACTION_CONTRACT_VERSION` exported from `crates/julie-extractors/src/lib.rs`. Verified by `tests/downstream_smoke.rs` which spawns a tempdir consumer crate, path-deps `julie-extractors`, builds and runs it end-to-end. Crate is honestly scoped as "path/git dep usable, not crates.io publishable" (four parser deps are git-only).
- **Release gate sweep**: `cargo xtask test full`, `dev`, `system`, `dogfood`, `extractors`, `parser-upgrade`, `cargo build --release`, examples, doctest, downstream smoke, package list, and rustdoc all have ledger rows. The current post-health-fix system gate passed at `88998e69`.
- **Real-world release evidence**: 22 verified repos, 110 representative specs, 0 skipped repos, 0 hard failures.
- **Daemon-mode live dogfood**: health READY/FULLY READY, call path, refs, semantic engine-version SQLite row, and no-op refresh all passed at `88998e69`.

## What's left, in priority order

### 1. Merge PR #20

PR: https://github.com/anortham/julie/pull/20

Use a merge commit. Do not rebase; the verification ledger cites commit SHAs.

### 2. Track direct Codex connector separately if needed

The running daemon data plane is verified through `julie-server tool ...`, but
Codex's hosted `mcp__julie__` connector still returns `Transport closed`.

## Conventions that override default behavior

- **TDD**: red test first → narrow test name → minimal green → regression. Bug fixes always start with a failing reproducer test. Full protocol in `CLAUDE.md`.
- **Test runners**: `cargo nextest run --lib <exact_name>` is the default while iterating. `cargo xtask test changed` after a localized batch. `cargo xtask test dev` once per coherent batch, not per edit. Tier table is in `CLAUDE.md`. The `pretool-broad-tests` enforcement hook was removed in v7.8.5; you self-enforce.
- **File size**: implementation ≤ 500 lines, test files ≤ 1000 lines.
- **Language-agnostic**: any path-based heuristic must work for Rust / Python / C# / Java / Go layouts. Never check `path.starts_with("src/")` or look for `Cargo.toml`.
- **Julie tools**: this project dogfoods Julie. Prefer `fast_search` / `get_symbols` / `deep_dive` / `fast_refs` / `edit_file` over grep / find / Read+Edit chains. The daemon at HEAD already has this workspace indexed.

## Verification ledger contract

Every gate command records `invariant | command | scope label | commit SHA | result | timestamp`. **Reuse a row only when its commit SHA matches HEAD exactly.** If HEAD differs, rerun. Template: `docs/plans/verification-ledger-template.md`.

## Escalation budget

- 3 failed iterations OR 90 min wall-clock on a single task → write `docs/plans/escalations/2026-05-10-<task-id>.md`, continue with other tasks.
- 5+ open escalations OR `cargo xtask test full` regression that survives gap closure → hard stop, write summary, wait for user.

## Run state right now

- Branch: `best-in-class-treesitter` in worktree at `.worktrees/best-in-class-treesitter/`. Base commit: `c0def8f6`.
- Not merged to `main`. Run `git -C .worktrees/best-in-class-treesitter log --oneline main..HEAD` for the current commit list. Evidence rows include code commits through `88998e69`; later docs commits record that evidence.
- 0 open escalations.
- v7.8.5 already released (`julie@b478f8bf` + tag `v7.8.5`), but it only covers the plugin-hook scope fix. The tree-sitter program work is **not yet in a release tag** — the next julie release after this work merges should be v7.9.0 (substantive behavior changes to extractor output shape via `EXTRACTION_CONTRACT_VERSION`).
