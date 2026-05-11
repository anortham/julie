# Julie tree-sitter "best-in-class" — Codex handoff

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

Phases 1–7 closed; Phase 8 release gates all green at HEAD `61a27e42`.

- **24 languages** emit `StructuredPendingRelationship` with `target.terminal_name` + `import_context` for cross-file calls/imports (Phase 4a). Each has a real fixture in `fixtures/extraction/<lang>/cross_file/` and a locking test in `crates/julie-extractors/src/tests/<lang>/cross_file_pending.rs`.
- **7 languages** have Recipe B no-wrong-edge locks (CSS, regex, Markdown, YAML, Razor, TOML, JSON). `capability_matrix_negative_cases_emit_no_wrong_edges` de-ignored.
- **Capability matrix** migrated from bare-string dead-doc evidence to typed `EvidenceRef` (Test / Fixture / Commit) with machine-checked resolver. 33 open gaps → 14 (12 exception + 2 still mislabeled open — see item 1 below).
- **Pillar 3 public API**: `extract_canonical`, `capability_snapshot()`, `EXTRACTION_CONTRACT_VERSION` exported from `crates/julie-extractors/src/lib.rs`. Verified by `tests/downstream_smoke.rs` which spawns a tempdir consumer crate, path-deps `julie-extractors`, builds and runs it end-to-end. Crate is honestly scoped as "path/git dep usable, not crates.io publishable" (four parser deps are git-only).
- **Release gate sweep** at HEAD `61a27e42`: `cargo xtask test full` 40/40 in 664s, `dev` 32/32 in 354s, `system` 6/6 in 86s, `dogfood` 2/2 in 225s, `extractors` 4/4 in 27s, `parser-upgrade` 2/2 in 1.6s, `cargo build --release` 3m10s, `cargo test --doc` pass, `downstream_smoke` 17.0s, `cargo doc` clean except 6 missing-docs warnings.
- **Real-world smoke evidence**: julie (rust) 38272 symbols, zod (typescript) 18536 symbols, flask (python) 4480 symbols. All pass.

## What's left, in priority order

### 1. Reclassify `lua.types` and `r.types` gaps (5 min)

`fixtures/extraction/capabilities.json` has two remaining `status: "open"` entries with intrinsic-N/A reasons ("dynamically typed, no static type system"). They should be `status: "exception"`. After the edit, re-run `cargo xtask certify tree-sitter --check`. The cert report's "Open Capability Gaps" section will then be empty.

### 2. Phase 6 release-profile real-world regen (~2 hr human curation + 30 min wall)

Smoke profile only covers 3 repos. The rubric §3 target is ~21 repos with per-repo `representative_specs`. Corpus list: `fixtures/extraction/tree-sitter-real-world-corpus.toml`. Add specs naming required symbols/relationships per repo, then:

```bash
cargo xtask certify tree-sitter --real-world --profile release --out docs/LANGUAGE_REAL_WORLD_EVIDENCE.json
cargo xtask certify tree-sitter --check
```

Append a Verification Ledger row to the plan.

### 3. Phase 5.4 doc-comment audit (~30 min)

`cargo doc -p julie-extractors --no-deps` emits 6 missing-docs warnings on existing public surface. New items already have docs. Walk the re-exports in `crates/julie-extractors/src/lib.rs`, add `///` comments on the 6 undocumented items, verify warnings go to 0, append a ledger row.

### 4. Cert report stale section cleanup (10 min)

`docs/LANGUAGE_CERTIFICATION_REPORT.md` has a "Historical Coverage Delta" section that now lists all 36 registry rows as "missing from the restored historical matrix" — because the historical matrix was deleted. Pure noise. Generator lives in `xtask/src/certify/tree_sitter/report.rs`. Either remove the section or rewrite it to confirm the historical matrix is intentionally deprecated.

### 5. Live MCP dogfood (user-driven)

Five `_TBD_` ledger rows. Sequence:

```bash
cargo build --release
# user restarts Claude Code so the MCP client respawns the server
```

Then in the new session:
- `manage_workspace health` — expect READY + nonzero counts
- `call_path extract_symbols_static extract_canonical` — expect one-hop edge
- `fast_refs extract_canonical` — expect public-API + real-world callers
- SQLite: read the engine-version row (verify column name against `src/database/schema.rs`); value must contain `2026-05-10.tree-sitter-best-in-class-v1`
- `manage_workspace refresh workspace_id=julie_<id>` — expect "already up-to-date" (no full reindex)

Append rows to both `docs/TREE_SITTER_QUALITY_BAR.md` and the plan's Verification Ledger.

### 6. Merge worktree → main

54 commits on `best-in-class-treesitter`, base `c0def8f6`. Push, merge to `main`. **Favor merge commit over rebase** — the verification ledger cites commit SHAs and rebasing invalidates them.

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
- 57 commits ahead of `main`, 0 merged. Run `git -C .worktrees/best-in-class-treesitter log --oneline main..HEAD` for the full list. The most recent commit is this handoff doc; the substantive work tip is `27cdf65c docs: record dev/system/dogfood/full tier evidence at 61a27e42`.
- 0 open escalations.
- v7.8.5 already released (`julie@b478f8bf` + tag `v7.8.5`), but it only covers the plugin-hook scope fix. The tree-sitter program work is **not yet in a release tag** — the next julie release after this work merges should be v7.9.0 (substantive behavior changes to extractor output shape via `EXTRACTION_CONTRACT_VERSION`).
