# Blast Radius Review Fix Design

**Date:** 2026-04-21
**Status:** Design, ready for implementation planning

## Summary

This design closes the gaps found in the post-ship review of `blast_radius`, `spillover_get`, and task-aware `get_context`.

The shipped feature is close, but five problems need cleanup before the surface is trustworthy:

1. identifier-derived impact edges lose relationship meaning and can point `why:` text at the wrong target
2. spillover paging is not idempotent because page fetches mint new handles
3. lenient MCP parsing covers arrays and integers, but misses boolean knobs on the new surface
4. skills and instructions teach agents flows the tool surface cannot satisfy
5. `get_context/pipeline.rs` is still too large and the failing-test linkage path is parked in the wrong file

The goal is not a redesign. The goal is to make the current tool set honest, deterministic, agent-usable, and cheap enough in context to earn its cost.

## Goals

1. Preserve identifier edge semantics across both `blast_radius` and `get_context`.
2. Make spillover paging stable for repeated fetches of the same handle.
3. Accept stringified boolean MCP params for `include_tests` and `prefer_tests`.
4. Align docs, skills, and server instructions with the real tool contract.
5. Split `get_context/pipeline.rs` under the repo file-size target while keeping behavior intact.
6. Add regression tests for each reviewed defect.

## Non-goals

- No new public tools.
- No ranking overhaul beyond fixing incorrect identifier-edge labeling and propagation.
- No Git revision support for `blast_radius` in this batch.
- No spillover persistence across sessions or daemon restarts.
- No broad retrieval redesign outside the touched paths.

## Findings That Drive This Design

### 1. Identifier fallback is semantically lossy

Today `identifier_incoming_edges` collapses all identifier-derived edges to `RelationshipKind::References`. That keeps the walk alive on languages such as TypeScript, but it throws away the distinction between calls, imports, and type usage.

`walk_impacts` then assigns identifier-derived edges to `frontier_ids.first()` as the `via` target. That is wrong for multi-seed walks and can be wrong for deeper hops.

This batch needs a shared identifier-edge helper that preserves:

- `containing_symbol_id`
- identifier-derived relationship kind
- target symbol id when it is known

Relevant code:

- [src/database/impact_graph.rs](/Users/murphy/source/julie/src/database/impact_graph.rs:20)
- [src/tools/impact/walk.rs](/Users/murphy/source/julie/src/tools/impact/walk.rs:65)
- [src/tools/get_context/pipeline.rs](/Users/murphy/source/julie/src/tools/get_context/pipeline.rs:136)

### 2. Spillover is not idempotent

The spillover store issues a new UUID handle for each page fetch. The row slice stays stable, but the textual response changes because the continuation handle changes.

That conflicts with the current `idempotent_hint = true` tool annotation and with the deterministic positioning of `blast_radius`.

Relevant code:

- [src/tools/spillover/store.rs](/Users/murphy/source/julie/src/tools/spillover/store.rs:129)
- [src/handler.rs](/Users/murphy/source/julie/src/handler.rs:2617)
- [src/handler.rs](/Users/murphy/source/julie/src/handler.rs:2658)

### 3. Lenient serde coverage is incomplete

The repo added lenient bool deserializers, but the two new tool knobs that need them are still on strict serde paths:

- `BlastRadiusTool.include_tests`
- `GetContextTool.prefer_tests`

That leaves real MCP clients one stringified boolean away from a bad request.

Relevant code:

- [src/tools/impact/mod.rs](/Users/murphy/source/julie/src/tools/impact/mod.rs:77)
- [src/tools/get_context/mod.rs](/Users/murphy/source/julie/src/tools/get_context/mod.rs:86)
- [src/utils/serde_lenient.rs](/Users/murphy/source/julie/src/utils/serde_lenient.rs:119)
- [src/utils/serde_lenient.rs](/Users/murphy/source/julie/src/utils/serde_lenient.rs:338)

### 4. Agent-facing docs drifted from implementation

The current skills and instructions teach flows that are either incomplete or wrong:

- `impact-analysis` asks for `symbol_ids` while taking a `<symbol_name>` argument and does not explain how to resolve the ID
- the same skill documents revision ranges as though Git refs are valid inputs
- `spillover_get(handle)` is documented with the wrong parameter name in the server instructions
- `get_context` task-shaping docs omit `max_hops` and `prefer_tests`
- cross-workspace guidance points at `manage_workspace`, but the affected skills do not allow that tool

Relevant docs:

- [JULIE_AGENT_INSTRUCTIONS.md](/Users/murphy/source/julie/JULIE_AGENT_INSTRUCTIONS.md:18)
- [JULIE_AGENT_INSTRUCTIONS.md](/Users/murphy/source/julie/JULIE_AGENT_INSTRUCTIONS.md:53)
- [.claude/skills/impact-analysis/SKILL.md](/Users/murphy/source/julie/.claude/skills/impact-analysis/SKILL.md:5)
- [.claude/skills/explore-area/SKILL.md](/Users/murphy/source/julie/.claude/skills/explore-area/SKILL.md:32)

### 5. `get_context/pipeline.rs` is still a blob

The recent refactor helped, but `pipeline.rs` is still 747 lines. That is above the repo target and above the fixup target from the prior plan set.

The best cut lines are already obvious:

- second-hop decision and merge logic belongs in `second_hop.rs`
- failing-test linkage hydration belongs in `task_signals.rs`
- spillover formatting and neighbor-overflow behavior stays where it is unless another split becomes necessary after the first pass

Relevant code:

- [src/tools/get_context/pipeline.rs](/Users/murphy/source/julie/src/tools/get_context/pipeline.rs:1)
- [src/tools/get_context/task_signals.rs](/Users/murphy/source/julie/src/tools/get_context/task_signals.rs:1)
- [src/tools/get_context/second_hop.rs](/Users/murphy/source/julie/src/tools/get_context/second_hop.rs:1)

## Design Decisions

### 1. Replace the current tuple helper with a typed identifier-edge model

`src/database/impact_graph.rs` should expose a typed row that preserves:

- `container_id`
- `relationship_kind`
- `target_id: Option<String>`

The helper should derive `relationship_kind` from identifier kind:

- `call` -> `RelationshipKind::Calls`
- `import` -> `RelationshipKind::Imports`
- `type_usage` -> `RelationshipKind::References`

If an identifier has `target_symbol_id`, the helper should carry it through. If the identifier was matched by name only, `target_id` can be `None`.

This model feeds both callers:

- `blast_radius` uses it to keep ranking and `why:` text grounded in the right target
- `get_context` uses it to keep identifier-derived neighbors aligned with impact-walk semantics

This keeps one implementation path without flattening everything into `References`.

### 2. Make spillover paging replayable

Fetching a page for handle `X` should always produce the same page body and the same continuation handle for the same request parameters.

The simplest design is to make the continuation handle deterministic from the stored entry and the page boundary, not minted on demand through a fresh UUID. Two workable shapes exist:

1. pre-materialize a stable chain of page entries when rows are first stored
2. derive child handles from parent handle plus offset in a deterministic way

Recommendation: derive child handles from `(prefix, owner_session_id, offset, default_limit, title)` using a stable hash. That avoids pre-building chains and keeps repeated fetches idempotent.

Scope rules:

- idempotence is per session and per stored spillover entry
- no cross-session reuse
- TTL expiry behavior stays intact

### 3. Apply lenient bool serde to new tool knobs

`include_tests` and `prefer_tests` should accept:

- JSON booleans
- `"true"` / `"false"`
- `"1"` / `"0"`

This should use the existing lenient bool helpers, not a new parser.

Tests need direct coverage for:

- `BlastRadiusTool { include_tests: "true" }`
- `GetContextTool { prefer_tests: "true" }`

### 4. Rewrite the agent-facing guidance around seed selection

`impact-analysis` should stop pretending it can start from a raw symbol name with `blast_radius(symbol_ids=...)` and no discovery step.

The revised flow should be:

1. resolve the symbol with `deep_dive` or `fast_search(search_target="definitions")`
2. run `blast_radius(symbol_ids=[...])` once the symbol id is known
3. use `spillover_get(spillover_handle=...)` if pagination appears
4. drill into high-risk callers with `deep_dive` and `fast_refs`

Revision-range docs should say "canonical revision numbers", not Git refs. If we want Git-aware revision support later, that needs its own design and tool path.

### 5. Bring `get_context` back under the repo limit

The refactor target for this batch is:

- `src/tools/get_context/pipeline.rs` under 500 lines if the cuts land cleanly
- fallback target under 600 if one more split is needed after review

The first split set:

- move second-hop logic into `second_hop.rs`
- move failing-test hydration and test-name matching into `task_signals.rs`

Behavior must remain unchanged except for any fixes needed to keep imports and test surfaces clean.

## Data Flow

### Impact walk

1. Seed resolution builds the changed-symbol set.
2. Relationship edges are collected from the relationships table.
3. Identifier-derived edges are collected through the shared helper with preserved semantics.
4. Relationship edges win when both sources describe the same caller.
5. Ranked impacts carry the right relationship kind and the right `via` target.
6. Formatter output reflects those corrected reasons.

### Spillover

1. Tool stores overflow rows once.
2. First response returns a stable `spillover_handle`.
3. `spillover_get(spillover_handle=...)` computes the same page slice and same next handle on replay.
4. TTL expiry or foreign-session access still fails.

### Task-shaped context

1. Tool params deserialize with lenient arrays, ints, and bools.
2. `TaskSignals::from_tool()` owns task-shaped parsing and failing-test hydration helpers.
3. `pipeline.rs` orchestrates search, allocation, spillover, and formatting without swallowing helper code that belongs elsewhere.

## Testing Strategy

### New or expanded tests

- `blast_radius` regression for identifier-derived relationship kinds and correct `why:` target labeling
- `blast_radius` determinism test that covers overflow output, not only the first page
- spillover idempotence test that fetches the same page twice and asserts identical output
- lenient serde tests for stringified booleans on both new knobs
- doc-facing metadata tests if tool-target metadata changes
- `get_context` task-input tests updated only if refactor moves symbols or helper paths

### Required test runs during implementation

- narrow RED/GREEN loops per touched subsystem
- `cargo nextest run --lib blast_radius`
- `cargo nextest run --lib spillover_tests`
- `cargo nextest run --lib get_context_task_inputs_tests`
- `cargo nextest run --lib serde_lenient_tests`
- `cargo xtask test changed`
- `cargo xtask test dev`

## Acceptance Criteria

- [ ] `blast_radius` preserves identifier-derived relationship meaning across impact walks.
- [ ] `blast_radius` `why:` text points at the correct upstream symbol when identifier-derived edges are involved.
- [ ] repeated `spillover_get` calls for the same handle return byte-identical output
- [ ] repeated overflowing `blast_radius` calls return byte-identical first pages
- [ ] `include_tests` and `prefer_tests` accept stringified booleans
- [ ] `impact-analysis` teaches a workable symbol-to-id flow
- [ ] revision-range docs describe canonical revision numbers, not Git refs
- [ ] `JULIE_AGENT_INSTRUCTIONS.md` uses `spillover_handle`, not `handle`
- [ ] cross-workspace guidance and skill frontmatter agree
- [ ] `src/tools/get_context/pipeline.rs` lands under the repo target or has an explicit, reviewable reason if one more split is required

## Risks

### 1. Identifier-edge semantics can still over-match on name collisions

The shared helper should preserve `target_symbol_id` when present, but name-based fallback can still over-match in collision-heavy code. This batch should fix semantic flattening, not promise perfect symbol disambiguation for unresolved identifiers.

### 2. Stable spillover handles must not leak session scope

Deterministic handle generation cannot drop the session guard. Same rows, wrong session, should still fail.

### 3. Refactor drift

The `get_context` split is meant to shrink the file, not to sneak in behavior changes. Review and tests need to treat this as a mechanical cut unless a fix is required.

## Recommendation

Implement this as one focused cleanup batch. The bugs are real, the fixes are local, and the user-facing value is clear. There is no reason to leave the current edge cases in place and hope agents work around them. That would be lazy engineering wearing a fake mustache.
