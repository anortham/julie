---
id: treesitter-world-class-hardening
title: Tree-Sitter World-Class Hardening
status: active
created: 2026-04-14T16:50:00.241Z
updated: 2026-04-14T17:10:50.301Z
tags:
  - treesitter
  - extractors
  - hardening
  - architecture
  - quality
  - implementation-plan
---

# Tree-Sitter World-Class Hardening

## Goal
Make `crates/julie-extractors` safe as a high-trust platform surface for new products and external reuse.

## Current Artifacts
- Design spec: `docs/plans/2026-04-14-treesitter-world-class-design.md`
- Implementation plan: `docs/plans/2026-04-14-treesitter-world-class-hardening.md`

## Scope
- Unify extraction entrypoints behind one canonical parse-and-extract pipeline
- Fix JSONL production-path correctness, path normalization, and ID stability
- Redesign relationship and pending-relationship precision
- Replace duplicated routing with a single registry abstraction
- Tighten shared policies such as doc comments and identifier semantics
- Redesign tests around invariants and regression coverage
- End with review gates and explicit world-class exit criteria

## Key Decisions
- API changes are allowed now if they materially improve the extractor core
- Wrong edges are worse than missing edges
- New defects discovered during execution are pulled into scope when they are blocking or Critical/Important
- Final world-class sign-off requires zero Critical and zero Important defects in extractor core behavior, extractor API behavior, and extractor test architecture

## Next Step
Choose execution mode:
1. Team-driven execution for the parallelizable language-migration tasks
2. Single-agent execution for strictly sequential implementation
