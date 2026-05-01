# Fast Search Metrics Follow-Up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Reduce remaining `fast_search` zero-hit churn after file mode by catching malformed scopes early, steering path and symbol misses toward better targets, and adding telemetry fields that explain why a search recovered or failed.

**Architecture:** Keep search execution behavior in the existing `src/tools/search/` pipeline, but move request-level diagnostics into a small helper so `src/tools/search/mod.rs` does not grow more. Extend `SearchTrace` for metrics instead of inferring behavior from output text. Update agent-facing instructions after behavior is covered by tests.

**Tech Stack:** Rust, Tantivy-backed Julie search, SQLite-backed telemetry, `cargo nextest`, `cargo xtask`.

---

## Metrics Basis

The plan comes from `~/.julie/daemon.db` telemetry gathered through May 1, 2026:

- Typed post-change slice: 2,759 `fast_search` calls from April 20 through May 1.
- `search_target="files"`: 134 calls, 5.2% zero-hit rate, about 13 ms average.
- `search_target="content"`: 2,025 calls, 19.6% zero-hit rate.
- Scoped content rescue reduced patterned content zeroes from 27.9% before rescue to 10.5% after rescue, with 40 rescued result sets.
- Remaining post-rescue content misses are led by `line_match_miss`, malformed multi-glob `file_pattern`, and symbol or path-shaped queries sent to `content`.

## File Structure

**Create**
- `src/tools/search/input_diagnostics.rs`  
  Request-level diagnostics before Tantivy or readiness checks. Owns malformed `file_pattern` detection and structured diagnostic execution results.

**Modify**
- `src/tools/search/mod.rs:21-30`  
  Register the new helper module.
- `src/tools/search/mod.rs:236-660`  
  Add the early diagnostic branch and stamp trace fields after execution.
- `src/tools/search/query.rs:38-47`  
  Reuse and, if needed, tighten whitespace-separated multi-glob detection without breaking literal-space glob matching.
- `src/tools/search/trace.rs:191-235`  
  Add new hint variants and trace fields.
- `src/tools/search/trace.rs:293-308`  
  Add a zero-result diagnostic constructor if that is cleaner than open-coding the trace in `input_diagnostics.rs`.
- `src/tools/search/hint_formatter.rs:23-201`  
  Add path-target and definitions-target hint builders and precedence.
- `src/tools/search/formatting.rs:126-328`  
  Expose exact definition match detection for trace stamping.
- `src/tools/search/execution.rs:167-281`  
  Set line-match strategy trace labels for content search.
- `src/handler/search_telemetry.rs:9-38`  
  Persist new trace fields.
- `JULIE_AGENT_INSTRUCTIONS.md:13-18` and `JULIE_AGENT_INSTRUCTIONS.md:56-64`  
  Strengthen guidance for `files` target and symbol lookup after tests pass.

**Test**
- `src/tests/tools/search/file_pattern_tests.rs:328-402`
- `src/tests/tools/search/promotion_tests.rs:16-156`
- `src/tests/tools/search/line_match_strategy_tests.rs:227-264`
- `src/tests/tools/search/file_mode_tests.rs:65-111`
- `src/tests/core/handler_telemetry.rs:140-380`
- `src/tests/tools/search/definition_promotion_tests.rs:49-259`
- `src/tests/integration/zero_hit_replay_tests.rs:255-391`

## Implementation Tasks

### Task 1: Request-Level Multi-Glob Diagnostics

**Files:**
- Create: `src/tools/search/input_diagnostics.rs`
- Modify: `src/tools/search/mod.rs:21-30`
- Modify: `src/tools/search/mod.rs:236-260`
- Modify: `src/tools/search/query.rs:38-47`
- Modify: `src/tools/search/trace.rs:223-308`
- Test: `src/tests/tools/search/file_pattern_tests.rs:328-402`
- Test: `src/tests/tools/search/file_mode_tests.rs:65-111`
- Test: `src/tests/core/handler_telemetry.rs:169-196`

**What to build:** Detect whitespace-separated multi-globs such as `src/** docs/**` before readiness checks or Tantivy search. Return a successful diagnostic result with zero hits, `hint_kind=file_pattern_syntax_hint`, and `file_pattern_diagnostic=whitespace_separated_multi_glob`.

**Approach:** Reuse `looks_like_whitespace_separated_globs` so literal-space glob behavior stays intact. The helper should build a `FastSearchExecution` with an empty `SearchExecutionResult`, `strategy="fast_search_input_diagnostic"`, the parsed `SearchTarget` kind, and the same correction text already used by `build_file_pattern_syntax_hint`. Keep the branch near the top of `execute_with_trace`, after target parsing and before readiness checks.

**Acceptance criteria:**
- [ ] `file_pattern="src/** docs/**"` returns the syntax correction without opening the workspace index.
- [ ] Trace metadata records `file_pattern_diagnostic=whitespace_separated_multi_glob` and `hint_kind=file_pattern_syntax_hint`.
- [ ] Literal-space glob tests still pass.
- [ ] `files` and `definitions` targets receive the same correction when the same malformed pattern is supplied.
- [ ] Worker-scope verification passes and the task is committed.

### Task 2: Target-Specific Zero-Hit Hints

**Files:**
- Modify: `src/tools/search/hint_formatter.rs:23-201`
- Modify: `src/tools/search/trace.rs:191-201`
- Modify: `src/tools/search/mod.rs:400-430`
- Test: `src/tests/tools/search/promotion_tests.rs:16-156`
- Test: `src/tests/core/handler_telemetry.rs:228-276`

**What to build:** Add two new hint kinds for content zeroes: one that steers path-like queries toward `search_target="files"`, and one that steers symbol-like queries toward `search_target="definitions"`. These should reduce repeated content misses where the query shape already tells us the target choice was off.

**Approach:** Add `HintKind::FileTargetHint` and `HintKind::DefinitionsTargetHint`. Keep precedence as syntax hint, out-of-scope hint, file-target hint, definitions-target hint, then multi-token hint. Path-like detection should be language-agnostic: slash or backslash separators, filename extensions, and path fragments with directory markers. Symbol-like detection should favor `::`, dotted identifiers, call-shaped strings ending with `(`, CamelCase, or snake_case single-token queries when the content search has `LineMatchMiss` or `TantivyNoCandidates`.

**Acceptance criteria:**
- [ ] `fixtures/real-world/php/index.php` as a content zero receives `file_target_hint`.
- [ ] `src/tools/search/mod.rs` as a content zero receives `file_target_hint`.
- [ ] `ArgAction::SetTrue`, `OS.has_feature`, `format_line_mode_output`, and `fast_refs_metadata(` receive `definitions_target_hint` when content search returns zero.
- [ ] Multi-token concept misses that are not path-like or symbol-like still receive `multi_token_hint`.
- [ ] Hint text contains a concrete replacement call with the same query and the better `search_target`.
- [ ] Worker-scope verification passes and the task is committed.

### Task 3: Trace Promotion And Strategy Fields

**Files:**
- Modify: `src/tools/search/trace.rs:223-235`
- Modify: `src/tools/search/execution.rs:167-281`
- Modify: `src/tools/search/formatting.rs:126-328`
- Modify: `src/tools/search/mod.rs:620-650`
- Modify: `src/handler/search_telemetry.rs:9-38`
- Test: `src/tests/core/handler_telemetry.rs:279-380`
- Test: `src/tests/tools/search/definition_promotion_tests.rs:49-259`
- Test: `src/tests/tools/search/promotion_tests.rs:85-156`

**What to build:** Persist trace fields for `line_match_strategy`, `definition_exact_match`, and `target_hint`. This gives future metric passes a direct answer for whether a content miss used `Substring`, `Tokens`, or `FileLevel`, and whether a definitions response rendered the exact-match promotion path.

**Approach:** Add serializable fields on `SearchTrace` with neutral defaults: `line_match_strategy: Option<String>`, `definition_exact_match: bool`, and `target_hint: Option<String>`. Set `line_match_strategy` inside content execution from the same strategy used by `line_mode_matches`. Expose a small `formatting` helper for exact definition name detection so telemetry and formatter share the same rule. Set `target_hint` when Task 2 chooses a file or definitions hint.

**Acceptance criteria:**
- [ ] Content telemetry records `line_match_strategy` for zero and non-zero content searches.
- [ ] Definitions telemetry records `definition_exact_match=true` when `format_definition_search_results` would render the promoted exact-match path.
- [ ] Hint telemetry records `target_hint="files"` or `target_hint="definitions"` when Task 2 fires.
- [ ] Existing trace serialization tests cover defaults so older callers remain source-compatible.
- [ ] Worker-scope verification passes and the task is committed.

### Task 4: Punctuation And Separator Regression Tests

**Files:**
- Modify: `src/tools/search/query.rs:301-445`
- Test: `src/tests/tools/search/line_match_strategy_tests.rs:227-264`

**What to build:** Add regression coverage for the punctuation-heavy shapes that still show up as content misses. Fix the line verifier only where a test proves Julie is rejecting a line that should match under the selected strategy.

**Approach:** Start with tests for `OS.has_feature`, `ArgAction::SetTrue`, `target_symbol_id.unwrap_or`, and `tool_name(&self)`. If the existing verifier already behaves correctly for a case, keep the test as coverage and do not change code for that case. If a false negative appears, normalize punctuation through `tokenize_text_sequence` or `term_matches_tokens` instead of adding path or Rust-specific special cases.

**Acceptance criteria:**
- [ ] Tests document which punctuation shapes match as literal substring and which rely on tokenized matching.
- [ ] Any verifier change is language-agnostic and uses existing tokenizer helpers.
- [ ] No Rust-only path, file, or symbol heuristics are added.
- [ ] Worker-scope verification passes and the task is committed.

### Task 5: Agent Guidance And Replay Evidence

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md:13-18`
- Modify: `JULIE_AGENT_INSTRUCTIONS.md:56-64`
- Modify: `docs/SEARCH_FLOW.md:91-103`
- Modify: `docs/2026-04-27-search-zero-hit-reduction-plan.md:1-8`
- Test: `src/tests/integration/zero_hit_replay_tests.rs:255-391`

**What to build:** Update documentation and injected agent guidance so path or basename hunts use `search_target="files"` sooner, and symbol hunts use `definitions` sooner. Re-run the zero-hit replay acceptance test after implementation to show the post-change miss profile.

**Approach:** Keep documentation short and operational. Add a `files` target entry to the search flow doc, strengthen `JULIE_AGENT_INSTRUCTIONS.md` examples, and append a dated metrics note to the prior zero-hit plan that records the May 1 findings and the new fields to watch.

**Acceptance criteria:**
- [ ] Tool guidance says `files` is the preferred target for path fragments, basenames, and file extensions.
- [ ] Tool guidance says `definitions` is the preferred target for symbol names and call-shaped identifiers.
- [ ] Search docs describe `content`, `definitions`, and `files` as three distinct targets.
- [ ] Replay evidence records raw zero-hit rate, without-recourse rate, top reasons, and hint mix for the new trace fields.
- [ ] Worker-scope verification passes and the task is committed.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Each worker runs the exact test it wrote with `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`. Workers use one RED run and one GREEN run unless the lead assigns more.

**Worker ceiling:** Workers may run only their assigned exact tests. They do not run `cargo xtask test changed`, `cargo xtask test dev`, or broad `cargo nextest run --lib`.

**Lead affected-change scope:** After a coherent batch lands, run `cargo xtask test changed`.

**Branch gate:** Before handoff, run `cargo xtask test dev`.

**Escalation triggers:** Run `cargo xtask test dogfood` if search scoring, tokenization, query construction, line verification, or replay fixture behavior changes beyond the planned helper scope. Run `cargo xtask test system` only if workspace readiness, daemon routing, or index lifecycle behavior changes.

**Verification ledger:** Record command, scope label, commit SHA, result, and timestamp. If the same HEAD already has passing evidence for the required scope, reuse that evidence instead of rerunning the same expensive gate.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Planning, architecture, decomposition, lead review, and finding triage.
- Harness mapping: Codex `gpt-5.5` medium or high.

**Implementation tier:** Bounded worker tasks from this plan with narrow file ownership.
- Harness mapping: Codex `gpt-5.4-mini` high.

**Mechanical tier:** Docs, rote test fixture updates, formatting, and manifests.
- Harness mapping: Codex `gpt-5.4-mini` low or medium.

**Coupled implementation tier:** Bounded cross-file edits with search semantics or telemetry coupling.
- Harness mapping: Codex `gpt-5.4-mini` xhigh, escalating to `gpt-5.3-codex` xhigh when tool-heavy debugging appears.

**Escalation tier:** Subtle correctness, high blast radius, weak tests, repeated failure, or search semantics disagreement.
- Harness mapping: Codex `gpt-5.3-codex` xhigh for review or first escalation, `gpt-5.5` high or xhigh for planning or architecture failure.

**Worker eligibility:** Implementation-tier workers are eligible when acceptance criteria are clear, write scope is narrow, verification is exact-test level, and the task does not depend on hidden search or workspace invariants.

**Escalation triggers:** Escalate when a worker fails twice, when a passing test still leaves plausible search behavior risk, when query semantics no longer match this plan, or when a change touches shared lifecycle, indexing, or public MCP contracts outside the named files.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note that in the worker report, and continue.

## Task Decomposition

- Task 1 owns `input_diagnostics.rs`, early request diagnostics, and malformed pattern tests.
- Task 2 owns hint classification and hint text.
- Task 3 owns trace shape, telemetry serialization, and exact-match promotion metrics.
- Task 4 owns tokenizer and line verifier regression coverage.
- Task 5 owns docs and replay evidence.

Tasks 1 and 4 can run in parallel. Task 2 should wait for Task 1 trace decisions. Task 3 should run after Task 2 so `target_hint` has concrete producers. Task 5 should run after Tasks 1 through 4 so docs match shipped behavior.

## Risks

- `src/tools/search/mod.rs` already exceeds the project target file size. New behavior should live in `input_diagnostics.rs` or existing focused helpers, with only narrow wiring in `mod.rs`.
- File and symbol heuristics can become language-specific if rushed. Keep detection shape-based and test examples from several naming conventions.
- Early diagnostics must preserve telemetry. Returning a plain `execution=None` result would hide the malformed pattern bucket again.
- `definition_exact_match` must use the same rule as formatting, or the metric will become ornamental.

## Completion Criteria

- All acceptance criteria are met.
- Worker exact tests pass for each task.
- Lead `cargo xtask test changed` passes after implementation batches.
- Lead `cargo xtask test dev` passes before handoff.
- Metrics-facing fields are present in `fast_search_metadata`.
- Documentation explains when to use `content`, `definitions`, and `files` without adding noisy prose.
