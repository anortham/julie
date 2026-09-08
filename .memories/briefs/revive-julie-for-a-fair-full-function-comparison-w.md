---
id: revive-julie-for-a-fair-full-function-comparison-w
title: Revive Julie for a fair full-function comparison with Miller
status: active
created: 2026-09-07T17:19:15.993Z
updated: 2026-09-07T20:43:36.966Z
tags:
  - julie-revival
  - miller-comparison
  - architecture
  - semantics
---

## Goal
Revive Julie as its own full-function code-intelligence tool and compare it fairly with Miller. Retirement-only direction is superseded.

## Constraints
Current julie-extractors with preserved syntax-aware edits/source bodies; flexible CLI sharing MCP execution; released MCP 2026-07-28 stateless lifecycle; concurrent agents/worktrees; optional qualified native semantics with Python retained for comparison. Continuous testing is excluded. No winner or model promotion presumed.

## Implementation program
Start with docs/plans/2026-09-07-julie-revival-program.md. Upstream U1 shipped as v2.41.0 at a71e18c1a6fae67b15d2d0aaa20793330fda901f; Julie's concrete next worker plan is docs/plans/2026-09-07-julie-extractor-migration.md. Follow with request-engine-mcp-cli, workspace-lifecycle, native-semantics and head-to-head-evaluation plans under the same date prefix.

## Review and success criteria
User assigns worker models; root Codex performs final implementation review and integrated review. Preserve exact interfaces, file ownership, narrow TDD commands, lead gates and source-bound evidence. Full mode needs actual current compatible vectors. Source-edit locks work across storage modes. Evaluation includes shared-service costs and real agent outcomes.

## Status
v2.41.0 publication and remote peeled tag verified. Inspected syntax/public fact contracts and reran 8 public syntax tests plus 14 fault tests on upstream main 5912bafe, whose source/manifests match the tag; all passed. Scope/limits in docs/findings/2026-09-07-julie-extractors-v2.41.0-readiness.md. J1 plan and program now reference the concrete release. Julie itself remains on v2.34.3; no Julie implementation or dependency edits have started.
