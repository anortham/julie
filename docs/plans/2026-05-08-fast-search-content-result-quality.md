# Fast Search Content Result Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make `fast_search` content results less noisy and more token-efficient by tightening file-level line matching for compound code terms and grouping same-file output so repeated paths do not dominate results.

**Architecture:** Local search-presentation and line-filtering change. Keep Tantivy query execution, index schema, scoring, and public tool parameters stable unless implementation evidence proves a deeper change is required.

**Tech Stack:** Rust, Julie MCP tools, Tantivy-backed `fast_search`, `cargo nextest`, `cargo xtask`.

**Architecture Quality:** This is a behavior correction inside the existing search module, not a new abstraction layer. Matching changes belong in `src/tools/search/query.rs`; output compaction belongs near line-mode/content formatting. `src/tools/search/line_mode.rs` is already over the target implementation-file size, so prefer extracting grouped line-output helpers instead of adding bulk there.

---

## Background

Observed live result:

```text
julie.fast_search({
  "workspace":"julie_528d4264",
  "query":"workspace_is_primary edit_file",
  "search_target":"content",
  "file_pattern":"src/tests/core/handler_telemetry.rs",
  "limit":100,
  "return_format":"full"
})
```

Current output repeats the same path on every matching line:

```text
src/tests/core/handler_telemetry.rs:8:use ...
src/tests/core/handler_telemetry.rs:9:use ...
src/tests/core/handler_telemetry.rs:10:use ...
```

Subagent investigation found two separate issues:

- Formatting issue: `format_line_mode_output` prints `path:line:text` for every line, even when all hits are in one file.
- Matching issue: `FileLevel` line filtering treats compound query terms as broad subtokens. `workspace_is_primary edit_file` routes to `FileLevel` today, so it can show lines containing only `workspace`, `primary`, `edit`, or `file`, which pulls in low-value imports and setup lines.
- Related issue: `Tokens` uses the same broad helper for exclusion queries, so compound-aware matching should cover both `FileLevel` and `Tokens` unless tests prove a compatibility risk.

Verified code touchpoints:

- `src/tools/search/query.rs:196` `line_match_strategy`
- `src/tools/search/query.rs:301` `line_matches`
- `src/tools/search/query.rs:472` `term_matches_tokens`
- `src/tools/search/line_mode.rs:659` `format_line_mode_output`
- `src/tools/search/formatting.rs:254` `format_content_locations_only`
- `src/tests/tools/search/line_match_strategy_tests.rs`

## Scope

In scope:

- Tighten `FileLevel` line matching for compound code terms.
- Keep simple-term file-level matching behavior intact.
- Group full content results by file.
- Group locations-only content results by file.
- Add regression tests for both matching quality and compact formatting.
- Capture before/after dogfood evidence for the user-reported query.

Out of scope for this plan:

- Changing Tantivy index schema.
- Changing MCP input schema for `fast_search`.
- Auditing every other tool. That is phase 2 and starts only after this `fast_search` change passes search gates.

## Design

### 1. Compound Term Matching

Current `FileLevel` behavior:

```rust
terms.iter().any(|term| term_matches_tokens(term, &line_tokens))
```

Current `Tokens` behavior:

```rust
required.iter().all(|term| term_matches_tokens(term, &line_tokens))
excluded.iter().all(|term| !term_matches_tokens(term, &line_tokens))
```

Problem: `term_matches_tokens("workspace_is_primary", ...)` tokenizes the term and matches if any component appears. The reported query uses `FileLevel`; exclusion queries use `Tokens` and have the same compound-subtoken weakness.

Desired behavior:

- Simple terms keep existing token/stem behavior.
- Compound code terms match only when the compound appears as a coherent identifier or contiguous token sequence.
- OR behavior across query terms remains for `FileLevel`, but each compound term becomes stricter internally.
- AND/exclusion behavior remains for `Tokens`, but required and excluded compound terms use the same stricter compound matcher.

Examples:

| Query term | Line | Expected |
|---|---|---|
| `workspace_is_primary` | `fn workspace_is_primary(...)` | match |
| `workspace_is_primary` | `workspace_label: Some("primary")` | no match |
| `edit_file` | `EditFileTool` | match if existing tokenizer normalizes this as contiguous `edit file` |
| `edit_file` | `file_path: ...` | no match |
| `-edit_file` | `file_path: ...` | should not exclude this line just because it contains `file` |
| `statistics` | `let stats = statistics()` | preserve existing simple-term behavior |

Implementation direction:

- Add a helper in `src/tools/search/query.rs`, tentatively `term_matches_line(term, line, line_tokens)`.
- Reuse existing `token_sequence_contains_contiguous_window` at `src/tools/search/query.rs:462`; do not reimplement window matching.
- Detect compound terms using existing tokenization output plus raw term shape: underscores, hyphens, dots, colons, slashes, camel case, or multiple normalized tokens.
- For compound terms, require one of:
  - case-insensitive raw term occurrence,
  - normalized contiguous token-window match,
  - existing identifier normalization that preserves compound adjacency.
- Route both `LineMatchStrategy::FileLevel` and `LineMatchStrategy::Tokens` through the compound-aware helper.
- Keep simple-term behavior equivalent to current `term_matches_tokens`.

### 2. Grouped Full Output

Current full line-mode output repeats the path per hit.

Desired full output shape:

```text
📄 File-level search in [julie_528d4264]: 'workspace_is_primary edit_file' (found 5 lines across 1 files)

src/tests/core/handler_telemetry.rs (5 lines)
  48: fn sample_file_hit() -> SearchHit {
  62:     tool: "edit_file".to_string(),
```

Acceptance:

- For N hits in one file, that file path appears once in the body.
- Line numbers and line content remain present.
- Multi-file output is grouped by file in stable result order.
- Existing header count remains accurate.

Implementation direction:

- Avoid expanding `src/tools/search/line_mode.rs`.
- Prefer a small output helper module, tentatively `src/tools/search/line_output.rs`, for grouping and rendering line matches.
- Update `format_line_mode_output` to delegate grouping to that helper.

### 3. Grouped Locations Output

Current `return_format="locations"` for content uses `format_content_locations_only` and repeats the path per hit.

Desired locations output shape:

```text
src/tests/core/handler_telemetry.rs: 48, 62, 79, 103
```

Acceptance:

- For N location hits in one file, that file path appears once.
- Multi-file output preserves first-seen file order.
- Line numbers are sorted in the same order as existing hits, not re-ranked.
- Empty/malformed line hits keep current defensive behavior.

Implementation direction:

- Use a small private grouping helper in `src/tools/search/formatting.rs` for locations output first.
- Share with `line_output.rs` only if the helper stays type-neutral; full output uses `LineModeSearchResult`, while locations output uses `OptimizedResponse<SearchHit>`.
- Do not change `return_format="locations"` for symbol/file search targets unless tests prove shared code makes that necessary.

## Implementation Tasks

### Task 1: Tighten compound line matching

Files:

- Modify: `src/tools/search/query.rs`
- Test: `src/tests/tools/search/line_match_strategy_tests.rs`

Steps:

1. Add failing tests:
   - `test_file_level_compound_identifier_matches_contiguous_identifier`
   - `test_file_level_compound_identifier_does_not_match_individual_subtokens`
   - `test_tokens_compound_required_identifier_does_not_match_individual_subtokens`
   - `test_tokens_compound_excluded_identifier_does_not_exclude_individual_subtokens`
   - `test_file_level_simple_terms_keep_tokenized_matching`
2. Run RED commands:
   ```bash
   cargo nextest run --lib test_file_level_compound_identifier_does_not_match_individual_subtokens
   cargo nextest run --lib test_tokens_compound_required_identifier_does_not_match_individual_subtokens
   ```
3. Add compound-aware helper and route both `LineMatchStrategy::FileLevel` and `LineMatchStrategy::Tokens` through it.
4. Reuse `token_sequence_contains_contiguous_window`; do not create a second window matcher.
5. Preserve simple-term behavior and same-line AND/exclusion semantics.
6. Run GREEN commands:
   ```bash
   cargo nextest run --lib test_file_level_compound_identifier_matches_contiguous_identifier
   cargo nextest run --lib test_file_level_compound_identifier_does_not_match_individual_subtokens
   cargo nextest run --lib test_tokens_compound_required_identifier_does_not_match_individual_subtokens
   cargo nextest run --lib test_tokens_compound_excluded_identifier_does_not_exclude_individual_subtokens
   cargo nextest run --lib test_file_level_simple_terms_keep_tokenized_matching
   ```

Acceptance:

- New compound tests fail before implementation and pass after.
- Reported query remains `FileLevel`; exclusion queries exercise `Tokens`; both paths use stricter compound matching.
- Existing line-match strategy tests still pass.
- No Tantivy query construction changes.

### Task 2: Compact full content output

Files:

- Prefer create: `src/tools/search/line_output.rs`
- Modify: `src/tools/search/line_mode.rs`
- Modify if needed: `src/tools/search/mod.rs`
- Test: existing search output test file under `src/tests/tools/search/`

Steps:

1. Audit existing rendered-output assertions before adding new tests:
   ```bash
   rg "format_line_mode_output|return_format.*full|found .*lines|:[0-9]+:" src/tests/tools/search src/tests/core
   ```
2. Update any old flat `path:line:text` assertions that cover line-mode full output as part of this task.
3. Add failing formatting test, name:
   - `test_fast_search_content_full_groups_same_file_hits`
4. Test fixture should construct or invoke line-mode output with at least three hits in one file.
5. Assert:
   - file path occurs once in body,
   - each line number/content remains present,
   - header count remains unchanged.
6. Implement grouped renderer.
7. Run:
   ```bash
   cargo nextest run --lib test_fast_search_content_full_groups_same_file_hits
   ```

Acceptance:

- Same-file full output is grouped.
- Multi-file order remains deterministic.
- `src/tools/search/line_mode.rs` does not grow materially; move rendering logic out if needed.

### Task 3: Compact locations-only content output

Files:

- Modify: `src/tools/search/formatting.rs`
- Share with `src/tools/search/line_output.rs` only if the helper stays type-neutral
- Test: existing search formatting/output test file under `src/tests/tools/search/`

Steps:

1. Add failing test:
   - `test_fast_search_content_locations_groups_same_file_hits`
2. Assert one file path occurrence for multiple line hits in the same file.
3. Assert line numbers are retained.
4. Implement grouping.
5. Run:
   ```bash
   cargo nextest run --lib test_fast_search_content_locations_groups_same_file_hits
   ```

Acceptance:

- `return_format="locations"` no longer repeats one path per matching line for content search.
- Symbol/file locations formatting remains unchanged unless a shared helper is intentionally reused and covered by tests.

### Task 4: Dogfood the reported query and record evidence

Files:

- No production files expected.
- Update this plan's verification ledger section with actual command results during execution.

Steps:

1. Build debug binary:
   ```bash
   cargo build
   ```
2. Run the dogfood query through CLI:
   ```bash
   ./target/debug/julie-server search "workspace_is_primary edit_file" --target content --workspace . --file-pattern src/tests/core/handler_telemetry.rs --limit 100 --standalone --json
   ```
3. Record:
   - output byte count,
   - number of returned line hits,
   - number of repeated `src/tests/core/handler_telemetry.rs:` prefixes in rendered text,
   - whether import/setup-only lines caused only by `file` or `workspace` subtokens are gone.

Acceptance:

- Reported query output is compact and materially less noisy.
- Any remaining noisy hit has a concrete reason from matcher behavior, not repeated path formatting or broad compound subtoken matching.

## Verification

Worker-level exact tests only:

```bash
cargo nextest run --lib test_file_level_compound_identifier_matches_contiguous_identifier
cargo nextest run --lib test_file_level_compound_identifier_does_not_match_individual_subtokens
cargo nextest run --lib test_tokens_compound_required_identifier_does_not_match_individual_subtokens
cargo nextest run --lib test_tokens_compound_excluded_identifier_does_not_exclude_individual_subtokens
cargo nextest run --lib test_file_level_simple_terms_keep_tokenized_matching
cargo nextest run --lib test_fast_search_content_full_groups_same_file_hits
cargo nextest run --lib test_fast_search_content_locations_groups_same_file_hits
```

Lead regression gates:

```bash
cargo xtask test changed
cargo xtask test dogfood
cargo xtask test dev
```

Use `cargo xtask test dogfood` because this changes search matching/tokenization behavior visible to dogfood quality checks.

Formatting/lint:

```bash
cargo fmt
cargo clippy
```

## Verification Ledger

Use `docs/plans/verification-ledger-template.md` format when executing this plan.

| Scope | Command | Commit | Result | Notes |
|---|---|---|---|---|
| RED compound matching | `cargo nextest run --lib test_file_level_compound_identifier_does_not_match_individual_subtokens` and `cargo nextest run --lib test_tokens_compound_required_identifier_does_not_match_individual_subtokens` | current worktree | PASS | Both failed before implementation for the expected broad subtoken matches |
| GREEN compound matching | exact tests from Task 1 plus `cargo nextest run --lib line_match_strategy_tests` | current worktree | PASS | FileLevel and Tokens paths pass; 27 line-match strategy tests pass |
| GREEN full formatting | `cargo nextest run --lib test_fast_search_content_full_groups_same_file_hits` and `cargo nextest run --lib line_output_tests` | current worktree | PASS | Full output groups same-file line hits |
| GREEN locations formatting | `cargo nextest run --lib test_fast_search_content_locations_groups_same_file_hits` and `cargo nextest run --lib lean_format_tests` | current worktree | PASS | Locations output groups multi-hit files and preserves single-hit `file:line` compatibility |
| Existing line mode compatibility | `cargo nextest run --lib line_mode::search_line_mode_tests` | current worktree | PASS | 19 tests passed; nextest reported 1 leaky test marker, no functional failure |
| Existing content locations compatibility | `cargo nextest run --lib content_locations_format_omits_matching_line_text` | current worktree | PASS | Existing single-hit locations format remains `src/app.rs:2` |
| Reported query dogfood | `./target/debug/julie-server search "workspace_is_primary edit_file" --target content --workspace . --file-pattern src/tests/core/handler_telemetry.rs --limit 100 --standalone --json` | current worktree | PASS | 879 bytes, 9 output lines, 0 content matches, 0 repeated `path:` prefixes; strict matcher removes previous noisy subtoken hits |
| Changed regression | `cargo xtask test changed` | current worktree | PASS | 22 buckets passed in 607.5s |
| Dogfood search regression | `cargo xtask test dogfood` | current worktree | PASS | 2 buckets passed in 234.2s |
| Dev regression | `cargo xtask test dev --timeout-multiplier 3` | current worktree | PASS | 22 buckets passed in 626.7s; plain `dev` hit load-related bucket timeouts, timed-out buckets passed standalone |
| Formatting | `cargo fmt` | current worktree | PASS | No output |
| Lint | `cargo clippy` | current worktree | PASS | Exit 0 with existing warning set; final output reported 200 warnings |

## Model Routing

Use RAZORBACK model routing:

- Plan execution lead: `gpt-5.5`, reasoning high or medium.
- Focused implementation worker: `gpt-5.4-mini`, reasoning xhigh.
- Gate review: `gpt-5.3-codex`, reasoning high.
- Escalation if search behavior or output compatibility becomes ambiguous: `gpt-5.5`, reasoning high or xhigh.

## Escalation Triggers

Escalate before expanding scope if any of these happen:

- Fix requires changing Tantivy query construction or index schema.
- Existing search quality tests assert broad compound-subtoken matching as intentional behavior.
- Grouped output breaks consumers that parse `path:line:text` in `return_format="full"`.
- CLI command shape differs from the documented `julie-server search` interface.
- `cargo xtask test dogfood` reports search quality regressions not explained by stricter line filtering.

## Done Criteria

- `fast_search` content output groups same-file full results.
- `fast_search` content locations output groups same-file line numbers.
- Compound code query terms no longer match arbitrary individual subtokens in `FileLevel` or `Tokens` line filtering.
- Exact tests pass.
- `changed`, `dogfood`, and `dev` gates pass or failures are proved unrelated with evidence.
- Reported query has recorded before/after evidence.
- Phase 2 tool audit starts only after this plan is complete.
