# Best-in-Class Tree-Sitter — Outcome Rubric

> Operational rubric for the autonomous run defined in `2026-05-10-best-in-class-tree-sitter-design.md`. Format follows the Anthropic Managed Agents rubric pattern: per-criterion gradeable statements scored independently. The autonomous loop iterates until every criterion below is `satisfied` against fresh evidence at the worktree HEAD.

> **DONE** = all criteria in sections 1–5 satisfied at the worktree HEAD, plus section 6 (manual MCP dogfood) handed off to the user with a staging note. The user merges the worktree after section 6 passes.

> Revised 2026-05-10 to incorporate Codex adversarial review findings: typed evidence schema, structured pending shape assertions, negative test cases, exception schema rule, semantic real-world assertions, doctest gate, packaging gate, VB.NET in release profile.

## 1. Doc Hygiene

- `docs/LANGUAGE_VERIFICATION_CHECKLIST.md` does not exist in the worktree.
- `docs/LANGUAGE_VERIFICATION_RESULTS.md` does not exist in the worktree.
- `docs/verification/` directory does not exist or is empty.
- `docs/findings/` deletions are committed (the `COMPILED-FINDINGS.md` plus per-LLM audit dirs that were staged for deletion at the start of the run are now gone from the working tree and the index).
- `docs/EXTRACTION_CONTRACT.md` exists, is ≤200 lines, links to `capabilities.json`, `LANGUAGE_CERTIFICATION_REPORT.md`, and `LANGUAGE_REAL_WORLD_EVIDENCE.json`, explains every tier in the Quality Bar's target group table, documents the typed evidence schema, and documents the structured-pending field contract.
- No file in the repo references `docs/findings/COMPILED-FINDINGS.md` (verified via repo-wide search). Every `evidence` field in `capabilities.json` is a typed object, not a string path.
- `docs/LANGUAGE_CERTIFICATION_REPORT.md` was regenerated from the checked-in capability, fixture, historical-doc, and real-world evidence state, and `cargo xtask certify tree-sitter --check` passes.
- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.{json,md}` were generated with `--profile release`; exact HEAD provenance for the generation run is recorded in the verification ledger, not used as a committed self-hash gate.
- `docs/TREE_SITTER_QUALITY_BAR.md` "Current Verdict" and "Current Open Gaps" sections were updated to reflect the closed state. No stale "open" entries remain.

## 2. Capability Matrix

- `cargo nextest run -p julie-extractors capability_matrix` passes at the worktree HEAD.
- The `capabilities.json` schema requires every gap row's `evidence` to be a typed object: `{"kind": "test"|"fixture"|"commit", "value": "<test-name|path|sha>", "command": "<verification-command>"}`. The capability matrix test rejects any row whose evidence is a bare string.
- For every gap evidence entry, the matrix test verifies the referenced artifact resolves: `kind: test` names exist in the test inventory; `kind: fixture` paths exist on disk; `kind: commit` SHAs resolve via `git cat-file`. A test fails if any evidence entry is unresolvable.
- A schema rule rejects any `exception` row whose `reason` field contains "not implemented", "not yet supported", "todo", "todo:", or equivalent placeholders. Exception reasons must describe an intrinsic-N/A condition (the language has no such concept) or a documented parser limitation. The Quality Bar §46 ban on implementation-thin exceptions is enforced as a test.
- Every row in `fixtures/extraction/capabilities.json` has `gap_status` of either `closed` (with typed evidence pointing at a passing test or fixture) or `exception` (with a reason satisfying the rule above and a named locking test that fails if the exception is removed).
- No row has `gap_status: open`.
- Every `exception` row's locking test is a real `#[test]` function in `crates/julie-extractors/src/tests/` that asserts the absence of the unsupported capability. Removing the exception (without code changes) causes the test to fail.
- The known-limitations from the historical `LANGUAGE_VERIFICATION_RESULTS.md` "Known Limitations" table are represented as `exception` rows with reasons satisfying the schema rule, or have been turned into `closed` rows: C++ header-only zero cross-file refs, C++ within-file constructor disambiguation, Lua class-like tables as variable kind.

### 2.1 Structured pending relationship contract

Every "extractor + fixture work" language listed in the design's per-language gap inventory has at least one golden fixture that emits a non-empty `structured_pending_relationships` array. Each emitted entry asserts:

- `target.terminalName` is the actual symbol name being referenced (not empty, not a placeholder).
- `target.namespacePath` (when applicable) reflects the qualifier path the source code wrote (e.g., `Phoenix.Router`, `App\Http\Controller`, `crate::module`).
- `target.receiver` (when the call is receiver-qualified) carries the receiver expression text or kind, not `None`.
- `target.importContext` (when the reference is import-qualified) carries the import statement context.
- `callerScopeSymbolId` resolves to a real symbol in the same fixture.
- `byteSpan` and `lineSpan` reflect the exact source location of the reference, not the root-node span.
- The fixture also includes at least one **negative case** for the same language: a call shape that must NOT produce a relationship edge (e.g., an unqualified call with multiple credible local targets). The capability matrix test asserts the negative case produces zero edges.

Languages currently using the registry's no-pending macro variant (per `crates/julie-extractors/src/registry.rs:40,673`) must be moved out of that variant before this criterion can pass.

### 2.2 Domain relationships (JSON, TOML, SQL, CSS, regex)

- JSON Schema `$ref` produces a relationship edge with exact `from_symbol_id`, `to_symbol_id`, and `RelationshipKind`. When the target exists locally, the relationship resolves to a concrete symbol; when external, a structured pending relationship is emitted with the same shape contract as §2.1. A negative case proves no edge is emitted for a malformed `$ref`.
- TOML produces relationship edges for Cargo-style dependency tables (`[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`) and pyproject `[tool.<x>]` tables. Each edge asserts exact direction, `RelationshipKind`, and target name. Negative case: a TOML key under a non-dependency table produces no relationship.
- SQL emits structured pending relationships for unresolved cross-file FK targets. SQL is removed from `NO_PENDING_CAPABILITIES` in code. JOIN, view-source, trigger-target, and FK relationships each have a golden fixture asserting exact endpoints. Negative case: a JOIN to a non-existent table produces a structured pending, not a wrong concrete edge.
- CSS custom property and keyframe references emit relationships when syntactically provable; otherwise structured pending. Asserted by golden fixtures with exact endpoints.
- Regex capture-group and backreference relationships are emitted with exact endpoints. Negative case: a non-existent backreference number does not produce a wrong edge.

## 3. Real-World Evidence

- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` covers every repo in the `release` profile of `fixtures/extraction/tree-sitter-real-world-corpus.toml` PLUS a VB.NET reference repo. Total `verified_repos` count is at least 22.
- VB.NET reference repo is added to the `release` profile in `tree-sitter-real-world-corpus.toml` and appears in the evidence file with `status: pass`. If no suitable repo can be located after a documented search (escalation file in `docs/plans/escalations/` describes the search and proposes alternatives), the autonomous run pauses for user input rather than ship without VB.NET evidence.
- Every repo's evidence row has zero `hard_failures`.
- Every repo's `min_relationships` threshold is **raised from 1 to a meaningful per-repo value**: at minimum 5× the repo's language file count, or a hand-tuned higher value reflecting the repo's structure. The new thresholds are committed to `tree-sitter-real-world-corpus.toml`.
- Every repo carries a **representative-correctness spec** in the corpus TOML — a small list of expected facts the certify run asserts after extraction:
  - At least one named symbol with a specified kind (e.g., "Phoenix.Router exists with kind=module").
  - At least one named symbol with a reference count ≥ N (e.g., "Phoenix.Router has ≥30 references; one is in lib/blog_web/router.ex").
  - At least one parent-child relationship asserting the parent_id link (e.g., "Phoenix.Router.match has parent_id pointing at Phoenix.Router").
  - At least one identifier asserting kind and span (e.g., "Phoenix.Router type_usage in lib/blog_web/router.ex appears at line N column M").
  - At least one type or doc-comment assertion when the language carries them.
- The certify run (`cargo xtask certify tree-sitter --real-world --profile release`) fails if any representative-correctness spec is unsatisfied. The xtask's `hard_failures` logic (currently in `xtask/src/tree_sitter_real_world.rs:309`) is extended from count-only to spec-driven.
- `cargo xtask certify tree-sitter --check` passes against the regenerated evidence.

## 4. Pillar-3 Deliverables

- `crates/julie-extractors/src/lib.rs` exports a documented public API surface. Every `pub` item has a doc comment. New private items default to `pub(crate)`; existing public items keep their visibility (no breaking pruning) so the main `julie` crate's re-exports through `src/extractors/mod.rs` continue to work unchanged.
- `crates/julie-extractors` exposes `pub fn capability_snapshot() -> &'static CapabilitySnapshot`. The snapshot is loaded from `capabilities.json` via `include_str!` + `OnceLock<CapabilitySnapshot>`. **No build script.** The capabilities.json file lives inside the crate's source tree (moved or copied with `cargo:rerun-if-changed` linkage from the workspace-root copy) so `include_str!` works.
- `CapabilitySnapshot` exposes `languages() -> impl Iterator<Item = &CapabilityRow>` and `get(language: &str) -> Option<&CapabilityRow>`. Each `CapabilityRow` carries tier, target capabilities, implemented capabilities, gap status, exception reason, and evidence entries.
- `crates/julie-extractors` exposes `pub const EXTRACTION_CONTRACT_VERSION: &str` distinct from the workspace's `SEMANTIC_ENGINE_VERSION` (which lives in `src/tools/workspace/indexing/engine_version.rs`). A test asserts the workspace's semantic engine version composes from `EXTRACTION_CONTRACT_VERSION` plus DB schema version plus index format version.
- `docs/EXTRACTION_CONTRACT.md` documents the `ExtractionResults` field-by-field invariants for `Symbol`, `Relationship`, `Identifier`, `TypeInfo`, `ParseDiagnostic`, and `NormalizedSpan`, including the structured pending relationship field contract from §2.1.
- `crates/julie-extractors/src/lib.rs` has a crate-level rustdoc comment with at least one runnable quickstart code block. The block compiles under `cargo test -p julie-extractors --doc` (NOT only `cargo doc`, which does not compile doctest bodies). The current `ignore` annotation in the lib.rs quickstart is removed.
- An example consumer exists at `crates/julie-extractors/examples/extract_file.rs` (or equivalent in-crate path). It takes a file path argument, calls the public extraction entry point, and prints the resulting symbols plus the capability snapshot for the detected language. `cargo build --examples -p julie-extractors` succeeds.
- `cargo package -p julie-extractors --list` succeeds and produces a publishable archive that includes the capabilities.json data (verified by grepping the file list). This is a publish dry-run; we do not actually publish.
- A CI step builds the examples and runs `cargo test --doc` (added to the relevant xtask bucket or workflow file).

## 5. Release Gates Green at HEAD

The following all pass at the worktree HEAD with timestamps recorded in a verification ledger row appended to either this rubric or a sibling ledger file:

- `cargo fmt --check`
- `git diff --check`
- `cargo xtask certify tree-sitter --check`
- `cargo xtask test bucket extractors`
- `cargo xtask test bucket parser-upgrade`
- `cargo xtask test changed`
- `cargo xtask test system`
- `cargo xtask test dogfood`
- `cargo xtask test full`
- `cargo build --release`
- `cargo build --examples -p julie-extractors`
- `cargo test -p julie-extractors --doc`
- `cargo doc -p julie-extractors --no-deps`
- `cargo package -p julie-extractors --list`

Any failure is a blocker, not a warning.

## 6. Live MCP Dogfood (Manual Handoff)

The agent stages this section but cannot complete it itself (requires Claude Code restart). The stage note explains what to run after release rebuild. The user signs off after running:

- `manage_workspace` health reports ready for Julie.
- `call_path extract_symbols_static extract_canonical` finds the production extraction edge.
- `fast_refs extract_canonical` returns definition plus references.
- SQLite records current schema version and the new `EXTRACTION_CONTRACT_VERSION` plus the composed semantic engine version.
- `manage_workspace refresh` reports up-to-date without repeating a full reindex.

## Iteration Discipline

- **Per-task budget.** 3 failed iterations OR 90 min wall-clock without measurable progress on a single gap → write `docs/plans/escalations/2026-05-10-<gap-id>.md` and continue with other work.
- **Per-batch checkpoint.** After each phase in the design's sequencing list, commit + push + write a brief progress checkpoint to `.memories/`.
- **Hard stop.** 5+ open escalations OR `cargo xtask test full` fails after gap closure → stop, write summary, wait for user.
- **Subagent rules.** Workers run only narrow targeted tests (`cargo nextest run --lib <name>`). The lead orchestrates `cargo xtask test changed` between batches and `cargo xtask test full` for the section-5 gate.
- **Pillar-aware grading.** The `/loop` driver reads this rubric each iteration and scores per criterion. A criterion that flips back from `satisfied` to `needs_revision` due to later edits triggers a regression escalation, not silent acceptance.

## Out of Scope

- New tree-sitter language registrations. The 34 + 2 variants stay; we deepen what's there.
- FFI/WASM/CLI deliverables. The `examples/` consumer is the demo.
- Per-language idiom-depth checklists beyond the tier contract.
- Comparative benchmarks against external code-intel tools.
- Pruning currently-public items in `julie-extractors` to `pub(crate)`. The main `julie` crate depends on those re-exports.
