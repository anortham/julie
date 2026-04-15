# Tree-Sitter World-Class Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use razorback:executing-plans to implement this plan task-by-task.
> **For Team Execution:** REQUIRED SUB-SKILL: Use razorback:team-driven-development if Tasks 5 through 8 are executed in parallel.

**Goal:** Replace the fragmented extractor surface in `crates/julie-extractors` with a canonical, registry-driven, invariant-tested tree-sitter pipeline that is safe for new product work and external reuse.

**Architecture:** Introduce one canonical extraction pipeline and one language registry, route all public entrypoints through that path, centralize path and identity normalization, redesign unresolved relationship context for precision, migrate high-risk extractors to shared rules, then finish with invariant-focused tests and a formal sign-off review.

**Tech Stack:** Rust, tree-sitter, anyhow, serde, julie-extractors test suite, `cargo xtask`, Julie code intelligence tools

---

## Execution Notes

- This is a light plan for same-session execution with review at each phase gate.
- Every task follows TDD: write failing tests first, run the narrow failing test, implement the smallest correct change, rerun the narrow test, then run the broader verification command for that task.
- For narrow red-green checks, use exact test-name filters such as `cargo test --lib <exact_test_name> 2>&1 | tail -10`.
- In sequential execution, the lead runs `cargo xtask test dev` after each completed task.
- In parallel execution, teammates run only their narrow red-green tests; the lead runs one `cargo xtask test dev` after integrating the approved parallel batch.
- Do not open a new workstream until the current task, or an explicitly approved parallel task group, passes its acceptance criteria and phase review.

### Task 1: Build the Canonical Registry and Extraction Pipeline

**Files:**
- Create: `crates/julie-extractors/src/registry.rs`
- Create: `crates/julie-extractors/src/pipeline.rs`
- Modify: `crates/julie-extractors/src/lib.rs:26-35,73-89`
- Modify: `crates/julie-extractors/src/manager.rs:14-34,40-267`
- Modify: `crates/julie-extractors/src/factory.rs:17-713`
- Create: `crates/julie-extractors/src/tests/pipeline.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Add one registry abstraction that owns language capability and dispatch, plus one canonical extraction entrypoint that returns normalized `ExtractionResults`. Keep the initial change narrow: the first goal is to create a single source of truth, not to solve every semantics issue in the same diff.

**Approach:**
Use function-pointer or equivalent compile-time dispatch, not trait-object gymnastics. The registry should know how to construct per-language extraction results, and `ExtractorManager` should stop hand-rolling parser setup in multiple public methods.

Use a shape close to this:

```rust
pub struct LanguageCapabilities {
    pub symbols: bool,
    pub relationships: bool,
    pub pending_relationships: bool,
    pub identifiers: bool,
    pub types: bool,
}

pub struct LanguageRegistryEntry {
    pub language: &'static str,
    pub capabilities: LanguageCapabilities,
    pub extract: fn(
        tree: &tree_sitter::Tree,
        file_path: &str,
        content: &str,
        workspace_root: &std::path::Path,
    ) -> Result<ExtractionResults, anyhow::Error>,
}

pub fn extract_canonical(
    file_path: &str,
    content: &str,
    workspace_root: &std::path::Path,
) -> Result<ExtractionResults, anyhow::Error> { /* parse once, dispatch once */ }
```

The first wrapper target is `ExtractorManager::extract_all()`. Do not change JSONL or pending-relationship semantics in this task beyond routing them through the new pipeline.

**Acceptance criteria:**
- [ ] `registry.rs` is the single source of truth for language capability and dispatch.
- [ ] `pipeline.rs` exposes the canonical parse-and-extract path.
- [ ] `ExtractorManager::extract_all()` delegates to the canonical path.
- [ ] `factory.rs` no longer owns the only load-bearing dispatch table.
- [ ] New pipeline tests assert parity of symbol names, normalized file paths, and presence of identifiers, relationships, pending relationships, and types for representative Rust, TypeScript, and Python fixtures.
- [ ] Task-specific narrow tests pass.

### Task 2: Lock Path, ID, and JSONL Invariants

**Files:**
- Create: `crates/julie-extractors/src/base/span.rs`
- Modify: `crates/julie-extractors/src/base/mod.rs:14-25`
- Modify: `crates/julie-extractors/src/base/extractor.rs:33-97,214-218`
- Modify: `crates/julie-extractors/src/base/creation_methods.rs:16-106`
- Modify: `crates/julie-extractors/src/base/types.rs:37-127,319-447`
- Modify: `crates/julie-extractors/src/manager.rs:117-179`
- Modify: `crates/julie-extractors/src/pipeline.rs`
- Modify: `src/watcher/handlers.rs:69-84`
- Create: `crates/julie-extractors/src/tests/path_identity.rs`
- Create: `crates/julie-extractors/src/tests/jsonl_pipeline.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Define one normalization model for file path, line and column positions, byte offsets, and IDs. Make JSONL a first-class case in the canonical pipeline, with file-global byte offsets and collision-resistant IDs.

**Approach:**
Introduce a normalized span type or equivalent helper so ID generation and stored positions do not depend on per-language ad hoc math. JSONL should parse record-by-record but normalize into file-global coordinates before results leave the canonical pipeline.

Use a shared shape close to this:

```rust
pub struct NormalizedSpan {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub start_byte: u32,
    pub end_byte: u32,
}

pub struct RecordOffset {
    pub line_delta: u32,
    pub byte_delta: u32,
}
```

`BaseExtractor::generate_id()` should hash against canonical file path plus canonical location, not transient line-local JSONL positions. If a helper or API split is needed, make it explicit rather than bending old signatures until they squeal.

**Acceptance criteria:**
- [ ] There is one normalization path for stored file paths and spans.
- [ ] JSONL extraction through the canonical production path emits file-global line and byte positions.
- [ ] JSONL IDs do not collide for repeated keys on different lines.
- [ ] No production code depends on `/tmp/test` or any fake workspace root.
- [ ] Watcher indexing continues to use the canonical extraction path successfully.
- [ ] Path and JSONL invariant tests fail before the change and pass after it.
- [ ] Task-specific narrow tests pass.

### Task 3: Clean Up the Public Extractor API Surface

**Files:**
- Modify: `crates/julie-extractors/src/manager.rs:40-259`
- Modify: `crates/julie-extractors/src/routing_symbols.rs:7-318`
- Modify: `crates/julie-extractors/src/routing_identifiers.rs:6-265`
- Modify: `crates/julie-extractors/src/routing_relationships.rs:6-260`
- Modify: `crates/julie-extractors/src/lib.rs:73-89`
- Create: `crates/julie-extractors/src/tests/api_surface.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Make an explicit decision on the convenience entrypoints. Either keep thin wrappers over canonical extraction, or remove them and migrate callers. The end state must not have multiple behaviorally distinct extraction paths.

**Approach:**
Preferred direction: keep `extract_all` or rename it as the canonical public API, then either:

- retain `extract_symbols`, `extract_identifiers`, and `extract_relationships` as thin projections over canonical results, or
- remove them if they add cost and confusion with no meaningful caller value.

If wrappers remain, they must require the same context as canonical extraction and must never accept caller-provided `symbols` as a shortcut around canonical parsing. That shortcut is how the current API drift happened.

**Acceptance criteria:**
- [ ] The public API has one canonical extraction entrypoint.
- [ ] Convenience entrypoints are either removed or reduced to projections over canonical results.
- [ ] The production code path no longer contains separate routing tables for symbols, identifiers, and relationships.
- [ ] API surface tests prove that the remaining public entrypoints produce outputs consistent with canonical extraction.
- [ ] Any breaking API change is documented in code comments and the later consumer docs task.
- [ ] Task-specific narrow tests pass.

### Task 4: Redesign Pending Relationships and Shared Resolution Context

**Files:**
- Create: `crates/julie-extractors/src/base/relationship_resolution.rs`
- Modify: `crates/julie-extractors/src/base/mod.rs:14-25`
- Modify: `crates/julie-extractors/src/base/types.rs:289-356,440-447`
- Modify: `crates/julie-extractors/src/base/creation_methods.rs:109-134`
- Create: `crates/julie-extractors/src/tests/relationship_precision.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Replace name-only unresolved relationship context with a structured model that preserves the information needed for later resolution. This task is about the shared model and helpers, not the language migrations yet.

**Approach:**
Add a shared unresolved-target or equivalent shape and make `PendingRelationship` carry it. Keep the model small but useful. It must preserve the data needed to distinguish local method names from receiver-qualified calls and imported aliases.

Use a model close to this:

```rust
pub struct UnresolvedTarget {
    pub display_name: String,
    pub terminal_name: String,
    pub receiver: Option<String>,
    pub namespace_path: Vec<String>,
    pub import_context: Option<String>,
}

pub struct PendingRelationship {
    pub from_symbol_id: String,
    pub target: UnresolvedTarget,
    pub caller_scope_symbol_id: Option<String>,
    pub kind: RelationshipKind,
    pub file_path: String,
    pub line_number: u32,
    pub confidence: f32,
}
```

Provide shared helpers so language modules can create resolved and unresolved edges without rebuilding IDs and metadata by hand.

**Acceptance criteria:**
- [ ] The shared model retains receiver, caller-scope, and namespace or import context where applicable.
- [ ] Shared helper APIs exist for building resolved and unresolved relationships.
- [ ] The old `callee_name`-only pending-edge contract is gone or reduced to compatibility glue.
- [ ] `relationship_precision.rs` contains passing duplicate-name and member-call ambiguity tests against the shared helper layer.
- [ ] Task-specific narrow tests pass.

### Task 5: Migrate JavaScript and TypeScript Relationship Extraction

**Files:**
- Modify: `crates/julie-extractors/src/javascript/mod.rs:25-383`
- Modify: `crates/julie-extractors/src/javascript/relationships.rs:13-254`
- Modify: `crates/julie-extractors/src/typescript/mod.rs:34-255`
- Modify: `crates/julie-extractors/src/typescript/relationships.rs:11-303`
- Modify: `crates/julie-extractors/src/tests/javascript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/javascript/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/typescript/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/typescript/relationships.rs`

**What to build:**
Migrate JS and TS to the shared unresolved-edge model, eliminate the split local-versus-pending call logic, and pin precise behavior for member calls and imported aliases.

**Approach:**
Keep ownership limited to JS and TS files in this task. Do not spill into Python, Java, or C# here.

**Acceptance criteria:**
- [ ] JS and TS no longer split local and pending call extraction across divergent naming rules.
- [ ] JS and TS use structured pending edges for receiver-qualified calls and imported aliases when local resolution is ambiguous.
- [ ] The listed JS and TS relationship test files pass with exact caller and callee assertions for the touched cases.
- [ ] Task-specific narrow tests pass.

### Task 6: Migrate Python, Java, and C# Relationship Extraction

**Files:**
- Modify: `crates/julie-extractors/src/python/relationships.rs:9-223`
- Modify: `crates/julie-extractors/src/java/relationships.rs:11-300`
- Modify: `crates/julie-extractors/src/csharp/relationships.rs:12-424`
- Modify: `crates/julie-extractors/src/tests/python/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/python/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/java/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/csharp/cross_file_relationships.rs`

**What to build:**
Migrate Python, Java, and C# off bare-name relationship resolution where the AST provides stronger context.

**Approach:**
Keep ownership limited to the listed OO-language files. The goal is to replace bare-name local resolution with shared context while preserving inheritance and cross-file pending behavior.

**Acceptance criteria:**
- [ ] Python, Java, and C# stop relying on bare symbol names as the only resolution key in the touched paths.
- [ ] The listed Python, Java, and C# relationship tests pass with exact caller and callee assertions for the touched cases.
- [ ] New or rewritten Python, Java, and C# tests assert that ambiguous local calls produce structured pending edges instead of resolved local edges.
- [ ] Task-specific narrow tests pass.

### Task 7: Migrate Go, PHP, and Ruby Relationship Extraction

**Files:**
- Modify: `crates/julie-extractors/src/go/relationships.rs`
- Modify: `crates/julie-extractors/src/php/relationships.rs`
- Modify: `crates/julie-extractors/src/ruby/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/go/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/php/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/ruby/cross_file_relationships.rs`

**What to build:**
Bring the systems-and-scripting wave onto the shared unresolved-edge model without regressing the cross-file behavior they already expose.

**Approach:**
Focus on receiver or namespace context where the grammar supports it. Do not invent precision that the tree does not provide.

**Acceptance criteria:**
- [ ] Go, PHP, and Ruby use the shared unresolved-edge context where relationship extraction is available.
- [ ] The listed cross-file relationship tests pass with exact touched-case assertions.
- [ ] New or rewritten Go, PHP, and Ruby tests assert that unsupported precision cases stay unresolved rather than becoming resolved local edges.
- [ ] Task-specific narrow tests pass.

### Task 8: Migrate Kotlin, Swift, and Scala Relationship Extraction

**Files:**
- Modify: `crates/julie-extractors/src/kotlin/relationships.rs`
- Modify: `crates/julie-extractors/src/swift/relationships.rs`
- Modify: `crates/julie-extractors/src/scala/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/kotlin/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/swift/cross_file_relationships.rs`
- Create: `crates/julie-extractors/src/tests/scala/cross_file_relationships.rs`
- Modify: `crates/julie-extractors/src/tests/scala/mod.rs`

**What to build:**
Finish the language migration wave for Kotlin, Swift, and Scala, and close the Scala cross-file relationship gap if Scala relationship extraction remains part of the supported surface.

**Approach:**
If Scala cannot support the same behavior because of a real grammar limit, record that in the later review artifact rather than hiding it in the implementation.

**Acceptance criteria:**
- [ ] Kotlin and Swift use the shared unresolved-edge context where relationship extraction is available.
- [ ] The listed Kotlin and Swift cross-file relationship tests pass.
- [ ] Scala either has a new passing cross-file relationship regression file or an explicit downgrade note is recorded in the task summary for carry-forward into the final review artifact.
- [ ] Task-specific narrow tests pass.

### Task 9: Tighten Shared Semantic Policies

**Files:**
- Modify: `crates/julie-extractors/src/base/extractor.rs:114-211`
- Modify: `crates/julie-extractors/src/base/types.rs:132-168`
- Modify: `crates/julie-extractors/src/tests/helpers.rs:18-28`
- Create: `crates/julie-extractors/src/tests/doc_comments.rs`
- Create: `crates/julie-extractors/src/tests/identifier_semantics.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Make shared semantic policy explicit for doc comments and identifier kinds. Remove dead shared semantics or implement them deliberately, but do not leave “supported by enum, unsupported by reality” hanging around.

**Approach:**
Tighten `find_doc_comment()` so it recognizes real documentation syntax instead of promoting generic comments into docs. For identifier semantics, make a deliberate call on `IdentifierKind::Import`:

- either implement it in the selected languages where import usage is a first-class identifier concept, or
- remove it from the shared public model and document that imports are represented through symbols and relationships instead.

Preferred direction: remove the dead identifier kind unless implementation across representative languages is small and clean.

**Acceptance criteria:**
- [ ] Generic `//`, `/*`, and `#` comments are no longer treated as docs unless the language policy says they are doc syntax.
- [ ] Doc-comment behavior is covered by explicit regression tests.
- [ ] `IdentifierKind::Import` is either implemented with tests or removed from the shared identifier model.
- [ ] Test helpers fail loudly and precisely when parser setup or parsing breaks.
- [ ] Task-specific narrow tests pass.

### Task 10: Build Cross-Cutting Type and Path Invariant Suites

**Files:**
- Modify: `crates/julie-extractors/src/tests/go/types.rs`
- Modify: `crates/julie-extractors/src/tests/typescript/relative_paths.rs:23-219`
- Create: `crates/julie-extractors/src/tests/type_invariants.rs`
- Create: `crates/julie-extractors/src/tests/path_invariants.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Add invariant suites for exact type mapping and path normalization, and replace the soft assertions in the existing path and type tests that are not pinning real behavior.

**Approach:**
Keep this task focused on type and path invariants only. Do not mix JSONL and smoke-test rewrites into it.

**Acceptance criteria:**
- [ ] `type_invariants.rs` and `path_invariants.rs` exist and pass.
- [ ] Existing type and path tests assert exact target values, not only presence.
- [ ] Path invariants are covered outside a single TypeScript-only test file.
- [ ] Task-specific narrow tests pass.

### Task 11: Replace JSONL and Smoke-Heavy Regression Coverage

**Files:**
- Modify: `crates/julie-extractors/src/tests/python/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/json/mod.rs:705-934`
- Create: `crates/julie-extractors/src/tests/jsonl_invariants.rs`
- Create: `crates/julie-extractors/src/tests/review_regressions.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:**
Turn the test suite from broad-but-soft into broad-and-sharp. The goal is not more tests by count, it is more invariants pinned at the public surface.

**Approach:**
Replace “non-empty”, “has one”, and “does not panic” assertions with exact value assertions. Move JSONL tests off local duplicate helper logic and onto the canonical production path. Prefer shared invariant modules for cross-cutting guarantees and keep language-specific files for grammar-specific edge cases.

**Acceptance criteria:**
- [ ] Existing smoke-heavy tests are rewritten or replaced where they were hiding real risk.
- [ ] JSONL tests cover the production path, not a local duplicate helper.
- [ ] `jsonl_invariants.rs` and `review_regressions.rs` exist and pass.
- [ ] The rewritten JSON and Python regression tests assert exact target values, not only presence.
- [ ] Task-specific narrow tests pass.

### Task 12: Final Review Artifact and Consumer Docs

**Files:**
- Create: `crates/julie-extractors/README.md`
- Modify: `crates/julie-extractors/src/lib.rs:73-89`
- Create: `docs/plans/2026-04-14-treesitter-world-class-review.md`

**What to build:**
Document the new extractor surface for consumers, then produce the review artifact that determines whether this program is complete.

**Approach:**
Write consumer-facing guidance that explains the canonical API, any removed or retained wrappers, path and ID semantics, JSONL semantics, and unresolved-relationship guarantees. Then request a fresh code review focused on correctness and regressions. Capture the outcome in the review artifact.

The review artifact must contain:
- verification commands run
- pass or fail status
- any remaining Minor findings
- downgrade records, if any

**Acceptance criteria:**
- [ ] `README.md` explains the supported public surface and semantic guarantees.
- [ ] Fresh review is requested and its findings are recorded in the review artifact.
- [ ] Any remaining Minor findings are documented with disposition.
- [ ] `cargo fmt`, `cargo clippy`, and `cargo xtask test full` pass.
- [ ] `docs/plans/2026-04-14-treesitter-world-class-review.md` records verification results, review findings, and downgrade records.

## External Gate

After Task 12, the repo owner reviews the review artifact and decides whether the extractor surface is ready for world-class sign-off.

If the fresh review reports any Critical or Important findings, reopen the impacted task set, fix those findings, rerun Task 12, and only then return to the sign-off decision.

## Recommended Execution Order

1. Task 1
2. Task 2
3. Task 3
4. Task 4
5. Task 5
6. Task 6
7. Task 7
8. Task 8
9. Task 9
10. Task 10
11. Task 11
12. Task 12

## Parallelism Notes

- Tasks 1 through 4 are sequential.
- Tasks 5 through 8 have exclusive file ownership and may run in parallel only after Task 4 lands and the shared unresolved-edge model is stable.
- Tasks 9 through 12 are sequential.

## Commit Shape

Use one commit per task. Suggested commit prefixes:

- `refactor(extractors):` for pipeline, registry, and API cleanup
- `fix(extractors):` for correctness changes such as JSONL and relationship precision
- `test(extractors):` for invariant-test conversions
- `docs(extractors):` for consumer-facing docs and review artifacts
