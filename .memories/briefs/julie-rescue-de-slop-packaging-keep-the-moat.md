---
id: julie-rescue-de-slop-packaging-keep-the-moat
title: "Julie Rescue: de-slop packaging, keep the moat"
status: active
created: 2026-06-03T14:35:36.960Z
updated: 2026-06-03T14:35:36.960Z
tags: []
---

## Decision (2026-06-03)
**Save Julie in place. Do NOT switch to Miller (.NET) or eros (Python).** The momentum problem is packaging, not Rust (`cargo check` is 3.6s). All three projects share the same `julie-extract` binary, so the only question is the *host* layer — and Miller/eros are the two "rewrite the host" experiments already built, both of which dropped Julie's moat (semantic/hybrid search + centrality reranking + token-budgeted get_context + 34-lang breadth + shipped plugin/CLI) to look fast. We decline that trade and harvest their good ideas instead.

## The two things strangling iteration
1. **Relink tax** — `src/lib.rs` pulls all 126k test LOC into ONE test binary that relinks on every edit; even a single targeted test relinks the monolith. Cure = per-crate test binaries via a workspace split.
2. **Daemon slop** — `src/daemon` (10.1k) + `src/adapter` (1.3k), 12 test markers; ~7-9k is "we run a bespoke daemon" tax and the home of the unsolved fast_search-hang/deep_dive-disconnect.

## 4-phase program (doc: docs/plans/2026-06-03-julie-rescue-design.md, branch julie-rescue)
1. **Untangle + leaf-crate split** (START HERE) — sever measured back-edges, MERGE search+analysis (they cycle on language-config types), extract `julie-core` + `julie-index`.
2. Peel off julie-tools / julie-runtime / julie-pipeline.
3. **Daemon teardown** → WAL readers + leader-election lock + ONE resident embedding-host (shared sidecar; per-session sidecars would OOM VRAM). Open sub-fork: shared on-disk Tantivy cross-process vs Miller's rebuild-in-memory model.
4. Tools 12→7 (edit trio→edit(op); fast_refs+call_path→trace) + harvests: build-failing convention test gate (Miller), token-ROI telemetry (eros), APPROVED-tool-list test.

## Caveat to resolve early
No shared-corpus retrieval bakeoff exists — Julie's moat is asserted, not measured vs Miller BM25+bridge / eros lancedb-hybrid. eros has a bakeoff harness with a julie-cli baseline. Run it before sinking the broad restructuring effort into the rescue; it also seeds Phase 4's promotion gate.

## Next step
User reviewing the design doc, then → writing-plans to turn Phase 1 into an implementation plan.
