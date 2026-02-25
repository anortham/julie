---
id: phase-3-get-context-tool
title: "Phase 3: get_context Tool"
status: completed
created: 2026-02-24T22:27:05.144Z
updated: 2026-02-25T00:41:17.147Z
tags:
  - phase-3
  - get-context
  - mcp-tool
---

# Phase 3: get_context Tool

## Overview
Build a new MCP tool that returns a token-budgeted subgraph of relevant code for a given query — search → rank by centrality → expand graph → adaptive token allocation → formatted output.

## Status: In Progress

## Design Docs
- Design: `docs/plans/2026-02-24-phase3-get-context-tool.md`
- Implementation plan: `docs/plans/2026-02-24-phase3-get-context-impl.md`

## Tasks
1. Tool scaffolding and MCP registration
2. Search + pivot selection
3. Graph expansion
4. Adaptive token allocation
5. Output formatting
6. Wire up full pipeline
7. Update agent instructions
8. Live testing and tuning

## Key Decisions
- Composes existing primitives (search_symbols, reference_score, relationships, TokenEstimator)
- Adaptive token budget: 1-2 pivots → 2000 tokens (deep), 3-5 → 3000 (balanced), 6+ → 4000 (broad)
- Pivot selection based on score distribution analysis
- 60/30/10 budget split: pivots (full code) / neighbors (signatures) / summary (file map)
- Post-search centrality reranking from Phase 2
