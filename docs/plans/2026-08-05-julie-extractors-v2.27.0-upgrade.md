# Plan: Upgrade Julie's extractor consumer to julie-extractors v2.27.0

**Date:** 2026-08-05
**Current pin:** `v2.16.0` (2026-07-18)
**Target pin:** `v2.27.0` (2026-08-05)
**Releases spanned:** 11 (v2.17.0 … v2.27.0)
**Precedent:** the v2.14.0 → v2.16.0 consumer upgrade (`.memories/2026-07-19/005152_673c.md`)

---

## 0. Decision required before any work starts

Julie's HEAD commit (`8593ed71`) formally **retires** the project:

> Julie is retired as of 2026-07-28. **v7.17.0 is the final release.** No further development, bug fixes, or
> releases will ship — including the open items in TODO.md, which are wontfix by policy.
> — `README.md:7-9`

The same announcement states the intent for this exact divergence:

> Extraction development continues upstream in julie-extractors, which Miller consumes at a newer
> version than Julie's final pin.
> — `README.md:17-19`

**This plan is written as requested and is technically complete, but executing it contradicts a
published policy.** The owner must pick one:

| Option | Meaning | Consequence |
|---|---|---|
| **A. Un-retire** | Julie accepts maintenance releases again | README/TODO retirement text must be rewritten; a v7.18.0 release follows |
| **B. One-off maintenance drop** | Ship the upgrade, keep the retirement | README needs an explicit carve-out; "final release" wording becomes false |
| **C. Do not execute** | Keep v2.16.0 as the final pin | This plan is archived as the record of what the upgrade would cost |

Everything below assumes A or B.

> Miller is **not** affected either way: it consumes the `julie-extract` **CLI binary + SQLite artifact**
> (.NET, pinned 2.20.0), not the Rust library. Julie is the only library-level consumer of the
> `julie-extractors` crate, so this API break is Julie's alone.

---

## 1. Evidence base

All findings below were produced by re-pinning in an isolated worktree and compiling, not by inference.

| Evidence | Method | Result |
|---|---|---|
| Compile break surface | `cargo check --workspace --all-targets` at v2.27.0 | 4 distinct breaks, 39 sites, 21 files |
| Clean-compile proof | Applied the migration, re-checked | **Zero errors, zero warnings** |
| Extraction output change | Built `julie-server`, indexed a C# fixture | 6 symbols where v2.16.0 emitted 3 |
| Contract version | `git show <tag>:…/lib.rs` at all 13 tags | **Byte-identical throughout** |
| Language count | Diff of `language_spec/specs.rs` | 36 → 38 specs = **34 → 36 user-facing** (`erlang`, `xml`) |

### 1.1 A trap: the naive re-pin

Changing only the root `Cargo.toml` puts **two** `julie-extractors` versions in the graph and produces
**335 confusing errors** (`expected ExtractionResults, found julie_extractors::ExtractionResults`).
The pin is duplicated across **six** manifests. Bump all six together; the real error count is 39.

---

## 2. What actually breaks (compile-verified)

| # | Break | Sites | Fix |
|---|---|---|---|
| 1 | `Relationship` gained `span: Option<NormalizedSpan>` (v2.17.0) and `reference_site_is_exact: bool` (v2.18.0) | **35** | add both fields to each struct literal |
| 2 | `StructuredPendingRelationship` gained `reference_site_is_exact: bool` | **1** | add the field |
| 3 | `StructuredPendingRelationship::with_span` split into `with_context_span` / `with_target_span` | **2** | use `with_context_span` (behaviour-preserving: sets span, leaves `exact = false`) |
| 4 | `ParseDiagnostic` gained `message: Option<String>` (v2.23.0) | **1** | add `message: None` |

The 35 `Relationship` sites span 19 files; the whole migration touches **21 source files** plus the
6 manifests and `Cargo.lock`.

**True production files — only 4:**
`julie-core/src/database/helpers.rs`, `julie-pipeline/src/resolver.rs`,
`julie-tools/src/navigation/fast_refs.rs`, `julie-tools/src/navigation/target_workspace.rs`.

Everything else is test or test-support code (`julie-core/src/test_support/db/rows.rs` ships in the
lib but is fixture-building). **The blast radius on shipped behaviour is very small** — the upgrade is
mostly a test-fixture migration.

### 2.1 What does *not* break — verified, not assumed

- The `ExtractFn` + `ExtractionLevel` signature change **does not reach Julie**; Julie calls through
  `ExtractorManager`, never the raw function pointer.
- `ParseDiagnosticKind::DepthTruncated` was added, but Julie never matches the enum — diagnostics are
  stored as serde JSON, and `message` is `#[serde(default)]`, so **old rows deserialize fine**.
- No variants added to `SymbolKind`, `RelationshipKind`, `Visibility`, or `SourceRegionKind`.
- `ExtractorManager`, the `language::*` API, and the type models are unchanged.
- Julie does **not** link `julie-extract-cli` or `julie-extract-artifact` (confirmed absent from `Cargo.lock`).
  Everything in the v2.19–v2.20 static-type resolution work is CLI-only and invisible to Julie.

---

## 3. The load-bearing hazard: a silent stale index

**This is the most important item in this plan, and it is invisible at compile time.**

`EXTRACTION_CONTRACT_VERSION` is **byte-identical at all 13 tags** from v2.16.0 to v2.27.0. Julie's
reindex trigger is built on that string:

```
SEMANTIC_INDEX_ENGINE_VERSION (hand-written literal, embeds the contract string verbatim)
  → semantic_index_engine_refresh_needed()          [indexing/index.rs:365-376]
  → effective_force_reindex                         [indexing/index.rs:133]
```

Re-pinning alone leaves that literal unchanged, so `semantic_index_engine_refresh_needed()` returns
`false`. The only other staleness test is a per-file content hash, which does not fire for unmodified
files. **Every already-indexed file keeps its v2.16.0-shaped rows permanently.**

The guard test built for exactly this purpose gives a **false green**: it asserts only that
`SEMANTIC_INDEX_ENGINE_VERSION.contains(EXTRACTION_CONTRACT_VERSION)`, which stays true.

### 3.1 Proof that the output really did change

A C# file indexed with the v2.27.0 binary, from `.julie/…/symbols.db`:

```
namespace | App.Core  | (none)                            ← new at v2.19.0
class     | Fixture   | f62ed2a7…  (parented to namespace) ← parent_symbol_id CHANGED
method    | Create    | 47820f8a…
method    | Compute   | 47820f8a…
variable  | requested | 66af213b…                          ← new at v2.20.0 (parameter)
variable  | scale     | 66af213b…                          ← new at v2.20.0 (local)
```

v2.16.0 produced three rows (class + two methods) with the class unparented and no namespace symbol.

The same shift is visible in the extractor's own C# fixtures (`fixtures/extraction/csharp/**/expected.json`),
which is the cleanest available measure of the new symbol classes:

| | v2.16.0 | v2.27.0 |
|---|---|---|
| total symbols | 356 | 388 |
| `variable` | **0** | **29** |
| `namespace` | **0** | **3** |

The +32 delta is exactly the new variables plus namespaces. These fixtures are small and curated —
treat the *kinds* as proven and the *ratio* as unrepresentative. Real growth scales with local-variable
density, so measure on a real C# repo during Task 8 before tuning Task 7.

### 3.2 Required remedy

Bump `SEMANTIC_INDEX_ENGINE_VERSION` **by hand** in
`src/tools/workspace/indexing/engine_version.rs`, appending a tag-tied marker, e.g.
`…+consumer-enrichments-v1+extractors-tag=v2.27.0`. Then strengthen the guard test so the false green
cannot recur: assert the literal also names the pinned tag, so a future re-pin without a version bump
fails RED.

---

## 4. Behavioural changes that need a deliberate decision

These compile and run, but they change what users see. None is a blocker; all are real.

| Change | Effect on Julie | Recommended response |
|---|---|---|
| **C# locals + parameters become `SymbolKind::Variable`** (v2.20.0) | New symbols scale with local-variable density. Julie has no kind- or role-based filter for them, and ranking gives `Variable` the same exact-title bonus as `Field`/`Property` — **a local can outrank the field it shadows**. Also multiplies `compute_reference_scores` cost, which already does five full passes over `symbols`. | Filter or de-boost `Variable` symbols carrying `role = local\|parameter` in ranking. **Highest-value follow-on.** |
| **C# file-scoped namespaces** (v2.19.0) | New namespace symbols; every type in such a file is re-parented. | Covered by the forced reindex (§3). |
| **XML becomes parser-backed** (v2.21.0) | `.xml`/`.xsd`/`.wsdl` were already indexed text-only; they now emit element symbols. XML is missing from `DOC_LANGUAGES` (so elements rank as code definitions) and from `NON_EMBEDDABLE_LANGUAGES` (so tag symbols pollute the vector space). | Add `xml` to both lists. |
| **Erlang is new** (v2.21.0) | `.erl`/`.hrl` index automatically. | Add a language config (§5, Task 5). |
| **`Identifier.code_context` is now always `None`** | Julie writes the column and never reads it — it is ~20% of the fixture DB. | Pure win; no action. Optionally drop the column later. |

Indexing and watching pick up new languages **automatically** — both derive from
`julie_extractors::language::supported_extensions()`. No per-language wiring is needed for discovery.

---

## 5. Work plan

Ordered so each task is independently verifiable. TDD per `CLAUDE.md`: the repo already ships the RED
for Task 1.

### Task 1 — Re-pin all six manifests
`xtask/tests/extractor_dependency_contract_tests.rs` hard-asserts `v2.16.0` across all six manifests.
That test is the built-in RED.

1. Run `cargo nextest run -p xtask extractor_dependency_release_is_v2_16_0` → **RED** after re-pin.
2. Bump the tag in: root `Cargo.toml`, `crates/julie-{core,index,pipeline,runtime,tools}/Cargo.toml`.
3. Rename the test to `…_is_v2_27_0` and update both assertions. Refresh `Cargo.lock`.
4. Re-run → **GREEN**.

*Effort: 1 task.*

> **Consider:** move the pin to `[workspace.dependencies]` so the version lives in one place. Six
> copies is what produced the 335-error trap in §1.1. Small, and it removes the failure mode permanently.

### Task 2 — Fix the four API breaks (39 sites, 21 files)
Mechanical. `cargo check --workspace --all-targets` is the gate; it reaches zero errors when done.

**This task is already solved.** A compile-verified migration covering Tasks 1 and 2 is committed
alongside this plan as `2026-08-05-julie-extractors-v2.27.0-migration.patch` (all six manifests +
`Cargo.lock` + all 39 sites). Apply with `git apply`, then re-check. Review the two judgment sites
below before accepting it wholesale.

Two sites deserve judgment rather than a blind `None`:
- `julie-core/src/database/helpers.rs:296` (`row_to_relationship`) — Julie's schema has no span
  columns, so `span: None` is correct **but lossy**: producer-attested spans are dropped on read.
- `julie-pipeline/src/resolver.rs:249` — could propagate `span` / `reference_site_is_exact` from the
  pending edge instead of hardcoding. Deferring is fine; record it as debt.

*Effort: 1 task.*

### Task 3 — Force the reindex (§3)
1. Add a RED test asserting `SEMANTIC_INDEX_ENGINE_VERSION` names the pinned extractor tag.
2. Bump the literal in `src/tools/workspace/indexing/engine_version.rs`.
3. Keep the existing `contains(EXTRACTION_CONTRACT_VERSION)` assertion.

*Effort: 1 task. **Do not ship the upgrade without this.***

### Task 4 — Fix the docs version string
`xtask/tests/docs_contract_tests.rs:247` asserts `docs/DEPENDENCIES.md` contains the literal string
`julie-extractors v2.16.0` (`docs/DEPENDENCIES.md:12`).

**Note the direction:** unlike Task 1, this test does **not** fail on re-pin — it reads the doc, not
the manifests, so it stays green while the doc silently goes stale. It only fails if you update the
doc without updating the assertion. Update both together, and also refresh the release links in
`DEPENDENCIES.md:12` (they point at v2.15.0/v2.16.0).

*Effort: 1 task.*

### Task 5 — Language parity for Erlang and XML
`CLAUDE.md` forbids shipping languages as second-class citizens. Julie has **34** embedded
per-language search configs (`crates/julie-index/languages/*.toml`), which exactly matched the 34
user-facing languages before this upgrade. `erlang` and `xml` are the two genuine new gaps
(`jsx`/`tsx` are aliased and were never separate configs). A missing config is not a crash —
`LanguageConfigs::get()` returns `Option` — but those languages get generic tokenization and no
test-evidence or annotation-class rules, and their literals are dropped entirely by the literal
bloat gate.

1. Add `erlang.toml` and `xml.toml`; register both in the `include_str!` list.
2. Add `xml` to `DOC_LANGUAGES` and `NON_EMBEDDABLE_LANGUAGES`.
3. Add `erl`/`hrl`/`xsd`/`wsdl` to the two hardcoded search-hint extension allowlists.
4. Fix `lifecycle_role_from_name` — it misclassifies Erlang Common Test teardown hooks
   (`end_per_suite`/`_testcase`/`_group`) as `FixtureSetup`.

*Effort: 1–2 tasks.*

### Task 6 — Correct the language count: 34 → **36**

**The count is 36, not 38.** Julie's existing "34" was never wrong. The extractor registry holds 38
`LanguageSpec` entries, but `jsx` and `tsx` are variants of `javascript`/`typescript`, not separate
user-facing languages — Julie's own config layer aliases them (`CONFIG_LANGUAGE_ALIASES`) and the
README never listed them. So 38 specs = **36 user-facing**, and the delta is exactly `erlang` + `xml`.

Update: `CLAUDE.md` and `AGENTS.md` (kept in sync by the pre-commit hook; 4 places each incl. the
language-list body), `README.md` (heading `Supported Languages (34)` plus 4 prose mentions —
Functional gains Erlang, Documentation gains XML), `docs/site/index.html` (the gh-pages site: title
plus the Functional/Documentation group counts), `docs/TREE_SITTER_QUALITY_BAR.md`,
`docs/FILEWATCHER.md`, `docs/INTELLIGENCE_LAYER.md`, `docs/RELATIVE_PATHS_CONTRACT.md`,
`docs/adr/ADR-0005…`, two source comments, and the `FastSearchTool` language-filter doc string
(shown to agents as MCP schema text).

*Effort: 1 task.*

### Task 7 — Rank-safety for the C# `Variable` flood (see §4)
De-boost or filter `Variable` symbols whose metadata carries `role = local | parameter`.
*Effort: 1 task. Recommended before any release.*

### Task 8 — Verification (see §6)

---

## 6. Verification

**The `extractor-dep-integration` bucket is in the `full` tier only, not `dev`** (verified via
`cargo xtask test list`). A `Cargo.toml` / `Cargo.lock` edit falls back to `dev`, so **neither
`cargo xtask test changed` nor `cargo xtask test dev` runs the extractor gate after a re-pin.**
Prescribe the bucket explicitly.

(The two hardcoded-version tests from Tasks 1 and 4 *are* covered by `dev`, because they live in the
`xtask-runner` bucket — `cargo nextest run -p xtask` — which is in both `dev` and `full`. So `dev`
catches the version strings but not the extractor integration gate. Run both.)

```bash
cargo xtask test bucket extractor-dep-integration   # the dependency-upgrade gate
cargo xtask test dev                                # batch regression, once
cargo xtask test dogfood                            # search/ranking changed (Tasks 5, 7)
cargo xtask test full                               # pre-merge
```

Plus a dogfood pass on a real C# repo to confirm the reindex fires and to eyeball the `Variable` volume.

Note two limits honestly:
- The `search_quality` fixture is a **frozen committed 100MB snapshot** with only 51 C# symbols. It is
  insulated from this upgrade *and blind to it* — it will not catch C# ranking regressions.
- The 34-embedded-config assertion still **passes** after the upgrade. That is the defect: its stated
  invariant is now false. Task 5 should make it assert parity with the extractor registry instead.

### Verification Ledger

Per `docs/plans/verification-ledger-template.md`.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Workspace compiles at v2.27.0 | `cargo check --workspace --all-targets` | affected-change | 8593ed71 | pass | 2026-08-06T01:05:00Z | no |
| Repo formatting clean | `cargo fmt --check` | affected-change | 8593ed71 | pass | 2026-08-06T01:05:00Z | no |
| Extractor dependency upgrade gate | `cargo xtask test bucket extractor-dep-integration` | extractor-dep | 8593ed71 | pass (3/3) | 2026-08-06T01:12:00Z | no |
| Batch regression | `cargo xtask test dev` | branch-gate | 8593ed71 | pass (27 buckets, 255.8s warm) | 2026-08-06T01:22:00Z | no |
| Search/scoring regression | `cargo xtask test dogfood` | expensive-specialist | 8593ed71 | pass (2 buckets, 656.8s warm) | 2026-08-06T01:45:00Z | no |
| Erlang/XML/C# index end-to-end | `julie-server search --standalone` on a 4-language fixture | affected-change | 8593ed71 | pass | 2026-08-06T01:52:00Z | no |

### Dogfood finding — pre-existing, NOT introduced here

Indexing a fixture with a C# field `command`, an XML `<xs:complexType name="Command">`, and a
markdown heading `# Command` showed the **XML element winning the top definition slot over the C#
field**. Root cause: `DOC_LANGUAGES` never reaches the unified reranker. `role_demotion`
(`reranker.rs:234`) matches only `vendor` and `generated`; `is_source_language` gates only a *bonus*
(`reranker.rs:464`), never a demotion. So a doc-language symbol wins purely on
`kind_boost` — XML and markdown both extract these as `Module` (30.0) against a C# `Field` (20.0).

**This predates the upgrade:** `kind_boost` and `role_demotion` are untouched by this change,
markdown was already in `DOC_LANGUAGES` at `8593ed71`, and markdown produces the same `Module` rows.
A markdown heading already outranked a C# field the same way.

XML makes it more visible, because `.xml`/`.xsd`/`.wsdl` are ubiquitous in .NET and Java repos and
were previously text-only. Adding XML to `DOC_LANGUAGES` and `NON_EMBEDDABLE_LANGUAGES` is still
correct — it fixes the source-phrase bonus and keeps XML out of the vector space — it simply cannot
fix a demotion path that was never wired.

**Follow-up (own TDD cycle, not this release):** wire `DOC_LANGUAGES` into a real demotion in
`rerank_unified`, sized so a doc symbol sits below a code symbol of any kind. It changes markdown,
JSON, TOML, and YAML ranking too, so it needs a full `dogfood` validation of its own.

---

## 7. Scope notes

**Deliberately out of scope** (net-new features, not upgrade requirements):

- Adopting `ExtractionLevel::Symbols` (library-visible via `extract_canonical_at`; Julie only extracts Full).
- Persisting relationship spans / `reference_site_is_exact` in schema columns (Julie hand-rolls spans
  into metadata JSON today). Would make Task 2's lossy `None` unnecessary.
- Reading parse diagnostics — Julie stores them and never reads them, so `depth_truncated` and the new
  `message` field are invisible.
- Surfacing `isStatic` metadata (persisted, unread).
- Marker facts (`code.marker.v1`, 37 languages) — these become queryable through Julie's existing
  `patterns` tool with **zero code change**, a free win from the upgrade rather than work.

**Naming hazard for whoever does Task 7:** the extractor's new `role` metadata (`local`/`parameter`)
collides with Julie's existing path-derived `role`. Do not conflate them.

---

## 8. Effort summary

| Phase | Tasks | Notes |
|---|---|---|
| Make it compile and reindex correctly | 1–4 | 4 tasks — the true minimum for a correct upgrade |
| Language parity + docs | 5–6 | 2–3 tasks — required by `CLAUDE.md` policy |
| Rank safety | 7 | 1 task — recommended before release |
| Verification | 8 | 1 session incl. the `full` tier and a real-repo dogfood |

**Roughly 8–9 agent tasks across 2–3 sessions.** Human time is needed only for the §0 retirement
decision and release approval.
