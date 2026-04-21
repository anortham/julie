# Blast Radius Review Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:team-driven-development (on Claude Code) or razorback:subagent-driven-development (elsewhere) to implement this plan. Fall back to razorback:executing-plans for single-task or tightly-sequential plans.

**Goal:** Fix the reviewed defects in `blast_radius`, `spillover_get`, and task-aware `get_context`, then align docs and skills with the real tool contract.

**Architecture:** Keep the current tool surface. Tighten the shared graph-walk substrate, make spillover paging stable, extend lenient serde to the missing boolean knobs, then clean the agent-facing docs and shrink `get_context/pipeline.rs` under the repo leash. The batch stays local to the current implementation paths. No redesign, no new public tool, no Git-ref revision support.

**Tech Stack:** Rust, rusqlite, rmcp, Markdown docs and skills, `cargo nextest`, `cargo xtask`

---

## Execution Notes

- Use TDD for each behavioral fix. Write the failing test first, run the narrow test to verify RED, implement the smallest fix, then rerun the same narrow test for GREEN.
- Keep the work in this order: impact semantics, spillover idempotence, lenient bool parsing, `get_context` refactor, docs and skills, then batch verification.
- Unless the user authorizes delegation in this thread, execute this plan in one session.
- Commit after each completed task or tight pair of linked tasks.

### Task 1: Preserve identifier-edge semantics in shared graph walking

**Files:**
- Modify: `src/database/impact_graph.rs`
- Modify: `src/database/mod.rs`
- Modify: `src/tools/impact/walk.rs`
- Modify: `src/tools/impact/ranking.rs`
- Modify: `src/tools/impact/mod.rs`
- Modify: `src/tools/get_context/pipeline.rs`
- Test: `src/tests/tools/blast_radius_determinism_tests.rs`
- Test: `src/tests/tools/blast_radius_tests.rs`

**What to build:** Replace the current identifier-edge helper with a typed representation that preserves `container_id`, identifier-derived relationship kind, and `target_id` when resolution is known. Feed that helper into both `walk_impacts` and `expand_graph_from_ids` so impact and context walks stop drifting apart.

**Approach:** Map identifier kinds to graph semantics with intent preserved: `call` becomes `Calls`, `import` becomes `Imports`, `type_usage` stays a weaker `References` signal. In `walk_impacts`, use the preserved target when building the `why:` label instead of pinning everything to `frontier_ids.first()`. Relationship rows still win over identifier-derived rows when both exist for the same source symbol.

**Acceptance criteria:**
- [ ] `blast_radius` no longer flattens identifier-derived callers into generic references when the identifier kind carries stronger meaning.
- [ ] `blast_radius` uses the correct upstream target in `why:` text for multi-seed and identifier-heavy walks.
- [ ] `get_context` and `blast_radius` share one identifier-edge helper, not two near-miss copies.
- [ ] Narrow tests cover the semantic fix and pass.

### Task 2: Make spillover paging replayable and idempotent

**Files:**
- Modify: `src/tools/spillover/store.rs`
- Modify: `src/tools/spillover/mod.rs`
- Modify: `src/tools/impact/mod.rs`
- Modify: `src/tests/tools/spillover_tests.rs`
- Test: `src/tests/tools/blast_radius_tests.rs`
- Test: `src/tests/tools/blast_radius_determinism_tests.rs`

**What to build:** Change spillover paging so the same handle and page request yield byte-identical output, including the continuation handle. Keep TTL expiry and foreign-session rejection intact.

**Approach:** Replace the UUID-per-page-follow-up behavior with stable continuation-handle generation tied to the stored spillover entry and next offset. The handle must stay session-scoped. Repeated calls on the same handle should not mutate the visible result. If the current annotation set stays in place, the implementation must honor it instead of winking at it.

**Acceptance criteria:**
- [ ] Repeated `spillover_get(spillover_handle=..., limit=...)` calls return identical output.
- [ ] Repeated overflowing `blast_radius` calls return identical first-page output.
- [ ] TTL expiry still fails with the same error shape.
- [ ] Foreign-session access still fails.

### Task 3: Extend lenient serde to the missing boolean knobs

**Files:**
- Modify: `src/tools/impact/mod.rs`
- Modify: `src/tools/get_context/mod.rs`
- Test: `src/tests/core/serde_lenient_tests.rs`

**What to build:** Apply the existing lenient bool deserializers to `BlastRadiusTool.include_tests` and `GetContextTool.prefer_tests`.

**Approach:** Reuse `deserialize_bool_lenient` or `deserialize_option_bool_lenient` as appropriate. Do not invent a new parser. Add direct regression coverage for stringified booleans on both fields.

**Acceptance criteria:**
- [ ] `"include_tests": "true"` deserializes cleanly for `BlastRadiusTool`.
- [ ] `"prefer_tests": "true"` deserializes cleanly for `GetContextTool`.
- [ ] Existing raw boolean behavior still works.
- [ ] Narrow serde tests pass.

### Task 4: Split `get_context/pipeline.rs` back under the repo limit

**Files:**
- Modify: `src/tools/get_context/pipeline.rs`
- Modify: `src/tools/get_context/task_signals.rs`
- Modify: `src/tools/get_context/second_hop.rs`
- Modify: `src/tools/get_context/mod.rs`
- Test: `src/tests/tools/get_context_task_inputs_tests.rs`

**What to build:** Move second-hop logic and failing-test hydration helpers out of `pipeline.rs`, then trim the file under the repo target without changing behavior.

**Approach:** Keep orchestration in `pipeline.rs`. Keep second-hop policy in `second_hop.rs`. Keep task-signal parsing and failing-test linkage hydration in `task_signals.rs`. If one more split is needed after those moves, make it deliberate and keep the result cohesive. Do not smuggle in behavior changes under cover of refactor dust.

**Acceptance criteria:**
- [ ] `src/tools/get_context/pipeline.rs` lands under 500 lines, or under 600 with an explicit note in the final summary about what blocked the last cut.
- [ ] Existing task-input behavior stays intact.
- [ ] `get_context_task_inputs_tests` pass without semantic regressions.

### Task 5: Align tool descriptions, instructions, and skills with reality

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: `src/handler.rs`
- Modify: `.claude/skills/impact-analysis/SKILL.md`
- Modify: `.claude/skills/explore-area/SKILL.md`
- Modify: `/Users/murphy/source/julie-plugin/skills/impact-analysis/SKILL.md`
- Modify: `/Users/murphy/source/julie-plugin/skills/explore-area/SKILL.md`

**What to build:** Fix the agent-facing surface so it teaches workable flows and the real parameter names.

**Approach:** In `impact-analysis`, teach the symbol-name-to-symbol-id resolution step before `blast_radius(symbol_ids=...)`, and describe revision ranges as canonical revision numbers. In `explore-area` and the server instructions, document `max_hops` and `prefer_tests` and use `spillover_handle`, not `handle`. If cross-workspace guidance mentions `manage_workspace`, the skill frontmatter must allow it.

**Acceptance criteria:**
- [ ] `impact-analysis` no longer implies that raw symbol names can go straight into `symbol_ids`.
- [ ] revision-range docs say canonical revision numbers, not Git refs.
- [ ] `JULIE_AGENT_INSTRUCTIONS.md` uses `spillover_handle`.
- [ ] `get_context` task-input guidance includes `max_hops` and `prefer_tests`.
- [ ] skill frontmatter matches the guidance it gives.
- [ ] plugin skill copies match the repo skill copies.

### Task 6: Batch verification and dogfooding

**Files:**
- Verify only

**What to build:** Run the focused verification ladder, then dogfood the touched flows against the repo.

**Approach:** Use the narrow test slices during task work, then run the calibrated batch checks once after the code and docs land. Dogfood the fixed paths through the live tool surface, not through memory and wishful thinking.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib blast_radius`
- [ ] `cargo nextest run --lib spillover_tests`
- [ ] `cargo nextest run --lib get_context_task_inputs_tests`
- [ ] `cargo nextest run --lib serde_lenient_tests`
- [ ] `cargo xtask test changed`
- [ ] `cargo xtask test dev`
- [ ] dogfood check: `blast_radius(file_paths=["src/tools/spillover/store.rs"])` surfaces real callers
- [ ] dogfood check: repeated spillover fetches are byte-identical
- [ ] dogfood check: `get_context(query="spillover", edited_files=["src/tools/spillover/store.rs"], max_hops=2)` still behaves coherently

## Review Focus

- Watch for name-collision fallout in identifier-derived edges. The fix should preserve more signal, not pretend unresolved identifiers are fully disambiguated.
- Watch for stable spillover handles accidentally weakening the session boundary.
- Watch for docs claiming Git-ref support. That would be the same bug wearing a fake beard.
