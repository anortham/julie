# Best-in-Class Tree-Sitter — Outcome Rubric

> Operational rubric for the autonomous run defined in `2026-05-10-best-in-class-tree-sitter-design.md`. Format follows the Anthropic Managed Agents rubric pattern: per-criterion gradeable statements scored independently. The autonomous loop iterates until every criterion below is `satisfied` against fresh evidence at the worktree HEAD.

> **DONE** = all criteria in sections 1–5 satisfied at the worktree HEAD, plus section 6 (manual MCP dogfood) handed off to the user with a staging note. The user merges the worktree after section 6 passes.

## 1. Doc Hygiene

- `docs/LANGUAGE_VERIFICATION_CHECKLIST.md` does not exist in the worktree.
- `docs/LANGUAGE_VERIFICATION_RESULTS.md` does not exist in the worktree.
- `docs/verification/` directory does not exist or is empty.
- `docs/EXTRACTION_CONTRACT.md` exists, is ≤200 lines, links to `capabilities.json`, `LANGUAGE_CERTIFICATION_REPORT.md`, and `LANGUAGE_REAL_WORLD_EVIDENCE.json`, and explains every tier in the Quality Bar's target group table.
- No file in the repo references `docs/findings/COMPILED-FINDINGS.md` (verified via repo-wide search). Every `evidence` field in `capabilities.json` points at a real test name, fixture path, or commit SHA.
- `docs/LANGUAGE_CERTIFICATION_REPORT.md` was generated at the worktree HEAD (the report's "Current HEAD" line matches `git rev-parse HEAD` exactly).
- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.{json,md}` were generated with `--profile release` and the JSON's `julie_head_in_evidence` matches `git rev-parse HEAD` exactly.
- `docs/TREE_SITTER_QUALITY_BAR.md` "Current Verdict" and "Current Open Gaps" sections were updated to reflect the closed state. No stale "open" entries remain.

## 2. Capability Matrix

- `cargo nextest run -p julie-extractors capability_matrix` passes at the worktree HEAD.
- Every row in `fixtures/extraction/capabilities.json` has `gap_status` of either `closed` (with `evidence` pointing at a passing test or fixture) or `exception` (with a reason field and a named locking test that fails if the exception is removed).
- No row has `gap_status: open`.
- Every `exception` row's locking test is a real `#[test]` function in `crates/julie-extractors/src/tests/` that asserts the absence of the unsupported capability. Removing the exception (without code changes) causes the test to fail.
- The known-limitations from the historical `LANGUAGE_VERIFICATION_RESULTS.md` "Known Limitations" table are represented as `exception` rows or have been turned into `closed` rows with new fixtures: C++ header-only zero cross-file refs, C++ within-file constructor disambiguation, Lua class-like tables as variable kind.
- Per-language fixture-only gaps (rust, c, cpp, go, zig, typescript, javascript, python, java, csharp, vbnet, php, ruby, swift, kotlin, scala, dart, elixir, lua, r, bash, powershell, gdscript) each have at least one golden fixture that emits a non-empty `pending_relationships` or `structured_pending_relationships` array.
- JSON Schema `$ref` produces a relationship edge that resolves to the schema-definition target when the target exists locally, or a structured pending relationship when external. Asserted by a JSON golden fixture.
- TOML Cargo-style dependency tables (`[dependencies]`, `[dev-dependencies]`, etc.) and pyproject `[tool.<x>]` tables produce relationship edges to the named dependency. Asserted by a TOML golden fixture covering both shapes.
- SQL emits structured pending relationships for unresolved cross-file FK targets. SQL is removed from `NO_PENDING_CAPABILITIES` in code, and a SQL golden fixture proves the structured pending output.

## 3. Real-World Evidence

- `docs/LANGUAGE_REAL_WORLD_EVIDENCE.json` `verified_repos` count equals 21.
- Every repo in the `release` profile of `fixtures/extraction/tree-sitter-real-world-corpus.toml` appears with `status: pass`. Specifically: julie, pandora, zls, zod, flask, cobra, gson, Slim, sinatra, Newtonsoft.Json, Alamofire, moshi, jq, nlohmann-json, riverpod, lite, cats, phoenix, express, kirigami, blazor-samples.
- A VB.NET reference repo is identified, added to the corpus, and present in the evidence file with `status: pass`. If no suitable repo can be located, an escalation file in `docs/plans/escalations/` documents the search and proposes an alternative.
- Every repo's evidence row has zero `hard_failures`.
- Every repo's `min_relationships` threshold is met or exceeded.
- `cargo xtask certify tree-sitter --check` passes against the regenerated evidence.

## 4. Pillar-3 Deliverables

- `crates/julie-extractors/src/lib.rs` exports a documented public API surface. Every `pub` item has a doc comment. Items not intended as public API are scoped to `pub(crate)`.
- `crates/julie-extractors` exposes `pub fn capabilities() -> &'static CapabilitySnapshot` returning a snapshot derived from `capabilities.json` at compile time. `CapabilitySnapshot` has accessors for tier, target capabilities, implemented capabilities, and exception list per language.
- `crates/julie-extractors` exposes `pub const SEMANTIC_ENGINE_VERSION: &str` matching the value in `src/database/schema.rs` (or wherever it is defined). A test asserts the constant and the schema value agree.
- `docs/EXTRACTION_CONTRACT.md` documents the `ExtractionResults` field-by-field invariants for `Symbol`, `Relationship`, `Identifier`, `TypeInfo`, `ParseDiagnostic`, and `NormalizedSpan`.
- `crates/julie-extractors/src/lib.rs` has a crate-level rustdoc comment with at least one runnable `quickstart` code block. `cargo doc -p julie-extractors --no-deps` succeeds without warnings about that doctest.
- An example consumer exists at `crates/julie-extractors/examples/extract_file.rs` (or equivalent path). It takes a file path argument, calls `extract_canonical` (or the public extraction entry point), and prints the resulting symbols plus the capability snapshot for the detected language. `cargo build --examples -p julie-extractors` succeeds.
- A CI step builds the examples (added to the relevant xtask bucket or workflow file).

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
- `cargo doc -p julie-extractors --no-deps`

Any failure is a blocker, not a warning.

## 6. Live MCP Dogfood (Manual Handoff)

The agent stages this section but cannot complete it itself (requires Claude Code restart). The stage note explains what to run after release rebuild. The user signs off after running:

- `manage_workspace` health reports ready for Julie.
- `call_path extract_symbols_static extract_canonical` finds the production extraction edge.
- `fast_refs extract_canonical` returns definition plus references.
- SQLite records current schema version and the new semantic engine version.
- `manage_workspace refresh` reports up-to-date without repeating a full reindex.

## Iteration Discipline

- **Per-task budget.** 3 failed iterations OR 90 min wall-clock without measurable progress on a single gap → write `docs/plans/escalations/2026-05-10-<gap-id>.md` and continue with other work.
- **Per-batch checkpoint.** After each tier (general programming → component/template → query/declarative → doc/data), commit + push + write a brief progress checkpoint to `.memories/`.
- **Hard stop.** 5+ open escalations OR `cargo xtask test full` fails after gap closure → stop, write summary, wait for user.
- **Subagent rules.** Workers run only narrow targeted tests (`cargo nextest run --lib <name>`). The lead orchestrates `cargo xtask test changed` between batches and `cargo xtask test full` for the section-5 gate.

## Out of Scope

- New tree-sitter language registrations. The 34 + 2 variants stay; we deepen what's there.
- FFI/WASM/CLI deliverables. The `examples/` consumer is the demo.
- Per-language idiom-depth checklists beyond the tier contract.
- Comparative benchmarks against external code-intel tools.
