# Search Matrix Investigation Design

## Problem

Julie's daemon telemetry gives us useful detail about Julie tool calls, especially `fast_search`, but it stops at Julie's handler boundary. The write path in [record_tool_call](/Users/murphy/source/julie/src/handler.rs:1636) only sees Julie MCP tools, not harness-native follow-up actions such as `Read`, `Edit`, `Update`, shell commands, or any client-side reasoning after the Julie call.

That leaves us with two blind spots:

1. We can measure Julie search outcomes, but we cannot infer agent-level recourse from Julie-only telemetry.
2. We can mine live failure shapes, but we do not have a controlled harness that runs representative search calls across a broad repo corpus and tells us which failures are reproducible, cross-language, or layout-specific.

The current telemetry remains useful for discovery, but it is not enough to drive search-quality progress on its own.

## Goal

Build a repo-local search matrix investigation harness that:

1. mines local Julie telemetry for high-signal query shapes
2. turns those shapes into a curated matrix of `fast_search` calls
3. runs that matrix across a broad corpus under `~/source`
4. records outcomes, diagnostics, and result samples in a machine-readable report
5. promotes stable, high-value cases into existing dogfood or regression fixtures

This harness is for investigation first, regression second. It should help us find baseline improvements now without pretending Julie-only telemetry answers the cross-tool recourse question.

## Non-goals

- Do not build a giant cross-product of every parameter combination.
- Do not try to infer agent success or failure from downstream non-Julie behavior.
- Do not turn the first version into a CI gate.
- Do not require Windows data to get started.

## Available building blocks

Julie already has pieces worth reusing:

- `tool_calls.metadata` with `fast_search` query, intent, filters, and trace fields in [search_telemetry.rs](/Users/murphy/source/julie/src/handler/search_telemetry.rs)
- search-analysis episode and trace parsing in [search_analysis.rs](/Users/murphy/source/julie/src/dashboard/search_analysis.rs)
- search compare run storage and replay shape in [search_compare.rs](/Users/murphy/source/julie/src/dashboard/search_compare.rs)
- existing search-quality fixtures under `fixtures/search-quality/**`
- existing dogfood culture and heavy search bucket in `cargo xtask test dogfood`

The new harness should sit beside these, not ignore them.

## Approach options

### Option A: telemetry-only triage

Use daemon queries, group by buckets, fix what looks common.

**Pros**
- fast to start
- grounded in live usage

**Cons**
- no controlled corpus
- no cross-language baseline
- no deterministic rerun
- no way to separate repo-specific noise from cross-repo failure patterns

### Option B: exhaustive search-call matrix

Generate a huge parameter matrix across query shapes, filters, limits, and corpus repos.

**Pros**
- broad coverage on paper

**Cons**
- combinatorial blow-up
- noisy reports
- expensive runs
- weak signal density
- hard to maintain

### Option C: curated matrix seeded by telemetry and known buckets

Mine live query shapes, normalize them into query families, run a curated set across a broad corpus, then promote strong cases into deterministic regression suites.

**Pros**
- grounded in real usage
- controlled reruns
- broad language coverage
- manageable maintenance cost
- clear path from investigation to regression

**Cons**
- needs curation
- first pass is observational, not a strict pass/fail gate

## Recommendation

Pick **Option C**.

This gives us the fastest path to useful signal without pretending the current telemetry can answer questions it cannot answer.

## Design

### 1. Separate mining from the matrix

The harness has two distinct stages:

1. **Mining stage**
   Read `~/.julie/daemon.db`, extract high-signal `fast_search` shapes, cluster them by bucket, and write a candidate seed report.
2. **Matrix stage**
   Execute a curated matrix file against a repo corpus and write a baseline report.

The mining stage is local and opportunistic. The matrix stage is committed and repeatable.

This split matters because telemetry is noisy and host-specific. The committed matrix should be edited by humans after review, not regenerated blindly on every run.

### 2. Corpus model

The harness should support three corpus profiles:

- `smoke`
  3 to 5 repos, fast local iteration
- `breadth`
  10 to 15 repos across major language and layout families
- `full`
  everything in the committed manifest that resolves on the current machine

The corpus manifest should be repo-name based, not absolute-path based. Resolution should search configurable roots, with `~/source` as the default.

Recommended starter corpus from the current machine:

- Rust: `julie`, `goldfish`, `razorback`
- TypeScript or JavaScript: `express`, `zod`, `rtk`
- Python: `flask`, `toon-python`
- C#: `Newtonsoft.Json`, `blazor-samples`, `SurgeryScheduling`
- Go: `cobra`
- Java: `gson`, `okhttp`
- Swift: `Alamofire`
- Ruby: `sinatra`
- Elixir: `phoenix`
- Zig: `zls`

This starter set gives us language spread plus layout spread:

- `src/`, `tests/`
- `lib/`, `spec/`
- `Sources/`, `Tests/`
- `.NET` project trees
- Go flat package layouts
- Java multi-module or package-heavy trees

### 3. Query-family matrix, not raw query dump

The committed matrix should organize cases by **query family**, not by raw daemon query string alone.

Each family represents a shape Julie should handle well:

- exact identifier
- CamelCase identifier
- snake_case identifier
- hyphenated token
- multi-token file-level query
- quoted phrase
- punctuation-heavy literal
- scoped content query with `file_pattern`
- alternation `file_pattern`
- exclusion `file_pattern`
- `exclude_tests=true`
- language-filtered search
- expected zero-hit with actionable hint

Each family can have a small number of variants. The rule is simple: enough cases to expose the failure mode, no filler.

### 4. Case format

Each committed case should include:

- `case_id`
- `family`
- `query`
- `search_target`
- optional `language`
- optional `file_pattern`
- optional `exclude_tests`
- `profile_tags`
- optional `repo_selector`
- expected mode:
  - `observational`
  - `expect_hits`
  - `expect_zero_hit_reason`
  - `expect_hint_kind`

The first version should bias toward `observational` and `expect_hint_kind` or `expect_zero_hit_reason`. Exact top-hit assertions should be reserved for cases where the repo fact is clear and stable.

### 5. Runner behavior

The runner should:

1. resolve the corpus manifest against search roots
2. ensure each repo is indexed and opened in daemon mode
3. execute each case against each eligible repo
4. capture:
   - hit count
   - top hits
   - `zero_hit_reason`
   - `file_pattern_diagnostic`
   - `hint_kind`
   - `relaxed`
   - latency
5. write JSON and Markdown reports

The report should group by:

- case family
- repo language
- repo
- failure bucket

The runner should also emit cross-repo summary flags:

- `cross_repo_zero_hit`
- `unattributed_zero_hit`
- `line_match_miss_cluster`
- `scoped_no_in_scope_cluster`
- `unexpected_hint`

### 6. Outputs

The harness should produce:

- machine-readable JSON
- a compact Markdown summary for humans
- a ranked list of promotion candidates for future dogfood tests

Promotion candidates are cases that are:

- reproducible
- high-signal
- stable across reruns
- tied to a named failure bucket or known query family

### 7. Promotion loop

The matrix is not the end state. The promotion loop is:

1. mine live telemetry
2. add or update matrix cases
3. run baseline across corpus
4. fix root causes
5. rerun baseline
6. promote stable cases into existing dogfood or regression fixtures

This keeps the matrix investigative and the dogfood suite deterministic.

## Command surface

Recommended command shape:

```bash
cargo xtask search-matrix mine --days 7 --out artifacts/search-matrix/seeds-YYYY-MM-DD.json
cargo xtask search-matrix baseline --profile smoke
cargo xtask search-matrix baseline --profile breadth --out artifacts/search-matrix/breadth-YYYY-MM-DD.json
```

`mine` is local and does not need committed outputs.

`baseline` reads committed matrix files plus the local corpus manifest resolution and writes reports under `artifacts/`.

## Files to add

- `docs/plans/2026-04-22-search-matrix-investigation-design.md`
- `fixtures/search-quality/search-matrix-cases.toml`
- `fixtures/search-quality/search-matrix-corpus.toml`
- `xtask/src/search_matrix.rs`
- `xtask/src/search_matrix_mine.rs`
- `xtask/src/search_matrix_report.rs`
- `xtask/tests/search_matrix_contract_tests.rs`

## Files to modify

- `xtask/src/main.rs`
- `xtask/Cargo.toml`
- `docs/TESTING_GUIDE.md`

## Acceptance criteria

- [ ] We can mine local daemon telemetry into a seed report without touching committed fixtures.
- [ ] We have a committed curated matrix file organized by query family.
- [ ] We have a committed corpus manifest with `smoke`, `breadth`, and `full` profiles.
- [ ] The baseline runner can execute the matrix across repos resolved from `~/source`.
- [ ] Reports include `zero_hit_reason`, `file_pattern_diagnostic`, and `hint_kind`.
- [ ] Reports group failures by query family and repo language.
- [ ] The first breadth run identifies a ranked short list of promotion candidates for dogfood tests.
- [ ] The harness does not claim agent-level recourse from Julie-only telemetry.

## Open questions

1. Should the first implementation index missing repos on demand, or require the operator to pre-index the corpus before running the matrix?
2. Should the committed corpus manifest contain one starter repo per language family, or a broader default set from day one?
3. Should `baseline` write one combined report, or one report per profile and date stamp?

## Recommendation on the open questions

1. Require pre-indexed repos for the first version. It keeps the baseline runner focused and avoids mixing indexing failures with search-quality failures.
2. Start with the broader `breadth` set listed above. The whole point is cross-language spread.
3. Write dated reports per profile. One combined report turns into sludge fast.
