# Tree-Sitter World-Class Hardening Design

**Status:** Draft for review

## Goal

Turn `crates/julie-extractors` into a world-class tree-sitter extraction layer that is safe to build new products on, including external projects, by fixing current correctness defects, permitting API cleanup where needed, and establishing a repeatable hardening loop until quality is no longer fragile or incidental.

## Why This Exists

The current extractor stack has strong raw coverage across languages, but it is not yet reliable enough to be treated as a high-trust platform surface.

The review found structural defects, not isolated paper cuts:

- JSONL handling is inconsistent across entrypoints and can store wrong line, byte, and ID data.
- Public extraction entrypoints can produce different answers for the same file.
- Path normalization and ID generation depend on call path details instead of invariant rules.
- Relationship resolution is often name-only, which breaks as codebases get larger and more realistic.
- Dispatch behavior is duplicated across multiple match tables and has already drifted.
- Test coverage is broad but uneven, with too much smoke coverage and too few invariant checks.

This is the right time to make API changes if they materially improve the extractor core. That becomes harder after the next project starts relying on the current shape.

## Product Standard

This hardening program should leave the extractor stack with these properties:

- Same source file, same output, regardless of which public entrypoint is used.
- Stable path and ID semantics across symbols, identifiers, relationships, pending relationships, and JSONL records.
- Relationship extraction that prefers precision over optimistic wrong edges.
- Cross-language behavior that is intentionally consistent, not copy-paste consistent by accident.
- Tests that pin invariants and catch regressions in realistic code, not only total failures.
- An API surface that external consumers can understand and trust.

## Design Principles

### 1. Canonical Extraction Pipeline

There should be one canonical extraction path:

`source + file path + workspace root + language context -> parse once -> extract once -> normalize once -> ExtractionResults`

Everything else should delegate to that path.

No secondary public API should reparse content, invent a fake workspace root, or maintain its own routing table.

### 2. Invariants Before Features

Correctness invariants outrank feature breadth.

If a language advertises identifiers or relationships, those outputs must meet the same core guarantees as symbol extraction. Silent partial support is worse than explicit unsupported behavior.

### 3. Precision Over Flattering Output

Wrong edges are more damaging than missing edges.

When the extractor cannot resolve a relationship with sufficient context, it should preserve structured unresolved information for later resolution rather than emitting a confident but incorrect link.

### 4. Shared Logic Should Be Shared

Cross-language behavior such as path normalization, ID generation, relationship construction, pending-relationship semantics, and doc-comment rules should live in shared infrastructure or shared helpers when possible.

Per-language implementations should own syntax details, not duplicate policy.

### 5. Hardening Is Iterative

This is not a one-shot refactor. Each phase should end with review, verification, and a decision on whether the stack is strong enough to advance or whether another iteration is required.

## Scope

This program covers all currently identified defects plus the supporting cleanup needed to make them stay fixed.

New defects discovered during execution are handled by closure rules, not by open-ended sprawl:

- If a newly found defect is in the touched workstream or would be rated Critical or Important for extractor correctness, it is pulled into the current hardening program.
- If it is adjacent but not blocking, it is added to the hardening backlog and reviewed at the next phase gate.
- The program is complete only when there are no open Critical or Important defects in extractor core behavior, extractor API behavior, or extractor test architecture.

### In Scope

- Public extractor API redesign where needed
- Canonicalization of extraction entrypoints
- JSONL correctness and production-path support
- Path normalization and ID stability rules
- Relationship and pending-relationship model redesign
- Routing and factory deduplication
- Cross-language consistency improvements
- Doc-comment extraction policy cleanup
- Identifier semantics cleanup, including dead or unsupported categories
- Test-suite redesign toward invariant and regression coverage
- Performance and review gates needed to call the stack production-grade
- Consumer-facing extractor documentation if API changes require it

### Out of Scope

- Adding new language support unless it is required to preserve parity during refactors
- Broad search/ranking changes outside extractor-produced data correctness
- Unrelated workspace/indexing refactors that are not required by extractor hardening

## Defect Inventory

### A. Entry-Point and API Inconsistency

- `extract_symbols`, `extract_all`, `extract_identifiers`, and `extract_relationships` do not behave as one surface.
- Some public paths parse once, some parse repeatedly.
- Some paths respect real workspace roots, others use `/tmp/test`.
- Some paths support languages or data kinds that sibling paths silently drop.

### B. Path and Identity Corruption

- Path normalization is tied to constructor call details rather than enforced centrally.
- IDs depend on normalized file path plus local position, which makes divergent normalization a correctness defect.
- JSONL line-by-line extraction adjusts only line numbers, not IDs or byte offsets.

### C. Relationship Precision Defects

- Many languages resolve callees and parents by bare symbol name only.
- Pending relationships retain too little context for high-quality later resolution.
- JS and TS split local and pending call handling across divergent paths.
- Relationship parity varies by language, which makes the graph depend on implementation accidents.

### D. Shared Semantic Policy Defects

- Doc-comment extraction treats generic comments as documentation in too many languages.
- Identifier categories are not consistently emitted or tested.
- Shared behavior is sometimes reimplemented manually instead of going through base helpers.

### E. Test Quality Defects

- Many tests only prove “not empty,” “has one,” or “does not panic.”
- JSONL production-path behavior is not pinned.
- Path invariants are not covered broadly enough.
- Relationship tests often fail to assert exact caller and callee identity.
- Type tests often fail to assert exact `symbol_id -> resolved_type` mapping.

## Target Architecture

### Canonical Public Surface

The extractor crate should expose one canonical extraction API that returns a full normalized result.

Other APIs may exist for convenience, but they should be thin views over canonical results, not separate implementations.

This means:

- one canonical parse-and-extract entrypoint
- one canonical normalization stage
- one mandatory language registry that defines capability and dispatch
- one shared unresolved-edge model

### Language Registry

Replace repeated `match language { ... }` blocks with a single registry abstraction as the source of truth.

The implementation may realize that registry as a static table, generated table, or equivalent compile-time structure, but the public architecture must have one registry abstraction that owns capability and dispatch decisions.

That registry should define:

- parser language binding
- supported capabilities
- extractor constructor path
- any language-specific normalization hooks

This removes current drift between factory and routing tables.

### Relationship Model

The relationship system should move from terminal-name matching to structured resolution inputs.

At minimum, unresolved edges should be able to retain enough context to distinguish:

- receiver-qualified calls
- imported names or aliases
- namespace or module context
- caller scope
- inheritance target context

If a target still cannot be resolved locally, the unresolved data should be rich enough for a later cross-file resolver to make a stronger decision than raw `callee_name` matching.

### JSONL Model

JSONL should be a first-class canonical input path, not a special branch in only one method.

The canonical model must preserve:

- record line number
- file-global byte offsets
- stable IDs with no collisions across repeated shapes on different lines

JSONL may also attach record metadata such as record index or record span, but all byte offsets exposed through extractor results must remain file-global so downstream consumers can treat JSONL results the same way as normal file results.

The chosen semantics must be documented and tested through the production entrypoint.

## Workstreams

### 1. API and Pipeline Unification

Design and land the canonical extraction surface.

Expected outcomes:

- parse once
- normalize once
- shared output semantics
- an explicit early decision on whether convenience entrypoints remain as thin compatibility shims or are removed

### 2. Path and ID Invariants

Define the exact rules for path storage and ID generation, then enforce them from one place.

Expected outcomes:

- no fake workspace roots
- same file always normalizes to the same stored path
- same symbol always hashes from the same identity inputs
- JSONL identities are file-global and collision-resistant

### 3. Relationship Precision Redesign

Upgrade relationship extraction and unresolved-edge representation to preserve scope and call context.

Expected outcomes:

- fewer wrong local edges
- stronger pending relationships
- unified behavior across JS, TS, Python, Java, C#, Go, PHP, Ruby, Kotlin, Swift, and Scala
- the same precision rules applied to any other language that emits `Calls`, `Extends`, `Implements`, `Uses`, or pending relationships

### 4. Shared Policy Cleanup

Tighten shared behavior such as doc-comment extraction and identifier semantics.

Expected outcomes:

- comment policy that reflects real documentation syntax
- explicit treatment of import identifiers and unsupported identifier kinds
- shared helper usage where duplicated logic is now drifting

### 5. Language Parity Sweep

Audit all supported extractors against the canonical guarantees after the shared refactors land.

Expected outcomes:

- no language silently missing capabilities that the public API implies
- no stale copy-paste logic bypassing shared rules
- explicit downgrade record only where a grammar imposes real limits

Any downgrade must be documented in the phase review and final review with:

- affected language and capability
- technical reason the grammar or parser blocks parity
- user-visible impact
- whether the exception blocks world-class exit

A downgrade blocks exit unless it is both non-critical to advertised platform behavior and explicitly approved during final review.

### 6. Test and Verification Redesign

Replace low-signal tests with invariant tests and regression tests.

Expected outcomes:

- exact caller/callee assertions
- exact type mapping assertions
- path invariant coverage across representative languages
- JSONL production-path coverage
- adversarial duplicate-name and scoped-call cases

### 7. Review and World-Class Exit Gate

After implementation phases, run a dedicated review pass against the new architecture and tests.

Expected outcomes:

- code review focused on correctness, regressions, and API quality
- verification evidence for the final surface
- a documented go/no-go judgment on whether the extractor stack is ready for the next product

## Quality Gates

Each phase must satisfy these gates before the next phase starts:

1. Design intent remains consistent with this document.
2. Public API changes are deliberate and documented.
3. New invariants are enforced by tests, not comments.
4. No duplicated routing logic is added back.
5. Phase review finds zero Critical defects in touched areas.
6. Any Important defect in touched areas is fixed before the next phase, or the phase is reopened and rescoped with written justification.
7. Review findings rejected as non-issues must include evidence.

## Exit Criteria

We should call this world-class only when all of the following are true:

- canonical extraction path is the single source of truth
- all public entrypoints produce consistent outputs or are removed
- JSONL correctness is pinned through the production path
- path and ID invariants are stable across supported inputs
- relationship extraction avoids name-only miswiring in JS, TS, Python, Java, C#, Go, PHP, Ruby, Kotlin, Swift, and Scala
- unresolved edges preserve receiver, caller-scope, and import or namespace context where applicable
- doc-comment and identifier semantics are explicit and tested
- regression coverage is invariant-driven, not smoke-driven
- a fresh review finds zero Critical and zero Important defects in extractor core behavior, extractor API behavior, and extractor test architecture
- any remaining Minor findings are documented with explicit follow-up disposition
- starting a new product on top of the extractor surface is an explicit final-review sign-off decision, not a vibes check

## Execution Notes

- Implementation should happen in an isolated git worktree.
- This program should be executed as a sequence of reviewed phases, not one giant dump of edits.
- Final review authority for world-class sign-off is the repo owner in this session, based on review findings and fresh verification evidence.
- No commit is included here because none was requested.

## Review Request

Once this scope looks right, the next artifact should be the implementation plan that breaks the work into executable tasks, review points, and verification steps.
