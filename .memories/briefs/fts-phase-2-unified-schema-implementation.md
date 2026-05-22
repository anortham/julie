---
id: fts-phase-2-unified-schema-implementation
title: FTS Phase 2 — Unified Schema Implementation
status: active
created: 2026-05-22T02:19:12.560Z
updated: 2026-05-22T02:19:12.560Z
tags:
  - fts-ranking
  - phase-2
  - unified-schema
  - search
  - tantivy
  - implementation
---

# FTS Phase 2 — Unified Schema Implementation

## Status
Spec approved at commit `70c7c27f` on `main`. Writing the implementation plan now.
Implementation directive (this session): write plan → codex-cli review → update → implement → codex-cli review → fix. No deferring, no follow-ups, finish in this session.

## Spec
`docs/plans/2026-05-21-fts-phase2-unified-schema-design.md`

## Goal
Close the FTS ranking gap from Julie 267/406 top1 → ≥ 350/406 (stretch ≥ 370/406, vs Eros lancedb-fts at 374). Structural fix: unify the two Tantivy doc types (`SymbolDocument` + `FileDocument`) into one `search_doc` schema with a `kind` discriminator, drop `search_target` dispatch, collapse rerankers, simplify the tokenizer.

## Decisions on the three pushback points (from spec author)
1. **Acceptance gate**: hold at top1 ≥ 350/406, stretch ≥ 370/406. Eros lancedb-fts = 374. Softer floor invites in-scope regressions; tighter floor is unjustified before measurement.
2. **`relationship_text` on file-rows**: leave EMPTY from start. Symbol-row relationships carry the dominant signal; aggregation can be added later with measurement. Simpler v1.
3. **Tokenizer ablation env vars**: DELETE `JULIE_ABLATE_STEMMING` and `JULIE_ABLATE_CAMEL_EMIT` (nothing to gate post-Phase-2). KEEP the matrix harness scaffold (`xtask/src/search_matrix.rs`, `Ablation` enum, `--ablation` CLI flag) as the future tuning vehicle.

## Acceptance gates (from spec section 130)
1. `search_target` is gone from `src/`, `xtask/`, agent instructions, skills, docs
2. One Tantivy doc type (`SearchDocument` replaces `SymbolDocument` + `FileDocument`)
3. One query path (`execute_search` replaces 3 per-target functions)
4. One reranker (Phase 1 cross-target helpers folded in)
5. Tokenizer simplified (no stemming, CamelCase only in `pretokenized_code` at index time, env vars gone)
6. Migration transparent via `SEARCH_COMPAT_MARKER_VERSION` bump 3 → 4
7. **Eros bakeoff**: top1 ≥ 350/406 against current main HEAD baseline
8. `cargo xtask test dev` + `cargo xtask test dogfood` green
9. `cargo xtask search-matrix run --profile smoke` green

## Plan ordering (from spec section 159)
Big-bang rewrite, single commit chain. Each commit must `cargo check` clean. Compat-marker handles in-place upgrade.

## First task (required by spec)
Run Eros bakeoff against `main` HEAD `722bdee5` (the merge-base) to establish baseline, ledger the number, then iterate Phase 2 to ≥ 350.

## Workflow this session
1. Draft plan via `razorback:writing-plans`
2. Codex review of plan
3. Update plan based on review
4. Execute via `razorback:subagent-driven-development`
5. Codex review of implementation
6. Fix and merge

