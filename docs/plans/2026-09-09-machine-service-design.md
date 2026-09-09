# Julie machine service design

**Status:** design for user review. Agreed in the 2026-09-09 brainstorm, revised after a Codex review
the same day (see the review record at the end). Not approved for implementation.

**Scope:** one Rust service per developer machine that replaces the per-session Miller and Julie
processes. It owns indexing, watching, embeddings, and the status page. Agents reach it through
stateless MCP over HTTP, a stdio shim, and a CLI. This document is the design. Implementation plans
follow after approval, one per phase.

## 1. Why

Miller and Julie both run one full runtime per agent session. Every session bootstraps a workspace,
competes for indexer leadership, opens the shared store, and runs its own watchers and caches. The
last month of Miller work was almost entirely the cost of that shape:

| Symptom | Evidence |
|---|---|
| WAL debt, held readers, idle leaders | Miller commits and plans from 2026-09-04 to 2026-09-08: WAL recurrence, reader retention, idle-leader fix |
| Store growth | Miller's own index: 22.6 MB of source became 1.1 GB. The dotnet/runtime benchmark index was 21.9 GB per worktree |
| Resolution history | A 2.4 GB family store held 1.1 GB of live resolution deltas: 154 deltas, 1.78 million identifier-delta rows, 352 MB of gap JSON. Exact resolves took 164 to 255 seconds |
| Stale worktree views | Twelve producer views for two live worktrees |
| Ten durable stores per workspace | symbols, store, coord, search, content, vectors, ct, history, plus workspaces and telemetry per machine |
| Slow, heavy tests | Suites test process coordination against real stores |

None of these is a SQLite limit. All of them come from many processes sharing durable mutable state.

Two external changes make a different shape possible now:

- MCP 2026-07-28 is stateless. No sessions, no `initialize`, no server-initiated JSON-RPC requests.
  Each call carries its version and capabilities. A Streamable HTTP endpoint is one POST per call.
  Cross-call state, when a server needs it, travels as ordinary tool arguments. Miller and Julie
  already pass an explicit `workspace_id` on every call, so their tool contract fits that model.
- Julie's revival commits of 2026-09-07 and 2026-09-08 built a shared request engine with CLI parity,
  MCP 2026-07-28 over Streamable HTTP through `rmcp` 3.0.1 (verified: the crate lists protocol
  version 2026-07-28 and implements `server/discover`), workspace lifecycle, and native semantics.
  That is most of the service.

## 2. Goals

- One coordinated runtime per machine. Sessions are clients, not runtimes.
- Two durable roots per checkout: one SQLite file and one Tantivy directory. One registry per machine.
  Everything else is derived, in memory, and disposable.
- A new worktree of an indexed repo is ready in seconds, by copying facts from a sibling checkout.
- Warm tool calls answer in tens of milliseconds. Trace and impact answer from memory.
- The fast test bucket runs in under ten seconds. The Linux full suite runs in under two minutes.
- Miller's ten-tool contract and agent guidance carry over unchanged in shape.
- Phases 2 and 3 are each net negative in lines of code against what they delete.

## 3. Non-goals

- Continuous testing in the first release. Its shape is fixed in section 11 so nothing blocks it.
- A global cross-repo index. Cross-workspace reads stay explicit, by `workspace_id`.
- A graph database, a new storage engine, or a new extraction format.
- Shared index storage across worktrees. Section 7 explains why this is a copy, not a share.
- Remote or multi-user service. The service binds to localhost for one user.
- Self-update, live version handoff, systemd units, launchd agents, or Windows Services.
- The MCP Tasks extension, Skills over MCP, and durable result continuations.

## 4. The complexity rule

This section is normative. Every plan, worker prompt, and review derived from this design carries it.

Miller and Julie both collapsed under accumulated coordination code. The pattern is the same every
time: a problem appears, the agent adds a lock, a retry, a generation counter, or a repair command, the
tests get slower, the next problem appears inside the fix. Nothing in that loop is a bug. The loop
itself is the bug.

**Rule: when a task needs new coordination, new durable state, or a new repair path, stop and find a
simpler design. Do not power through.** Specifically:

1. A worker who needs any of these words in new code stops and reports before writing it: lock,
   lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair,
   continuation, handoff. The lead answers with a design change or an explicit written exception.
2. A worker who needs a third durable root per checkout, or a second per machine, stops. The design
   changes or the root does not exist.
3. A worker who needs a migration for a derived index stops. Derived indexes are deleted and rebuilt.
4. A worker whose focused test needs more than one process, a real repository, or more than two
   seconds stops and moves the behavior behind an in-memory seam.
5. A worker who cannot delete the code they are replacing in the same change stops. Parallel old and
   new paths are how the last two rewrites grew.
6. A reviewer who finds a fix that adds a branch to handle a state the design says cannot exist
   rejects the fix and asks what produced the state.

The rule has a positive form. Each design choice below deletes a class of code. Keep the deletion:

| Choice | Deletes |
|---|---|
| One process, one writer per checkout | Fencing, leader election, claims, busy-retry ladders, cross-process locks, publication locks |
| Derived indexes are disposable | Migrations, repair tools, generation tracking, compatibility shims |
| Stateless MCP | Session state, handshake, resumability, server-initiated requests |
| No persisted resolution or edges | Garbage collection, cursors, retention policy, vacuum jobs |
| In-memory graph | A graph query layer and its storage format |
| One store per checkout | Family discovery, view manifests, view retirement, cross-view filtering |
| Facts are immutable rows | Read transactions held across tool calls, and the WAL debt they cause |
| No live handoff | Drain protocol, version negotiation between service processes |
| No continuations | Durable continuation pages and their expiry |

Section 12 gives the budgets that make the rule measurable.

## 5. Architecture

```
agent session ──HTTP──▶ ┌──────────────────────────────────────────────┐
agent session ──stdio──▶│ shim ─▶│                                        │
julie CLI ──────HTTP──▶ │        │  julie-service (one per machine)       │
browser ────────HTTP──▶ │        │                                        │
                        │  request engine ─ tools ─ snapshots ─ indexes  │
                        │  watcher ─ extractor (in process) ─ jobs        │
                        │  embedding child (julie-semantic-sidecar serve) │
                        └──────────────────────────────────────────────┘
                             ~/.julie/registry.sqlite
                             <checkout>/.julie/facts.sqlite  tantivy/
```

### 5.1 Process model

- `julie-service` is a single Rust binary. The first client starts it if it is not running. It exits
  after a configurable idle period with no clients and no pending work. It is disposable runtime
  state: killing it loses nothing durable.
- It binds a localhost TCP port chosen at start, then writes the port and a random bearer token to
  `~/.julie/service.json` by atomic rename, mode 0600. Bind first, write last, so a readable file
  always names a listening service. A client that reads the file and cannot connect deletes it, starts
  a service, and retries once. Requests without the token are rejected.
- Version mismatch: a client that finds a service of an incompatible version fails with a one-line
  message naming both versions and the command `julie service restart`. There is no live handoff.
- One embedding child per service, spawned in `serve` mode, restarted on exit. The `broker` mode of
  the sidecar is not used.
- A crash in the service is handled by restart, not by in-process recovery. Extraction runs under
  `catch_unwind` per file so a Rust panic in one extractor records a diagnostic for that file. Aborts
  and native faults crash the service, and the next client starts a new one. That is the whole story.

### 5.2 Transports

All three bindings call the same request engine with the same typed requests and return the same
typed results. No binding has behavior of its own.

1. **Streamable HTTP MCP, 2026-07-28.** The primary path for Claude Code, Codex, and Cursor. One POST
   per call at `/mcp`. `server/discover` advertises versions and capabilities. Tool lists are
   deterministic and carry `ttlMs` and `cacheScope`. A long operation returns a normal result with a
   `job_id` and the caller polls `workspace status`. The Tasks extension is not used.
2. **stdio shim.** A thin mode of the same binary: `julie mcp-stdio`. It reads newline JSON-RPC,
   forwards to the service over HTTP, and writes replies. It exists for hosts that only launch
   subprocesses. It holds no state.
3. **Plain JSON HTTP.** `/api/<tool>` for the CLI and the status page. Same request and result
   types as the MCP tools, without the JSON-RPC envelope.

Older hosts that still speak 2025-11-25 get the compatibility behavior `rmcp` provides. This design
adds no compatibility code of its own. A tool that needs more input from the user returns the spec's
`input_required` result and the client retries; that is the only server-to-client ask.

### 5.3 Request engine

The revived Julie request engine is the base. A request is a typed struct. A tool is a function from
request plus a read snapshot to a result. The engine resolves `workspace_id` to a checkout, takes the
current snapshot of that checkout's derived indexes, runs the tool, and drops the snapshot. Tools never
touch storage directly. Tools never spawn work. A tool that needs background work enqueues a job and
returns its id in the result.

Results are bounded. A caller who needs more passes an explicit offset or range on the next call.
There are no continuation tokens and no durable continuation pages.

### 5.4 Module layout

The service is carved out of the existing Julie crates by deletion, not by adding a crate beside them.

| Crate | Owns | Comes from |
|---|---|---|
| `julie-facts` | `facts.sqlite` schema, blob table, path table, single writer, sibling seed copy | `julie-core` database modules, cut down |
| `julie-index` | Tantivy projection, in-memory graph, vector scan, snapshot type | existing `julie-index` |
| `julie-engine` | request types, tool functions, `workspace_id` resolution | existing `request_engine` and `julie-tools` |
| `julie-service` | HTTP server, MCP binding, stdio shim, jobs, watcher, extractor host, status page | `julie-runtime`, `handler`, `dashboard`, `workspace_runtime` |
| `julie` (bin) | CLI verbs over `/api` | existing `cli` |

Deleted outright, and phase 2 does not pass until they are gone: `leadership.rs`, daemon lock guards,
publication locks, per-session workspace bootstrap, the continuation store, the Python embedding host,
the sidecar broker client, and migrations for derived state. The ADRs in `docs/adr/` stay in force
where their subject survives: the strict path resolver, the typed MCP error classifier, the
prepared-once edit invariant, and source-aware language detection. ADR-0004's per-path edit lock becomes
a per-path in-process mutex; the invariant holds, the cross-process form goes away.

## 6. Storage

### 6.1 Per checkout: two durable roots

Every checkout root, main or linked worktree, has its own `<checkout>/.julie/`. Nothing in it is
shared with another checkout.

1. **`facts.sqlite`.** The only durable database. One writer: the service. Tables:
   - `blobs(hash, language, extractor_version, byte_len)` — one row per unique file content seen in
     this checkout.
   - `symbols`, `identifiers`, `spans`, `diagnostics`, and the other fact tables, each keyed by
     `blob_hash`. The row schema is the canonical extraction schema from `julie-extractors`, which is
     the producer contract when extraction runs in process. Facts are extracted once per blob and
     extractor version and never change after insert.
   - `paths(path, blob_hash)` — which blob each path currently holds.
   - `vectors(blob_hash, symbol_ordinal, encoder_id, vector)` — symbol embeddings (section 8).
   - `encoder(id, model_checksum, dimensions, pooling, normalization, instruction_policy)`.
   - `test_verdicts` (empty until CT lands, section 11).
   Rows for blobs no path references stay until the user runs `workspace rebuild`, which deletes
   `.julie/` and reindexes. There is no vacuum job and no garbage collector.
2. **`tantivy/`.** A Tantivy index over symbols and source bodies. Tantivy writes many segment files,
   so this is one durable root, not one file. Rebuilt from `facts.sqlite` when missing or when its
   recorded schema version differs. Never migrated.

Adding a third root is a design change, not a task.

### 6.2 Per machine: one database and one log

`~/.julie/registry.sqlite` holds checkouts and their roots. It is small and rebuildable by rescanning
known roots. `service.json` beside it is runtime state. Telemetry is an append-only JSONL log with
size-based rotation, kept because the benchmark harness and CT read it. A log is not a database; the
budget counts databases.

### 6.3 The in-memory graph

Reference edges are not stored. When a checkout becomes hot the service loads its symbols and
identifiers from `facts.sqlite`, resolves identifiers to symbols, and holds the result as adjacency
arrays indexed by a dense symbol id. Trace, impact, callers, and callees are walks over those arrays.

Resolution is a pure function from `(paths, facts)` to edges. It runs once per load and incrementally
per changed file. Its result is a cache. If the service restarts, it recomputes. The 1.1 GB of
persisted resolution history in Miller does not exist here.

Bound: a checkout's graph is loaded in full up to a configurable symbol count. Above that the service
loads per file on demand and evicts cold checkouts by least recent use. The status page shows graph
size, load time, and resident memory per checkout so the bound is tuned from evidence.

### 6.4 Snapshots

A snapshot is an `Arc` to the graph and a Tantivy searcher. It holds no SQLite transaction. Fact rows
are immutable once written, so a tool may read them with short queries at any time and see the same
bytes the graph was built from. The writer publishes a new snapshot after each commit by swapping the
`Arc`. Readers never block writers, writers never block readers, and no read transaction is ever held
across a tool call.

## 7. Worktrees

The brainstorm first agreed on shared per-family storage with a manifest per view. The Codex review
argued for one store per checkout, and the user accepted that change on 2026-09-09 for its simplicity.
The reasons:

- Family identity is fragile. Linked worktrees are easy, but bare repos, submodules, and alternate
  object stores are not, and the code to tell them apart is exactly the kind Miller drowned in.
- Sharing needs something to reason about which view owns which row. That is view manifests, view
  filters in every query, and view retirement. Miller's twelve stale views came from there.
- The byte multiplication that motivated sharing was the 48x resolution layer. This design does not
  persist resolution, so the per-checkout store is small. Duplicating a small store is cheap.
- The time multiplication was extraction and resolution. Resolution is in memory now. Extraction is
  avoided by copying.

**Opening a new checkout of a repo the service already knows:** the service finds a registered sibling
by `git rev-parse --git-common-dir`, copies `blobs` and fact rows for every hash present in the new
tree from the sibling's `facts.sqlite`, extracts only the blobs the sibling does not have, builds
`paths`, and rebuilds `tantivy/`. This is a one-time read of another checkout's file and a write to
this checkout's own. Nothing is shared afterwards. If no sibling exists or the common-dir lookup
fails, the checkout indexes from scratch. That is the only fallback.

In the agreed fleet of two to eight worktrees differing from main by tens of files, the copy takes
seconds and the extraction is a few files.

When a checkout root disappears, its registry row is removed on the next `workspace list` or by
`workspace remove`. There is nothing else to clean because nothing else knows about it.

## 8. Semantics

- Runtime: `julie-semantic-sidecar` in `serve` mode, one child per service, NDJSON over stdin and
  stdout. The service owns its lifetime. No broker, no sockets, no accelerator lease.
- The Python embedding host is removed from the product. It remains in the evaluation harness only as
  the quality baseline.
- Vectors live in the `vectors` table of `facts.sqlite`. Query is a brute-force cosine scan over the
  checkout's vectors, held in memory beside the graph. An ANN index is added only if a measured query
  on a real checkout exceeds the latency budget. That decision needs a number, and it is not made
  here. This removes a C++ dependency and a durable root from the first release.
- Encoder identity is one row. A different identity means delete the `vectors` rows and rebuild.
  Vectors from two encoders never mix.
- Model choice is a measured decision, not a design decision. Run the existing semantic scorecard
  corpus through the native sidecar for each candidate: BGE-small, Qwen3 0.6B, and CodeRankEmbed
  converted to GGUF if llama.cpp's nomic-bert support carries it. Ship the smallest model within one
  top-five case of the Python baseline. Record the result as a finding.
- Lexical-only mode does zero semantic work. Exact symbol and path queries never wait on inference.
- CUDA is out until a measured embed rate on a real workspace shows Vulkan is the bottleneck.

## 9. Tool contract, guidance, and hooks

- The ten Miller tools carry over as the public contract: `search`, `inspect`, `context`, `trace`,
  `impact`, `edit`, `patterns`, `content`, `workspace`, `tests`. Parameter shapes, compact output, and
  JSON contracts stay as Miller documents them. Julie's tool names retire behind that contract.
- Miller's agent guidance, routing block, `miller-*` skills, and session hooks move to this repository
  and rename, in phase 6, after the read tools prove the model.
- The benchmark harness from Miller and the `xtask-eval` revival harness from Julie merge into one
  harness in this repository. The head-to-head plan already defines the frozen experiment contract.
- The PreToolUse hook that denies unfiltered full-suite test runs ships with CT (section 11). Until
  then the hook is not installed.

## 10. Status page

`GET /status` returns one JSON document. `GET /` renders the same document as HTML. Both are
read-only. The document has, for the service: version, uptime, RSS, client count, queue depth, current
job, embedding child state. For each checkout: root, whether the root exists, watcher state, time of
the last file event, blob count, facts file size, Tantivy state (`present`, `building`, `stale`,
`absent`) with age, graph symbol count, load time, resident memory, vector count, and the last write
time. For the whole service: the last fifty requests with tool, checkout, latency, and outcome, and the
last twenty errors with full detail.

When a future bug hunt needs a fact that is not in the document, the fix is to add it to the document.
No log-grep-only diagnostics.

## 11. Continuous testing, deferred

CT is not in the first release and is not on this design's critical path. Its shape is fixed here so
the core does not have to change for it:

- Test selection is a walk from changed blobs to test symbols over the in-memory graph. It is a
  function in `julie-index`, not a daemon.
- Verdicts are rows in `facts.sqlite`: `test_verdicts(test_id, blob_set_hash, verdict, duration,
  recorded_at)`. A test is stale when its current blob set hash has no verdict row.
- The runner is a job type in the service, like extraction and embedding.
- `tests status` is a read of `test_verdicts`. `tests run` enqueues stale tests and returns a job id.
  `--all` runs the whole suite and requires a reason string, which is recorded.
- The PreToolUse hook denies an unfiltered `cargo test`, `dotnet test`, or `npm test` when CT is on
  for that checkout, and answers with the exact `julie test` command to run instead.
- Telemetry counts full-suite runs per session. The status page shows the count.

Miller's CT provider code for language runners is the input to that phase. The `ct.db` and the
separate daemon are not.

## 12. Complexity budgets and measurement

These are gates in CI and in review. A red gate is a failed build.

| Budget | Value | Measured by |
|---|---|---|
| Durable roots | 2 per checkout, 1 database per machine | a test that lists `.julie/` and `~/.julie/` after a full index |
| Fast bucket | under 10 s wall, warm, unit tests only | `cargo xtask test fast` timing gate |
| Full suite, Linux | under 120 s wall; excludes model download, real-repo fixtures, and Windows | CI timing gate |
| Multi-process tests | one bucket, under 20 s | CI timing gate |
| Coordination words in new code | zero without a written exception | `scripts/complexity-words.sh` on the diff, reviewed |
| Module size | 500 lines per file, excluding tests | a script gate |
| Dependencies | new crate needs a one-line justification in the PR | `cargo deny` plus review |
| Net lines, phases 2 and 3 | each phase negative against what it deletes | `tokei` before and after, gate at phase exit |
| Clean build time | reported after phase 1, gated at that number plus 20% afterwards | CI timing |
| Resident memory per hot checkout | reported after phase 3 on the Julie and Miller repos, gated at that number plus 20% afterwards | status page sample in the replay harness |
| Service line count | reported per release | `tokei` in CI |

Julie today is about 240k lines. The service should end smaller by deleting what section 4 forbids,
and the trend line is the evidence. Two budgets are set from measurement rather than guessed, because a
guessed number is either ignored or gamed.

### 12.1 Ponytail, an aside

Ponytail is a prompt plugin that makes an agent ask, in order: does this need to exist, is it already
here, does the standard library or an installed dependency cover it, can it be one line, and only then
write code. It does not change the service shape and it is not part of this design's acceptance. It is
recorded here because the user asked how to know whether it helps.

Measure it the same way the head-to-head measures products:

1. Pick ten implementation tasks from the phase 1 plan, all small enough to finish in one session.
2. Run each task twice from the same commit, once with Ponytail on and once off, with the same model
   and the same plan text. Randomize the order.
3. Record per run: net lines added, files touched, new dependencies, coordination words added, fast
   bucket time after the change, and whether the task's acceptance tests pass.
4. Keep Ponytail if the on-arm passes the same acceptance tests and adds fewer coordination words or
   fewer net lines in at least seven of ten tasks. Drop it if it fails any acceptance test the off-arm
   passes.

That is a one-session experiment. Its result goes in `docs/findings/`.

## 13. Testing strategy

- Tools are tested through the request engine with an in-memory `facts.sqlite`, a RAM Tantivy index,
  and a small fixture tree. No test opens a real repository.
- The single writer is tested as a function: given a path change, assert the resulting rows, Tantivy
  documents, and graph edges.
- The graph is tested as pure resolution: given facts, assert edges.
- The sibling seed copy is tested with two fixture trees that share most files.
- Transports are tested with one contract test per binding that sends the same request three ways and
  asserts identical results.
- One multi-process bucket covers: service start by first client, stale `service.json` recovery,
  token rejection, idle exit, version mismatch message, and stdio shim forwarding. It uses a temporary
  home and a fixture tree.
- Windows runs the same suite through the existing guest, outside the 120 s budget.
- Performance evidence is a separate replay harness, not a unit test. It reports until a budget is
  set from its numbers, then gates.

## 14. Phases

Each phase becomes one implementation plan after this design is approved.

1. **Service skeleton.** `julie-service` binary, `service.json` contract, token, idle exit, `/status`,
   `/api/workspace`, stdio shim, one contract test per binding, against real Claude Code, Codex, and
   Cursor clients. No indexing yet.
   **Gate:** if stateless MCP plus the shim needs session state or the Tasks extension to work with
   any of the three hosts, stop and reconsider before phase 2.
2. **Facts and paths.** `facts.sqlite` schema, single writer, in-process extractor, watcher, sibling
   seed copy. `workspace open`, `list`, `remove`, `rebuild`, `status`.
   **Gate:** the deletion list in section 5.4 is empty, and the phase is net negative in lines.
3. **Derived indexes and read tools.** Tantivy projection, in-memory graph, snapshots. `search`,
   `inspect`, `trace`, `impact`, `context`, `patterns` on the new engine.
   **Gate:** every budget in section 12 holds on the Julie and Miller repos, and the phase is net
   negative in lines. If not, stop and redesign before phase 4.
4. **Semantics.** Sidecar child, `vectors` table, brute-force scan, encoder identity, model scorecard
   finding.
5. **Edit and content.** `edit` and `content` on the new engine, per-path in-process mutex.
6. **Contract and guidance.** Miller tool names, compact output, hooks, skills, merged benchmark
   harness. Head-to-head run against Miller.
7. **Continuous testing.** Section 11, as its own design review first.

## 15. Risks

- **Host support.** A host that only speaks 2025-11-25 must work through the shim or `rmcp` fallback.
  Phase 1 tests all three hosts and gates on it.
- **One process, one failure domain.** A service crash interrupts every session. Mitigation: the
  service holds nothing durable in memory, restarts in under a second, and requests are idempotent, so
  clients retry. The embedding child is a separate process for the same reason.
- **Large repositories.** The in-memory graph has a bound and an eviction policy, but the numbers are
  guesses until phase 3 measures them on the dotnet/runtime fixture.
- **Brute-force vectors.** Fine for the agreed fleet. A very large checkout with semantics on may need
  ANN. The status page shows vector count and query latency so the decision has a number.
- **Duplicate bytes across worktrees.** Eight worktrees hold eight stores. Each is small without
  persisted resolution, but the number is unmeasured. Phase 2 reports store size per checkout.
- **Containers and SSH.** Localhost does not cross those boundaries. Out of scope for the first
  release; the token file contract leaves room for a forwarded port later.
- **Extraction in process.** An abort or native fault in a parser crashes the service. The client
  starts a new one and the offending file is not retried until it changes.

## 16. The June 2026 daemon, and why this is not a repeat

Julie already ran a resident HTTP daemon with a stdio adapter from May to June 2026. The rescue program
of 2026-06-03 measured it at 10,126 lines in `src/daemon/` plus 1,343 in `src/adapter/`, with twelve
test markers, and named it "the home of the unsolved hang and disconnect bugs". Phase 3d deleted it and
moved to the in-process leader-locked server that runs today. The rescue design also said the long-game
shape is a resident Rust process. This design is that shape. It must not grow the same way. What was
different then, and what is different now:

| June 2026 daemon | This service |
|---|---|
| Stateful MCP: sessions, `initialize`, roots, per-connection state bridged by the adapter | Stateless MCP: one POST per call, no session, the shim forwards bytes |
| Four-file lifecycle: `daemon.pid`, `daemon.state`, `daemon.lock`, `daemon.singleton`, then a kernel lock plus discovery plus token file, plus legacy migration | One file, `service.json`, written after bind. No singleton lock. A second service that loses the port bind exits |
| Stale-binary detection, restart-pending state, version handoff | None. Version mismatch is an error message and a manual restart |
| Per-workspace SQLite connection pool, mutation gate, eight background writers taking the gate directly | One writer per checkout, immutable fact rows, no read transactions held, no pool |
| Tests spawned dozens of daemon subprocesses | Tests bind an ephemeral port in process; one small multi-process bucket |
| Hangs never root-caused | Every request has a deadline and appears on the status page with its outcome |

Budget: the process model in `julie-service` (start, `service.json`, token, idle exit, shim spawn and
forward) is at most 600 lines excluding tests. The June daemon's equivalent was over 4,000. If the
600-line budget does not hold, the design is wrong, not the budget.

## Architecture Quality

**Affected modules:** every Julie crate; the root `src` tree; Miller's guidance, hooks, and benchmark
assets, which move here.

**Caller-facing interface:** the ten-tool contract over three bindings, the `julie` CLI, `/status`,
and the `service.json` client contract. Smaller than today: Julie's own tool names, continuations, the
Python host, and the broker client go away.

**Depth/locality check:** storage policy lives in `julie-facts` and `julie-index` only. Tools see a
snapshot. Bindings see typed requests. Jobs see the writer. No layer above `julie-index` knows about
SQLite or Tantivy.

**Test surface:** the request engine with in-memory stores; pure resolution; one contract test per
binding; one bounded multi-process bucket.

**Seams/adapters:** the snapshot type is the one seam between storage and tools. The job type is the
one seam between tools and background work. No adapter for alternative stores, transports beyond the
three named, or alternative extractors.

**Rejected shortcuts:** keeping both the old per-session runtime and the service during transition;
a graph database; LanceDB or DuckDB as the primary store; shared family storage with per-view
manifests; keeping the Python embedding host as a shipped fallback; persisting resolved edges;
per-view sidecar files; live version handoff; the Tasks extension; durable continuations.

**Architecture risk:** high. This replaces the runtime shape of two products. The gates after phases
1, 2, and 3 are the controls.

## Review record, 2026-09-09

Codex (gpt-5.5, high reasoning, read-only) reviewed the first draft. Disposition:

**Accepted, design changed:** one store per checkout with sibling seed copy instead of shared family
storage; no Tasks extension; no live version handoff; no continuations; no SQLite read transaction in
snapshots; no vacuum job; vectors in `facts.sqlite` with brute-force scan instead of a usearch file;
`catch_unwind` scoped to Rust panics only; producer contract restated as the canonical row schema;
"three durable files" restated as two durable roots; explicit scope on the full-suite budget; clean
build and memory budgets added from measurement; net-lines gate on phases 2 and 3; gate added after
phase 1; phase 2 gate requires the deletion list empty; Skills over MCP dropped.

**Accepted, wording fixed:** `workspace_id` is the tool contract, not a spec requirement; the spec
removes server-initiated JSON-RPC requests but keeps `input_required` results.

**Rejected:** dropping the telemetry log. The benchmark harness and CT read it, and a rotating JSONL
file is not a database. Removing the Ponytail section. The user asked for it; it is now marked as an
aside outside the design's acceptance.
