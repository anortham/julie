---
id: phase-2-graph-centrality-ranking
title: "Phase 2: Graph Centrality Ranking"
status: completed
created: 2026-02-24T21:29:11.982Z
updated: 2026-02-24T22:18:13.866Z
tags:
  - phase-2
  - graph-centrality
  - search-ranking
---

# Phase 2: Graph Centrality Ranking

## Overview
Boost search ranking of well-connected symbols using pre-computed reference scores from the relationship graph. Leverages Julie's 14 relationship types extracted by 30 tree-sitter parsers.

## Status: In Progress

## Design Docs
- Design: `docs/plans/2026-02-24-phase2-graph-centrality.md`
- Implementation plan: `docs/plans/2026-02-24-phase2-graph-centrality-impl.md`

## Tasks
1. Schema migration — add `reference_score` column (migration 009)
2. Compute weighted reference scores from relationships
3. Hook into indexing pipeline (after relationships stored)
4. Batch query for reference scores
5. Integrate centrality boost into search scoring
6. Tuning and verification

## Key Decisions
- Pre-compute at index time (not on-demand)
- Weighted by relationship kind: Calls=3, Implements/Imports/Extends=2, Uses/References=1
- Logarithmic scaling: `score *= 1.0 + ln(1 + ref_score) * 0.3`
- Post-search boost (not Tantivy custom scorer)
- Self-references excluded

