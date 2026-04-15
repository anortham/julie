---
id: tree-sitter-hardening-completion-sprint
title: Tree-Sitter Hardening Completion Sprint
status: active
created: 2026-04-15T13:53:49.487Z
updated: 2026-04-15T13:53:49.487Z
tags:
  - treesitter
  - extractors
  - hardening
  - world-class
---

# Tree-Sitter Hardening Completion Sprint

## Goal
Finish the remaining Important gaps in `crates/julie-extractors` so the tree-sitter hardening work reaches the plan's world-class exit gate.

## Scope
- Remove the extra public extraction path that bypasses canonical parsing and JSONL handling.
- Finish structured pending migration in the remaining touched JS/TS/C# paths.
- Tighten shared semantic policy for doc comments and dead identifier semantics.
- Replace the missing Task 9-11 invariant suites and smoke-heavy JSONL coverage.
- Produce the consumer docs, review artifact, and full verification evidence.

## Execution Order
1. Canonical API and structured pending cleanup
2. Shared semantics hardening
3. Invariant suite and regression rewrite
4. Docs, review artifact, and final verification

