---
id: token-efficiency-improvements
title: Token Efficiency Improvements
status: completed
created: 2026-03-31T00:16:54.384Z
updated: 2026-03-31T00:50:20.449Z
tags:
  - token-efficiency
  - optimization
---

# Token Efficiency Improvements

## Goal
Reduce token consumption of Julie MCP tool outputs by 30-60% across the most common tool calls, without changing tool descriptions or server instructions.

## Tasks (7 total)
1. **get_symbols default → "structure"** — 50-80% savings per call (TINY effort)
2. **fast_search return_format="locations"** — 70-90% savings for lookups (SMALL)
3. **deep_dive(full) enforce token cap** — prevent 4000+ token blowups (MEDIUM)
4. **Group-by-file in search/refs output** — 5-15% savings on multi-match (SMALL)
5. **Drop kind prefix when signature contains it** — 50-150 tokens/file (SMALL)
6. **get_context compact mode tightening** — 15-30% savings per call (MEDIUM)
7. **/efficient skill** in julie-plugin repo (SMALL, depends on Task 2)

## Execution Order
- Batch 1 (parallel): Tasks 1, 3, 5
- Batch 2 (parallel): Tasks 2, 4
- Batch 3: Task 6
- Batch 4: Task 7

## Plan Document
`docs/superpowers/plans/2026-03-30-token-efficiency.md`

## Status
Plan written, awaiting execution approach decision.

