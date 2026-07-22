# Xtask Changed-Selection Module Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Complete Phase 2B by decomposing `xtask/src/changed.rs` into focused diff collection, mapping, selection policy, and rendering modules without changing its public API, path precedence, budget behavior, or CLI output.

**Architecture:** Keep `xtask::changed` as the public facade and home of `ChangedSelectionMode` and `ChangedSelection`. Move Git/path collection, selection and budget policy, and output rendering into private child modules, re-exporting the existing four public functions from their original path. Keep path routing private under `changed::mapping`; because the current router alone is 545 lines, divide it into contiguous harness/front, crate, and product rule groups invoked in the exact current order.

**Tech Stack:** Rust, Cargo xtask, cargo-nextest, Git CLI.

**Architecture Quality:** Affected module is `xtask::changed`; caller-facing interfaces remain `ChangedSelectionMode`, `ChangedSelection`, `collect_changed_paths`, `select_changed_buckets`, `render_changed_selection`, and `apply_changed_scale`. The facade stays deep by hiding Git probing, normalization, first-match routing, fallback policy, declared-budget calculation, scaling, and presentation. Dependencies are in-process except Git collection, whose existing `XTASK_CHANGED_PATHS` override is the local-substitutable test seam; no new adapter or public seam is justified. Architecture risk is medium because rule ordering and rendered text are caller-visible.

## Global Constraints

- Preserve every existing public name, signature, field, enum variant, return value, error path, rationale line, CLI string, bucket order, fallback union, and first-match routing decision.
- Do not convert the routing chain into a declarative table or otherwise combine the split with mapping-policy changes.
- Invoke the mapping rule groups in the same contiguous order as the current `buckets_for_path`: harness/front rules, crate rules, then product/top-crate rules.
- Keep every new production implementation file at or below 500 lines and `changed/tests.rs` at or below 1000 lines.
- Keep `xtask/src/main.rs`, `xtask/src/lib.rs`, and `xtask/tests/changed_tests.rs` unchanged.
- Use the repository-pinned Rust 1.97.0 toolchain and preserve macOS 11.0 release settings.
- Follow TDD: add the failing module-size boundary test before moving implementation.

## Architecture Quality

**Affected modules:** `xtask::changed` and its new private children.

**Caller-facing interface:** The six existing names remain available from `xtask::changed` with identical shapes. Callers continue to collect paths, select buckets, optionally scale an over-budget selection, render the result, and execute the returned bucket list.

**Depth/locality check:** Selection policy remains the single owner of mode transitions and budget decisions. Mapping owns only path-to-bucket and fallback classification. Rendering owns all `CHANGED:` text. Diff collection owns Git/environment probing and path normalization.

**Test surface:** Existing integration tests continue to call only the public facade. The new structural test checks file boundaries; it does not create a public API for private routing helpers.

**Seams/adapters:** `XTASK_CHANGED_PATHS` remains the only local substitute for Git collection. Private `pub(super)` seams are allowed only where selection calls diff, mapping, or rendering and where the mapping dispatcher calls its three ordered rule groups.

**Rejected shortcuts:** A declarative rule-table rewrite risks changing first-match behavior; exposing route helpers would widen the public API; combining collection and selection would make the common caller simpler only by removing currently tested composability; splitting into dozens of rule files would be shallow over-decomposition.

**Architecture risk:** Medium. The public surface is small, but 57 selection references and 43 focused integration tests make mapping precedence, fallback rationale, and output stability load-bearing.

---

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/test_tiers.toml`, and `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`.

**Baseline evidence:** `cargo nextest run -p xtask --test changed_tests` passes 43/43 at `914edd9697a41c68f40a0826ca1f575b67275c3e`.

**Worker red/green scope:** `cargo nextest run -p xtask --test changed_boundary_tests changed_implementation_files_stay_within_limit`.

**Worker ceiling:** Run only the exact new boundary test once red and once green during the TDD loop. The lead owns existing changed-selection suites and broader gates.

**Worker gate invariant:** The public facade, each production child, and each mapping rule-group file stay at or below 500 lines; `changed/tests.rs` stays at or below 1000 lines; every expected private boundary exists.

**Lead affected-change scope:** Before committing the implementation while its xtask paths are still in the working-tree diff, run `cargo xtask test changed`; it must select and pass `xtask-runner`.

**Branch gate:** Run `cargo xtask test dev` once at the final current HEAD before handoff.

**Replay/metric evidence:** `cargo nextest run -p xtask --test changed_tests` and `cargo nextest run -p xtask` are hard behavior gates. `cargo fmt -p xtask -- --check`, file line counts, unchanged caller files, and exact public symbol paths are hard structural gates. Durations are report-only.

**Escalation triggers:** Any public API edit, route decision change, bucket ordering change, fallback/OverBudget mode change, CLI snapshot change, or changed-path collection failure blocks completion. No system, reliability, dogfood, or release tier is required unless live impact after the split reaches product code.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/plans/2026-07-22-changed-selection-module-boundary-verification.md`. Evidence is reusable only at the exact recorded HEAD and scope.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Split changed selection behind its facade | None - serial | Create `xtask/src/changed/diff.rs`, `xtask/src/changed/policy.rs`, `xtask/src/changed/rendering.rs`, `xtask/src/changed/mapping.rs`, `xtask/src/changed/mapping/front.rs`, `xtask/src/changed/mapping/crates.rs`, `xtask/src/changed/mapping/product.rs`, `xtask/src/changed/tests.rs`, `xtask/tests/changed_boundary_tests.rs`, and `docs/plans/2026-07-22-changed-selection-module-boundary-verification.md`; modify `xtask/src/changed.rs` only | Not applicable - single task. | Not applicable - single task. |

### Task 1: Split changed selection behind its facade

**Files:**
- Create: `xtask/src/changed/diff.rs`
- Create: `xtask/src/changed/policy.rs`
- Create: `xtask/src/changed/rendering.rs`
- Create: `xtask/src/changed/mapping.rs`
- Create: `xtask/src/changed/mapping/front.rs`
- Create: `xtask/src/changed/mapping/crates.rs`
- Create: `xtask/src/changed/mapping/product.rs`
- Create: `xtask/src/changed/tests.rs`
- Create: `xtask/tests/changed_boundary_tests.rs`
- Create: `docs/plans/2026-07-22-changed-selection-module-boundary-verification.md`
- Modify: `xtask/src/changed.rs:1-1842`

**Interfaces:**
- Consumes: `TestManifest`, `runner::declared_expected_seconds`, the Git CLI invoked through `process::program_command`, and the current `xtask::changed` public surface.
- Produces: The identical six public paths under `xtask::changed`, backed by private diff, mapping, policy, and rendering modules.

**Contract inputs:** `xtask/src/main.rs`, `xtask/src/lib.rs`, and `xtask/tests/changed_tests.rs` must compile unchanged. `select_changed_buckets` must preserve normalization, ignore-before-fallback-before-mapping order, mapped-plus-dev fallback unions, 60-second OverBudget policy, canonical bucket sorting, rationale ordering, and mode transitions. `apply_changed_scale` must remain `unique(mapped ∪ dev)` and remove only the `--scale` next-step line.

**File ownership:** Create `xtask/src/changed/diff.rs`, `xtask/src/changed/policy.rs`, `xtask/src/changed/rendering.rs`, `xtask/src/changed/mapping.rs`, `xtask/src/changed/mapping/front.rs`, `xtask/src/changed/mapping/crates.rs`, `xtask/src/changed/mapping/product.rs`, `xtask/src/changed/tests.rs`, `xtask/tests/changed_boundary_tests.rs`, and `docs/plans/2026-07-22-changed-selection-module-boundary-verification.md`; modify `xtask/src/changed.rs` only

**Serialization required:** Not applicable - single task.

**Dependency reason:** Not applicable - single task.

**Step 1: Write the failing test**

Create `xtask/tests/changed_boundary_tests.rs` with this contract:

```rust
use std::{fs, path::PathBuf};

#[test]
fn changed_implementation_files_stay_within_limit() {
    for relative_path in [
        "xtask/src/changed.rs",
        "xtask/src/changed/diff.rs",
        "xtask/src/changed/policy.rs",
        "xtask/src/changed/rendering.rs",
        "xtask/src/changed/mapping.rs",
        "xtask/src/changed/mapping/front.rs",
        "xtask/src/changed/mapping/crates.rs",
        "xtask/src/changed/mapping/product.rs",
    ] {
        assert_line_limit(relative_path, 500);
    }

    assert_line_limit("xtask/src/changed/tests.rs", 1000);
}

fn assert_line_limit(relative_path: &str, limit: usize) {
    let contents = fs::read_to_string(repo_file(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"));
    let line_count = contents.lines().count();

    assert!(
        line_count <= limit,
        "{relative_path} has {line_count} lines; limit is {limit}"
    );
}

fn repo_file(relative_path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(relative_path)
}
```

**Step 2: Run the test to verify it fails**

Run: `cargo nextest run -p xtask --test changed_boundary_tests changed_implementation_files_stay_within_limit 2>&1 | tail -20`

Expected: FAIL because `xtask/src/changed.rs` has 1842 lines against the 500-line limit.

**Step 3: Write the minimal implementation**

Replace `xtask/src/changed.rs` with the stable facade below, retaining the existing type definitions exactly:

```rust
mod diff;
mod mapping;
mod policy;
mod rendering;

#[cfg(test)]
mod tests;

pub use diff::collect_changed_paths;
pub use policy::{apply_changed_scale, select_changed_buckets};
pub use rendering::render_changed_selection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedSelectionMode {
    NoChanges,
    Buckets,
    OverBudget,
    FallbackToDev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedSelection {
    pub mode: ChangedSelectionMode,
    pub changed_paths: Vec<String>,
    pub bucket_names: Vec<String>,
    pub fallback_paths: Vec<String>,
    pub rationale: Vec<String>,
    pub ignored_paths: Vec<String>,
}
```

Move existing bodies without changing statements or strings:

- `diff.rs`: `collect_changed_paths`, `has_head`, `git_lines`, `normalize_paths`, `normalize_path`, and `should_ignore`; expose only `collect_changed_paths` publicly and `normalize_paths` as `pub(super)`.
- `policy.rs`: `select_changed_buckets`, `apply_changed_scale`, `maybe_push_bucket`, and `sort_bucket_names`; keep `FAST_BUDGET_SECS = 60` here.
- `rendering.rs`: `render_changed_selection` and `render_fallback_rationale`; expose the rationale helper only to `policy`.
- `mapping.rs`: the bucket constants, `FallbackRule`, fallback file/prefix classification, the shared `get_context_test_buckets_for_path` helper, the ordered `buckets_for_path` dispatcher, `matches_exact`, and `matches_prefix`. Declare private `front`, `crates`, and `product` children.
- `mapping/front.rs`: move the contiguous current routing rules from the initial `xtask/` check through the pre-crate embedding rules, plus their private `handler_tool_buckets_for_path` helper. Return `Option<&'static [&'static str]>` so the dispatcher can preserve first-match fallthrough.
- `mapping/crates.rs`: move the contiguous `julie-core`, `julie-index`, `julie-pipeline`, `julie-runtime`, and `julie-tools` routing rules unchanged. Return `Option<&'static [&'static str]>`.
- `mapping/product.rs`: move the remaining top-crate/product rules plus `search_test_buckets_for_path`. Return `Option<&'static [&'static str]>` and preserve internal ordering.
- `tests.rs`: move the current inline `#[cfg(test)] mod tests` contents without changing assertions or test names.

The dispatcher must remain exactly ordered:

```rust
pub(super) fn buckets_for_path(path: &str) -> &'static [&'static str] {
    front::buckets_for_path(path)
        .or_else(|| crates::buckets_for_path(path))
        .or_else(|| product::buckets_for_path(path))
        .unwrap_or(&[])
}
```

Do not modify `xtask/src/main.rs`, `xtask/src/lib.rs`, or `xtask/tests/changed_tests.rs`.

**Step 4: Run compile and exact green verification**

Run: `cargo check -p xtask`

Expected: PASS with the unchanged public imports in `xtask/src/main.rs` and `xtask/tests/changed_tests.rs`.

Run: `cargo nextest run -p xtask --test changed_boundary_tests changed_implementation_files_stay_within_limit 2>&1 | tail -20`

Expected: PASS 1/1 with every production file at or below 500 lines and `changed/tests.rs` at or below 1000.

**Step 5: Apply commit mode**

- `serial-worker-commit`: after the exact green test and lead inline review, checkpoint and commit the owned implementation files. Record the implementation SHA before running exact-commit lead gates.

**Acceptance criteria:**
- [x] The exact boundary test fails at the 1842-line baseline and passes after the split.
- [x] `changed.rs`, `diff.rs`, `policy.rs`, `rendering.rs`, `mapping.rs`, and all mapping rule-group files are each at or below 500 lines; `tests.rs` is at or below 1000.
- [x] All six caller-facing `xtask::changed` names retain their exact signatures and shapes.
- [x] First-match route precedence, path normalization, ignored/fallback handling, mapped-plus-dev union, 60-second budget behavior, canonical bucket order, and rationale/CLI text remain unchanged.
- [x] `xtask/src/main.rs`, `xtask/src/lib.rs`, and `xtask/tests/changed_tests.rs` require no edits.
- [x] `cargo nextest run -p xtask --test changed_tests` passes unchanged.
- [x] `cargo nextest run -p xtask` passes as the lead focused gate.
- [x] The dirty implementation diff makes `cargo xtask test changed` select and pass `xtask-runner` before the implementation commit.
- [x] `cargo xtask test dev` passes at the final current HEAD.
- [x] The verification ledger records all hard gates at the exact implementation commit and the final branch gate at current HEAD.
- [x] Worker-scope verification passes and the change is committed under serial-worker-commit mode.
