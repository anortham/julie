# Julie Test Control Plane — Design

**Date:** 2026-07-21  
**Status:** Draft (revised after Codex review)  
**Slice:** Fast test control plane (Jul 20 improvement audit item #1)

## Purpose

Make Julie’s test orchestration cheap to invoke and enforce a real **fast / scale** boundary so agents get a trustworthy local gate without cold-linking the product server or silently expanding into a multi-minute suite.

## Constraints

- Peel product-dependent eval harnesses into a sibling package; lean `xtask` must not depend on the root `julie` crate (nor eval-only deps).
- Declared **fast** budget: **60 seconds** sum of resolvable bucket `expected_seconds`. Soft wall-clock overrun warnings only (do not fail solely because a machine is slow).
- Keep existing path→bucket maps in `changed`; add a **budget gate** on top rather than rewriting `changed.rs`.
- Out of scope: semantic routing shadow mode, Rust sidecar bakeoff, handler decomposition, full deletion of path routing rules.

## Prerequisites (before timing calibration)

- Fix or isolate the current `nano` baseline failure: schema version is 30 while a migration unit test still asserts 29 after applying all migrations (`crates/julie-core` migrations tests). Do not use a red `nano` run as recalibration evidence.

## Success Criteria

1. Cold `cargo xtask test list` (and other lean xtask commands) do not compile/link the root `julie` package; `cargo tree -p xtask -e normal --depth 1` matches an allowlist without `julie`, `rusqlite`, `serde_json`, `tempfile`, or `tokio` (those move with eval unless lean xtask still needs them).
2. `cargo xtask test fast` exists; `TestManifest::validate` **hard-fails** if `fast` is missing or `sum(fast.expected_seconds) > 60`.
3. `cargo xtask test changed`:
   - under budget → run mapped buckets;
   - over budget → **`OverBudget`** selection: print mapped buckets + declared sum + next-step commands; **do not auto-run `dev`** (that can drop mapped coverage, e.g. dogfood). Optional `--scale` runs `unique(mapped ∪ dev)` and says so explicitly.
4. `cargo xtask-eval …` works via `.cargo/config.toml` alias (not merely a binary name).
5. Docs (`CLAUDE.md` / `AGENTS.md` / `TESTING_GUIDE.md`) describe `fast`, `OverBudget`, and the `xtask-eval` move.
6. Warm `fast` tier wall (bucket execution only, after a warm build) is demonstrated ≤ ~60s p50 on a maintainer machine; cold wall (including prebuild) is reported separately and may exceed 60s without falsifying the declared budget.

## Architecture Quality

**Affected modules:** `xtask` (runner, manifest, changed, cli), new `xtask-eval`, workspace `Cargo.toml`, `.cargo/config.toml`, `xtask/test_tiers.toml`, agent/testing docs.

**Caller-facing interface:**

- `cargo xtask test {list,nano,smoke,fast,changed,dev,…}` — no product link
- `cargo xtask test changed --scale` — explicit scale union when over budget
- `cargo xtask-eval {search-matrix,eval …}` — alias + package; keeps `julie` path dependency

**Depth/locality:** Package boundary split for eval; budget gate is a thin layer on selection/validation; prebuild accounting is an explicit runner contract change.

**Test surface:** Manifest `validate` tests (missing/valid/over-budget `fast`); changed under/over/`--scale` tests; dep allowlist test; `xtask-eval` bucket + nextest; alias smoke.

**Seams/adapters:** CLI migration error for old `cargo xtask search-matrix|eval`; Cargo alias for `xtask-eval`.

**Rejected shortcuts:** Cargo feature on a single xtask crate; rewriting all path→bucket rules; replacing over-budget mapped sets with bare `dev`.

**Architecture risk:** **high** (runner timing semantics, changed fallback coverage, workspace/CLI split).

## Current timing reality (2026-07-21)

| Set | Declared `expected_seconds` sum | Notes |
|-----|----------------------------------|--------|
| `nano` = `core-database` + `core-fast` | **65s** | Already over 60s declared |
| `smoke` | **80s** | |
| Proposed naive union (`nano ∪ smoke`) | **140s** | Unusable as `fast` without recalibration |
| `dev` | **589s** | Under 600s unit assertion only |

Individual buckets already >60s declared include indexing/editing/dogfood/search-quality/etc. A `changed` selection that maps to any of those is always over the fast budget.

Codex live evidence: a `nano` run took **~76s real** while the summary reported **~29s** because `prebuild_test_binary` (`cargo nextest run --no-run --lib`, 600s timeout) runs **before** `total_elapsed` starts and always builds the root `--lib` graph.

## Design

### 1. Package split

- Add workspace member `xtask-eval/` with binary `xtask-eval`.
- Move into it: `search_matrix`, `search_matrix_mine`, `search_matrix_report`, `search_ablation`, related CLI parsing, `julie` path dep, and product-dependent tests (e.g. `search_matrix_contract_tests`).
- Add `.cargo/config.toml` alias: `xtask-eval = "run -q -p xtask-eval --"`.
- Lean `xtask` keeps: test runner, manifest, changed, inventory, sync-plugin, dev-link / dev-restart.
- Strip eval-only normal deps from lean `xtask` once unused (`rusqlite`, `serde_json`, `tempfile`, `tokio` as applicable).
- Add bucket `xtask-eval` (`cargo nextest run -p xtask-eval`), route `xtask-eval/**` in `changed`, include in `full` (and any scale-oriented tiers that should cover tooling).
- Invoking deprecated `cargo xtask search-matrix|eval` returns a clear error pointing at `cargo xtask-eval …`.
- Update in-code doc comments that still show old commands (e.g. `search_matrix_report`).

### 2. Tiers, budgets, and runner timing

#### 2a. Declared budget vs wall clock

- **Declared budget (hard):** sum of bucket `expected_seconds` for the selected set. Used for manifest validation and `changed` gate.
- **Warm wall (soft):** time spent in bucket execution after binaries exist; warn at ~1.5× expected (existing behavior).
- **Cold wall (reported):** include `prebuild_test_binary` elapsed in the run summary. Clarifies that cold `fast` may exceed 60s without meaning the declared budget is a lie.
- Change prebuild to target packages implied by the selected buckets where practical (avoid always linking root `--lib` when the selected set is only `-p julie-core` / `-p xtask`). If multi-package selection is too hard for v1, keep a single prebuild but **always account its time** and document that cold ≠ declared.

#### 2b. `fast` tier membership (target after calibration)

Exact membership is **measurement-gated**. Working target:

| Bucket | Role | Today declared | Recalibration intent |
|--------|------|----------------|----------------------|
| `core-database` | `-p julie-core` | 5s | Keep / tighten from warm p50 |
| `core-fast` | misc fast core / watcher filtering | 60s | **Must shrink** (split or retarget) so `nano`/`fast` fit ≤60 |
| `xtask-runner` | `-p xtask` | 15s | Keep if warm p50 allows inside `fast` |
| Optional: thin CLI slice | not full `cli` (45s + `cargo build`) | — | Only if warm evidence fits residual budget |

Rules:

- `nano ⊆ fast`
- `sum(fast.expected_seconds) ≤ 60` enforced in **`TestManifest::validate`** (production path), not only `#[cfg(test)]`
- Also move the existing 600s `dev` check into `validate` (or keep as unit test **and** add production check) so “manifest enforcement” is real
- Do **not** lower `expected_seconds` without warm p50/p95 evidence recorded in the implementation plan / verification ledger

Measurement protocol:

1. Fix prerequisite migration assertion.
2. Warm the selected packages once.
3. Run each candidate bucket 3× warm; record p50/p95.
4. Set `expected_seconds` ≈ p95 with small headroom; re-sum `fast`.
5. Separately record one cold run including prebuild for docs (“cold may be N×”).

#### 2c. `changed` budget gate

After existing path→bucket mapping (including special buckets):

1. Resolve each selected name through a **single bucket-plan resolver** that covers manifest buckets **and** specials like `system-health` (today: accepted in `changed` but metadata lives privately in `runner.rs`). Prefer moving `system-health` into `test_tiers.toml` so pricing is uniform.
2. `declared = sum(resolver.expected_seconds)`.
3. If `declared ≤ 60` → `ChangedSelectionMode::Buckets` (run mapped).
4. If `declared > 60` → `ChangedSelectionMode::OverBudget`:
   - Print mapped buckets, declared sum, and concrete next commands.
   - **Do not** replace the selection with bare `dev` (drops coverage such as `tools-dogfood-repo-index`).
   - Exit non-zero so agents notice.
5. If `--scale` → run `unique(mapped ∪ dev)` with explicit “scale union” rationale (preserves mapped + adds dev).

### 3. Testing, docs, migration

**Acceptance tests**

- Lean xtask dep allowlist / no `julie` in `cargo tree -p xtask`.
- `TestManifest::validate`: missing `fast`, valid `fast`, over-budget `fast`.
- `changed`: under-budget buckets; over-budget → `OverBudget` (no auto-run); `--scale` → mapped ∪ dev.
- `system-health` (or other specials) priced correctly in the budget sum.
- `xtask-eval` bucket runs `cargo nextest run -p xtask-eval`; changed routes `xtask-eval/**`.
- Alias: document/assert `.cargo/config.toml` contains `xtask-eval`.
- Runner summary includes prebuild elapsed (unit or contract test with fake executor).

**Docs**

- Update `CLAUDE.md`, `AGENTS.md`, `docs/TESTING_GUIDE.md`: `fast`, warm vs cold, `OverBudget`, `changed --scale`, `cargo xtask-eval`.
- Keep `nano` / `smoke` as named subsets; `fast` is the default ≤60s-declared local gate.

**Migration**

- Old `cargo xtask search-matrix|eval` → explicit migration error.
- Nextest filters inside buckets unchanged except splits needed to make `core-fast` fit.
- Out of scope: full rewrite of path→bucket rules; semantic routing; sidecar bakeoff.

## Data flow

```text
git diff / paths
    → select_changed_buckets (existing maps + specials via resolver)
    → budget_gate(declared ≤ 60?)
         yes → run mapped buckets
         no  → OverBudget (print + exit nonzero)
              optional --scale → run unique(mapped ∪ dev)

cargo xtask test fast
    → load manifest (validate: fast present, sum ≤ 60)
    → prebuild (account time) → run buckets → report warm + cold components

cargo xtask-eval search-matrix|eval
    → alias → xtask-eval binary → product julie APIs
```

## Error handling

- Manifest load: hard error if `fast` missing/over declared budget, invalid bucket refs, or unresolvable tier entries.
- Deprecated xtask eval subcommands: hard error with replacement command.
- `changed` OverBudget: non-zero exit, no silent scale.
- Soft overrun: warning when warm wall > ~1.5× expected; cold prebuild reported, not used to “pass” the declared budget by omission.

## Acceptance checklist

- [ ] Prerequisite migration schema assertion fixed/isolated
- [ ] Workspace member `xtask-eval` + `.cargo/config.toml` alias
- [ ] Lean `xtask` dep allowlist (no `julie` / eval-only deps)
- [ ] Eval modules/tests moved; `xtask-eval` bucket + `changed` route + `full` inclusion
- [ ] Old `cargo xtask search-matrix|eval` migration error
- [ ] `fast` tier present; `validate()` enforces ≤60 declared
- [ ] Warm p50/p95 evidence table for recalibrated buckets; `nano ⊆ fast`
- [ ] Prebuild elapsed included in run summary; cold vs warm documented
- [ ] Bucket-plan resolver covers `system-health` (preferably preferred)
- [ ] `changed` OverBudget + `--scale` (mapped ∪ dev) tested
- [ ] Docs updated
- [ ] `cargo check -p xtask`, `cargo check -p xtask-eval`, relevant xtask/xtask-eval tests green

## Non-goals

- Replacing path→bucket maps with only `fast`/`scale` labels
- Changing product test assertions except the prerequisite schema fix and timing metadata
- Miller-style hard-kill at 30s wall
- Claiming cold-link `fast` ≤60s without package-scoped prebuild work (v1 may report cold honestly instead)

## Review history

- 2026-07-21: Initial draft approved in brainstorming.
- 2026-07-21: Codex review — rejected as written; findings folded into this revision (prebuild accounting, OverBudget vs bare `dev`, Cargo alias, xtask-eval bucket/routing, production `validate`, system-health pricing, measurement protocol, dep allowlist, nano prerequisite).
