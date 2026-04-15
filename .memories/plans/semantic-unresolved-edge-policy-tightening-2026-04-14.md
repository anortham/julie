---
id: semantic-unresolved-edge-policy-tightening-2026-04-14
title: Semantic Unresolved Edge Policy Tightening
status: active
created: 2026-04-15T02:02:58.791Z
updated: 2026-04-15T02:02:58.791Z
tags:
  - planning
  - tree-sitter
  - extractors
  - relationships
  - unresolved-edges
---

# Semantic Unresolved Edge Policy Tightening

## Goal
Remove obvious runtime and stdlib noise from unresolved-edge emission while preserving plausible cross-file project calls.

## Scope
- Tighten Go unresolved package-call policy so stdlib imports like `fmt` do not emit pending edges, while project package calls such as `utils.HelperFunction` remain pending.
- Tighten PowerShell unresolved command policy so built-in cmdlets like `Write-Output` and `Get-ChildItem` do not emit pending edges, while cross-file project functions and user-defined Verb-Noun functions still do.
- Lock existing Bash and R builtin filtering as explicit regression-test contracts.
- Run batch verification with targeted regressions first, then `cargo xtask test dev`.

## Files
- `crates/julie-extractors/src/go/relationships.rs`
- `crates/julie-extractors/src/powershell/relationships.rs`
- `crates/julie-extractors/src/tests/go/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/powershell/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/bash/cross_file_relationships.rs`
- `crates/julie-extractors/src/tests/r/cross_file_relationships.rs`

## Tasks
1. Tighten Go stdlib package filtering with TDD.
2. Tighten PowerShell built-in cmdlet filtering with TDD.
3. Lock Bash and R builtin filtering as explicit contract tests.
4. Run final regression verification for the batch.

## Constraints
- Keep policy local to each extractor.
- Do not add commits unless the user requests them.
- Keep filters evidence-based: drop obvious runtime noise only when the extractor can prove it.
- Preserve project-like unresolved calls.

