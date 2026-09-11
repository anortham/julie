# Head-to-head retrieval matrix: Julie vs Miller

## Scope

- Retrieval matrix only: 23 search rows and 23 inspect rows.
- One machine. Corpus roots are the ten public repos pinned in `cases.json`.
- Manifest and results live under `docs/eval/`. Neither product indexes that tree as a corpus.
- No agent episodes. No Rust `xtask-eval revival` harness.
- No winner by construction.

Inspect targets are the first top-level symbol on the expected file whose kind is not `import`, `package`, `namespace`, `module`, or `using`. Search output is never used to pick a target.

Search scoring uses only the first `expected_any` path.

The next kept run must record Julie vector coverage (`select count(*) from vectors` and `from symbols` on `$JULIE_HOME/indexes/<workspace_id>/facts.sqlite`) and each Julie row's `readiness` field. Use `--require-semantics` so the run aborts if any repo has zero vectors.

## Lexical-only run

Pair: `docs/eval/head-to-head/results/20260911T040446Z.json` and `docs/eval/head-to-head/results/20260911T040446Z.md`.

This pair is a lexical-only Julie run. Every one of the ten corpus indexes had zero rows in `vectors` during both runs. The service had no `julie-semantic-sidecar` binary next to `target/release/julie-server`, so `embedding_child` was `PROVIDER_UNAVAILABLE`. Julie ran lexical search. Miller ran its normal pipeline. The numbers below are not a fair semantic comparison.

The first run after Miller `workspace open` queued background indexes. That run is discarded. This pair is the second run on warm indexes.

Inspect targets in that pair were the first listed top-level name, often an import or package (`finalhandler`, `typing`, `Foundation`, `rack`). That is why inspect top-1 is low for both products in this pair.

### Per task class

| class | n | Julie top-1 | Julie top-5 | Miller top-1 | Miller top-5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| retrieval.concept | 13 | 5/13 (38%) | 6/13 (46%) | 6/13 (46%) | 7/13 (54%) |
| retrieval.implementation | 10 | 1/10 (10%) | 2/10 (20%) | 5/10 (50%) | 7/10 (70%) |
| inspect.symbol | 23 | 3/23 (13%) | 6/23 (26%) | 4/23 (17%) | 8/23 (35%) |

### Per tool

| tool | n | p50 ms | p95 ms | p50 bytes | p95 bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| julie.fast_search | 23 | 36 | 231 | 589 | 780 |
| miller.search | 23 | 268 | 719 | 1552 | 2045 |
| julie.deep_dive | 23 | 2 | 4 | 291 | 5931 |
| miller.inspect | 23 | 106 | 182 | 398 | 10186 |

### Trade-offs (lexical Julie only)

- Miller ranked the expected source file more often on natural-language search. Julie compact search is smaller and faster, and more often ranked changelog or docs hits first.
- Julie `deep_dive` is much faster. Miller `inspect` JSON is larger.
- Compact Julie search groups hits under one file path. Miller search JSON is a symbol list with `file` fields.

### Disagreements (scoring_pass differs)

Read from the kept JSON texts.

Search:

- `alamofire-auth-refresh-window.search`: Julie ranked `Source/Features/AuthenticationInterceptor.swift`. Miller ranked generated `docs/Classes/AuthenticationInterceptor/RefreshWindow.html`.
- `cobra-command-execute.search`: Julie returned zero content matches (tokens must share a line). Miller ranked `command.go` `ExecuteContext`.
- `flask-blueprint-registration.search`: Julie ranked `CHANGES.rst`. Miller ranked `src/flask/sansio/blueprints.py`.
- `flask-request-context-session.search`: Julie ranked `docs/design.rst`. Miller ranked `src/flask/sessions.py` `open_session` (expected file is `src/flask/ctx.py`; Miller still passed top-5).
- `flask-view-dispatch.search`: Julie ranked `docs/views.rst`. Miller ranked `src/flask/views.py` `dispatch_request`.
- `gson-reflective-fields.search`: Julie ranked `Troubleshooting.md`. Miller ranked `TypeAdapters.java` first and still had the expected factory in top-5.
- `jq-compile-bytecode.search`: Julie ranked `src/jq.h` and still had a top-5 pass. Miller ranked `src/compile.h` and missed `src/execute.c` in top-5. Scoring uses only the first expected path (`src/execute.c`).
- `moshi-json-adapter-null-wrapper.search`: Julie ranked `NullSafeJsonAdapter.kt` (top-5 pass). Miller ranked `Moshi.kt` and missed `JsonAdapter.kt` in top-5.
- `newtonsoft-serializer-internal-reader.search`: Julie ranked `JsonSerializerInternalReader.cs` (the other `expected_any` file). Miller ranked `JsonSerializer.cs`. Scoring uses only the first expected path.
- `nlohmann-binary-reader.search`: Julie ranked `README.md`. Miller ranked `docs/mkdocs/mkdocs.yml` first and still had a top-5 pass.
- `nlohmann-json-pointer.search`: Julie ranked `docs/mkdocs/docs/home/exceptions.md`. Miller ranked `include/nlohmann/detail/json_pointer.hpp`.
- `sinatra-route-compile.search`: Julie ranked `CHANGELOG.md`. Miller ranked `lib/sinatra/base.rb`.

Inspect:

- `flask-request-context-session.inspect`: Julie returned `No symbol found: 'contextvars'`. Miller ranked the `import contextvars` in `src/flask/ctx.py`.
- `nlohmann-binary-reader.inspect`: Julie ranked `binary_reader.hpp` for `cbor_tag_handler_t`. Miller ranked `docs/mkdocs/mkdocs.yml`.
- `nlohmann-sax-parser.inspect`: Julie ranked `parser.hpp` for `parse_event_t`. Miller ranked `docs/mkdocs/mkdocs.yml`.
- `sinatra-route-dispatch.inspect` and `sinatra-route-compile.inspect`: Julie ranked `.github/workflows/test.yml` for `rack`. Miller ranked `require 'rack'` in `lib/sinatra/base.rb`.

### Binaries and corpus

- Julie: `target/release/julie-server` built in this worktree before the run. No sidecar beside that binary.
- Miller: existing `~/source/miller/src/Miller.Server/bin/Release/net10.0/miller` (not rebuilt; that repo was not edited).
- Repo commits: see `docs/eval/head-to-head/cases.json` `repos`.

## Semantic run

Not run yet. Fill this section after vectors are complete and the matrix is rerun with `--require-semantics`.

- Results pair:
- Julie semantic coverage table:
- Per task class:
- Per tool:
- Disagreements:
