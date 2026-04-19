# Autonomous Execution Report — call_path and rewrite_symbol hardening

**Status:** Complete
**Plan:** docs/plans/2026-04-19-call-path-and-rewrite-symbol-hardening.md
**Branch:** feature/call-path-rewrite-symbol-hardening
**PR:** (filled by Step 7 terminal pointer after gh pr create)
**Duration:** ~3h (dogfood → plan → approval → team dispatch → review → fix → push)
**Phases:** 1/1 complete (single team-driven execution phase)
**Tasks:** 8/8 complete (all plan tasks; plus 2 on-the-path fixes)

## What shipped

- **Task 1** (`rewrite_symbol`): `replace_signature` and `replace_body` now return explicit errors when the grammar's `body` field is missing (kills the silent full-symbol clobber for Rust trait methods, Java interface methods, and any declaration-without-body across all 34 languages). Field-listing in `replace_body` error gives the caller the node's actual tree-sitter field names. Commit `f0e84a39`.
- **Task 2** (`rewrite_symbol`): centralized no-op detection. If `modified_content == original_content`, return an informational "No changes" message instead of an empty diff. Applies to all 6 operations. Skips `EditingTransaction` begin on no-op. Commit `f0e84a39`.
- **Task 3** (`rewrite_symbol`): dry-run output now includes the replaced span's byte range, line range, and old content (with 15-head/5-tail elision for spans over 30 lines). Closes the "caller blind to the span" footgun that led to the original `replace_body`-without-braces corruption. Commit `f0e84a39`.
- **Task 4** (`rewrite_symbol`): per-operation docstring rewritten to spell out grammar dependence — brace-delimited vs indent-delimited, declarations-without-body error condition. Commit `f0e84a39`.
- **Task 5** (`call_path`): added `from_file_path` and `to_file_path` optional params. New `file_path_matches_suffix` helper in `resolution.rs` (pub fn, reusable). Replaces `.contains()` with segment-aware suffix matching. Commit `2f976f4b`.
- **Task 6** (`call_path`): `edge_label` and `relationship_priority` now exhaustive over the three BFS-traversed kinds (`Calls`, `Instantiates`, `Overrides`) with `unreachable!` for filtered-out variants. Removed dead `Extends | Implements` and `_ => "reference"` branches. Commit `2f976f4b`.
- **Task 7** (tests): 11 cross-language tests for `rewrite_symbol` across Python, Java, Ruby, Go, Rust. Includes a byte-for-byte file-unchanged regression test for the Rust trait method `replace_signature` case — the primary bug this hardening pass was designed to prevent. Commit `cd4ebcf3`.
- **Task 8** (tests): 6 active + 1 `#[ignore]`d tests for `call_path` disambiguation, covering all 5 plan acceptance cases plus the qualified-name pass case plus the trait-impl-limitation documentation case. Commits `f90db57e`, `4d3e707c`.
- **On-the-path fix** (xtask): added new `call_path_*` test modules to the `tools-misc` bucket in `test_tiers.toml` (they weren't in any bucket — would have silently missed regression). Bumped `tools-misc` timeout from 120s to 210s (14 sequential nextest invocations blew past old timeout). Updated matching manifest contract test. Commits `6e218b37`, `a7eb3c46`.
- **On-the-path fix** (dashboard): updated `test_projects_page_shows_workspace_controls_and_cleanup_log` and `test_projects_page_keeps_row_actions_compact` to match the Open→Refresh UI change from commit `5bb843ed` on main. Pre-existing regression on main that was blocking `cargo xtask test dev`. Commit `295894ab`.
- **Codex-review follow-up** (`rewrite_symbol`): replaced `.contains()` substring filter at `rewrite_symbol.rs:493` with `file_path_matches_suffix`, and bypassed `find_symbol`'s inner substring filter by passing `None`. Added two regression tests covering suffix-vs-substring and bogus-filter-not-found. Commit `6f86b1c7`.

## Judgment calls (non-blocking decisions made)

- `docs/plans/2026-04-19-call-path-and-rewrite-symbol-hardening.md:Task 3` — Chose visibility (show old span in dry-run) over validation (round-trip tree-sitter re-parse) because the original v1 design says "narrow tool, one job" and the validation was feature creep beyond what the plan was meant to cover. The silent-corruption class of bug is caught by letting the caller see the braces in the old content.
- `src/tools/navigation/resolution.rs:194` — Made `file_path_matches_suffix` `pub fn` rather than `pub(crate)` because it's a pure utility with no sensitive internals; max visibility for cross-module reuse.
- `src/tools/navigation/call_path.rs:resolve_unique_symbol` — Passed `None` to `find_symbol` (not the file_path filter) to apply a strict suffix filter afterward instead of `find_symbol`'s preferential-with-fallback semantics. Strict matching is correct for disambiguation; preferential was wrong semantics for `call_path`.
- `src/tools/editing/rewrite_symbol.rs:488` (Codex fix) — Same choice: passed `None` to `find_symbol` to bypass its internal `.contains()` filter, then applied `file_path_matches_suffix` as the only filter. Mirrors `call_path`'s pattern.
- `src/tests/tools/call_path_disambiguation_tests.rs:test_trait_impl_qualified_name_limitation` — Marked `#[ignore]` rather than flipping the assertion to pass. Reason: the expected long-term state is that trait-impl qualified names resolve correctly once the extractor is fixed; `#[ignore]` preserves that intent clearly and lets the test auto-pass when the underlying fix lands.
- `xtask/test_tiers.toml:tools-misc.timeout_seconds` — Bumped to 210s (expected 60s) rather than splitting the bucket. Splitting would restructure the xtask config in ways unrelated to this plan; a timeout bump is the minimal change.
- `src/tests/dashboard/projects_actions.rs:140-147` — Fixed two failing dashboard tests on-the-path even though they were pre-existing on main. Reason: they were blocking `cargo xtask test dev` from going green, which blocks the pre-merge review gate. Small, localized, obvious (invert Open→Refresh in assertions).

## External review (codex, adversarial)

- **Findings:** 2 (both high severity)
- **Verified real, fixed:** 1 (commit: `6f86b1c7`)
  - `rewrite_symbol` substring-match file filter could mutate wrong file — fixed by using `file_path_matches_suffix` (the helper we already shipped for `call_path`) and bypassing `find_symbol`'s inner substring filter. Two regression tests added covering suffix-vs-substring and bogus-filter-not-found cases.
- **Dismissed:** 1
  - `rewrite_symbol` TOCTOU race between file read (line 557) and commit-via-rename — dismissed as **out-of-scope**. Reason: the plan scope was grammar-dependent silent clobbers and per-endpoint disambiguation UX, not concurrent-write safety. Race pre-dates this plan, affects all editing paths (not just `rewrite_symbol`), and fixing properly needs a separate design decision (digest vs mtime vs hash-of-snapshot; user-facing stale-file error semantics). Filed as follow-up.
- **Flagged for your review:** 0

(codex-cli does not surface per-request token counts in its JSON output; cost line omitted by design.)

## Tests

All 10 xtask dev-tier buckets pass (265.6s total): cli, core-database, core-embeddings, tools-get-context, tools-search, tools-workspace, tools-misc, core-fast, daemon, dashboard. Narrow tests for this plan's work: 95 passing (feature work) + 2 passing (Codex-fix regression tests). No failures, no flakes.

## Blockers hit

None. Two issues surfaced during execution were resolved in-band:

1. Brief compile blocker when `rewrite-symbol-impl` ran tests before `call-path-impl`'s `pub(crate) fn edge_label` commit landed — self-resolved within the same commit batch.
2. `cargo xtask test dev` failed on pre-existing dashboard tests on main (Open→Refresh UI drift); fixed on-the-path with a two-line test update.

## Files changed

```
 .memories/2026-04-19/155211_c0b2.md                |  59 +++
 docs/plans/2026-04-19-call-path-and-rewrite-symbol-hardening.md | 284 +++++
 src/tests/dashboard/projects_actions.rs            |  13 +-
 src/tests/mod.rs                                   |   5 +-
 src/tests/tools/call_path_disambiguation_tests.rs  | 391 ++++++
 src/tests/tools/call_path_tests.rs                 |  20 +-
 src/tests/tools/editing/mod.rs                     |   1 +
 src/tests/tools/editing/rewrite_symbol_cross_language_tests.rs | 421 ++++++
 src/tests/tools/editing/rewrite_symbol_tests.rs    | 325 ++++
 src/tools/editing/rewrite_symbol.rs                | 217 ++-
 src/tools/navigation/call_path.rs                  |  55 ++-
 src/tools/navigation/resolution.rs                 |   8 +
 xtask/test_tiers.toml                              |   6 +-
 xtask/tests/manifest_contract_tests.rs             |   6 +-
 14 files changed, 1744 insertions(+), 67 deletions(-)
```

## Next steps

- Review the PR (link in terminal pointer)
- **Follow-up: trait-impl parent_id extractor work.** The `#[ignore]`d test `test_trait_impl_qualified_name_limitation` documents that `Receiver::method` qualified names fail for methods defined in `impl Trait for Struct`. Fix is at the extractor level and needs to be done per-language for all 34 grammars (or at a shared layer that normalizes parent_id across trait impls).
- **Follow-up: TOCTOU concurrent-write safety for `rewrite_symbol` (and `edit_file`).** Codex Finding #2. Needs a separate plan with design decisions: what's the expected digest (hash of snapshot vs mtime), what's the user-facing "stale file" error, does `EditingTransaction` take the check or does the caller?
- **Follow-up: `rewrite_symbol.rs` file size.** 680 lines vs the 500-line CLAUDE.md limit. This hardening pass added 142 lines on top of an already-violating baseline. Refactor opportunistically on next touch — likely extract `span_for_operation` and helpers into a sibling module.
