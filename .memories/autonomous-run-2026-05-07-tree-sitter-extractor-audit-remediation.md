# Autonomous Execution Report - Tree-sitter Extractor Audit Remediation

**Status:** Complete
**Plan:** `docs/plans/2026-05-06-tree-sitter-extractor-audit-remediation.md`
**Branch:** `codex/tree-sitter-extractor-audit-remediation`
**PR:** https://github.com/anortham/julie/pull/18
**Duration:** Multiple sessions, 2026-05-06 to 2026-05-07
**Phases:** 17/17 complete
**Tasks:** 17/17 complete

## What shipped
- Added the compiled extractor audit findings and execution plan, then completed all 17 remediation tasks.
- Tightened SQL, JSON, TOML, YAML, Markdown, and data-format extraction contracts with exact tests and refreshed goldens.
- Strengthened identity, parent, `TypeInfo`, visibility, doc-comment, and multi-declaration behavior across core extractor infrastructure.
- Expanded relationship precision for C, C++, TypeScript, JavaScript, Ruby, PHP, Dart, Swift, Scala, Elixir, Go, Rust, Zig, QML, Regex, and scripting languages.
- Improved Vue, HTML, CSS, and Razor embedded extraction with file-relative span tests and shared offset projection where it matched the parser model.
- Hardened capability-matrix, core kind parsing, database row mapping, and weak tests so bad enum or relationship data fails loudly instead of degrading silently.
- Completed Task 17 by proving degraded parser results preserve file identity, zero-symbol file rows, and parse diagnostics through workspace indexing storage.

## Judgment calls
- `crates/julie-extractors/src/base/types.rs` - Kept enum expansion limited to variants with real extractor output in the same task wave, because adding aspirational variants would make the capability matrix less honest.
- `crates/julie-extractors/src/razor/relationships.rs` - Left Razor out of the shared embedded span helper because Razor already works from file-relative tree nodes, while the helper is for offsetting embedded child parses.
- `crates/julie-extractors/src/manager.rs` - Kept `ExtractorManager::extract_all` as a direct canonical-pipeline delegate for Task 17 because the degraded `ExtractionResults` contract already satisfied callers.
- `src/tools/workspace/indexing/processor.rs` - Added a crate-private processor injection seam for Task 17 tests instead of trying to coerce real tree-sitter into returning `None` with huge files or timeouts.
- `fixtures/extraction/capabilities.json` - Recorded known pending-relationship evidence gaps rather than pretending every language claim was already backed by golden output.

## External review
External review: none (not requested for this run).

## Tests
- Final branch gate at `430ee1ee`: `cargo xtask test dev` passed, 22 buckets in 345.2s.
- Task 17 focused checks passed: degraded parse file identity, extractor parse-`None` diagnostic, existing workspace parse diagnostics, and indexing pipeline blast-radius tests.
- Task 17 affected-change gate passed: `cargo xtask test changed`, tools-workspace and workspace-init buckets in 39.0s.
- Earlier task ledger rows record exact extractor tests, golden refreshes, specialist buckets, changed gates, and dev gates for Tasks 10 through 16.

## Blockers hit
- None.

## Files changed
- 311 files changed from `main` merge base `cd903f1375122061690806d0b5789b9db9d79fa7` to `430ee1ee`.
- 23,237 insertions and 2,779 deletions.
- Major areas: `crates/julie-extractors/src/**`, `crates/julie-extractors/src/tests/**`, `fixtures/extraction/**`, `docs/findings/**`, `docs/plans/**`, `.memories/**`, and targeted Julie database/indexing callers.

## Next steps
- Review PR: https://github.com/anortham/julie/pull/18
- Pay special attention to broad extractor fixture churn and the capability-gap policy, since those are the places a reviewer can most usefully sanity-check intent.
