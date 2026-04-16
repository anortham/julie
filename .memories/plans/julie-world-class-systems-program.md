---
id: julie-world-class-systems-program
title: Julie World-Class Systems Program
status: active
created: 2026-04-16T07:23:42.025Z
updated: 2026-04-16T07:23:42.025Z
tags:
  - architecture
  - daemon
  - indexing
  - sqlite
  - tantivy
  - embeddings
  - dashboard
  - windows
---

# Julie World-Class Systems Program

## Goal
Rebuild Julie's core systems into a high-trust foundation for AI agents by prioritizing correctness, reliability, repairability, and cross-platform behavior over feature work.

## North Star
- SQLite is the canonical source of truth
- Tantivy, vectors, and analysis outputs are rebuildable projections
- The dashboard is the visible face of runtime truth
- Windows behavior is designed, not patched

## Plan Set
- `docs/plans/2026-04-16-julie-world-class-systems-program-plan.md`
- `docs/plans/2026-04-16-julie-control-plane-lifecycle-implementation-plan.md`
- `docs/plans/2026-04-16-julie-indexing-engine-implementation-plan.md`
- `docs/plans/2026-04-16-julie-canonical-storage-projections-implementation-plan.md`
- `docs/plans/2026-04-16-julie-embedding-runtime-implementation-plan.md`

## Execution Order
1. Shared health, revision, dashboard, and harness baseline
2. Control-plane lifecycle refactor
3. Indexing-engine unification
4. Canonical storage and projection redesign
5. Embedding runtime contract and degraded-mode cleanup
6. Convergence, rebuild, and dogfood pass

## Gates
- Track 1 and Track 2 wait for the shared contract baseline
- Track 3 waits for explicit indexing states and routing
- Track 4 must report through the shared health model before completion
- Each phase ends with reliability, benchmark, and dogfood checks

## Success Conditions
- Deterministic daemon lifecycle and stale-binary replacement
- Unified indexing across full, catch-up, and watcher paths
- Projection lag is visible and repairable without daemon restart
- Embedding runtime truth is surfaced to tools and dashboard
- Julie feels calm, fast, and trustworthy in daily dogfooding
