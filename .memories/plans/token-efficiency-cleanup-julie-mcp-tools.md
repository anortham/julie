---
id: token-efficiency-cleanup-julie-mcp-tools
title: Token Efficiency Cleanup — Julie MCP Tools
status: completed
created: 2026-03-03T21:50:01.765Z
updated: 2026-03-03T22:31:17.538Z
tags:
  - token-efficiency
  - cleanup
  - formatting
---

# Token Efficiency Cleanup — Julie MCP Tools

## Steps
1. Strip dead fields from OptimizedResponse (shared.rs, scoring.rs, search/mod.rs)
2. Remove health report filler (health.rs)
3. Remove workspace clean filler (list_clean.rs)
4. get_context — drop raw ref_score, remove compact file map (formatting.rs)
5. get_context — shorten readable separators (formatting.rs)
6. get_context — make compact the default (formatting.rs, mod.rs)

## Deferred
- Workspace list one-line format
- Embedding health compaction
- fast_refs kind labels

