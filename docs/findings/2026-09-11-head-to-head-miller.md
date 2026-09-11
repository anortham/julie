# Head-to-head retrieval matrix: Julie vs Miller

## Scope

- Retrieval matrix only: 23 search rows and 23 inspect rows.
- One machine. Corpus roots are the ten public repos pinned in `cases.json`.
- Manifest and results live under `docs/eval/`. Neither product indexes that tree as a corpus.
- No agent episodes. No Rust `xtask-eval revival` harness.
- No winner by construction.

Inspect targets are the first top-level symbol on the expected file whose kind is not `import`, `package`, `namespace`, `module`, or `using`. Search output is never used to pick a target.

Search scoring uses only the first `expected_any` path.

The semantic run records Julie vector coverage and each Julie row's `readiness` field. `--require-semantics` aborts if any repo has zero vectors.

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

Pair: `docs/eval/head-to-head/results/20260911T045558Z.json` and `docs/eval/head-to-head/results/20260911T045558Z.md`.

Ran twice with `--require-semantics`. The first pair (`20260911T045513Z`) warmed indexes and is discarded. This pair is the second run.

Vectors are present on all ten repos. Coverage is definition-kind vectors over all symbols (sqlite `count(*)` on `vectors` and `symbols`). MCP stdio replies in this binary did not include a `readiness` object, so every Julie row has `readiness` and `readiness_coverage` unset. No Julie row reported a readiness status other than missing. The coverage table uses the sqlite fallback.

Inspect targets in this pair skip import/package/namespace/module/using. Search rows are unchanged from the lexical pair.

Search top-5 is the same as the lexical-only pair. Default `fast_search` still ranked changelog and docs hits first on the same rows. Inspect top-1 rose because the targets are now definitions, not because search ranking changed.

### Julie semantic coverage

| repo | workspace_id | coverage | symbols | vectors | source |
| --- | --- | --- | ---: | ---: | --- |
| alamofire | alamofire_fc100108 | 3438/59278 | 59278 | 3438 | sqlite |
| cobra | cobra_011de3e1 | 428/4095 | 4095 | 428 | sqlite |
| express | express_d48d16da | 499/5997 | 5997 | 499 | sqlite |
| flask | flask_d0f003c5 | 771/6463 | 6463 | 771 | sqlite |
| gson | gson_1e35f048 | 1967/16375 | 16375 | 1967 | sqlite |
| jq | jq_8a91e32c | 2530/13922 | 13922 | 2530 | sqlite |
| moshi | moshi_8aa968c1 | 1409/9340 | 9340 | 1409 | sqlite |
| newtonsoft-json | newtonsoft_json_fbef4a5c | 4554/40075 | 40075 | 4554 | sqlite |
| nlohmann-json | nlohmann-json_d3e504cd | 2630/32697 | 32697 | 2630 | sqlite |
| sinatra | sinatra_5bdf5e98 | 1331/6977 | 6977 | 1331 | sqlite |

### Per task class

| class | n | Julie top-1 | Julie top-5 | Miller top-1 | Miller top-5 |
| --- | ---: | ---: | ---: | ---: | ---: |
| retrieval.concept | 13 | 5/13 (38%) | 6/13 (46%) | 6/13 (46%) | 7/13 (54%) |
| retrieval.implementation | 10 | 1/10 (10%) | 2/10 (20%) | 5/10 (50%) | 7/10 (70%) |
| inspect.symbol | 23 | 20/23 (87%) | 23/23 (100%) | 19/23 (83%) | 23/23 (100%) |

### Per tool

| tool | n | p50 ms | p95 ms | p50 bytes | p95 bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| julie.fast_search | 23 | 37 | 72 | 589 | 780 |
| miller.search | 23 | 250 | 423 | 1552 | 2045 |
| julie.deep_dive | 23 | 5 | 13 | 733 | 1629 |
| miller.inspect | 23 | 104 | 204 | 1734 | 8486 |

### Disagreements (scoring_pass differs)

Read from the kept JSON texts.

Search:

- `alamofire-auth-refresh-window.search`: Julie ranked `Source/Features/AuthenticationInterceptor.swift`. Miller ranked generated `docs/Classes/AuthenticationInterceptor/RefreshWindow.html`.
- `cobra-command-execute.search`: Julie returned zero content matches (tokens must share a line). Miller ranked `command.go` `ExecuteContext`.
- `flask-blueprint-registration.search`: Julie ranked `CHANGES.rst`. Miller ranked `src/flask/sansio/blueprints.py`.
- `flask-request-context-session.search`: Julie ranked `docs/design.rst`. Miller ranked `src/flask/sessions.py` first and still passed top-5 for `src/flask/ctx.py`.
- `flask-view-dispatch.search`: Julie ranked `docs/views.rst`. Miller ranked `src/flask/views.py` `dispatch_request`.
- `gson-reflective-fields.search`: Julie ranked `Troubleshooting.md`. Miller ranked `TypeAdapters.java` first and still had the expected factory in top-5.
- `jq-compile-bytecode.search`: Julie ranked `src/jq.h` and still passed top-5 with `src/execute.c`. Miller ranked `src/compile.h` and missed `src/execute.c` in top-5.
- `moshi-json-adapter-null-wrapper.search`: Julie ranked `NullSafeJsonAdapter.kt` and still passed top-5. Miller ranked `Moshi.kt` and missed `JsonAdapter.kt` in top-5.
- `newtonsoft-serializer-internal-reader.search`: Julie ranked `JsonSerializerInternalReader.cs` (the other `expected_any` file). Miller ranked `JsonSerializer.cs`. Scoring uses only the first expected path.
- `nlohmann-binary-reader.search`: Julie ranked `README.md`. Miller ranked `docs/mkdocs/mkdocs.yml` first and still passed top-5.
- `nlohmann-json-pointer.search`: Julie ranked `docs/mkdocs/docs/home/exceptions.md`. Miller ranked `include/nlohmann/detail/json_pointer.hpp`.
- `sinatra-route-compile.search`: Julie ranked `CHANGELOG.md`. Miller ranked `lib/sinatra/base.rb`.

Inspect:

- `jq-compile-bytecode.inspect`: Julie listed `jq_state` first in `src/jq.h`. Miller ranked `src/execute.c`.
- `nlohmann-binary-reader.inspect`: Julie ranked `binary_reader.hpp` for `cbor_tag_handler_t`. Miller ranked `docs/mkdocs/mkdocs.yml` first.
- `nlohmann-sax-parser.inspect`: Julie ranked `parser.hpp` for `parse_event_t`. Miller ranked `docs/mkdocs/mkdocs.yml` first.

## Julie backend comparison

Pair: `docs/eval/head-to-head/results/20260911T050543Z.json` and `docs/eval/head-to-head/results/20260911T050543Z.md`.

Ran once with `--julie-backends auto,hybrid,semantic --require-semantics` on warm indexes. Search rows are labeled `<id>.<backend>`. Miller ran once per search row. Inspect rows are not backend-split.

Julie's default `auto` backend is lexical. Hybrid and semantic close or reverse the gap on natural-language rows. Changing the default is a product decision recorded for the owner, not made here.

Stdio MCP replies still carry no `readiness` object (the JSON API does). Coverage stays sqlite-based.

| class | n | Miller top-1 | Miller top-5 | auto top-1 | auto top-5 | hybrid top-1 | hybrid top-5 | semantic top-1 | semantic top-5 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| retrieval.concept | 13 | 6/13 (46%) | 7/13 (54%) | 5/13 (38%) | 6/13 (46%) | 7/13 (54%) | 10/13 (77%) | 9/13 (69%) | 10/13 (77%) |
| retrieval.implementation | 10 | 5/10 (50%) | 7/10 (70%) | 1/10 (10%) | 2/10 (20%) | 6/10 (60%) | 9/10 (90%) | 9/10 (90%) | 10/10 (100%) |

Example: `flask-blueprint-registration.search` ranks `CHANGES.rst` under auto and `src/flask/sansio/blueprints.py` first under hybrid and semantic.
