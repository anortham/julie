# julie-extractors 2.0.2 Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make julie consume the external `anortham/julie-extractors` library crate (v2.0.x, pinned to the contract-bump tag `v2.0.3`) as a git-dependency, delete the vendored in-tree copy and its now-upstream test/certification apparatus, and force a one-time reindex so the changed extraction behavior actually takes effect.

**Architecture:** Dependency-source shift (vendored workspace member → external git-dep) + test-ownership shift (per-extractor golden/capability/certification suites move upstream). julie keeps its own DB/Tantivy/daemon/watcher/indexing unchanged. The only julie `src/` code edit is a `SEMANTIC_INDEX_ENGINE_VERSION` bump that ties the engine version to the pinned extractor tag, forcing reindex on this upgrade and on every future re-pin. Perf is explicitly out of scope (a separate Track 2).

**Tech Stack:** Rust, Cargo (git dependencies with nested git sub-deps), `cargo nextest`, the `cargo xtask` test runner (`xtask/test_tiers.toml` buckets + `xtask/src/changed.rs` diff routing).

**Architecture Quality:** Approved shape — no new julie-internal module boundaries; the consumed API is the stable `julie_extractors` crate surface (verified drop-in compatible at commit `61b225a`). The architecture risk is **stale-index drift**, now resolved cleanly: upstream is cutting a **new tag** that bumps `EXTRACTION_CONTRACT_VERSION` (`2026-05-29.bridge-anchors-v2` → `2026-06-03.ecmascript-swift-shape-v3`) to cover the Swift/JS/TS behavior change, so julie just **syncs its embedded engine-version literal to the new constant** — a real RED→GREEN enforced by `src/tests/core/engine_version.rs`, which changes the engine version and forces a one-time reindex. (No synthetic `+extractor-dep` segment needed; that was the fallback for pinning a stale-contract tag.) The secondary risk is **verification-gate restructuring** (xtask bucket routing is shared infra): the removed buckets are pinned by exact-match self-tests in `xtask/tests/manifest_contract_*` and `xtask/tests/changed_tests.rs`, so the manifest edit and the expected-snapshot edit must stay byte-aligned or `cargo nextest run -p xtask` fails — this is the highest-churn part of the migration and is enumerated file-by-file in Task B. The routing for root `Cargo.toml`/`Cargo.lock` is deliberately simplified to fall back to the full `dev` tier (their original `DEV_FALLBACK_FILES` intent, previously shadowed by the parser-upgrade override) so an unrelated dependency bump is not under-tested by a thin extractor bucket; `extractor-dep-integration` rides along because it is a `dev`-tier member. Both risks are addressed below. If code reality contradicts this shape, report a plan mismatch rather than redesigning locally.

Design doc (approved): `docs/plans/2026-06-02-julie-extractors-migration-design.md`.

## Prerequisite (upstream, user-owned — gates Task A) — ✅ SATISFIED

**Resolved 2026-06-02.** Tag `v2.0.3` is live in `anortham/julie-extractors` at commit
`a9b3839`, and it ships `EXTRACTION_CONTRACT_VERSION = "2026-06-03.ecmascript-swift-shape-v3"`
(verified via `git show v2.0.3:crates/julie-extractors/src/lib.rs`). The earlier `v2.0.2`
tag (`61b225a`) carried the OLD contract string and is NOT the pin target. The plan below is
fully bound to these concrete values:
- **Pin target:** `tag = "v2.0.3"`
- **Contract string:** `2026-06-03.ecmascript-swift-shape-v3`
- **New engine-version literal:** `extractors=2026-06-03.ecmascript-swift-shape-v3+schema=2026-05-05.reference-identifier-v3`

Tasks B, C, D do not depend on the tag *value*, but they have their own ordering (B → C → D;
see Execution Order) and all require Task A's dep swap to have landed first (the build must
resolve before the xtask/fixture surface is reshaped).

---

## Verification Strategy

**Project source of truth:** `CLAUDE.md` / `AGENTS.md` (test-tier table) and `docs/TESTING_GUIDE.md`; `RAZORBACK.md` for tier ownership; `xtask/test_tiers.toml` for bucket definitions.

**Worker red/green scope:** the narrowest `cargo nextest run --lib <exact_test_name>` for the behavior touched. Specifically:
- engine version: `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract`
- extraction smoke: `cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs`
- xtask compiles: `cargo check -p xtask`
- xtask routing/manifest self-tests: `cargo nextest run -p xtask`

**Worker ceiling:** a worker may run the specific `--lib <test>` for its task plus `cargo check`. Workers do **not** run `cargo xtask test changed/dev/dogfood` — the lead owns those.

**Worker gate invariant:**
- engine-version test proves: julie's stored-index engine version still embeds the extractor contract constant (drift detection wired).
- extraction smoke proves: the consumed extractor dependency still yields the symbols julie's pipeline expects across ~29 languages.
- `cargo check -p xtask` proves: the xtask binary still compiles after the certify command + bucket removals.

**Lead affected-change scope:** `cargo xtask test changed` during the local loop; `cargo xtask test dev` once per completed batch. After bucket edits, also `cargo xtask test bucket extractor-dep-integration`.

**Branch gate:** `cargo xtask test dev` **and** `cargo xtask test dogfood` green, plus the manual **reindex-fires** check and the **end-to-end dogfood reindex** (Task E).

**Replay/metric evidence:** none. `real_world_parser_upgrade_contracts_assert_expected_outputs` is a **presence** gate (it asserts named symbols/identifiers exist, not counts — `real_world_contract.rs:305-318,327-337`), so the 2.0.x additive changes don't trip it. The only authorized edit is *removing* an expected entry that the documented Swift stdlib filtering legitimately drops (record old→new in the ledger). `dogfood`/`search-quality` is a ranking guard over a frozen prebuilt snapshot, not a live-extraction gate (see Task E Step 1).

**Escalation triggers:** any `src/` consumption site needing a change beyond the engine-version bump (means an API drift the recon missed → stop, escalate); a `real_world_contract` presence failure for a symbol the language *should* still emit (a regression, not the documented Swift filter); `cargo check` failing to resolve the git dep.

**Assigned verification failure:** workers stop and report; they do not present a failing gate as evidence. The one exception is removing a single Swift-stdlib expected entry from `real_world_contract` that the documented behavior change drops, which this plan authorizes (Task A / Task E).

**Verification ledger:** record invariant, command, scope label, commit SHA, result, timestamp for each gate. Use `docs/plans/verification-ledger-template.md`.

---

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier** (planning, lead review, shared-invariant edits): `RAZORBACK.md` → Codex `gpt-5.5 high`; Claude Opus. Owns Task A (engine version / reindex) and Task B (xtask bucket routing — `RAZORBACK.md` classes xtask bucket routing + changed-file selection as strategy/gate-review).

**Implementation tier** (bounded edits from a clear contract): Codex `gpt-5.5 low/medium`; Claude Sonnet. Eligible for Task C (certify command deletion — bounded, mechanical, named symbols) and Task D (fixtures/docs).

**Mechanical tier** (docs, fixtures, rote edits): Codex `gpt-5.4-mini`; Claude Haiku/Sonnet-low. Eligible for the docs-redirect sub-tasks of Task D only (no gate ownership).

**Gate-interpretation reviewer / Escalation tier:** Codex `gpt-5.5 high/xhigh`; Claude Opus. For any verification failure, a `real_world_contract` presence failure that may be a real regression, or Cargo.lock resolution problems.

**Worker eligibility:** implementation-tier workers may take Task C and Task D only after this plan's contract (exact symbols/files) is fixed and with the stated verification ceiling. Tasks A and B stay lead/strategy-owned because they touch the index engine version and xtask routing (shared invariants per `RAZORBACK.md`).

**Escalation triggers:** two failed worker attempts; any consumption-site change beyond the engine-version bump; a presence failure that reads as a regression.

**Mechanical exclusion:** mechanical workers cannot own the engine-version bump, the bucket routing, or the `real_world_contract` presence gate.

**Unsupported harness behavior:** if the harness cannot select per-agent models, use `inherit` and note it.

---

## File Structure (what changes)

**Modify:**
- `Cargo.toml` — workspace `members`, the `julie-extractors` dependency line.
- `Cargo.lock` — regenerated.
- `src/tools/workspace/indexing/engine_version.rs:16-17` — bump `SEMANTIC_INDEX_ENGINE_VERSION`.
- `src/tests/tools/call_path_tests.rs:618-663` — rename the synthetic `crates/julie-extractors/...` workspace-crate fixture paths to a neutral name (the test fabricates temp files; it does not navigate the real crate).
- `xtask/test_tiers.toml` — remove 3 buckets, remove them from `dev`/`full` tiers, add `extractor-dep-integration`.
- `xtask/src/changed.rs` — delete `is_extractor_path`/`is_parser_upgrade_path` + the parser-upgrade early-return, route `src/extractors/` to the new bucket, let `Cargo.toml`/`Cargo.lock` fall back to `dev`, and update the `sort_bucket_names` order list.
- `xtask/tests/changed_tests.rs` — update/remove the routing self-tests that assert the deleted buckets (lines 173-179, 189-193, 206-211, 607).
- `xtask/tests/manifest_contract_tests.rs:213,269` — replace `extractor-units` with `extractor-dep-integration` in the `dev`/`full` expected tier arrays.
- `xtask/tests/support/manifest_contract_expected.rs` — replace the 3 removed bucket entries (name lists ~106/119/145; metadata defs ~626/637/666) with the single `extractor-dep-integration` entry, mirroring `test_tiers.toml` exactly.
- `xtask/src/cli.rs` — remove the `certify` command surface (`CertifyCommand`, `CliCommand::Certify`, `parse_certify_command`, `parse_certify_options`, `ParsedCertifyOptions`, `default_tree_sitter_certify_out`, the `"certify"` parse arm at 207, the `validate_cli_command` arm at 338, and fix the no-command bail at 201).
- `xtask/src/main.rs:20-21,56-61,172-189` — remove the certify `use` imports, the `CliCommand::Certify(_)` branch of the `unreachable!` arm (56-61), and the certify dispatch.
- `xtask/src/lib.rs:13-17` — remove the 5 `tree_sitter_*` `pub mod` lines.
- Contributor docs: `CLAUDE.md`, `AGENTS.md`, `README.md` (Quick Reference, 568-575), `docs/README.md` (Quality Reports index, 27-28), `docs/DEVELOPMENT.md` (`cargo test -p julie-extractors`, line 20), `docs/ADDING_NEW_LANGUAGES.md`, `docs/TREE_SITTER_UPGRADES.md`, `docs/TREE_SITTER_QUALITY_BAR.md`, `docs/TREE_SITTER_REVIEW_FINDINGS_STATUS.md`, `docs/DEPENDENCIES.md`, `docs/EXTRACTION_CONTRACT.md`.

**Delete:**
- `crates/julie-extractors/` (entire directory).
- `xtask/src/tree_sitter_certification.rs`, `tree_sitter_certification_data.rs`, `tree_sitter_certification_report.rs`, `tree_sitter_real_world.rs`, `tree_sitter_real_world_report.rs`, `xtask/src/tree_sitter_real_world/`.
- `xtask/tests/tree_sitter_certification_tests.rs`.
- `fixtures/extraction/` (entire golden corpus).
- `docs/LANGUAGE_CERTIFICATION_REPORT.md`, `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json`, `docs/LANGUAGE_REAL_WORLD_EVIDENCE.md`.

**Keep (do NOT delete):** `fixtures/real-world/**`, `fixtures/qml/real-world/**`, `fixtures/r/real-world/**` (used by `real_world_contract.rs`); `src/tools/workspace/indexing/engine_version.rs`; `src/tests/core/engine_version.rs`; `src/tests/integration/real_world_contract.rs`; all `src/extractors/` re-export layer; all `src/` consumption sites.

---

## Task A: Consume the external crate + force reindex (Strategy/lead)

**Files:**
- Modify: `Cargo.toml` (lines 2, 41)
- Modify: `src/tools/workspace/indexing/engine_version.rs:16-17`
- Modify: `src/tests/tools/call_path_tests.rs:618-663` (rename synthetic fixture paths)
- Modify: `Cargo.lock` (regenerated)
- Delete: `crates/julie-extractors/`

**What this does:** swaps the path-dep for a git-dep on the **new contract-bump tag** (expected `v2.0.3` — see Prerequisite), removes the in-tree crate, and syncs the engine-version literal to the new contract constant so the changed extraction behavior forces a reindex. Throughout this task, substitute `v2.0.3` with the exact tag the user pushed and `2026-06-03.ecmascript-swift-shape-v3` with the exact `EXTRACTION_CONTRACT_VERSION` that tag ships (working-tree value: `2026-06-03.ecmascript-swift-shape-v3`).

**Step 1: Pre-flight — find exact-string assertions on the current engine version, and rename the synthetic in-tree-crate fixture paths in the call-path test.**

```bash
cd /Users/murphy/source/julie
# Any code that hardcodes the FULL engine-version string would break on the bump:
grep -rn "extractors=2026-05-29.bridge-anchors-v2+schema=2026-05-05.reference-identifier-v3" src/ xtask/ docs/
# Any TEST referencing the in-tree crate path:
grep -rn "crates/julie-extractors" src/tests/
```
Expected: the first grep matches only `engine_version.rs:17` (the definition). If any *other* exact-match site exists, update it in this task.

The second grep is **NOT empty** — `src/tests/tools/call_path_tests.rs:618-663` (`test_call_path_resolves_workspace_crate_glob_reexport` or similar) fabricates an in-memory workspace whose files are named `crates/julie-extractors/src/lib.rs` and `crates/julie-extractors/src/pipeline.rs`, and asserts the call path resolves to `crates/julie-extractors/src/pipeline.rs` (lines 626-632, 642, 660). These are synthetic temp files written by `setup_indexed_workspace_files`; the test exercises julie's *own* glob-re-export call-path resolution and does **not** read the real crate, so deleting `crates/julie-extractors/` does not break it. But the path now misleads (julie no longer vendors that crate). **Rename the synthetic crate path to a neutral workspace-member name** (e.g. `crates/sample-lib/src/lib.rs`, `crates/sample-lib/src/pipeline.rs`, and the matching `to_file_path` + `target_file` assertions). Keep the `pub use julie_extractors::*;` line in `src/extractors/mod.rs` of the fixture (that glob re-export is the behavior under test) — only the fabricated `crates/...` member path changes. After the rename, `grep -rn "crates/julie-extractors" src/tests/` is empty.

**Step 2: Swap the workspace members + dependency in `Cargo.toml`.**

Line 2:
```toml
members = [".", "xtask"]
```
Line 41 (replace the path dep):
```toml
# Julie Extractors — external standalone product, consumed as a pinned git dependency.
julie-extractors = { git = "https://github.com/anortham/julie-extractors", tag = "v2.0.3" }
```

**Step 3: Delete the in-tree crate.**

```bash
git rm -r crates/julie-extractors
# remove the now-empty crates/ dir if nothing else lives there
rmdir crates 2>/dev/null || true
```

**Step 4: Regenerate the lockfile and verify the git dep + its nested git sub-deps resolve.**

```bash
cargo build 2>&1 | tail -20
```
Expected: clean build. `Cargo.lock` now lists `julie-extractors` at the new version with `source = "git+https://github.com/anortham/julie-extractors?tag=v2.0.3#<sha>"` and re-resolves the 4 git tree-sitter sub-deps. If the build fails on a consumption site (not just resolution), STOP — that is an API drift the recon missed; escalate.

**Step 5: Observe RED — the engine-version anchor test now fails.**

With `v2.0.3` pinned, `julie_extractors::EXTRACTION_CONTRACT_VERSION` is now `2026-06-03.ecmascript-swift-shape-v3`, but julie's `SEMANTIC_INDEX_ENGINE_VERSION` literal still embeds the old `2026-05-29.bridge-anchors-v2`.
Run: `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract 2>&1 | tail -10`
Expected: **FAIL** — the asserted `contains(EXTRACTION_CONTRACT_VERSION)` is false because the literal embeds the old contract string. This RED is the migration's drift signal working as designed.

**Step 6: Sync the embedded literal to the new contract constant (GREEN).**

`src/tools/workspace/indexing/engine_version.rs`, replace lines 16-17 (use the EXACT `2026-06-03.ecmascript-swift-shape-v3` the tag ships):
```rust
pub const SEMANTIC_INDEX_ENGINE_VERSION: &str =
    "extractors=2026-06-03.ecmascript-swift-shape-v3+schema=2026-05-05.reference-identifier-v3";
```
Run: `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract 2>&1 | tail -10`
Expected: **PASS**. This literal change also changes `SEMANTIC_INDEX_ENGINE_VERSION` itself → stored indexes stamped with the old engine version will be invalidated (the reindex; proven in Task E). If any *other* test asserts the exact old engine-version string (found in Step 1), update it to the new literal.

**Step 7: Verify julie's own extraction smoke; adjust the expected set only if a previously-expected symbol disappeared.**

Run: `cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs 2>&1 | tail -30`

This test asserts **presence** of named symbols/identifiers per language (`real_world_contract.rs:305-318,327-337` use `.any(...)`, not count equality). So the JS/TS import-resolution rewrite or new relationships will **not** break it — adding symbols is invisible to a presence check. It only fails if the 2.0.x change *removed* a symbol the expected set still names (e.g. Swift stdlib filtering dropping a symbol that was previously asserted). If that happens, inspect the diff: if the now-missing symbol is exactly what the documented behavior change removes (a filtered Swift stdlib/framework symbol), delete that one expected entry and record old→new in the ledger. If a real, language-appropriate symbol is missing (a regression, not the documented filter), STOP and escalate.
Expected: PASS unmodified, in the common case.

**Step 8: Commit.**

```bash
git add Cargo.toml Cargo.lock src/tools/workspace/indexing/engine_version.rs src/tests/tools/call_path_tests.rs
git commit -m "feat(extractors): consume external julie-extractors v2.0.3 as git-dep; sync contract version + force reindex"
```

**Acceptance criteria:**
- [ ] `Cargo.toml` uses the git-dep at `tag = "v2.0.3"` (the contract-bump tag, NOT v2.0.2); `crates/julie-extractors/` deleted; removed from `[workspace] members`.
- [ ] `cargo build` clean; `Cargo.lock` committed with the git source line.
- [ ] `engine_version.rs` literal synced to `2026-06-03.ecmascript-swift-shape-v3`; the anchor test went RED (Step 5) then GREEN (Step 6); engine version string actually changed.
- [ ] Only `engine_version.rs` changed in `src/` consumption code (plus the `call_path_tests.rs` synthetic-path rename and any exact-string-assertion site found in Step 1).
- [ ] `real_world_parser_upgrade_contracts_assert_expected_outputs` green (the test asserts symbol/identifier **presence**, not counts — see Step 7; record any expected-set edit in the ledger).
- [ ] `grep -rn "crates/julie-extractors" src/tests/` is empty after the call-path fixture rename.

---

## Task B: Reshape xtask test buckets + diff routing (Strategy/lead)

**Files:**
- Modify: `xtask/test_tiers.toml` (lines 8, 13, 415-452)
- Modify: `xtask/src/changed.rs` (lines 342-345, 396-408, 955-1003, 1015-1030)
- Modify: `xtask/tests/changed_tests.rs` (lines 173-179, 189-193, 206-211, 607 — routing self-tests)
- Modify: `xtask/tests/manifest_contract_tests.rs` (lines 213, 269 — dev/full expected tier arrays)
- Modify: `xtask/tests/support/manifest_contract_expected.rs` (name lists ~106/119/145; metadata defs ~626/637/666)

**What this does:** removes the three now-upstream extractor buckets and replaces the re-pin gate with a julie-owned `extractor-dep-integration` bucket. The two exact-match self-test files (`manifest_contract_*`, `changed_tests`) pin the bucket inventory and routing, so they update in lockstep with the manifest — this is the highest-churn part of the migration.

**Step 1: In `xtask/test_tiers.toml`, remove `extractor-units` from the `dev` (line 8) and `full` (line 13) tier arrays.** (It is the only extractor bucket currently in a tier.) Add `extractor-dep-integration` to both arrays in its place.

**Step 2: Delete the three bucket definitions** `[buckets.extractors]` (415-426), `[buckets.extractor-units]` (428-441), `[buckets.parser-upgrade]` (443-452).

**Step 3: Add the replacement bucket** (place it where the old ones were, ~line 415):
```toml
[buckets.extractor-dep-integration]
# Julie-owned gate that fires when the external julie-extractors dependency is
# re-pinned (Cargo.toml / Cargo.lock). The per-language golden/capability/
# certification suites now live UPSTREAM in anortham/julie-extractors; this bucket
# proves the consumed extractor dependency still produces what julie's pipeline
# expects: the engine-version drift anchor + the real-world extraction smoke.
expected_seconds = 60
timeout_seconds = 180
scope_label = "extractor-dep"
notes = "extractor dependency integration: contract-version anchor + real-world extraction smoke"
commands = [
  "cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract",
  "cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs",
]
```

**Step 4: Update `xtask/src/changed.rs` routing.**

The deleted buckets formerly captured four path classes: `crates/julie-extractors/**`, `fixtures/extraction/**`, root `Cargo.toml`/`Cargo.lock`, and `src/extractors/**`. After this migration the first two paths no longer exist, root `Cargo.toml`/`Cargo.lock` should route to **`dev`** (a general dependency bump must not be under-tested by a thin extractor gate — and `Cargo.toml`/`Cargo.lock` are already in `DEV_FALLBACK_FILES` at lines 10-11, previously shadowed by the parser-upgrade override), and only `src/extractors/**` (the thin re-export wrapper) routes to the new `extractor-dep-integration` bucket. `extractor-dep-integration` is a `dev`-tier member, so a manifest/lock edit that re-pins the extractor dep runs it *as part of* the dev fallback — strictly more coverage than the old thin routing.

(a) `fallback_rule_for_path` (342-345): **delete** the early-return block entirely:
```rust
fn fallback_rule_for_path(path: &str) -> Option<(FallbackRule, String)> {
    // (removed: the is_extractor_path/is_parser_upgrade_path short-circuit)
    if let Some(exact_file) = DEV_FALLBACK_FILES
```
Now `Cargo.toml`/`Cargo.lock` match `DEV_FALLBACK_FILES` → `dev` fallback, as the list always intended.

(b) `buckets_for_path` (396-408): delete the `is_parser_upgrade_path` branch (396-398) and the `is_extractor_path` branch (400-402); change the `src/extractors/` branch (404-408) to route to the new bucket. Do **not** add a `Cargo.toml`/`Cargo.lock` branch here (they are handled by the dev fallback in (a)):
```rust
    // src/extractors/ is the thin re-export wrapper over the external
    // julie-extractors crate; a change there runs the julie-owned dep-integration gate.
    if matches_prefix(path, &["src/extractors/"]) {
        return &["extractor-dep-integration"];
    }
```

(c) `sort_bucket_names` order array (955-1003): remove `"extractors"`, `"extractor-units"`, `"parser-upgrade"` (961-963); insert `"extractor-dep-integration"` in their place.

(d) **Delete** both `is_extractor_path` (1015-1018) and `is_parser_upgrade_path` (1020-1030). No replacement function is needed (the two remaining helpers `matches_exact`/`matches_prefix` stay). If the compiler flags `matches_exact` as now-unused, leave it only if still referenced elsewhere; otherwise remove it too.

**Step 5: Update the xtask self-tests (exact-match — must mirror the manifest).** Two distinct test surfaces pin the removed buckets; update both:

(i) **Routing self-tests** in `xtask/tests/changed_tests.rs`:
- 173-179: a `crates/julie-extractors/src/...` case asserting `["extractors"]` → the path no longer exists; **remove** this case.
- 189-193: a `fixtures/extraction/...` case asserting `["parser-upgrade"]` → path deleted; **remove**.
- 206-211: a `crates/julie-extractors/Cargo.toml` case asserting `["parser-upgrade"]` → **remove** (or repoint to a root-`Cargo.toml` → `FallbackToDev` assertion).
- 607: another `["extractors"]` assertion → inspect its input path; if `crates/`/`fixtures/extraction`, remove; if `src/extractors/`, repoint to `["extractor-dep-integration"]`.
- **Add** a case proving the new routing: `src/extractors/mod.rs` → `["extractor-dep-integration"]`, and `Cargo.toml` (or `Cargo.lock`) → `ChangedSelectionMode::FallbackToDev` (bucket set = the `dev` tier, which now includes `extractor-dep-integration`).

(ii) **Manifest-contract self-tests** (the exact bucket-inventory snapshot):
- `xtask/tests/manifest_contract_tests.rs:213` (dev expected array) and `:269` (full expected array): replace `"extractor-units".to_string()` with `"extractor-dep-integration".to_string()` in both.
- `xtask/tests/support/manifest_contract_expected.rs`: remove the three removed buckets from the name list (~106 `"extractors"`, ~119 `"extractor-units"`, ~145 `"parser-upgrade"`) and the metadata-def map (~626 `extractors`, ~637 `extractor-units`, ~666 `parser-upgrade`); add ONE `extractor-dep-integration` entry in the same positions, mirroring `test_tiers.toml` Step 3 exactly:
```rust
        (
            "extractor-dep-integration",
            ExpectedBucketMetadata {
                scope_label: "extractor-dep",
                owner: "lead",
                expensive: false,
                notes: Some(
                    "extractor dependency integration: contract-version anchor + real-world extraction smoke",
                ),
            },
        ),
```
(`owner`/`expensive` are derived defaults, not TOML fields; keep them `"lead"`/`false` to match every other non-expensive lead bucket.)

Sanity grep — after the edits, this must return nothing outside deleted files:
```bash
grep -rn '"extractors"\|"extractor-units"\|"parser-upgrade"\|fixtures/extraction\|crates/julie-extractors' xtask/src/ xtask/tests/ | grep -v tree_sitter_certification_tests.rs
```
(The `tree_sitter_certification_tests.rs` hits are removed when that file is deleted in Task C.)

**Step 6: Verify.**

```bash
cargo check -p xtask 2>&1 | tail -5
cargo nextest run -p xtask 2>&1 | tail -15            # manifest-contract + changed-routing self-tests
cargo xtask test bucket extractor-dep-integration 2>&1 | tail -10
```
Expected: all green. (Note: the `extractors` bucket's removed command referenced `cargo xtask certify tree-sitter --check`; that command is removed in Task C — sequence B before C so no live bucket calls a deleted command. `tree_sitter_certification_tests.rs` still compiles/passes here because the certify code it tests is not removed until Task C.)

**Step 7: Commit.**

```bash
git add xtask/test_tiers.toml xtask/src/changed.rs xtask/tests/
git commit -m "test(xtask): replace upstream-owned extractor buckets with extractor-dep-integration gate"
```

**Acceptance criteria:**
- [ ] `extractors`/`extractor-units`/`parser-upgrade` removed from manifest definitions, the `dev`/`full` tiers, the `sort_bucket_names` order, AND both self-test files (`manifest_contract_tests.rs`, `support/manifest_contract_expected.rs`).
- [ ] `extractor-dep-integration` bucket added and in `dev`+`full` (manifest + both self-tests); `src/extractors/` routes to it; root `Cargo.toml`/`Cargo.lock` route to `FallbackToDev`.
- [ ] `cargo check -p xtask` clean; `cargo nextest run -p xtask` green; `cargo xtask test bucket extractor-dep-integration` green.
- [ ] The Step 5 sanity grep returns nothing outside `tree_sitter_certification_tests.rs`.

---

## Task C: Remove the `certify tree-sitter` command surface (Implementation, gated after B)

**Files:**
- Delete: `xtask/src/tree_sitter_certification.rs`, `tree_sitter_certification_data.rs`, `tree_sitter_certification_report.rs`, `tree_sitter_real_world.rs`, `tree_sitter_real_world_report.rs`, `xtask/src/tree_sitter_real_world/`, `xtask/tests/tree_sitter_certification_tests.rs`
- Modify: `xtask/src/lib.rs:13-17`, `xtask/src/main.rs:20-21,172-189`, `xtask/src/cli.rs` (certify items)

**What this does:** the `certify tree-sitter` command only certified the extractors that now live upstream; with the `extractors` bucket gone (Task B) it has no caller. Remove the whole command.

**Step 1: Delete the impl files and the test.**
```bash
git rm xtask/src/tree_sitter_certification.rs \
       xtask/src/tree_sitter_certification_data.rs \
       xtask/src/tree_sitter_certification_report.rs \
       xtask/src/tree_sitter_real_world.rs \
       xtask/src/tree_sitter_real_world_report.rs \
       xtask/tests/tree_sitter_certification_tests.rs
git rm -r xtask/src/tree_sitter_real_world
```

**Step 2: `xtask/src/lib.rs`** — remove the five `pub mod tree_sitter_*;` lines (13-17).

**Step 3: `xtask/src/main.rs`** — three removals:
- the two `use xtask::tree_sitter_*;` imports (20-21);
- inside the `Test` arm's `validate_cli_command` match (56-61), remove the `CliCommand::Certify(_)` line from the `unreachable!("validated test command changed shape")` alternation (it is one of the `|`-joined variants; dropping it keeps the arm exhaustive once `CliCommand::Certify` no longer exists);
- the entire `CliCommand::Certify(...)` dispatch arm (~172-189).

**Step 4: `xtask/src/cli.rs`** — remove the certify surface (every reference to `CliCommand::Certify` / `CertifyCommand` must go or `cargo check -p xtask` fails):
- the `CertifyCommand` enum (126-136);
- the `CliCommand::Certify(CertifyCommand)` variant (in the `CliCommand` enum);
- the `"certify" => Ok(CliCommand::Certify(...))` parse arm (207);
- **the `CliCommand::Certify(command) => Ok(CliCommand::Certify(command)),` arm inside `validate_cli_command` (338)** — this is the one the original plan missed; leaving it is a hard compile break;
- the no-command bail (201): it currently reads `<test|search-matrix|certify>` but the binary actually supports `test`, `search-matrix`, `sync-plugin`, `dev-link`, `dev-restart`, `eval` (see the parse match 204-212). Do NOT shrink it to `<test|search-matrix>` (that drops real commands). Replace with the accurate set: `expected `cargo xtask <test|search-matrix|sync-plugin|dev-link|dev-restart|eval> ...``;
- the functions `parse_certify_command` (398-), `parse_certify_options` (659-), the `ParsedCertifyOptions` struct (433-), and `default_tree_sitter_certify_out` (723-).
- Do NOT touch `Baseline`/`Ablation`/search-matrix items (119-124) — unrelated.

**Step 5: Verify the xtask binary still compiles and its tests pass.**
```bash
cargo check -p xtask 2>&1 | tail -5
cargo nextest run -p xtask 2>&1 | tail -15
grep -rn "Certify\|CertifyCommand\|certify" xtask/src/ | grep -v search-matrix   # expect: empty
```
Expected: clean + green, grep empty. Resolve any dangling reference the compiler flags (e.g., an unused import or a helper only used by certify).

**Step 6: Commit.**
```bash
git add -A xtask/
git commit -m "chore(xtask): remove certify tree-sitter command (extractor certification moved upstream)"
```

**Acceptance criteria:**
- [ ] All 5 certify impl files + the `tree_sitter_real_world/` dir + the certify test deleted; `lib.rs`/`main.rs` (imports + unreachable arm + dispatch) / `cli.rs` (enum + variant + parse arm + `validate_cli_command` arm + bail + 4 fns) certify wiring removed.
- [ ] `cargo check -p xtask` clean; `cargo nextest run -p xtask` green; `grep -rn certify xtask/src/` empty; `cargo xtask certify` now errors with `unsupported xtask command \`certify\`` (the 212 catch-all, not the no-command bail).

---

## Task D: Delete the golden corpus + redirect docs (Implementation/mechanical, gated after B AND C)

**Sequencing:** `fixtures/extraction/` is still referenced by `xtask/src/changed.rs` (`is_parser_upgrade_path`, removed in Task B), `xtask/src/tree_sitter_real_world.rs:30` (`DEFAULT_TREE_SITTER_REAL_WORLD_CORPUS`, deleted in Task C), and `xtask/tests/tree_sitter_certification_tests.rs` (deleted in Task C). So the **fixture deletion (Step 1) must run after both B and C** — otherwise the Step 1 grep gate trips or the deletion strands live references. The docs edits (Steps 3-4) have no code dependency but are bundled here for write-scope cleanliness.

**Files:**
- Delete: `fixtures/extraction/`, `docs/LANGUAGE_CERTIFICATION_REPORT.md`, `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json`, `docs/LANGUAGE_REAL_WORLD_EVIDENCE.md`
- Modify: contributor + tree-sitter docs (see below)

**Step 1: Prove the golden corpus is unreferenced by julie's own code/tests, then delete it.** (Run only after Tasks B and C have landed.)
```bash
cd /Users/murphy/source/julie
grep -rn "fixtures/extraction" src/ xtask/    # expect: EMPTY only after B (changed.rs) + C (tree_sitter_real_world.rs + cert test) land
git rm -r fixtures/extraction
```
If any `src/`/`xtask/` reference remains, STOP — Task B or C is incomplete; do not delete that path.

**Step 2: Confirm the real-world keep-set is intact.** Do NOT delete these — `real_world_contract.rs` reads them:
```bash
git status --porcelain fixtures/real-world fixtures/qml/real-world fixtures/r/real-world   # expect: no deletions
```
Guard against silent no-op tests: confirm `src/tests/integration/real_world_validation/real_world_refactoring_tests.rs` and siblings still have non-empty fixture dirs (they skip-when-empty). If any of their dirs were under `fixtures/extraction/`, either keep those fixtures or delete the now-hollow test — never leave it skipping.

**Step 3: Delete the certification output docs.**
```bash
git rm docs/LANGUAGE_CERTIFICATION_REPORT.md docs/LANGUAGE_REAL_WORLD_EVIDENCE.json docs/LANGUAGE_REAL_WORLD_EVIDENCE.md
```

**Step 4: Redirect contributor docs** so they point language/parser work at the external repo instead of `crates/julie-extractors/`. In each, replace "add the language/parser in `crates/julie-extractors/...` and run `cargo xtask certify ...`" guidance with: "Language and parser work now happens in `anortham/julie-extractors`; release a new tag there, then re-pin julie's `julie-extractors` git-dep in `Cargo.toml`, and sync julie's `SEMANTIC_INDEX_ENGINE_VERSION` literal to the tag's `EXTRACTION_CONTRACT_VERSION` (the changed contract string forces a reindex; the `engine_version` test enforces the sync)." Files:
- `CLAUDE.md` and `AGENTS.md` — remove the `cargo xtask certify tree-sitter ...` Quick Reference lines; update the "Adding a new language/parser" + plugin/extractor sections; update the "🏆 Current Language Support" / "Crown Jewels" framing to note extractors are now external. Keep CLAUDE.md ≡ AGENTS.md (the pre-commit hook enforces equality).
- `README.md:568-575` — remove the three `cargo xtask certify tree-sitter ...` lines from the Quick Reference block (regenerate / refresh / `--check`); they reference the deleted command.
- `docs/README.md:27-28` — the "Quality Reports" index lists `LANGUAGE_CERTIFICATION_REPORT.md` and `LANGUAGE_REAL_WORLD_EVIDENCE.json`, both deleted in this task; remove those two bullets (leave the still-present `TREE_SITTER_REVIEW_FINDINGS_STATUS.md` / `LANGUAGE_VERIFICATION_*` entries).
- `docs/DEVELOPMENT.md:20` — replace the `cargo test -p julie-extractors typescript_extractor -- --nocapture` example (the `julie-extractors` package is no longer in this workspace) with a julie-owned narrow-test example, or redirect per-extractor testing to the external repo.
- `docs/ADDING_NEW_LANGUAGES.md` — redirect to the external repo's workflow (or replace with a short stub pointing there).
- `docs/TREE_SITTER_UPGRADES.md`, `docs/TREE_SITTER_QUALITY_BAR.md`, `docs/TREE_SITTER_REVIEW_FINDINGS_STATUS.md` — note the parser inventory + certification now live upstream; fix the stale `call_path … → crates/julie-extractors/src/pipeline.rs` self-navigation example in `TREE_SITTER_QUALITY_BAR.md:212` (extractor source is no longer in julie's index; suggest `manage_workspace(operation="open")` on the external repo).
- `docs/DEPENDENCIES.md`, `docs/EXTRACTION_CONTRACT.md` — update the `crates/julie-extractors` references to the git-dep + external repo.
Leave historical `docs/plans/*` and `docs/release-notes/*` untouched (point-in-time records).

Final sweep — confirm no non-historical doc still tells contributors to certify or edit the in-tree crate:
```bash
rg -n "cargo xtask certify|crates/julie-extractors|cargo test -p julie-extractors|LANGUAGE_CERTIFICATION_REPORT|LANGUAGE_REAL_WORLD_EVIDENCE" \
  --glob '!docs/plans/**' --glob '!docs/release-notes/**' --glob '!docs/verification/**' . 2>/dev/null
```
Expect: empty (or only intentional "now lives upstream" redirect text). Any other hit is an unconverted reference.

**Step 5: Verify docs sync + extraction smoke still green.**
```bash
diff <(sed -n '/Julie — Development Guidelines/,$p' CLAUDE.md) <(sed -n '/Julie — Development Guidelines/,$p' AGENTS.md) >/dev/null && echo "CLAUDE.md ≡ AGENTS.md body OK" || echo "SYNC MISMATCH — fix before commit"
cargo nextest run --lib real_world_parser_upgrade_contracts_assert_expected_outputs 2>&1 | tail -5
```
(The pre-commit hook also enforces the CLAUDE.md/AGENTS.md sync.)

**Step 6: Commit.**
```bash
git add -A
git commit -m "docs(extractors): drop upstream-owned golden corpus + cert docs; redirect language workflow to anortham/julie-extractors"
```

**Acceptance criteria:**
- [ ] `fixtures/extraction/` + the 3 cert docs deleted (after B+C); real-world keep-set (incl. `qml/real-world`, `r/real-world`) intact; no skip-when-empty test left hollow.
- [ ] Contributor docs redirect language/parser work to the external repo; `CLAUDE.md`, `AGENTS.md`, `README.md`, `docs/README.md`, `docs/DEVELOPMENT.md`, and the tree-sitter docs no longer tell contributors to edit `crates/julie-extractors/`, run `cargo xtask certify`, or `cargo test -p julie-extractors`.
- [ ] The Step 4 final `rg` sweep (excluding historical dirs) returns empty / redirect-only.
- [ ] CLAUDE.md ≡ AGENTS.md; `real_world_*` smoke green.

---

## Task E: Branch-gate verification + dogfood (Lead-owned)

**Step 1: Affected-change + batch gates.**
```bash
cargo xtask test dev 2>&1 | tail -20
cargo xtask test dogfood 2>&1 | tail -20
```
Expected: green. **Scope caveat:** `dogfood` (`search-quality`) loads a **prebuilt 100MB SQLite snapshot** and backfills Tantivy from it (`src/tests/tools/search_quality/helpers.rs:191`) — it does **not** re-run extraction, so it is a *ranking/scoring* regression guard, not a live-extraction check. That snapshot still contains `crates/julie-extractors/...` paths and symbols indexed from the now-deleted in-tree crate (`fixtures/databases/julie-snapshot/metadata.json:426`); that is expected and fine (the snapshot is a frozen ranking baseline, intentionally not rebuilt here). The live-extraction proof is Step 3, not this step. Do not treat a green `dogfood` as evidence the new extractor dependency extracts correctly.

**Step 2: Prove the reindex actually fires (the Critical drift fix).**
- Build the release binary and reindex julie itself against a *pre-migration* index (one stamped with the old `SEMANTIC_INDEX_ENGINE_VERSION`).
- Confirm the engine-version mismatch triggers a **full reindex** on session connect / catch-up (not a no-op). Check the daemon log for an engine-version-mismatch → reindex line:
```bash
cargo build --release
# point a session/CLI at the workspace, then:
grep -iE "engine.version|reindex|stale.*index" ~/.julie/daemon.log.$(date +%Y-%m-%d) | tail -20
```
Expected: evidence that the old index was invalidated and rebuilt. If it does NOT reindex, the drift fix failed — STOP and escalate (the engine-version bump isn't wired into the staleness check the way assumed).

**Step 3: End-to-end extraction dogfood.** After the reindex, spot-check that extraction still produces usable symbols/relationships across languages via julie's own tools (CLI or MCP): `fast_search`, `get_symbols`, `deep_dive` on a Rust, a TypeScript, a Python, and a Swift file. Confirm Swift no longer shows stdlib-framework symbols as cross-file pending relationships (the expected 2.0.2 behavior change).

**Step 4: Ledger + final review.** Record every gate in the verification ledger. Lead does final integration review (assumptions, not just diffs), especially any `real_world_contract` presence-set edit and the reindex evidence.

**Acceptance criteria:**
- [ ] `cargo xtask test dev` + `cargo xtask test dogfood` green.
- [ ] Reindex-fires evidence captured (daemon log).
- [ ] End-to-end dogfood: extraction works across Rust/TS/Python/Swift; Swift stdlib filtering observed.
- [ ] Verification ledger complete; Swift behavior change noted for release notes.

---

## Execution Order & Parallelism

**Sequential: A → B → C → D → E.** The original "D parallel with B/C" was wrong — `fixtures/extraction/` is referenced by `changed.rs` (until B), `tree_sitter_real_world.rs` (until C), and `tree_sitter_certification_tests.rs` (until C), so D's fixture deletion can only run once both B and C have removed those references (Codex finding).

- **Task A first** — everything depends on the workspace resolving the git-dep.
- **Task B before Task C** — B removes the `extractors` bucket whose command calls `cargo xtask certify`; C then removes the command (no live bucket ever calls a deleted command).
- **Task D after C** — its fixture deletion needs B+C done; its doc edits have no code dependency.
- **Task E last** — branch gate, lead-owned.

**Subagent-driven note:** the safe parallel slice is narrow because the fixture/xtask coupling crosses write scopes. The low-risk parallelism is to dispatch Task D's *docs-only* edits (`CLAUDE.md`/`AGENTS.md`/`README.md`/`docs/**`, write scope = docs) concurrently with Task B (write scope = `xtask/`), then run Task D's `git rm fixtures/extraction` step only after C. If in doubt, run strictly sequentially — the wall-clock cost is small and the coupling is real.

## Rollback

Revert the Task A commit (restores `crates/julie-extractors/`, the path-dep, and the old engine version) — the only side effect is a re-revert reindex. The xtask/docs/fixtures commits revert independently.
