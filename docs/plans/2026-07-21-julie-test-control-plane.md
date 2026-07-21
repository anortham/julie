# Julie Test Control Plane Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make lean `cargo xtask` cheap to invoke and enforce a real ≤60s-declared `fast` / scale boundary with honest cold timing and coverage-safe `changed` behavior.

**Architecture:** Split product-dependent eval into `xtask-eval` (+ Cargo alias). Keep path→bucket maps; add declared-budget gate (`OverBudget` / `--scale`). Enforce `fast` ≤60 in `TestManifest::validate`. Account prebuild wall time. Recalibrate bucket `expected_seconds` from warm measurements.

**Tech Stack:** Rust workspace, Cargo aliases, TOML test manifest, cargo-nextest via xtask runner.

**Architecture Quality:** Approved shape from `docs/plans/2026-07-21-julie-test-control-plane-design.md` — lean `xtask` vs `xtask-eval`; `ChangedSelectionMode::OverBudget`; production `validate()` budget checks; shared bucket-plan resolver including `system-health`. Architecture risk: **high**.

## Global Constraints

- Declared `fast` budget: **60** seconds sum of resolvable `expected_seconds`.
- Over-budget `changed`: mode **`OverBudget`**, non-zero exit, **no** bare `dev` replacement; `--scale` runs `unique(mapped ∪ dev)`.
- Lean `xtask` must not depend on root `julie` (nor eval-only deps once unused).
- `.cargo/config.toml` must define `xtask-eval = "run -q -p xtask-eval --"`.
- Spec: `docs/plans/2026-07-21-julie-test-control-plane-design.md` (approved).
- TDD: failing test first for behavior changes; workers run exact test names only.
- Commit mode: `serial-worker-commit` unless lead says otherwise.
- Worktree: `/Users/murphy/source/julie/.worktrees/test-control-plane` on `feat/test-control-plane`.

## Verification Strategy

**Project source of truth:** `AGENTS.md` / `CLAUDE.md` xtask tiers; design acceptance checklist.

**Worker red/green scope:** `cargo nextest run -p xtask -- <exact_test>` or `cargo nextest run -p julie-core -- <exact_test>` / `cargo nextest run -p xtask-eval -- <exact_test>` as owned.

**Worker ceiling:** Exact named tests only (≤2 runs per fix). No `xtask test changed/dev`.

**Worker gate invariant:** Assigned test proves the task’s behavior contract.

**Lead affected-change scope:** `cargo xtask test changed` from worktree after coherent batches; `cargo check -p xtask` and `cargo check -p xtask-eval`.

**Branch gate:** `cargo xtask test fast` (once defined) + `cargo nextest run -p xtask` + `cargo nextest run -p xtask-eval` before handoff.

**Escalation triggers:** Runner timing / prebuild changes → also run a warm `fast` timing capture for the ledger.

**Verification ledger:** Use `docs/plans/verification-ledger-template.md`; record warm p50/p95 for recalibrated buckets.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 0: Fix migration-029 schema assertion | Batch A | `crates/julie-core/src/tests/database/migrations.rs` | No | None - safe parallel batch. |
| Task 1: Scaffold `xtask-eval` + alias + move modules | None - serial after A optional | `xtask-eval/**`, `xtask/src/{search_*,main,cli,lib}.rs`, `xtask/Cargo.toml`, `Cargo.toml`, `.cargo/config.toml`, moved tests | Yes | Establishes package boundary others depend on. |
| Task 2: Manifest `fast` + `validate()` budgets | Batch B | `xtask/src/manifest.rs`, `xtask/test_tiers.toml`, contract expected fixtures | Yes | Needs Task 1 if touching expected CLI contracts; else can follow Task 1. |
| Task 3: Bucket resolver + `system-health` in manifest | Batch B | `xtask/src/runner.rs` (special bucket), `xtask/src/changed.rs` (pricing), `xtask/test_tiers.toml` | Yes | Shares manifest/runner with Task 2 — serialize after Task 2. |
| Task 4: `OverBudget` + `--scale` | Batch C | `xtask/src/changed.rs`, `xtask/src/cli.rs`, `xtask/src/main.rs`, `xtask/tests/changed_tests.rs` | Yes | Needs resolver from Task 3. |
| Task 5: Prebuild timing in summary | Batch C | `xtask/src/runner.rs`, runner tests | No w.r.t. Task 4 if files disjoint; prefer after Task 3 | Prefer serial after Task 3 to avoid runner conflicts. |
| Task 6: Recalibrate `expected_seconds` + define `fast` | None - serial | `xtask/test_tiers.toml`, ledger notes | Yes | Needs green Task 0 + warm measurements after split. |
| Task 7: Docs + dep allowlist acceptance | None - serial | `CLAUDE.md`, `AGENTS.md`, `docs/TESTING_GUIDE.md`, allowlist test | Yes | Last — documents final CLI. |

**Commit mode:** `serial-worker-commit`

**TDD:** Write failing test → RED → minimal impl → GREEN. Approach notes only; use Miller for current code.

---

### Task 0: Fix migration-029 final schema assertion

**Files:**
- Modify: `crates/julie-core/src/tests/database/migrations.rs` (`test_migration_029_adds_extractor_enrichment_tables` ~694–709)
- Test: same file

**Interfaces:**
- Consumes: `SymbolDatabase::new` applies migrations through `LATEST_SCHEMA_VERSION` (30)
- Produces: Test asserts enrichment tables exist and final version is `LATEST_SCHEMA_VERSION` (or explicitly applies only through 29 if a mid-chain API exists — prefer assert final = latest + tables present)

**Approach:** Opening a v28 DB runs all migrations → version 30. Asserting `29` is stale. Fix assertion to `LATEST_SCHEMA_VERSION` while keeping table-existence checks for 029’s tables.

**Verify:** `cargo nextest run -p julie-core -- test_migration_029_adds_extractor_enrichment_tables`

---

### Task 1: Scaffold `xtask-eval` and peel product dep from `xtask`

**Files:**
- Create: `xtask-eval/Cargo.toml`, `xtask-eval/src/main.rs`, moved modules (`search_matrix*`, `search_ablation`, CLI bits for eval/search-matrix)
- Modify: root `Cargo.toml` members; `xtask/Cargo.toml` (drop `julie` + unused deps); `xtask/src/{lib,main,cli}.rs`; `.cargo/config.toml` alias; move `xtask/tests/search_matrix_contract_tests.rs` → `xtask-eval/tests/…`
- Modify: `xtask/test_tiers.toml` — add `xtask-eval` bucket; include in `full`
- Modify: `xtask/src/changed.rs` — route `xtask-eval/**` like `xtask/`
- Update: in-code docs still showing `cargo xtask search-matrix`

**Interfaces:**
- Produces: `cargo xtask-eval search-matrix|eval …`; lean `cargo xtask` migration error for old subcommands
- Alias: `xtask-eval = "run -q -p xtask-eval --"`

**Approach:** Move modules with minimal API churn. Keep shared helpers in lean xtask only if eval-free. Add nextest bucket `cargo nextest run -p xtask-eval`.

**Verify:**
- `cargo tree -p xtask -e normal --depth 1` has no `julie`
- `cargo check -p xtask` and `cargo check -p xtask-eval`
- `cargo nextest run -p xtask-eval -- <moved contract test>`
- `cargo xtask search-matrix` → clear migration error (manual/CLI parse test)

---

### Task 2: Add `fast` tier and production `validate()` budget checks

**Files:**
- Modify: `xtask/src/manifest.rs` (`validate` ~57+)
- Modify: `xtask/test_tiers.toml` `[tiers]` — add `fast` (membership may be provisional until Task 6; must satisfy ≤60 or Task 2 uses a temporary minimal set that validates, Task 6 expands with evidence)
- Modify: `xtask/tests/support/manifest_contract_expected.rs` and related contract tests

**Interfaces:**
- Produces: `validate` hard-fails if `fast` missing or `sum(expected) > 60`; also enforce `dev` ≤600 in `validate` (lift from cfg(test)-only assertion)

**Approach:** TDD with `TestManifest::from_str` fixtures: missing/valid/over-budget `fast`.

**Verify:** `cargo nextest run -p xtask -- manifest_tests_` (exact new test names)

---

### Task 3: Shared bucket-plan resolver + `system-health` in manifest

**Files:**
- Modify: `xtask/src/runner.rs` (SPECIAL_BUCKET / `system-health` ~102–109)
- Modify: `xtask/test_tiers.toml` — promote `system-health` into `[buckets]`
- Modify: `xtask/src/changed.rs` — price selections via resolver; remove special-case that treats missing manifest entry as free

**Interfaces:**
- Produces: `fn bucket_plan(manifest, name) -> Option<&BucketPlan>` (or equivalent) used by runner + changed budget sum

**Verify:** unit test that a selection including `system-health` uses its declared 30s in the sum; `cargo nextest run -p xtask -- <exact>`

---

### Task 4: `changed` OverBudget + `--scale`

**Files:**
- Modify: `xtask/src/changed.rs` — add `ChangedSelectionMode::OverBudget`; budget post-filter after mapping
- Modify: `xtask/src/cli.rs` / `main.rs` — parse `--scale`; on OverBudget print + non-zero exit without running; on `--scale` run `unique(mapped ∪ dev)`
- Test: `xtask/tests/changed_tests.rs`

**Interfaces:**
- Consumes: resolver from Task 3; FAST_BUDGET_SECS = 60
- Produces: modes `Buckets | OverBudget | FallbackToDev (existing shared-infra path) | NoChanges`

**Approach:** Keep existing `FallbackToDev` for unmapped/shared-infra paths. Budget gate only applies when mapped buckets exist and declared sum > 60.

**Verify:** under-budget → Buckets; dogfood-only path → OverBudget (not silent drop); `--scale` → contains dogfood ∪ dev; exact nextest filters

---

### Task 5: Account prebuild time in run summary

**Files:**
- Modify: `xtask/src/runner.rs` — `prebuild_test_binary` / `run_named_buckets` (~312–470)
- Test: runner unit tests with fake executor

**Interfaces:**
- Produces: summary includes `prebuild_elapsed`; total cold wall = prebuild + bucket times
- Docs note: declared budget ≠ cold wall

**Approach:** v1 keep `cargo nextest run --no-run --lib` if package-scoped prebuild is too large; still **measure and print** it. Optional follow-up: derive `-p` set from selected bucket commands.

**Verify:** fake executor test asserts prebuild duration appears in rendered summary

---

### Task 6: Warm measurement + recalibrate `fast` membership

**Files:**
- Modify: `xtask/test_tiers.toml` — set `expected_seconds` from warm p50/p95; ensure `nano ⊆ fast` and sum ≤60; split `core-fast` if needed
- Create/update: verification ledger row under `docs/plans/` or memory with measurement table

**Approach:** After Task 0 green, warm packages, run candidate buckets 3×, record p50/p95, set expected ≈ p95 + headroom. One cold run documented separately.

**Verify:** `TestManifest::load` succeeds; `cargo xtask test fast` completes; ledger updated

---

### Task 7: Docs + lean dep allowlist test

**Files:**
- Modify: `CLAUDE.md`, `AGENTS.md`, `docs/TESTING_GUIDE.md`
- Test: xtask unit/integration asserting dep allowlist or `cargo tree` wrapper in `xtask` tests / docs contract

**Approach:** Document `fast`, warm vs cold, `OverBudget`, `changed --scale`, `cargo xtask-eval`.

**Verify:** docs contract tests if present; `cargo nextest run -p xtask -- <allowlist test>`

---

## Out of scope

- Full rewrite of path→bucket maps
- Semantic routing / sidecar bakeoff
- Claiming cold `fast` ≤60 without package-scoped prebuild

## Ready for execution

Plan is light for same-session `subagent-driven-development`. Design already committed on `feat/test-control-plane` (`432108e0`).
