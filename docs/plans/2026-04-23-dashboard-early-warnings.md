# Dashboard Early Warnings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Build an annotation-derived early warning report and display it on the Julie dashboard without reviving the old security-risk scoring system.

**Architecture:** The report layer reads stored `AnnotationMarker` rows, classifies them through defaulted language config sections, and emits observable signals with evidence. The dashboard renders those signals per workspace through a `/signals/{workspace_id}` route and htmx partials. No score, vulnerability label, exploit claim, or sink guess should appear in this surface.

**Tech Stack:** Rust, SQLite, serde, Tera, Axum, htmx, existing Julie annotation storage and dashboard patterns

---

## Scope

This plan covers the report service needed by the dashboard and the dashboard page that presents it. It does not add a CLI command. The old `security_risk` module is fossil code from the shelved codehealth work; workers should remove it instead of rebranding it.

## File Structure

```text
src/
├── analysis/
│   ├── mod.rs                         # MODIFY: export early_warnings
│   └── early_warnings.rs              # CREATE: annotation-derived report model and generator
├── dashboard/
│   ├── mod.rs                         # MODIFY: register signals routes
│   └── routes/
│       ├── mod.rs                     # MODIFY: expose signals module
│       ├── intelligence.rs            # MODIFY: share workspace DB helper
│       └── signals.rs                 # CREATE: signals page, summary partial, refresh handler
├── database/
│   ├── migrations.rs                  # MODIFY: migration 021 for cached reports
│   └── schema.rs                      # MODIFY: fresh schema creates cached report table
├── search/
│   └── language_config.rs             # MODIFY: annotation classes and warning marker config
└── tools/
    └── metrics/
        └── query.rs                   # MODIFY: remove old security_risk fields and wording

dashboard/templates/
├── signals.html                       # CREATE: main workspace signals page
└── partials/
    ├── signals_summary.html           # CREATE: lazy report summary
    ├── signals_entry_points.html      # CREATE: observed entry point markers
    ├── signals_auth_gaps.html         # CREATE: no-auth-marker-observed candidates
    └── signals_review_markers.html    # CREATE: explicit review marker hits

src/tests/
├── analysis/
│   ├── mod.rs                         # MODIFY: wire early_warning_report_tests
│   └── early_warning_report_tests.rs  # CREATE: report generation behavior
├── core/
│   └── early_warning_report_cache.rs  # CREATE: cache schema and invalidation tests
└── dashboard/
    ├── integration.rs                 # MODIFY: page route and empty state tests
    └── projects_actions.rs            # MODIFY: project detail Signals link
```

## Naming And Product Rules

- Use `Signals`, `Early Warnings`, `Entry Points`, `Auth Coverage`, and `Review Markers`.
- Do not use `Security Risk`, `vulnerability`, `exploit`, `critical`, or HIGH/MEDIUM/LOW labels.
- Every row must include evidence: symbol name, file path, line, language, and the exact annotation marker that caused the row.
- Auth-gap rows must say "No auth marker observed on this symbol or owner", not "unauthenticated".
- If global middleware or framework auth is not represented by annotations, the dashboard must say coverage is unknown, not missing.

## Task 1: Remove Fossil Security-Risk Surfaces

**Files:**
- Delete: `src/analysis/security_risk.rs`
- Delete: `src/tests/analysis/security_risk_tests.rs`
- Modify: `src/tools/metrics/query.rs:1-305`
- Modify: `src/database/identifiers.rs:152`
- Modify: `src/tests/daemon/database.rs:313-320`
- Modify: `src/tests/integration/daemon_lifecycle.rs:704-709`
- Modify: `src/tests/tools/get_context_formatting_tests.rs:851-887`
- Modify: `src/tests/tools/get_context_scoring_tests.rs:138-156`

**What to build:** Remove the dead scoring module and user-facing metadata fields that preserve the old false-alarm story. Keep change risk and test linkage behavior as-is.

**Approach:** Delete the orphan module and tests because `src/analysis/mod.rs` no longer exports them and `src/tests/analysis/mod.rs` no longer wires the tests. In `metrics/query.rs`, drop `security_risk_score`, `security_risk_label`, `security_risk` sorting, and the default security sort fallback; use `centrality` as the default sort. Replace fixture references that inject `security_risk` JSON with neutral metadata or `change_risk` where the test needs a metadata blob.

**Acceptance criteria:**
- [ ] `rg "security_risk|SecurityRisk|security risk" src` returns no matches.
- [ ] Metrics output no longer prints `Security:` rows.
- [ ] Query metrics rejects `sort_by=security_risk` or falls back to `centrality` with no JSON path query for `$.security_risk`.
- [ ] Existing metrics and daemon tests still assert the behavior they care about without inserting old security-risk metadata.
- [ ] Tests pass, committed.

## Task 2: Add Annotation Classification Config

**Files:**
- Modify: `src/search/language_config.rs:12-20`
- Modify: `languages/python.toml`
- Modify: `languages/typescript.toml`
- Modify: `languages/javascript.toml`
- Modify: `languages/java.toml`
- Modify: `languages/csharp.toml`
- Modify: `languages/kotlin.toml`
- Modify: `languages/rust.toml`
- Modify: `src/search/language_config.rs:205-225`

**What to build:** Add defaulted config sections that classify canonical annotation keys into dashboard-safe categories.

**Approach:** Add `AnnotationClassesConfig` with `entrypoint`, `auth`, `auth_bypass`, `middleware`, `scheduler`, `test`, and `fixture` vectors. Add `EarlyWarningConfig` with `review_markers` and a `schema_version` integer used in cache keys. Store keys in lowercase canonical form, matching `AnnotationMarker.annotation_key`. Populate only languages where current annotation extraction gives useful evidence. Leave missing sections empty by default.

**Acceptance criteria:**
- [ ] Existing language configs load when the new sections are absent.
- [ ] Config tests verify populated and absent sections.
- [ ] Python config classifies route decorators such as `app.route` and auth decorators such as `login_required`.
- [ ] Java, Kotlin, and C# configs classify route annotations, auth annotations, and explicit bypass markers such as `allowanonymous` or `permitall`.
- [ ] TypeScript config classifies common controller/route decorators and guard decorators without claiming framework coverage where marker names are ambiguous.
- [ ] Rust config stays conservative, with test markers and any route markers only when exact canonical keys are known.
- [ ] Tests pass, committed.

## Task 3: Build Annotation-Derived Early Warning Report

**Files:**
- Create: `src/analysis/early_warnings.rs`
- Modify: `src/analysis/mod.rs:7-11`
- Test: `src/tests/analysis/early_warning_report_tests.rs`
- Modify: `src/tests/analysis/mod.rs:1-2`

**What to build:** A serializable report that inventories annotation-derived entry points and highlights review candidates without scoring them.

**Approach:** Add `EarlyWarningReport`, `EntryPointSignal`, `AuthCoverageCandidate`, `ReviewMarkerSignal`, and `ReportSummary` structs. Implement `generate_early_warning_report(db, language_configs, options)` where options include `workspace_id`, `file_pattern`, `fresh`, and `limit_per_section`. The generator should call `db.get_all_symbols()`, build a symbol map by id, apply file-pattern filtering before classification, and classify symbols by comparing `symbol.annotations[*].annotation_key` against language config categories. Auth coverage should inspect the symbol plus its owner chain through `parent_id`, so controller-level auth covers method-level routes.

**Acceptance criteria:**
- [ ] Entry point rows come from `AnnotationMarker` matches against config `entrypoint` keys.
- [ ] Auth coverage candidates are entry points where neither the symbol nor its owner chain has an `auth` marker.
- [ ] Explicit review marker rows come from config `auth_bypass` and `review_markers`.
- [ ] Each row includes symbol name, kind, language, file path, start line, annotation display text, annotation key, and raw text when present.
- [ ] No numeric risk score is computed or serialized.
- [ ] If a workspace has no classified annotation markers, the report serializes an empty state with counts at zero.
- [ ] JSON roundtrip test passes for the full report.
- [ ] Report generation for a fixture with parent-level auth proves the child route is covered.
- [ ] Tests pass, committed.

## Task 4: Cache Reports On Canonical Revision And Config Version

**Files:**
- Modify: `src/database/schema.rs:9-33`
- Modify: `src/database/schema.rs:198-220`
- Modify: `src/database/migrations.rs:16`
- Modify: `src/database/migrations.rs:97-122`
- Modify: `src/database/migrations.rs:906-913`
- Modify: `src/analysis/early_warnings.rs`
- Test: `src/tests/core/early_warning_report_cache.rs`

**What to build:** Store generated reports so dashboard loads stay fast and refreshes line up with the index lifecycle.

**Approach:** Add `early_warning_reports` with `workspace_id`, `canonical_revision`, `projection_revision`, `config_schema_version`, `file_pattern`, `generated_at`, and serialized JSON. Add migration 021 and fresh-schema creation. Use `get_latest_canonical_revision()` and `get_projection_state("tantivy", workspace_id)` for the cache key. A refresh request bypasses cache and writes a new row. File pattern should be part of the key so scoped views do not collide with whole-workspace views.

**Acceptance criteria:**
- [ ] Fresh databases create `early_warning_reports`.
- [ ] Migration 021 creates the same table and indexes on existing databases.
- [ ] First report call writes cache; second equivalent call reads from cache.
- [ ] Changing canonical revision invalidates the cached report.
- [ ] Changing `EarlyWarningConfig.schema_version` invalidates the cached report.
- [ ] Refresh bypasses cache and updates `generated_at`.
- [ ] Tests pass, committed.

## Task 5: Add Dashboard Signals Page

**Files:**
- Create: `src/dashboard/routes/signals.rs`
- Modify: `src/dashboard/routes/mod.rs:3-11`
- Modify: `src/dashboard/mod.rs:123-177`
- Modify: `src/dashboard/routes/intelligence.rs:204-225`
- Create: `dashboard/templates/signals.html`
- Create: `dashboard/templates/partials/signals_summary.html`
- Create: `dashboard/templates/partials/signals_entry_points.html`
- Create: `dashboard/templates/partials/signals_auth_gaps.html`
- Create: `dashboard/templates/partials/signals_review_markers.html`
- Modify: `dashboard/templates/partials/project_detail.html:155`
- Test: `src/tests/dashboard/integration.rs:261-287`
- Test: `src/tests/dashboard/projects_actions.rs:360-398`

**What to build:** Add a per-workspace dashboard page that presents annotation-derived signals and refreshes the cached report on demand.

**Approach:** Add `GET /signals/{workspace_id}` for the page, `GET /signals/{workspace_id}/summary` for the lazy report partial, and `POST /signals/{workspace_id}/refresh` for a cache bypass. Share the workspace database helper used by the Intelligence page instead of duplicating daemon workspace validation. Link to Signals from the project detail panel beside Intelligence. Keep top nav unchanged unless the page gets a workspace picker.

**Acceptance criteria:**
- [ ] `/signals/{workspace_id}` returns 200 for an indexed workspace and 404 for an unknown workspace.
- [ ] Empty state says no classified annotation markers were found.
- [ ] Summary shows counts for observed entry points, auth coverage candidates, and review markers.
- [ ] Entry point table shows exact marker evidence and links symbol rows back to file paths and line numbers.
- [ ] Auth coverage table uses "No auth marker observed" language.
- [ ] Review marker table shows explicit bypass or review markers without risk labels.
- [ ] Refresh button posts with CSRF protection and swaps in a fresh summary.
- [ ] Project detail includes a Signals link for each ready workspace.
- [ ] Tests pass, committed.

## Task 6: Verification Gate

**Files:**
- No new files.

**What to build:** Run focused tests first, then the project batch gate.

**Approach:** Use TDD during implementation. For each task, write the narrow failing test first, run the exact test, implement, then rerun that exact test. Main session handles broader gates after workers finish.

**Acceptance criteria:**
- [ ] `cargo nextest run --lib early_warning_report` passes.
- [ ] `cargo nextest run --lib language_config` passes.
- [ ] `cargo nextest run --lib dashboard` passes for the new or touched dashboard tests.
- [ ] `cargo xtask test changed` passes.
- [ ] `cargo xtask test dev` passes once for the completed batch.
- [ ] `./target/debug/julie-server search "@app.route" --workspace . --standalone --json` still returns parseable JSON after the changes.
- [ ] Manual dashboard check confirms Signals renders for this workspace after `cargo build`.

## Review Notes For Implementers

- Use @razorback:test-driven-development for every code task.
- Use @razorback:systematic-debugging for any failing test or unexpected dashboard behavior.
- Use @razorback:verification-before-completion before claiming the batch is done.
- Use @razorback:requesting-code-review before final integration.
- Do not resurrect `security_risk` under a new name. If a row cannot cite annotation evidence or an exact configured marker, it does not belong on this dashboard.
