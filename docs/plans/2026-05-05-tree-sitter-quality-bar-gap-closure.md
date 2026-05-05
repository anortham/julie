# Tree-Sitter Quality Bar Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Close the known gaps in `docs/TREE_SITTER_QUALITY_BAR.md` by making target capabilities explicit and by implementing native relationship output where Julie currently has meaningful missing graph semantics.

**Architecture:** Keep `capabilities` as the implemented extractor contract, add explicit target and gap metadata to the fixture matrix, and make tests fail when a language lowers implemented claims without preserving the fixed target. Add language-local relationship extractors for native reference features only where the parser or existing manual parser can prove the edge without guessing.

**Tech Stack:** Rust, tree-sitter, extractor golden fixtures, `cargo nextest`, `cargo xtask test`.

---

## Scope

This plan is lead-owned in the current Codex run. The Julie project asks for subagents, but this Codex session only permits subagents when the user explicitly asks for them. The plan remains decomposed so it can be delegated later if the harness policy changes.

## File Structure

- Modify: `fixtures/extraction/capabilities.json`
- Modify: `crates/julie-extractors/src/tests/capability_matrix.rs`
- Modify: `crates/julie-extractors/src/language_spec.rs`
- Modify: `crates/julie-extractors/src/registry.rs`
- Modify: `crates/julie-extractors/src/regex/mod.rs`
- Create: `crates/julie-extractors/src/regex/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/regex/mod.rs`
- Modify: `crates/julie-extractors/src/vue/mod.rs`
- Create: `crates/julie-extractors/src/vue/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/vue/mod.rs`
- Modify: `crates/julie-extractors/src/css/mod.rs`
- Create: `crates/julie-extractors/src/css/relationships.rs`
- Modify: `crates/julie-extractors/src/markdown/mod.rs`
- Create: `crates/julie-extractors/src/markdown/relationships.rs`
- Modify: `crates/julie-extractors/src/yaml/mod.rs`
- Create: `crates/julie-extractors/src/yaml/relationships.rs`
- Modify: fixture sources and expected outputs under `fixtures/extraction/{css,markdown,regex,vue,yaml}/basic/`
- Modify: `docs/TREE_SITTER_QUALITY_BAR.md`

## Task 1: Target Capability Matrix

**What to build:** Extend each capability row with `target_capabilities` and `capability_gaps`. Keep `capabilities` as implemented behavior so registry compatibility stays obvious. A gap entry names the capability, status (`open` or `exception`), reason, required closure, and evidence path. Tests fail when a target is true, implementation is false, and no explicit gap or exception exists.

**Approach:** Start with a failing capability-matrix test that requires the new fields on every language row. Then add validation that target false plus implementation true is invalid, gap entries reference real capabilities, open gaps require closure text and evidence, and exception gaps require a non-implementation reason plus test evidence. Once code gaps close, `vue`, `regex`, `css`, `markdown`, and `yaml` should have met relationship targets. `json` and `toml` should keep relationship targets false with explicit non-applicability reasons.

**Acceptance criteria:**
- The fixture matrix distinguishes target and implemented capabilities for every supported language.
- Lowering `capabilities.relationships` no longer hides a true target requirement.
- Open gaps are explicit, machine-checked, and tied to evidence.
- `capability_matrix_matches_registry_entries` still verifies implemented capabilities against the registry.

## Task 2: Regex Relationships

**What to build:** Implement local regex `References` relationships from regex patterns or containing named groups to capture-group symbols for named backreferences and numeric backreferences.

**Approach:** Add a failing test for `(?<word>\w+)-\k<word>` and a failing test for `(a)(b)\2\1` only if tree-sitter exposes enough span information to map numeric backrefs safely. Use named-group metadata and symbol spans for lookup. Numeric relationships must preserve capture order and skip unresolved or ambiguous backrefs rather than producing wrong edges.

**Acceptance criteria:**
- Named backrefs produce local `References` relationships to the matching named group symbol.
- Numeric backrefs produce local `References` relationships only when the referenced capture index maps to a known capturing group.
- Unknown backrefs produce no confident local edge.
- The regex golden fixture includes relationship output.

## Task 3: Vue Relationships

**What to build:** Implement Vue SFC relationships for local script calls, template bindings/events, style class references, and external component usage.

**Approach:** Reuse existing script and template parsing helpers. Local script calls already exist as identifiers, so local call relationships should resolve only when the called name uniquely matches a local function symbol. Template must add `References` edges from the component symbol to local script symbols for `{{ title }}` and `@click="increment"`. Template component tags that do not have local symbols should become structured pending references rather than guessed local edges if registry support is added during the task; otherwise keep them out of local relationships and leave the target gap open in the matrix for pending Vue references.

**Acceptance criteria:**
- `format("Worker")` in `<script setup>` produces a `Calls` relationship to the local `format` symbol.
- `{{ title }}` produces a `References` relationship to the local `title` symbol.
- `@click="increment"` produces a `References` or `Calls` relationship only when `increment` is a local method or function symbol.
- External component tags are not resolved to unrelated local symbols.
- The Vue golden fixture includes relationship output.

## Task 4: Query and Data Reference Review

**What to build:** Close the incomplete target review by implementing or explicitly marking native reference semantics for `css`, `markdown`, `yaml`, `json`, and `toml`.

**Approach:** Implement local `References` relationships for CSS custom property uses and keyframe animation uses, Markdown links to local headings, and YAML aliases to anchors. Keep JSON and TOML relationship targets false with explicit non-applicability reasons, because standalone JSON and TOML do not have native reference syntax comparable to anchors or links.

**Acceptance criteria:**
- CSS `var(--brand)` references the local custom property symbol when present.
- CSS `animation: spin ...` references the local `@keyframes spin` symbol when present.
- Markdown `[label](#heading)` references the local heading symbol when present.
- YAML `*anchor` references the local `&anchor` symbol when present.
- JSON and TOML relationship targets are explicitly false for non-applicability, not because implementation is thin.
- Related golden fixtures include relationship output where relationships are meaningful.

## Task 5: Documentation and Evidence

**What to build:** Update `docs/TREE_SITTER_QUALITY_BAR.md` so the open gaps table reflects the new matrix schema and implemented relationship evidence.

**Approach:** Keep the status honest. Do not mark the overall bar achieved unless the required release gates and live dogfood evidence are recorded at the current commit. If code gaps close but release gates are not complete yet, say exactly that.

**Acceptance criteria:**
- The open gap list does not claim `vue`, `regex`, or data-reference review gaps are still open after tests and fixtures prove them.
- Verification ledger entries use scope labels that distinguish current-contract gates from fixed-target gates.
- Any remaining release-only gap is concrete and tied to the exact command or live MCP check still needed.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, `docs/TREE_SITTER_QUALITY_BAR.md`.

**Worker red/green scope:** Use exact test filters first:
- `cargo nextest run -p julie-extractors capability_matrix_requires_target_capabilities`
- `cargo nextest run -p julie-extractors regex_relationships_resolve_named_backrefs`
- `cargo nextest run -p julie-extractors vue_relationships_resolve_script_and_template_refs`
- Exact CSS, Markdown, and YAML relationship tests added in Task 4.

**Worker ceiling:** Exact tests only during RED/GREEN. The lead owns buckets.

**Worker gate invariant:** Each exact test proves one native reference behavior or one matrix validation rule. Golden fixture regeneration is not evidence until `golden_fixtures_match_canonical_extraction` passes without `UPDATE_GOLDEN`.

**Lead affected-change scope:** `cargo xtask test changed`.

**Branch gate:** `cargo xtask test dev`. Add `cargo xtask test dogfood` because graph output affects refs and navigation. Add `cargo xtask test bucket parser-upgrade` because expected extraction fixtures change.

**Replay/metric evidence:** Golden fixture diffs are hard gates. Relationship counts alone are report-only unless the tests assert the exact edge target and kind.

**Escalation triggers:** Parser node shapes cannot support a claimed native reference, relationship implementation would require guessing, matrix schema changes reveal additional target gaps, or exact tests pass while golden fixtures lose existing symbols, identifiers, types, or diagnostics.

**Assigned verification failure:** Stop and diagnose unless the failing command is intentionally RED for a test just written.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp in `docs/TREE_SITTER_QUALITY_BAR.md` once gates pass for the current HEAD.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Planning, target semantics, and final review. Codex route: `gpt-5.5 medium/high` when delegation is available.

**Implementation tier:** Bounded extractor tasks with exact tests. Codex route: `gpt-5.4-mini xhigh` when delegation is available and task ownership is disjoint.

**Coupled implementation tier:** Matrix schema plus registry and language-spec changes. Codex route: `gpt-5.3-codex high` if delegated.

**Gate review:** Golden fixture and bucket interpretation. Codex route: `gpt-5.3-codex high`.

**Escalation tier:** Parser limitation, weak tests, or unexpected graph correctness risk. Codex route: `gpt-5.5 high/xhigh`.

**Unsupported harness behavior:** Current lead session will execute directly unless the user explicitly requests subagents, because this Codex tool policy gates subagent spawning on explicit user request.

