# Blast Radius & Task Context Fixup Plan

> **For Claude:** REQUIRED SUB-SKILL: Use @razorback:subagent-driven-development to execute this plan. Dispatch each task to a fresh subagent with inline review by the lead.

**Goal:** Fix correctness, determinism, and documentation gaps in the shipped blast_radius, spillover_get, and task-aware get_context surface.

**Background:** Post-ship review (self + Codex second opinion) found one real correctness bug, two determinism bugs, a plan-compliance slip on file size, and stale skills. No architecture problem. See conversation of 2026-04-21 for full findings.

**Tech stack:** Rust, rusqlite, `cargo nextest`, `cargo xtask`.

---

## Execution Notes

- Tasks A, B, C, D touch disjoint files and can be dispatched in parallel.
- Each subagent runs ONLY the narrowest test for its change, per `CLAUDE.md`. The lead runs `cargo xtask test changed` after the batch, then `cargo xtask test dev` before handoff.
- Keep `@razorback:test-driven-development` discipline: RED first for behavioral fixes (Task A, Task B). Task C is a refactor (no new tests, move existing). Task D is docs (no tests).
- No codehealth ranking, no Rust-only heuristics, no cross-workspace assumptions in any fix.

---

## Task A: Fix impact module correctness and determinism

**Files:**
- Create: `src/database/impact_graph.rs` (shared identifier-expansion helper)
- Modify: `src/tools/impact/walk.rs`
- Modify: `src/tools/impact/mod.rs`
- Modify: `src/tools/impact/formatting.rs`
- Modify: `src/database/mod.rs` (wire new module)
- Modify: `src/tools/get_context/pipeline.rs` (use shared helper in place of inline block at lines 135-158)
- Modify: `src/tests/tools/blast_radius_tests.rs`
- Add: `src/tests/tools/blast_radius_determinism_tests.rs`

**What to build:**

1. **Identifier-aware walk (Fix 1).** Extract the identifier-expansion block from `get_context/pipeline.rs:135-158` into `src/database/impact_graph.rs` as a helper that returns `(neighbor_id, relationship_kind, direction)` tuples derived from identifiers with kind in `["type_usage", "import", "call"]`. Call it from `walk_impacts` after `get_relationships_to_symbols`, merging results with relationship-based entries taking priority (first-seen wins, same as pipeline.rs already does). Call it from `get_context/pipeline.rs` too so one code path feeds both tools.

2. **Deterministic, narrowed likely_tests (Fix 2).** In `collect_likely_tests`:
   - Prefer identifiers with `target_symbol_id` in `seed_ids` (resolved matches) over name-based matches.
   - Before truncation, rank by `IdentifierRef.confidence` desc, then `containing_symbol.file_path` asc, then `containing_symbol.id` asc. Tie-break is total and stable.
   - Add `ORDER BY` to the underlying query (or sort in Rust) so SQLite's row order stops leaking in.
   - Stop mixing `linked_tests` (bare names) and `linked_test_paths` (paths) into one `Vec<String>`. Either emit both lists under separate headings (`Likely tests`, `Related test symbols`) or drop the bare names from the primary list. Pick the two-heading option — names are still useful context.

3. **Dedupe `relationship_priority` (Fix 5).** Delete the copy in `walk.rs:139-149`. Import `ranking::relationship_priority`. Verify walk's `relation_order` still compiles using the shared function.

4. **Default blast_radius to compact (Fix 8).** Either change `SpilloverFormat::from_option` default to compact, or override in `impact/mod.rs::run_with_db` so `format: None` maps to `Compact` for this tool. Leave `spillover_get`'s default alone — it often serves paged human-readable follow-ups.

**Acceptance criteria:**
- [ ] `blast_radius(file_paths=["src/tools/spillover/store.rs"])` surfaces at least the handler.rs, pipeline.rs, and impact/mod.rs callers shown by `fast_refs(symbol="SpilloverStore")`.
- [ ] Two identical back-to-back `blast_radius` calls produce byte-identical output.
- [ ] "Likely tests" output contains only file paths. If bare test names are included, they appear under a separate "Related test symbols" heading.
- [ ] `walk_impacts` uses the shared helper; `pipeline.rs::expand_graph_from_ids` uses the same helper; no identifier-expansion logic is duplicated.
- [ ] `relationship_priority` exists in exactly one place.
- [ ] `blast_radius(..., format: None)` returns compact-format output.
- [ ] Narrow tests pass: `cargo nextest run --lib test_blast_radius_ranks_direct_callers_and_uses_spillover`, plus new determinism test.

## Task B: Stabilize test_linkage name-match fallback

**Files:**
- Modify: `src/analysis/test_linkage.rs`
- Modify: `src/tests/analysis/test_linkage_tests.rs` (add determinism test)

**What to build:**

Add explicit `ORDER BY s_prod.id, s_prod.file_path` to stmt3 (`src/analysis/test_linkage.rs:181-194`) so `max_by_key` sees a stable row order when directory-depth and name-bonus scores tie. Also make the `max_by_key` tie-breaker explicit: when multiple candidates tie on `dir_score + name_bonus`, pick the one with the smallest `prod_id` lexicographically. This removes the residual nondeterminism Codex flagged.

**Acceptance criteria:**
- [ ] Two consecutive runs of `compute_test_linkage` on the same database produce identical `linked_tests`, `linked_test_paths`, and `evidence_sources` in every row.
- [ ] Determinism test asserts the ordering directly.
- [ ] Narrow test passes: `cargo nextest run --lib test_linkage`.

## Task C: Split get_context pipeline back under the leash

**Files:**
- Create: `src/tools/get_context/second_hop.rs`
- Modify: `src/tools/get_context/pipeline.rs`
- Modify: `src/tools/get_context/task_signals.rs`
- Modify: `src/tools/get_context/mod.rs` (register new module)

**What to build:**

Pure refactor — no behavior change.

- Move `should_expand_second_hop`, `select_second_hop_seeds`, and `merge_expansions` into `second_hop.rs`. Re-export if anything outside the module used them (nothing currently does, they're all `fn` / `pub(crate)`).
- Move `hydrate_failing_test_links` and `test_name_matches_signal` into `task_signals.rs` next to `TaskSignals`. Keep the `pub(crate)` visibility pipeline.rs needs.
- Target: `pipeline.rs` under 600 lines. No loss of test coverage — existing tests should pass untouched.

**Acceptance criteria:**
- [ ] `wc -l src/tools/get_context/pipeline.rs` returns under 600.
- [ ] `cargo xtask test changed` passes (scoped to get_context tests at minimum).
- [ ] No public API changes; no test file modifications needed beyond possibly updating `use` paths.

## Task D: Update Julie skills for the new surface

**Files:**
- Modify: `.claude/skills/impact-analysis/SKILL.md`
- Modify: `.claude/skills/explore-area/SKILL.md`
- Modify: `~/source/julie-plugin/skills/impact-analysis/SKILL.md` (sync)
- Modify: `~/source/julie-plugin/skills/explore-area/SKILL.md` (sync)

**What to build:**

**`impact-analysis` skill:**
- Add `mcp__julie__blast_radius` and `mcp__julie__spillover_get` to the `allowed-tools` frontmatter line.
- Replace the current Step 0/1/2 chain (`get_context → fast_refs → deep_dive`) with: Step 1 `blast_radius` for one-shot impact, Step 2 `fast_refs`/`deep_dive` for drill-down on individual high-risk callers, Step 3 `spillover_get` for long impact lists.
- Keep the risk-categorization section; it's still useful after blast_radius.
- Mention revision-range seeds (`from_revision`, `to_revision`) as the flow for "what changed since last deploy?"

**`explore-area` skill:**
- Document task inputs on `get_context`: when the user has `edited_files`, a `stack_trace`, or a `failing_test`, pass those to bias pivot selection.
- Document `max_hops=2` as the knob for bounded second-hop expansion when first-hop is thin.
- Document `prefer_tests=true` for when the user wants test-linked symbols in neighbor slots.
- Mention `spillover_get` for when `get_context` returns a `spillover_handle`.

**Acceptance criteria:**
- [ ] Both skills reference the new tools in their `allowed-tools` frontmatter.
- [ ] `impact-analysis` leads with `blast_radius`, not the manual three-step chain.
- [ ] `explore-area` documents all six new `get_context` task inputs.
- [ ] Plugin copies match source copies byte-for-byte.

---

## Final Verification

- Run narrow tests per task.
- Run `cargo xtask test changed`.
- Run `cargo xtask test dev`.
- Dogfood the fixed flow:
  - `blast_radius(file_paths=["src/tools/spillover/store.rs"])` — must surface real callers.
  - Run the same call twice — outputs must match byte-for-byte.
  - `blast_radius(from_revision=X, to_revision=Y)` for a known revision pair in this repo's history.
  - `get_context(query="spillover", edited_files=["src/tools/spillover/store.rs"], max_hops=2)` — verify second-hop only fires when first-hop is thin.
- Load the updated `impact-analysis` skill and run it end-to-end against a real target symbol to confirm the new tool chain is coherent.

## Review Gate

- No external adversarial review needed for this batch; it's a focused fixup of an already-reviewed feature. The lead does inline review per subagent-driven-development.
- If any P0 acceptance criterion cannot be met, stop and escalate rather than ship partial.
