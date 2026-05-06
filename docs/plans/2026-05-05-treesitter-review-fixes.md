# Treesitter Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Fix the verified tree-sitter extractor and resolver review findings, then lock the missed invariants into tests.

**Architecture:** Start with behavior regressions because the current tests miss several wrong-edge cases. Keep extractor fixes local to each language module, keep resolver fixes language-gated, and only split oversized files after behavior is green so refactor noise does not hide semantic changes.

**Tech Stack:** Rust, tree-sitter extractors, Julie workspace resolver, cargo nextest, cargo xtask test tiers.

---

## Verified Finding Summary

| Finding | Verdict | Priority |
|---|---|---|
| Vue emits structured pending relationships but leaves legacy pending relationships empty | Confirmed | Critical |
| API-surface parity test omits Vue | Confirmed | Critical |
| Capability-matrix evidence test does not enforce pending parity | Confirmed | Critical |
| `std`/`core`/`alloc` namespace resolver scoring is Rust-specific but not Rust-gated | Confirmed | Critical |
| CSS animation regex can emit wrong keyframe edges and scans comments | Confirmed | Critical |
| Markdown can emit self edges and overwrites duplicate heading slugs | Confirmed | Important |
| YAML alias resolution uses substring matching for anchors | Confirmed | Important |
| QML property relationships use synthetic target IDs | Confirmed | Important |
| Rust reexport crate-root detection matches nested path windows | Confirmed | Important |
| Unconditional debug `println!` calls in bash and zig tests | Confirmed | Minor |
| `language_spec.rs` is 550 lines and `mod_tests.rs` is 3298 lines | Confirmed | Organization |
| Vue relationship helper slices strings directly instead of using extractor text helper | Confirmed | Minor |
| Rust inner doc comments `//!` and `/*!` are not recognized | Confirmed | Minor |
| Macro-generated registry bodies have cosmetic indentation drift | Confirmed | Minor |
| Lua doc-comment ordering is harmless today but fragile if style discrimination is added | Confirmed | Minor |
| CSS and Markdown duplicate `containing_symbol` logic | Confirmed | Minor |

## File Map

- Modify `crates/julie-extractors/src/registry.rs` for Vue degraded pending parity.
- Modify `crates/julie-extractors/src/tests/api_surface.rs` to include Vue in the parity case table.
- Modify `crates/julie-extractors/src/tests/capability_matrix.rs` to assert fixture-level pending parity.
- Modify `fixtures/extraction/vue/basic/expected.json` after regenerating or hand-updating the expected degraded pending list.
- Modify `src/tools/workspace/indexing/resolver/namespace.rs` for Rust-gated `std`/`core`/`alloc` scoring.
- Modify or add resolver tests near the existing namespace or relationship-precision tests.
- Modify `crates/julie-extractors/src/css/relationships.rs` and `crates/julie-extractors/src/tests/css/mod.rs`.
- Modify `crates/julie-extractors/src/markdown/relationships.rs` and `crates/julie-extractors/src/tests/markdown/relationships.rs`.
- Modify `crates/julie-extractors/src/yaml/relationships.rs`, `crates/julie-extractors/src/yaml/mod.rs`, and `crates/julie-extractors/src/tests/yaml/mod.rs`.
- Modify `crates/julie-extractors/src/qml/relationships.rs` and QML relationship tests.
- Modify `src/tools/workspace/indexing/resolver/rust_reexports.rs` and its tests.
- Modify `crates/julie-extractors/src/tests/bash/cross_file_relationships.rs` and `crates/julie-extractors/src/tests/zig/cross_file_relationships.rs` to remove noisy prints.
- Split `crates/julie-extractors/src/language_spec.rs` into a small API module plus a specs data module.
- Split `src/tests/tools/workspace/mod_tests.rs` into focused modules under `src/tests/tools/workspace/`.
- Modify `crates/julie-extractors/src/vue/relationships.rs` to use the extractor text helper.
- Modify registry macros in `crates/julie-extractors/src/registry.rs` only if the indentation fix can stay mechanical.
- Modify doc-comment style tests when adding Rust `//!` and `/*!` support.

## Task 1: Restore Pending Relationship Parity

**Files:**
- Modify `crates/julie-extractors/src/registry.rs`
- Modify `crates/julie-extractors/src/tests/api_surface.rs`
- Modify `crates/julie-extractors/src/tests/capability_matrix.rs`
- Modify `fixtures/extraction/vue/basic/expected.json`

**Work:**
- Add a failing Vue case to `test_public_api_surface_preserves_structured_pending_for_remaining_registry_wave`.
- In `extract_vue`, derive `pending_relationships` from `structured_pending_relationships.clone().into_iter().map(|pending| pending.into_pending_relationship())`.
- Add a capability-matrix helper that parses each fixture expected JSON and asserts that `pending_relationships` equals the degraded form of `structured_pending_relationships`.
- Update the Vue golden fixture so `pending_relationships` contains the `HeaderBar` pending entry.

**Acceptance Criteria:**
- The Vue API-surface case fails before the extractor fix and passes after.
- The capability-matrix test fails against the current Vue fixture and passes after the fixture and extractor are corrected.
- The degraded pending payload is not handcrafted differently from the structured payload.

## Task 2: Rust-Gate Namespace Resolver Scoring

**Files:**
- Modify `src/tools/workspace/indexing/resolver/namespace.rs`
- Add or modify resolver tests under `src/tests/tools/workspace/`

**Work:**
- Add a failing test where a non-Rust pending target has namespace root `std`, `core`, or `alloc` and a non-Rust candidate path happens to end with matching segments.
- Change `namespace::score` so the `std`/`core`/`alloc` branch only applies when `language_of(&pending.file_path) == Some("rust")` and the candidate language is Rust.
- Preserve the existing Rust behavior for `std::collections::HashMap::new` style calls.

**Acceptance Criteria:**
- Non-Rust namespace roots no longer get the Rust-specific 500-point bonus or filtering behavior.
- Existing Rust resolver tests for standard-library namespace preservation still pass.

## Task 3: Stop CSS Wrong Edges

**Files:**
- Modify `crates/julie-extractors/src/css/relationships.rs`
- Modify `crates/julie-extractors/src/tests/css/mod.rs`

**Work:**
- Add failing tests for `animation: var(--anim) 1s` with a `@keyframes var` rule and for `animation-name` inside a CSS comment.
- Prefer tree-sitter declaration traversal for `animation-name` and custom-property uses. If keeping regex as an interim implementation, restrict keyframe relationships to `animation-name:` and skip comment spans.
- Do not emit a keyframe relationship for shorthand values unless the value token is parsed well enough to identify a real animation name.

**Acceptance Criteria:**
- Existing `animation: spin 1s linear` coverage is either adjusted to an explicit supported `animation-name: spin` case or backed by a parser-aware shorthand implementation.
- Comments and CSS functions do not create relationships.

## Task 4: Fix Markdown Relationship Precision

**Files:**
- Modify `crates/julie-extractors/src/markdown/relationships.rs`
- Modify `crates/julie-extractors/src/tests/markdown/relationships.rs`

**Work:**
- Add a failing self-link test for `# Self` plus `[self](#self)`.
- Add a duplicate-heading test with two `Overview` headings and a local link from a later section.
- Add `source.id != target.id` filtering before relationship creation.
- Replace the single-value slug map with deterministic duplicate handling. Prefer a stable first-match policy that matches Markdown anchor behavior unless existing project docs expect later duplicates.

**Acceptance Criteria:**
- Self-links do not emit relationships.
- Duplicate heading resolution is deterministic and tested.

## Task 5: Fix YAML Exact Anchor Resolution

**Files:**
- Modify `crates/julie-extractors/src/yaml/relationships.rs`
- Modify `crates/julie-extractors/src/yaml/mod.rs`
- Modify `crates/julie-extractors/src/tests/yaml/mod.rs`

**Work:**
- Add a failing prefix-collision test where `*foo` must not match `&foobar`.
- Extract exact anchor names from symbol metadata or signatures with token boundaries.
- Use the same exact lookup for relationship extraction and alias identifier resolution.

**Acceptance Criteria:**
- `*foo` resolves only to `&foo`.
- Identifier `target_symbol_id` and relationship `to_symbol_id` agree.

## Task 6: Fix QML Property Relationship Targets

**Files:**
- Modify `crates/julie-extractors/src/qml/relationships.rs`
- Modify QML tests under `crates/julie-extractors/src/tests/qml/`

**Work:**
- Add a failing test proving property binding relationships target an actual `SymbolKind::Property` ID.
- Resolve member-expression properties against component-local property symbols first.
- Emit no relationship when the property cannot be resolved to a real symbol.

**Acceptance Criteria:**
- No emitted QML relationship has a synthetic `to_symbol_id` like `property_width`.
- Property binding edges point to real extracted symbols.

## Task 7: Anchor Rust Reexport Workspace Crates

**Files:**
- Modify `src/tools/workspace/indexing/resolver/rust_reexports.rs`
- Modify resolver tests under `src/tests/tools/workspace/`

**Work:**
- Add failing tests for nested paths such as `vendor/foo/src/lib.rs` and `examples/foo/src/main.rs`.
- Replace sliding-window crate checks with exact workspace-member root matching. If this resolver layer does not currently receive workspace roots, thread enough route or workspace metadata through the caller instead of guessing from path substrings.
- Keep direct reexport behavior for normal `crate::module::Item` resolution unchanged.

**Acceptance Criteria:**
- Nested vendor or example crates are not treated as workspace crate roots unless they are explicit workspace members.
- Existing Rust reexport tests remain green.

## Task 8: Remove Test Noise

**Files:**
- Modify `crates/julie-extractors/src/tests/bash/cross_file_relationships.rs`
- Modify `crates/julie-extractors/src/tests/zig/cross_file_relationships.rs`

**Work:**
- Remove unconditional `println!` blocks or gate them behind an explicit debug flag.
- Keep the assertions unchanged unless they are weak.

**Acceptance Criteria:**
- Passing tests produce no debug stdout from these files.

## Task 9: Organization Cleanup

**Files:**
- Split `crates/julie-extractors/src/language_spec.rs`
- Split `src/tests/tools/workspace/mod_tests.rs`

**Work:**
- Move the static language spec table into `crates/julie-extractors/src/language_spec/specs.rs` and keep the public types and API in `language_spec/mod.rs`.
- Split workspace tests by behavior, for example resolver, lifecycle, indexing, and registry tests.
- Keep public module paths stable.

**Acceptance Criteria:**
- New implementation files stay under 500 lines.
- Test modules stay under 1000 lines where the touched split makes that practical.
- No public API behavior changes.

## Task 10: Minor Extractor Hygiene

**Files:**
- Modify `crates/julie-extractors/src/vue/relationships.rs`
- Modify `crates/julie-extractors/src/language_spec.rs` or its split modules from Task 9
- Modify `crates/julie-extractors/src/registry.rs` only if the macro formatting change is mechanical
- Modify shared extractor helpers only if the helper remains language-agnostic

**Work:**
- Replace Vue direct byte slicing with the same defensive node-text helper pattern used by peer extractors.
- Add Rust doc-comment tests for `//!` and `/*!`, then support those forms explicitly.
- Reorder Lua block comment matching ahead of broad Lua line matching if the style list survives the Task 9 split.
- Extract duplicated CSS and Markdown `containing_symbol` logic only if the helper is generic and keeps the self-edge behavior explicit at call sites.
- Fix macro body indentation in `registry.rs` only if rustfmt or a mechanical rewrite can own the result without changing generated behavior.

**Acceptance Criteria:**
- Minor cleanup does not change extractor output except for newly supported Rust doc comments and the deliberate wrong-edge fixes from earlier tasks.
- Any shared helper remains language-agnostic.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Workers run exact tests only, for example `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`.

**Worker ceiling:** Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, or broad `cargo nextest` filters.

**Worker gate invariant:** Each worker test must prove the specific bug fixed by that task, not just that extraction still runs.

**Lead affected-change scope:** After a coherent batch, run `cargo xtask test changed`.

**Branch gate:** Before handoff, run `cargo xtask test dev`.

**Specialist gates:** Run `cargo xtask test bucket extractors` after extractor tasks. Run any exact resolver tests added for namespace and reexport changes before the lead affected-change scope.

**Escalation triggers:** If crate-root anchoring requires new workspace metadata flow, treat Task 7 as strategy-tier work before implementation. If CSS shorthand parsing grows beyond a local parser traversal, split it into a separate scoped task.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless the task explicitly updates that gate.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Use `gpt-5.5` medium or high for decomposition and lead review.

**Implementation tier:** Use `gpt-5.4-mini` xhigh only for boxed-in local edits with narrow tests.

**Coupled implementation tier:** Use `gpt-5.3-codex` high for resolver changes and hidden-invariant extractor fixes.

**Gate review:** Use `gpt-5.3-codex` high for failed-test interpretation or review-finding triage.

**Escalation tier:** Use `gpt-5.5` high or xhigh if Task 7 requires changing resolver contracts or workspace metadata routing.

**Worker eligibility:** Tasks 1, 3, 4, 5, 6, and 8 can be delegated with disjoint file ownership. Tasks 2 and 7 need stronger review because they affect resolver semantics. Task 9 should happen after behavior fixes are green.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
