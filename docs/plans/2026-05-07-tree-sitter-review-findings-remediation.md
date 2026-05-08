# Julie Tree-Sitter Findings Remediation Implementation Plan

> **For Hermes:** Use `subagent-driven-development` when implementation starts. Do not begin code changes until the plan gate is approved.

**Goal:** Fix the validated Julie tree-sitter review findings TS-RF-001 through TS-RF-008 in a sequence that protects shared identity contracts first, then language-family correctness, then report/capability debt.

**Architecture:** Treat this as a correctness program, not a rewrite. The first invariant is that spans, IDs, parent links, relationships, and type rows stay aligned after normalization/rekeying. After that gate, run isolated parser-family lanes with TDD so each regression proves one failure mode at a time.

**Tech Stack:** Rust, tree-sitter extractors, Julie database/indexing code, `cargo nextest`, `cargo xtask`, existing fixture/capability docs, Kanban routing.

---

## Inputs We Trust

- `docs/TREE_SITTER_REVIEW_FINDINGS_STATUS.md`
- Live code paths validated against `main`:
  - `src/tools/workspace/indexing/resolver.rs`
  - `src/database/bulk_operations.rs`
  - `src/database/helpers.rs`
  - `crates/julie-extractors/src/base/types.rs`
  - `crates/julie-extractors/src/base/results_normalization.rs`
  - `crates/julie-extractors/src/html/scripts.rs`
  - `crates/julie-extractors/src/vue/identifiers.rs`
  - `crates/julie-extractors/src/vue/script_setup.rs`
  - `crates/julie-extractors/src/vue/style.rs`
  - `crates/julie-extractors/src/regex/mod.rs`
  - `crates/julie-extractors/src/regex/patterns.rs`
  - `crates/julie-extractors/src/sql/relationships.rs`
  - test surfaces listed below
- Evidence sources:
  - `docs/LANGUAGE_CERTIFICATION_REPORT.md`
  - `fixtures/extraction/capabilities.json`
  - `docs/plans/verification-ledger-template.md`

---

## Sequencing Decision

1. **Fix the shared identity/rekeying contract first** — TS-RF-001 and TS-RF-002.  
   These have the highest blast radius because they affect relationship IDs, remapped embedded spans, and any downstream row that keys off those IDs. If this layer is wrong, later evidence can lie.

2. **Then fix the Vue lane** — TS-RF-003 and TS-RF-004 together.  
   They share the same SFC extraction surface and the same test fixture family. Keep the code fix and the exact line-coverage assertion in one lane.

3. **Then handle the independent embedded span bugs** — TS-RF-005 and TS-RF-007 as separate workstreams.  
   Same owner profile is fine, but do not merge the regressions; HTML and regex fail for different reasons.

4. **Then close the SQL relationship gap** — TS-RF-006.  
   This is a real feature gap, but not the first thing to fix because it does not corrupt the shared identity contract.

5. **Last: the capability/reporting gap** — TS-RF-008.  
   This is review/report debt, not a parser bug. It should only be closed once the executable evidence exists.

---

## Kanban Map

- `t_a60f28db` — planning gate / sequencing (`fred-johnson`)
- `t_ab5cbe7d` — Task 1: TS-RF-001/002 (`sakai`)
- `t_1998f4c5` — Task 2: TS-RF-003/004 (`sakai`)
- `t_ed069b93` — Task 3: TS-RF-005 (`sam-rosenberg`)
- `t_62b607c6` — Task 4: TS-RF-006 (`sakai`)
- `t_6b3dcadf` — Task 5: TS-RF-007 (`sam-rosenberg`)
- `t_676924d0` — Task 6: TS-RF-008 (`bull-de-baca`)

---

## Workstreams

### Task 1: Shared relationship identity and embedded rekeying
**Owner:** `sakai`

**Covers:** TS-RF-001, TS-RF-002

**Files**
- Modify `src/tools/workspace/indexing/resolver.rs`
- Modify `src/database/bulk_operations.rs`
- Modify `src/database/helpers.rs`
- Modify `crates/julie-extractors/src/base/results_normalization.rs`
- Modify `crates/julie-extractors/src/base/types.rs` only if the shared span/ID contract needs a helper
- Modify `crates/julie-extractors/src/html/scripts.rs`
- Modify `crates/julie-extractors/src/vue/style.rs`
- Test `src/tests/tools/workspace/resolver.rs`
- Test `crates/julie-extractors/src/tests/html/script_style.rs`
- Test `crates/julie-extractors/src/tests/vue/mod.rs`
- Test `crates/julie-extractors/src/tests/path_identity.rs`

**Test-first acceptance criteria**
- Add a regression in `src/tests/tools/workspace/resolver.rs` proving two same-caller / same-callee pending calls on different lines do **not** collapse into the same resolved relationship ID.
- Add or tighten a regression in `crates/julie-extractors/src/tests/path_identity.rs` proving rekeying preserves distinct same-start symbols and keeps type rows aligned with the new symbol IDs.
- Prove the embedded remap path refreshes IDs after applying host offsets so HTML script/style and Vue style symbols do not leave stale IDs behind.
- If the embedded remap path still leaves stale IDs, the failing test must prove that the remapped symbol, its parent reference, its identifiers, its relationships, and its type row are all updated together before the results leave normalization.

**Verification commands**
- `cargo nextest run --lib test_build_resolved_relationship`
- `cargo nextest run --lib test_rekey_normalized_locations_preserves_distinct_same_start_symbols_and_type_rows`
- `cargo nextest run --lib test_symbol_ids_do_not_collide_for_same_row_column_different_spans`
- `cargo nextest run -p julie-extractors --lib test_html_script_and_style_ranges_delegate_to_js_and_css_extractors`
- `cargo nextest run -p julie-extractors --lib test_vue_style_delegates_to_css_extractor_with_offsets`

**Acceptance**
- Relationship IDs include enough callsite identity to survive same-caller/same-callee collisions.
- Rekeying after normalization leaves no stale IDs in symbols, identifiers, relationships, or type rows.
- Embedded HTML/Vue style remaps do not keep old IDs once the host offset is applied.

---

### Task 2: Vue SFC identifier offsets and exact `<script setup>` line coverage
**Owner:** `sakai`

**Covers:** TS-RF-003, TS-RF-004

**Files**
- Modify `crates/julie-extractors/src/vue/identifiers.rs`
- Modify `crates/julie-extractors/src/vue/script_setup.rs`
- Test `crates/julie-extractors/src/tests/vue/script_setup.rs`

**Test-first acceptance criteria**
- Add a regression that proves Vue identifier extraction carries section offsets through byte spans, not just line offsets.
- Tighten the `<script setup>` regression so it asserts exact file-relative line numbers for at least:
  - one variable,
  - one import,
  - one function declaration,
  - one macro or framework-specific symbol from the existing fixture family.
- If the exact line assertion fails, fix the offset math in `vue/script_setup.rs` rather than weakening the test.

**Verification commands**
- `cargo nextest run -p julie-extractors --lib test_script_setup_line_numbers_are_file_relative`
- `cargo nextest run -p julie-extractors --lib test_script_setup_ref_and_computed`
- `cargo nextest run -p julie-extractors --lib test_script_setup_function_declarations`
- `cargo nextest run -p julie-extractors --lib test_script_setup_imports`

**Acceptance**
- Vue SFC identifiers are extracted once per file, with corrected byte/line offsets and stable containing-symbol IDs.
- The `<script setup>` tests prove the line math is exact, not merely “close enough.”

---

### Task 3: HTML embedded body offset detection
**Owner:** `sam-rosenberg`

**Covers:** TS-RF-005

**Files**
- Modify `crates/julie-extractors/src/html/scripts.rs`
- Test `crates/julie-extractors/src/tests/html/script_style.rs`
- Optional helper coverage: `crates/julie-extractors/src/tests/embedded_spans.rs`

**Test-first acceptance criteria**
- Add a regression where the script/style body text also appears in an attribute or sibling text, and prove the extractor still chooses the real embedded body range.
- The implementation must stop relying on `node_text.find(content)` for the offset decision.
- The test must verify both byte range and line/column mapping after the fix.

**Verification commands**
- `cargo nextest run -p julie-extractors --lib test_html_script_and_style_ranges_delegate_to_js_and_css_extractors`
- Run the new duplicate-text regression by name once it is added.

**Acceptance**
- HTML embedded offsets come from tree-aware bounds, not substring coincidence.
- The regression proves duplicated text cannot bind the wrong offset.

---

### Task 4: SQL view-source and trigger-target relationships
**Owner:** `sakai`

**Covers:** TS-RF-006

**Files**
- Modify `crates/julie-extractors/src/sql/relationships.rs`
- Test `crates/julie-extractors/src/tests/sql/relationships.rs`
- Update `fixtures/extraction/capabilities.json`
- Update `docs/LANGUAGE_CERTIFICATION_REPORT.md`

**Test-first acceptance criteria**
- Add failing coverage for the missing view-to-table and trigger-to-table edges.
- Keep the old FK/JOIN behavior intact while adding the new relationships.
- Update the capability fixture and certification report only after the executable evidence exists.

**Verification commands**
- `cargo nextest run -p julie-extractors --lib test_sql_pending_relationships_do_not_use_dead_synthetic_ids`
- `cargo nextest run -p julie-extractors --lib test_sql_view_and_trigger_relationships_target_real_tables`

**Acceptance**
- SQL emits the missing view/trigger relationship edges.
- Capability reporting matches the extractor behavior and does not pretend unsupported paths are already fixed.

---

### Task 5: Regex fallback symbol spans
**Owner:** `sam-rosenberg`

**Covers:** TS-RF-007

**Files**
- Modify `crates/julie-extractors/src/regex/mod.rs`
- Modify `crates/julie-extractors/src/regex/patterns.rs`
- Test `crates/julie-extractors/src/tests/regex/mod.rs`
- Test `crates/julie-extractors/src/tests/regex/task15.rs`
- Test `crates/julie-extractors/src/tests/regex/flags.rs` if the fallback path touches those fixtures

**Test-first acceptance criteria**
- Add a regression proving fallback symbols are built from the actual text-match range, not the root parser node.
- Assert exact byte and line spans for at least one lookaround/unicode-property fallback case.
- Do not weaken the test to accept a whole-pattern/root-node span.

**Verification commands**
- `cargo nextest run -p julie-extractors --lib test_lookarounds_still_extracted`
- `cargo nextest run -p julie-extractors --lib test_extract_lookahead_with_doc_comment`
- `cargo nextest run -p julie-extractors --lib test_regex_constructs_have_distinct_symbol_kinds`

**Acceptance**
- Regex fallback symbols no longer inherit root-node spans.
- The exact-span assertions prove the fallback path is location-accurate.

---

### Task 6: Capability matrix and certification alignment
**Owner:** `bull-de-baca`

**Covers:** TS-RF-008

**Files**
- Modify `crates/julie-extractors/src/tests/capability_matrix.rs`
- Update `fixtures/extraction/capabilities.json`
- Update `docs/LANGUAGE_CERTIFICATION_REPORT.md`

**Test-first acceptance criteria**
- Add or tighten a test that fails if target behavior is marked as supported without executable evidence.
- Keep gap rows honest: if behavior is still missing, the report must say so plainly.
- If a prior task closes the gap, update the fixture/report so the gap disappears for real.

**Verification commands**
- `cargo nextest run -p julie-extractors --lib capability_matrix_requires_target_capabilities`
- `cargo nextest run -p julie-extractors --lib capability_matrix_records_known_gaps_for_languages_with_unfixed_findings`

**Acceptance**
- The capability matrix reports reality, not aspiration.
- Any remaining gaps are documented debt with evidence, not accidental false support.

---

## Grouping Rules

- **Group together:** TS-RF-001 + TS-RF-002, TS-RF-003 + TS-RF-004.
- **Keep separate:** TS-RF-005, TS-RF-006, TS-RF-007, TS-RF-008.
- **Do not merge HTML and regex fixes** just because the same owner can do them.
- **Do not move certification/report work ahead of executable proof.**
- **Expect file overlap:** Task 3 reuses `crates/julie-extractors/src/html/scripts.rs`, so it should run after the shared identity/rekey work settles.

---

## Delivery Gates

- Every task starts with a failing regression.
- Every task ends with targeted tests passing.
- After each task, run the smallest relevant `cargo xtask test changed` bucket before moving on.
- Final handoff only after the relevant focused gates are green and the verification ledger is populated.

---

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| TS-RF-001 callsite-aware resolved relationship IDs | `cargo nextest run --lib test_build_resolved_relationship` | resolver relationship ID builder | `72138dc0 + working tree` | Passed, 2 tests | 2026-05-08T03:08:59Z | No |
| TS-RF-001 batch resolution preserves distinct same-caller callsites | `cargo nextest run --lib test_resolve_batch_keeps_same_caller_target_calls_distinct_by_line` | resolver batch ID collision regression | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-002 normalized location rekeying preserves type rows | `cargo nextest run -p julie-extractors --lib test_rekey_normalized_locations_preserves_distinct_same_start_symbols_and_type_rows` | extractor normalization rekeying | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-002 span-derived IDs include full span identity | `cargo nextest run -p julie-extractors --lib test_symbol_ids_do_not_collide_for_same_row_column_different_spans` | extractor span identity | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-002 HTML embedded script rekeys child parent IDs | `cargo nextest run -p julie-extractors --lib test_html_inline_script_offset_rekeys_child_parent_ids` | HTML embedded symbol rekeying | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-003 and TS-RF-004 Vue script setup offset coverage | `cargo nextest run -p julie-extractors --lib tests::vue::script_setup` | Vue script setup identifiers and symbols | `72138dc0 + working tree` | Passed, 14 tests | 2026-05-08T03:08:59Z | No |
| TS-RF-002 Vue style embedded CSS IDs stay host-relative | `cargo nextest run -p julie-extractors --lib test_vue_style_delegates_to_css_extractor_with_offsets` | Vue style embedded CSS offsets | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-005 HTML embedded body offset ignores attribute collisions | `cargo nextest run -p julie-extractors --lib test_html_inline_script_and_style_offsets_ignore_attribute_collisions` | HTML embedded body offset collision regression | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-007 regex fallback symbols use exact spans | `cargo nextest run -p julie-extractors --lib test_regex_constructs_have_distinct_symbol_kinds` | Regex fallback span regression | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T03:08:59Z | No |
| TS-RF-006 SQL emits view-source and trigger-target edges | `cargo nextest run -p julie-extractors --lib tests::sql::relationships` | SQL relationship regression module | `72138dc0 + working tree` | Passed, 3 tests | 2026-05-08T04:27:17Z | No |
| TS-RF-008 stale SQL relationships gap cannot survive executable evidence | `cargo nextest run -p julie-extractors --lib capability_matrix_sql_relationship_gap_closes_with_view_and_trigger_evidence` | SQL capability gap regression | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T04:27:17Z | No |
| TS-RF-006 and TS-RF-008 SQL golden evidence matches canonical extraction | `cargo nextest run -p julie-extractors --lib golden_fixtures_match_canonical_extraction` | SQL golden relationship evidence | `72138dc0 + working tree` | Passed, 1 test | 2026-05-08T04:27:17Z | No |
| TS-RF-006 and TS-RF-008 certification report reflects closed SQL relationship gap | `cargo xtask certify tree-sitter --out docs/LANGUAGE_CERTIFICATION_REPORT.md` | tree-sitter certification report refresh | `72138dc0 + working tree` | Passed, report regenerated | 2026-05-08T04:27:17Z | No |
| Changed-file regression gate | `cargo xtask test changed` | project changed-file gate | `72138dc0 + working tree` | Passed, 22 buckets | 2026-05-08T03:35:24Z | No |
| Extractor golden, capability, and certification gate | `cargo xtask test bucket extractors` | extractor bucket | `72138dc0 + working tree` | Passed, 1 bucket | 2026-05-08T03:24:41Z | No |
| Final extractor golden, capability, and certification gate | `cargo xtask test bucket extractors` | extractor bucket | `72138dc0 + working tree` | Passed, 1 bucket in 6.4s | 2026-05-08T04:27:17Z | No |
| Final changed-file regression gate | `cargo xtask test changed` | project changed-file gate | `72138dc0 + working tree` | Passed, 22 buckets in 592.4s | 2026-05-08T04:27:17Z | No |
