# Tree-Sitter Review Findings Status

Reviewed against `main` at `0bcc7c48` on 2026-05-07.

This file tracks concrete review findings that were raised during the v7.8.0 tree-sitter review. The point is to keep these out of chat-only memory. Generated certification reports show capability gaps, but they do not cover every storage, ID, span, or performance defect below.

## Current Findings

| ID | Severity | Finding | Current status | Evidence | Closure |
| --- | --- | --- | --- | --- | --- |
| TS-RF-001 | Critical | Resolved pending relationship IDs can collapse multiple callsites from the same caller to the same callee. | Open. `build_resolved_relationship` still derives the ID from `from_symbol_id`, `target.id`, and kind, while relationship storage treats the ID as the row key. | `src/tools/workspace/indexing/resolver.rs`, `src/database/schema.rs`, `src/database/bulk_operations.rs` | Include callsite identity, such as file path and line number, in resolved relationship IDs. Add a regression with two same-caller same-callee pending calls on different lines. |
| TS-RF-002 | High | Embedded HTML and Vue symbols get remapped spans but may keep stale span-derived IDs. | Open. `Symbol::apply_normalized_span` updates spans only. HTML script/style and Vue style remapping call it directly without rekeying IDs or dependent references. | `crates/julie-extractors/src/base/types.rs`, `crates/julie-extractors/src/html/scripts.rs`, `crates/julie-extractors/src/vue/style.rs` | After embedded offset remapping, refresh symbol IDs and update parent IDs, relationships, identifiers, and type rows that refer to the old IDs. |
| TS-RF-003 | High | Vue identifier extraction reparses the SFC per identifier and leaves byte ranges section-relative. | Open. `create_identifier_with_offset` calls `parse_vue_sfc` while creating each identifier, adjusts lines, but does not adjust bytes. `containing_symbol_id` is computed before the offset rewrite. | `crates/julie-extractors/src/vue/identifiers.rs` | Parse the Vue SFC once, carry section offsets through identifier extraction, adjust byte spans, then compute or rekey containing symbol IDs after offset correction. |
| TS-RF-004 | High | Vue `<script setup>` line positions need exact file-relative coverage. | Open validation gap. Current code uses `section.start_line + row` for variables, imports, and standalone macro calls. The existing regression only asserts the line is file-relative enough, not the exact source line. | `crates/julie-extractors/src/vue/script_setup.rs`, `crates/julie-extractors/src/tests/vue/script_setup.rs` | Add exact line assertions for variable, import, function, and macro symbols in a fixture where the correct source line is unambiguous. Fix offsets if the exact regression fails. |
| TS-RF-005 | High | HTML embedded body offset detection can bind to matching attribute text. | Open. `embedded_content_start_byte` uses `node_text.find(content)`, so a script/style body that also appears in an attribute can map to the wrong byte offset. | `crates/julie-extractors/src/html/scripts.rs` | Derive embedded content start from tree-sitter child/body ranges, or from tag delimiter bounds, not a substring search over the whole node text. Add a regression with duplicated attribute/body text. |
| TS-RF-006 | Medium | SQL view-source and trigger-target relationships are not implemented. | Open and recorded in certification. SQL currently handles FK and JOIN relationships, while select/from view dependencies and trigger target relationships remain absent. | `crates/julie-extractors/src/sql/relationships.rs`, `crates/julie-extractors/src/tests/sql/relationships.rs`, `fixtures/extraction/capabilities.json`, `docs/LANGUAGE_CERTIFICATION_REPORT.md` | Implement view-to-table and trigger-to-table relationships. Add golden fixture evidence and close the SQL `relationships` capability gap. |
| TS-RF-007 | Medium | Regex fallback symbols use the root node span. | Open. Fallback lookaround and unicode-property extraction passes the root node into symbol creation, so fallback symbols can inherit whole-pattern spans. | `crates/julie-extractors/src/regex/mod.rs`, `crates/julie-extractors/src/regex/patterns.rs` | Create fallback symbols from text match ranges, or synthesize normalized spans from byte offsets before creating symbols. Add exact span assertions. |
| TS-RF-008 | Medium | Capability gates can still represent target behavior as an open gap instead of executable evidence. | Partially mitigated. The capability matrix now requires pending evidence or an explicit gap row for target pending relationships, and the certification report surfaces those gaps. It still permits many target capabilities to remain open as documented debt. | `crates/julie-extractors/src/tests/capability_matrix.rs`, `fixtures/extraction/capabilities.json`, `docs/LANGUAGE_CERTIFICATION_REPORT.md` | Keep gap rows honest, but close them by adding golden fixture evidence or downgrading unsupported target capabilities. Treat gap rows as release debt, not proof of support. |

## Already Tracked Elsewhere

- SQL relationship and pending-relationship gaps appear in `docs/LANGUAGE_CERTIFICATION_REPORT.md`.
- HTML and regex pending-relationship exceptions appear in `docs/LANGUAGE_CERTIFICATION_REPORT.md`.
- Vue remediation history appears in `docs/findings/COMPILED-FINDINGS.md` and `docs/plans/2026-05-06-tree-sitter-extractor-audit-remediation.md`, but the exact Vue status rows above were not previously captured as current review findings.

## Verification Rule

Do not close a row here from memory. Close it only when the fix has:

- A focused regression test that fails before the fix and passes after it.
- The relevant golden or certification evidence updated when the finding affects reported extractor capability.
- A link to the commit or plan ledger entry that records the verification.
